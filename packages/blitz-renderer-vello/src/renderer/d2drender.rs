use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use vello::peniko;
use windows::{
    core::*, Win32::Graphics::Direct2D::Common::*, Win32::Graphics::Direct2D::*,
    Win32::Graphics::Dxgi::Common::*, Win32::Graphics::Dxgi::*, Win32::System::Com::*,
};

use super::multicolor_rounded_rect::{Edge, ElementFrame};
use crate::util::{Color, ToColorColor};
use blitz_dom::node::{
    ImageData, ListItemLayout, ListItemLayoutPosition, Marker, NodeData, RasterImageData,
    TextBrush, TextInputData, TextNodeData,
};
use blitz_dom::{local_name, BaseDocument, ElementNodeData, Node};
use blitz_traits::Devtools;

use color::{AlphaColor, DynamicColor, Srgb};
use euclid::{Point2D, Transform3D};
// Add a unit type for our Point2D
type UnknownUnit = euclid::UnknownUnit;

use parley::Line;
use style::color::AbsoluteColor;
use style::{
    dom::TElement,
    properties::ComputedValues,
    OwnedSlice,
};
use image::imageops::FilterType;
use parley::layout::PositionedLayoutItem;
use style::values::generics::color::GenericColor;
use style::values::generics::image::{
    GenericCircle, GenericEllipse, GenericEndingShape, ShapeExtent,
};
use style::values::specified::percentage::ToPercentage;
use style::values::computed::image::{ Gradient as StyloGradient };
use style::values::generics::image::GradientFlags;
use style::values::computed::CSSPixelLength;
use taffy::Layout;
use windows_numerics::{self, Matrix3x2};

#[cfg(feature = "svg")]
use vello_svg::usvg;

const CLIP_LIMIT: usize = 1024;
static CLIPS_USED: AtomicUsize = AtomicUsize::new(0);
static CLIP_DEPTH: AtomicUsize = AtomicUsize::new(0);
static CLIP_DEPTH_USED: AtomicUsize = AtomicUsize::new(0);
static CLIPS_WANTED: AtomicUsize = AtomicUsize::new(0);

/// Helper trait for converting color types to Direct2D color format
pub trait ToD2dColor {
    /// Convert to a D2D1_COLOR_F structure
    fn to_d2d_color(&self) -> D2D1_COLOR_F;
}

impl ToD2dColor for AlphaColor<Srgb> {
    fn to_d2d_color(&self) -> D2D1_COLOR_F {
        // Access the components array [r, g, b, a]
        D2D1_COLOR_F {
            r: self.components[0] / 255.0, // Red
            g: self.components[1] / 255.0, // Green
            b: self.components[2] / 255.0, // Blue
            a: self.components[3] / 255.0, // Alpha
        }
    }
}

pub trait AbsoluteColorExt {
    /// Convert to a D2D1_COLOR_F structure
    fn to_d2d_color(&self) -> D2D1_COLOR_F;
}

impl AbsoluteColorExt for AbsoluteColor {
    fn to_d2d_color(&self) -> D2D1_COLOR_F {
        // Extract RGB components from ColorComponents
        let r = self.components.0;
        let g = self.components.1;
        let b = self.components.2;
        
        // Create D2D1_COLOR_F with the extracted components
        D2D1_COLOR_F {
            r,  // Red component
            g,  // Green component
            b,  // Blue component
            a: self.alpha,  // Alpha component
        }
    }
}

/// Draw the current tree to the current Direct2D surface
pub fn generate_d2d_scene(
    rt: &mut ID2D1DeviceContext,
    dom: &BaseDocument,
    scale: f64,
    width: u32,
    height: u32,
    devtool_config: Devtools,
) {
    CLIPS_USED.store(0, atomic::Ordering::SeqCst);
    CLIPS_WANTED.store(0, atomic::Ordering::SeqCst);

    let generator = D2dSceneGenerator {
        dom,
        scale,
        width,
        height,
        devtools: devtool_config,
    };
    generator.generate_d2d_scene(rt);
}

pub struct D2dSceneGenerator<'dom> {
    dom: &'dom BaseDocument,
    scale: f64,
    width: u32,
    height: u32,
    devtools: Devtools,
}

impl D2dSceneGenerator<'_> {
    fn node_position(&self, node: usize, location: Point2D<f64, f64>) -> (Layout, Point2D<f64, f64>) {
        let layout = self.layout(node);
        let pos: Point2D<f64, f64> = Point2D::new(
            location.x + layout.location.x as f64,
            location.y + layout.location.y as f64
        );
        (layout, pos)
    }

    fn layout(&self, child: usize) -> Layout {
        self.dom.as_ref().tree()[child].unrounded_layout
    }

    /// Generate a Direct2D scene from the DOM
    pub fn generate_d2d_scene(&self, rt: &mut ID2D1DeviceContext) {
        unsafe {
            // Clear the render target with white background
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }));

            // Set the transform to account for scale
            let scale_matrix = Matrix3x2 {
                M11: self.scale as f32,
                M12: 0.0,
                M21: 0.0,
                M22: self.scale as f32,
                M31: 0.0,
                M32: 0.0,
            };
            rt.SetTransform(&scale_matrix);
        }

        let viewport_scroll = self.dom.as_ref().viewport_scroll();

        // Get and render the root element
        let root_element = self.dom.as_ref().root_element();
        let root_id = root_element.id;
        let bg_width = (self.width as f32).max(root_element.final_layout.size.width);
        let bg_height = (self.height as f32).max(root_element.final_layout.size.height);

        // Draw background color if defined
        let background_color = {
            let html_color = root_element
                .primary_styles()
                .unwrap()
                .clone_background_color();
            if html_color == GenericColor::TRANSPARENT_BLACK {
                root_element
                    .children
                    .iter()
                    .find_map(|id| {
                        self.dom
                            .as_ref()
                            .get_node(*id)
                            .filter(|node| node.data.is_element_with_tag_name(&local_name!("body")))
                    })
                    .and_then(|body| body.primary_styles())
                    .map(|style| {
                        let current_color = style.clone_color();
                        style
                            .clone_background_color()
                            .resolve_to_absolute(&current_color)
                    })
            } else {
                let current_color = root_element.primary_styles().unwrap().clone_color();
                Some(html_color.resolve_to_absolute(&current_color))
            }
        };

        if let Some(bg_color) = background_color {
            let color = bg_color.as_srgb_color();
            let color_f = color.to_d2d_color();
            unsafe {
                let brush = self.create_solid_color_brush(rt, color_f);
                if let Ok(brush) = brush {
                    rt.FillRectangle(
                        &D2D_RECT_F {
                            left: 0.0,
                            top: 0.0,
                            right: bg_width,
                            bottom: bg_height,
                        },
                        &brush,
                    );
                }
            }
        }

        // Render the root element at position (-viewport_scroll.x, -viewport_scroll.y)
        self.render_element(
            rt,
            root_id,
            Point2D::new(
                -(viewport_scroll.x as f64),
                -(viewport_scroll.y as f64)
            )
        );

        // Render debug overlay if enabled
        if self.devtools.highlight_hover {
            if let Some(hover_id) = self.dom.as_ref().get_hover_node_id() {
                self.render_debug_overlay(rt, hover_id);
            }
        }

        // Reset transform
        unsafe {
            rt.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: 0.0,
                M32: 0.0,
            });
        }
    }

    fn render_debug_overlay(&self, rt: &mut ID2D1DeviceContext, node_id: usize) {
        let scale = self.scale;
        let viewport_scroll = self.dom.as_ref().viewport_scroll();
        let mut node = &self.dom.as_ref().tree()[node_id];

        let taffy::Layout { location, size, border, padding, margin, .. } = node.final_layout;
        let taffy::Size { width, height } = size;

        let padding_border = padding + border;
        let scaled_pb = padding_border.map(|v| f64::from(v) * scale);
        let scaled_padding = padding.map(|v| f64::from(v) * scale);
        let scaled_border = border.map(|v| f64::from(v) * scale);
        let scaled_margin = margin.map(|v| f64::from(v) * scale);

        let content_width = width - padding_border.left - padding_border.right;
        let content_height = height - padding_border.top - padding_border.bottom;

        let taffy::Point { x, y } = node.final_layout.location;

        let mut abs_x = x;
        let mut abs_y = y;
        
        // Find the absolute position by traversing parent nodes
        while let Some(parent_id) = node.layout_parent.get() {
            node = &self.dom.as_ref().tree()[parent_id];
            abs_x += node.final_layout.location.x;
            abs_y += node.final_layout.location.y;
        }

        abs_x -= viewport_scroll.x as f32;
        abs_y -= viewport_scroll.y as f32;

        // Apply scale factor
        let abs_x = f64::from(abs_x) * scale;
        let abs_y = f64::from(abs_y) * scale;
        let width = f64::from(width) * scale;
        let height = f64::from(height) * scale;
        let content_width = f64::from(content_width) * scale;
        let content_height = f64::from(content_height) * scale;

        unsafe {
            // Create brushes for each part of the box model
            let fill_color = Color::from_rgba8(66, 144, 245, 128); // blue for content
            let padding_color = Color::from_rgba8(81, 144, 66, 128); // green for padding
            let border_color = Color::from_rgba8(245, 66, 66, 128); // red for border
            let margin_color = Color::from_rgba8(249, 204, 157, 128); // orange for margin

            let fill_brush = self.create_solid_color_brush(rt, fill_color.to_d2d_color()).unwrap();
            let padding_brush = self.create_solid_color_brush(rt, padding_color.to_d2d_color()).unwrap();
            let border_brush = self.create_solid_color_brush(rt, border_color.to_d2d_color()).unwrap();
            let margin_brush = self.create_solid_color_brush(rt, margin_color.to_d2d_color()).unwrap();

            // Draw margin area (outmost)
            let margin_rect = D2D_RECT_F {
                left: (abs_x - scaled_margin.left) as f32,
                top: (abs_y - scaled_margin.top) as f32,
                right: (abs_x + width + scaled_margin.right) as f32,
                bottom: (abs_y + height + scaled_margin.bottom) as f32,
            };
            rt.FillRectangle(&margin_rect, &margin_brush);

            // Draw border area
            let border_rect = D2D_RECT_F {
                left: abs_x as f32,
                top: abs_y as f32,
                right: (abs_x + width) as f32,
                bottom: (abs_y + height) as f32,
            };
            rt.FillRectangle(&border_rect, &border_brush);

            // Draw padding area
            let padding_rect = D2D_RECT_F {
                left: (abs_x + scaled_border.left) as f32,
                top: (abs_y + scaled_border.top) as f32,
                right: (abs_x + width - scaled_border.right) as f32,
                bottom: (abs_y + height - scaled_border.bottom) as f32,
            };
            rt.FillRectangle(&padding_rect, &padding_brush);

            // Draw content area (innermost)
            let content_rect = D2D_RECT_F {
                left: (abs_x + scaled_pb.left) as f32,
                top: (abs_y + scaled_pb.top) as f32,
                right: (abs_x + scaled_pb.left + content_width) as f32,
                bottom: (abs_y + scaled_pb.top + content_height) as f32,
            };
            rt.FillRectangle(&content_rect, &fill_brush);
        }
    }

    fn render_element(&self, rt: &mut ID2D1DeviceContext, node_id: usize, location: Point2D<f64, f64>) {
        let node = &self.dom.as_ref().tree()[node_id];

        // Early return if the element is hidden
        if matches!(node.style.display, taffy::Display::None) {
            return;
        }

        // Only draw elements with a style
        if node.primary_styles().is_none() {
            return;
        }

        // Hide elements with "hidden" attribute
        if let Some("true" | "") = node.attr(local_name!("hidden")) {
            return;
        }

        // Hide inputs with type=hidden
        if node.local_name() == "input" && node.attr(local_name!("type")) == Some("hidden") {
            return;
        }

        // Hide elements with invisible styling
        let styles = node.primary_styles().unwrap();
        if styles.get_effects().opacity == 0.0 {
            return;
        }

        // Check for overflow and clipping
        let overflow_x = styles.get_box().overflow_x;
        let overflow_y = styles.get_box().overflow_y;
        let should_clip = !matches!(overflow_x, style::values::computed::Overflow::Visible) || 
                          !matches!(overflow_y, style::values::computed::Overflow::Visible);
        let clips_available = CLIPS_USED.load(atomic::Ordering::SeqCst) <= CLIP_LIMIT;

        // Get position and layout information
        let (layout, box_position) = self.node_position(node_id, location);
        let taffy::Layout { location: _, size, border, padding, .. } = node.final_layout;
        let scaled_pb: taffy::Rect<f64> = (padding + border).map(f64::from);
        let content_position: Point2D<f64, f64> = Point2D::new(
            box_position.x + scaled_pb.left,
            box_position.y + scaled_pb.top
        );
        let content_box_size: euclid::Size2D<f64, f64> = euclid::Size2D::new(
            (size.width - padding.left - padding.right - border.left - border.right) as f64,
            (size.height - padding.top - padding.bottom - border.top - border.bottom) as f64
        );

        // Don't render things that are out of view
        let scaled_y = box_position.y * self.scale;
        let scaled_content_height = content_box_size.height.max(size.height as f64) * self.scale;
        if scaled_y > self.height as f64 || scaled_y + scaled_content_height < 0.0 {
            return;
        }

        // Set up transform for this element
        unsafe {
            let transform = Matrix3x2 {
                M11: self.scale as f32,
                M12: 0.0,
                M21: 0.0,
                M22: self.scale as f32,
                M31: (content_position.x * self.scale) as f32,
                M32: (content_position.y * self.scale) as f32,
            };
            rt.SetTransform(&transform);
        }

        // Set up clipping if needed
        // let mut layer_params = None;
        if should_clip && clips_available {
            CLIPS_USED.fetch_add(1, atomic::Ordering::SeqCst);
            CLIPS_WANTED.fetch_add(1, atomic::Ordering::SeqCst);
            
            unsafe {
                // Create clipping geometry
                let clip_rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: content_box_size.width as f32,
                    bottom: content_box_size.height as f32,
                };
                
                // Push layer with clip rect
                use std::mem::ManuallyDrop;
                
                let params = D2D1_LAYER_PARAMETERS1 {
                    contentBounds: clip_rect,
                    geometricMask: ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::default(),
                    opacity: 1.0,
                    opacityBrush: ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                };
                // layer_params = Some(params.clone());
                
                let layer = rt.CreateLayer(None).unwrap();
                rt.PushLayer(&params, &layer);
            }
        }

        // Create an element context
        let cx = self.element_cx(node, layout, box_position);
        
        // Draw the element's components
        cx.stroke_effects(rt);
        cx.stroke_outline(rt);
        cx.draw_outset_box_shadow(rt);
        cx.draw_background(rt);
        cx.draw_inset_box_shadow(rt);
        cx.stroke_border(rt);
        cx.stroke_devtools(rt);

        // Draw content with correct scroll offset
        let content_position = Point2D::new(
            content_position.x,
            content_position.y - node.scroll_offset.y as f64
        );
        
        unsafe {
            // Update transform for scrolled content
            let transform = Matrix3x2 {
                M11: self.scale as f32,
                M12: 0.0,
                M21: 0.0,
                M22: self.scale as f32,
                M31: (content_position.x * self.scale) as f32,
                M32: (content_position.y * self.scale) as f32,
            };
            rt.SetTransform(&transform);
        }
        
        cx.draw_image(rt);
        #[cfg(feature = "svg")]
        cx.draw_svg(rt);
        cx.draw_input(rt);
        cx.draw_text_input_text(rt, content_position);
        cx.draw_inline_layout(rt, content_position);
        cx.draw_marker(rt, content_position);
        
        // Draw any child nodes
        cx.draw_children(rt);

        // Pop layer if we pushed one
        if should_clip && clips_available {
            unsafe {
                rt.PopLayer();
            }
        }
    }

    fn render_node(&self, rt: &mut ID2D1DeviceContext, node_id: usize, location: Point2D<f64, f64>) {
        let node = &self.dom.as_ref().tree()[node_id];
        match &node.data {
            NodeData::Element(_) => {
                self.render_element(rt, node_id, location);
            },
            NodeData::Text(_) => {
                // Text nodes are handled by their parent elements in draw_inline_layout
            },
            _ => {}
        }
    }

    fn element_cx<'w>(
        &'w self,
        node: &'w Node, 
        layout: Layout,
        box_position: Point2D<f64, f64>
    ) -> ElementCx<'w> {
        let style = node
            .stylo_element_data
            .borrow()
            .as_ref()
            .map(|element_data| element_data.styles.primary().clone())
            .unwrap_or(
                ComputedValues::initial_values_with_font_override(style::properties::style_structs::Font::initial_values()).to_arc(),
            );

        let scale = self.scale;

        // Create frame using the same approach as in render.rs
        let frame = ElementFrame::new(&style, &layout, scale);

        // Handle transform (similar to render.rs)
        let mut transform = Transform3D::identity()
            .then_scale(scale, scale, 1.0)
            .then_translate(euclid::vec3(box_position.x, box_position.y, 0.0));

        // Apply CSS transform property
        let (t, has_3d) = &style
            .get_box()
            .transform
            .to_transform_3d_matrix(None)
            .unwrap_or((Transform3D::default(), false));
        
        if !has_3d {
            // Handle 2D transforms - equivalent to the Vello implementation
            if !has_3d {
                // Get transform origin from CSS
                let transform_origin = &style.get_box().transform_origin;
                
                // Calculate the transform origin in pixels
                let origin_x = transform_origin
                    .horizontal
                    .resolve(CSSPixelLength::new(frame.border_box.width() as f32))
                    .px() as f64;
                
                let origin_y = transform_origin
                    .vertical
                    .resolve(CSSPixelLength::new(frame.border_box.height() as f32))
                    .px() as f64;
                
                // Create a transform that:
                // 1. Translates to the transform origin
                // 2. Applies the CSS transform
                // 3. Translates back from the origin
                let origin_transform: Transform3D<f64, f64, f64> = Transform3D::translation(origin_x, origin_y, 0.0);
                let inverse_origin: Transform3D<f64, f64, f64> = Transform3D::translation(-origin_x, -origin_y, 0.0);
                
                // Convert t from f32 to f64 to match other transforms
                let t_f64: Transform3D<f64, UnknownUnit, UnknownUnit> = t.cast::<f64>();
                
                // Combine the transforms: inverse_origin * transform * origin
                transform = inverse_origin
                    .then(&t_f64)
                    .then(&origin_transform)
                    .then(&transform);
            }
        }

        let element = node.element_data().unwrap();

        ElementCx {
            context: self,
            frame,
            style,
            pos: box_position,
            scale,
            node,
            element,
            transform,
            #[cfg(feature = "svg")]
            svg: element.svg_data(),
            text_input: element.text_input_data(),
            list_item: element.list_item_data.as_deref(),
            devtools: &self.devtools,
        }
    }
    
    // Helper function to create D2D solid color brush
    fn create_solid_color_brush(&self, rt: &ID2D1DeviceContext, color_f: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {

        let properties = D2D1_BRUSH_PROPERTIES {
            opacity: 1.0,
            transform: Matrix3x2::default(),
        };
        
        unsafe { rt.CreateSolidColorBrush(&color_f, Some(&properties)) }
    }
}

/// Ensure that the `resized_image` field has a correctly sized image
fn ensure_resized_image(data: &RasterImageData, width: u32, height: u32) {
    let mut resized_image = data.resized_image.borrow_mut();

    if resized_image.is_none()
        || resized_image
            .as_ref()
            .is_some_and(|img| img.width != width || img.height != height)
    {
        let image_data = data
            .image
            .clone()
            .resize_to_fill(width, height, FilterType::Lanczos3)
            .into_rgba8()
            .into_raw();

        let peniko_image = peniko::Image {
            data: peniko::Blob::new(Arc::new(image_data)),
            format: peniko::ImageFormat::Rgba8,
            width,
            height,
            alpha: 1.0,
            x_extend: peniko::Extend::Pad,
            y_extend: peniko::Extend::Pad,
            quality: peniko::ImageQuality::High,
        };

        *resized_image = Some(Arc::new(peniko_image));
    }
}

struct ElementCx<'a> {
    context: &'a D2dSceneGenerator<'a>,
    frame: ElementFrame,
    style: style::servo_arc::Arc<ComputedValues>,
    pos: Point2D<f64, f64>,
    scale: f64,
    node: &'a Node,
    element: &'a ElementNodeData,
    transform: Transform3D<f64, f64, f64>,
    #[cfg(feature = "svg")]
    svg: Option<&'a usvg::Tree>,
    text_input: Option<&'a TextInputData>,
    list_item: Option<&'a ListItemLayout>,
    devtools: &'a Devtools,
}

impl ElementCx<'_> {
    fn with_maybe_clip(
        &self, 
        rt: &mut ID2D1DeviceContext, 
        mut condition: impl FnMut() -> bool,
        mut cb: impl FnMut(&ElementCx<'_>, &mut ID2D1DeviceContext)
    ) {
        let clip_wanted = condition();
        let mut clips_available = false;
        if clip_wanted {
            CLIPS_WANTED.fetch_add(1, atomic::Ordering::SeqCst);
            clips_available = CLIPS_USED.load(atomic::Ordering::SeqCst) <= CLIP_LIMIT;
        }
        let do_clip = clip_wanted & clips_available;
    
        // Create a layer for clipping if needed
        if do_clip {
            unsafe {
                let clip_rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.border_box.width() as f32,
                    bottom: self.frame.border_box.height() as f32,
                };
                
                let layer = rt.CreateLayer(None).unwrap();
                CLIPS_USED.fetch_add(1, atomic::Ordering::SeqCst);
                let depth = CLIP_DEPTH.fetch_add(1, atomic::Ordering::SeqCst) + 1;
                CLIP_DEPTH_USED.fetch_max(depth, atomic::Ordering::SeqCst);
                
                let params = D2D1_LAYER_PARAMETERS1 {
                    contentBounds: clip_rect,
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::default(),
                    opacity: 1.0,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                };
                
                rt.PushLayer(&params, &layer);
            }
        }
    
        // Execute the callback
        cb(self, rt);
        
        // Pop the layer if we pushed one
        if do_clip {
            unsafe {
                rt.PopLayer();
                CLIP_DEPTH.fetch_sub(1, atomic::Ordering::SeqCst);
            }
        }
    }

    fn draw_inline_layout(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        if self.node.is_inline_root {
            // Get inline layout data from the element
            if let Some(inline_layout) = &self.element.inline_layout_data {
                self.stroke_text(rt, inline_layout.layout.lines(), pos);
            }
        }
    }

    fn stroke_text<'a>(
        &self,
        rt: &mut ID2D1DeviceContext,
        lines: impl Iterator<Item = Line<'a, TextBrush>>,
        pos: Point2D<f64, f64>,
    ) {
        let transform = Matrix3x2 {
            M11: self.scale as f32,
            M12: 0.0,
            M21: 0.0,
            M22: self.scale as f32,
            M31: (pos.x * self.scale) as f32,
            M32: (pos.y * self.scale) as f32,
        };

        unsafe {
            rt.SetTransform(&transform);

            for line in lines {
                for item in line.items() {
                    if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                        let x = glyph_run.offset();
                        let y = glyph_run.baseline();

                        let run = glyph_run.run();
                        let font_size = run.font_size();
                        let metrics = run.metrics();
                        let style = glyph_run.style();

                        // Get the brush color from the style
                        let text_color = match style.brush.brush {
                            peniko::Brush::Solid(colorAlpha) => colorAlpha.to_d2d_color(),
                            // Handle other brush types if needed
                            _ => D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                        };

                        let text_brush = self.context.create_solid_color_brush(rt, text_color).unwrap();

                        // In a full implementation, we'd use DirectWrite to render text
                        // Draw a rectangle representation of each glyph run
                        let glyph_rect = D2D_RECT_F {
                            left: x as f32,
                            top: (y - metrics.ascent) as f32,
                            right: (x + glyph_run.advance()) as f32,
                            bottom: (y + metrics.descent) as f32,
                        };
                        
                        rt.FillRectangle(&glyph_rect, &text_brush);

                        // Draw decorations (underline, strikethrough) if present
                        if let Some(underline) = &style.underline {
                            let underline_brush = self.context.create_solid_color_brush(rt, text_color).unwrap();
                            let underline_y = y + metrics.underline_offset;
                            let underline_size = metrics.underline_size;
                            
                            let underline_rect = D2D_RECT_F {
                                left: x as f32,
                                top: underline_y as f32,
                                right: (x + glyph_run.advance()) as f32,
                                bottom: (underline_y + underline_size) as f32,
                            };
                            
                            rt.FillRectangle(&underline_rect, &underline_brush);
                        }
                        
                        if let Some(strikethrough) = &style.strikethrough {
                            let strikethrough_brush = self.context.create_solid_color_brush(rt, text_color).unwrap();
                            let strikethrough_y = y - metrics.ascent / 2.0;
                            let strikethrough_size = metrics.strikethrough_size;
                            
                            let strikethrough_rect = D2D_RECT_F {
                                left: x as f32,
                                top: strikethrough_y as f32,
                                right: (x + glyph_run.advance()) as f32,
                                bottom: (strikethrough_y + strikethrough_size) as f32,
                            };
                            
                            rt.FillRectangle(&strikethrough_rect, &strikethrough_brush);
                        }
                    }
                }
            }
        }
    }

    fn draw_text_input_text(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        // Render text in text inputs
        if let Some(input_data) = self.text_input {
            let transform = Matrix3x2 {
                M11: self.scale as f32,
                M12: 0.0,
                M21: 0.0,
                M22: self.scale as f32,
                M31: (pos.x * self.scale) as f32,
                M32: (pos.y * self.scale) as f32,
            };

            unsafe {
                rt.SetTransform(&transform);

                // Render selection/caret if input is focused
                if self.node.is_focussed() {
                    // Create selection highlight brush
                    let selection_brush = self.context.create_solid_color_brush(
                        rt, 
                        D2D1_COLOR_F { r: 0.0, g: 0.478, b: 1.0, a: 0.4 }
                    ).unwrap();
                    
                    let cursor_brush = self.context.create_solid_color_brush(
                        rt,
                        D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }
                    ).unwrap();

                    // Draw selection rectangles
                    for rect in input_data.editor.selection_geometry().iter() {
                        let d2d_rect = D2D_RECT_F {
                            left: rect.x0 as f32,
                            top: rect.y0 as f32,
                            right: rect.x1 as f32,
                            bottom: rect.y1 as f32,
                        };
                        rt.FillRectangle(&d2d_rect, &selection_brush);
                    }

                    // Draw cursor
                    if let Some(cursor) = input_data.editor.cursor_geometry(1.5) {
                        // In Direct2D, convert the cursor shape to a rectangle
                        let cursor_rect = D2D_RECT_F {
                            left: cursor.x0 as f32,
                            top: cursor.y0 as f32,
                            right: cursor.x1 as f32,
                            bottom: cursor.y1 as f32,
                        };
                        rt.FillRectangle(&cursor_rect, &cursor_brush);
                    }
                }

                // Render the actual text
                if let Some(layout) = input_data.editor.try_layout() {
                    self.stroke_text(rt, layout.lines(), pos);
                }
            }
        }
    }

    fn draw_marker(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        if let Some(ListItemLayout {
            marker,
            position: ListItemLayoutPosition::Outside(layout),
        }) = self.list_item {
            // Right align and pad the bullet when rendering outside
            let x_padding = match marker {
                Marker::Char(_) => 8.0,
                Marker::String(_) => 0.0,
            };
            let x_offset = -(layout.full_width() / layout.scale() + x_padding);

            // Align the marker with the baseline of the first line of text in the list item
            let y_offset = if let Some(first_text_line) = &self
                .element
                .inline_layout_data
                .as_ref()
                .and_then(|text_layout| text_layout.layout.lines().next())
            {
                (first_text_line.metrics().baseline
                    - layout.lines().next().unwrap().metrics().baseline)
                    / layout.scale()
            } else {
                0.0
            };

            let marker_pos = Point2D::new(
                pos.x + x_offset as f64,
                pos.y + y_offset as f64,
            );

            // Use the stroke_text method to render the marker text
            self.stroke_text(rt, layout.lines(), marker_pos);
        }
    }

    fn draw_children(&self, rt: &mut ID2D1DeviceContext) {
        // Iterate through child nodes and render them
        for &child_id in &self.node.children {
            self.context.render_node(rt, child_id, self.pos);
        }
    }

    #[cfg(feature = "svg")]
    fn draw_svg(&self, rt: &mut ID2D1DeviceContext) {
        // SVG rendering in Direct2D would require complex implementation
        // Basic approach would be to convert SVG paths to Direct2D geometries
        if let Some(svg) = self.svg {
            // This is a simplified placeholder
            unsafe {
                let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255).to_d2d_color()).unwrap();
                
                // Draw a rectangle as a placeholder for SVG
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.border_box.width() as f32,
                    bottom: self.frame.border_box.height() as f32,
                };
                
                rt.DrawRectangle(&rect, &brush, 1.0, None);
            }
        }
    }

    #[cfg(feature = "svg")]
    fn draw_svg_bg_image(&self, rt: &mut ID2D1DeviceContext, idx: usize) {
        // Similar to draw_svg, but for background images
        // Simplified implementation
    }

    fn draw_image(&self, rt: &mut ID2D1DeviceContext) {
        let width = self.frame.content_box.width() as u32;
        let height = self.frame.content_box.height() as u32;
        let x = self.frame.content_box.origin().x;
        let y = self.frame.content_box.origin().y;
        
        // Update transform to include content box position
        let transform = Matrix3x2 {
            M11: self.scale as f32,
            M12: 0.0,
            M21: 0.0,
            M22: self.scale as f32,
            M31: ((self.pos.x + x) * self.scale) as f32,
            M32: ((self.pos.y + y) * self.scale) as f32,
        };
        
        unsafe {
            rt.SetTransform(&transform);
        }

        if let Some(image_data) = self.element.raster_image_data() {
            // Ensure we have the correctly sized image
            ensure_resized_image(image_data, width, height);
            let resized_image = image_data.resized_image.borrow();
            
            if let Some(img) = resized_image.as_ref() {
                unsafe {
                    // In a real implementation we'd create a D2D bitmap from the image data
                    // This is simplified as creating D2D bitmaps requires multiple steps
                    
                    // Create D2D bitmap properties
                    let props = D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                        colorContext: std::mem::ManuallyDrop::new(None),
                    };
                    
                    // Create D2D bitmap from the image data
                    // In real code, we'd need to handle errors properly
                    let bitmap_data = img.data.as_ref();
                    let size = D2D_SIZE_U { width, height };
                    
                    // The following is a placeholder that demonstrates how you would
                    // create and draw a bitmap in Direct2D:
                    if let Ok(bitmap) = rt.CreateBitmap(size, Some(bitmap_data.as_ptr() as *const _), 
                                                      width * 4, &props) {
                        // Draw the bitmap at position (0,0) with its full size
                        let dest_rect = D2D_RECT_F {
                            left: 0.0,
                            top: 0.0,
                            right: width as f32,
                            bottom: height as f32,
                        };
                        
                        rt.DrawBitmap(
                            &bitmap,
                            Some(&dest_rect),
                            1.0,
                            D2D1_INTERPOLATION_MODE_LINEAR,
                            None,
                            None, // Adding the missing perspective transform parameter
                        );
                    }
                }
            }
        }
        
        // Reset transform back to default for this element
        unsafe {
            let base_transform = Matrix3x2 {
                M11: self.scale as f32,
                M12: 0.0,
                M21: 0.0,
                M22: self.scale as f32,
                M31: (self.pos.x * self.scale) as f32,
                M32: (self.pos.y * self.scale) as f32,
            };
            rt.SetTransform(&base_transform);
        }
    }

    fn draw_raster_bg_image(&self, rt: &mut ID2D1DeviceContext, idx: usize) {
        // Similar to draw_image, but for background images
        // Implementation would follow the same pattern
    }

    fn stroke_devtools(&self, rt: &mut ID2D1DeviceContext) {
        if self.devtools.show_layout {
            unsafe {
                // Determine stroke color based on display mode
                let stroke_color = match self.node.style.display {
                    taffy::Display::Block => Color::new([1.0, 0.0, 0.0, 1.0]),
                    taffy::Display::Flex => Color::new([0.0, 1.0, 0.0, 1.0]),
                    taffy::Display::Grid => Color::new([0.0, 0.0, 1.0, 1.0]),
                    taffy::Display::None => Color::new([0.0, 0.0, 1.0, 1.0]),
                };
                
                let brush = self.context.create_solid_color_brush(rt, stroke_color.to_d2d_color()).unwrap();
                
                // Use border_box as in the original implementation
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.border_box.width() as f32,
                    bottom: self.frame.border_box.height() as f32,
                };
                
                // Use stroke width of 1.0 scaled by self.scale
                rt.DrawRectangle(&rect, &brush, self.scale as f32, None);
            }
        }
    }

    fn draw_background(&self, rt: &mut ID2D1DeviceContext) {
        // Handle clipping as in the Vello implementation
        CLIPS_WANTED.fetch_add(1, atomic::Ordering::SeqCst);
        let clips_available = CLIPS_USED.load(atomic::Ordering::SeqCst) <= CLIP_LIMIT;
        
        // Create a layer for clipping if needed
        if clips_available {
            unsafe {
                // Create clipping geometry based on frame
                let factory: ID2D1Factory = rt.GetFactory().unwrap();
                let clip_rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.padding_box.width() as f32,
                    bottom: self.frame.padding_box.height() as f32,
                };
                
                // Create geometry for clipping - always use rounded rectangle
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: clip_rect,
                    // Use actual radius values if we have border radius, otherwise use 0
                    radiusX: if self.frame.has_border_radius() {
                        self.frame.border_top_left_radius_width as f32
                    } else {
                        0.0
                    },
                    radiusY: if self.frame.has_border_radius() {
                        self.frame.border_top_left_radius_height as f32
                    } else {
                        0.0
                    },
                };
                let geometry = factory.CreateRoundedRectangleGeometry(&rounded_rect).unwrap();
                
                // Create layer parameters with the geometry mask
                let layer = rt.CreateLayer(None).unwrap();
                CLIPS_USED.fetch_add(1, atomic::Ordering::SeqCst);
                let depth = CLIP_DEPTH.fetch_add(1, atomic::Ordering::SeqCst) + 1;
                CLIP_DEPTH_USED.fetch_max(depth, atomic::Ordering::SeqCst);
                
                let params = D2D1_LAYER_PARAMETERS1 {
                    contentBounds: clip_rect,
                    geometricMask: std::mem::ManuallyDrop::new(Some(geometry.into())),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::default(),
                    opacity: 1.0,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                };
                
                rt.PushLayer(&params, &layer);
            }
        }

        // Draw background color (solid frame)
        self.draw_solid_frame(rt);
        
        // Handle background images
        let segments = &self.style.get_background().background_image.0;
        for (idx, segment) in segments.iter().enumerate().rev() {
            match segment {
                style::values::computed::image::Image::None => {
                    // Do nothing
                },
                style::values::computed::image::Image::Gradient(gradient) => {
                    self.draw_gradient_frame(rt, gradient);
                },
                style::values::computed::image::Image::Url(_) => {
                    self.draw_raster_bg_image(rt, idx);
                    #[cfg(feature = "svg")]
                    self.draw_svg_bg_image(rt, idx);
                },
                _ => {
                    // Other types not yet implemented
                    // Would include PaintWorklet, CrossFade, ImageSet
                }
            }
        }

        // Pop the layer if we pushed one
        if clips_available {
            unsafe {
                rt.PopLayer();
                CLIP_DEPTH.fetch_sub(1, atomic::Ordering::SeqCst);
            }
        }
    }
    
    fn draw_solid_frame(&self, rt: &mut ID2D1DeviceContext) {
        let current_color = self.style.clone_color();
        let background_color = &self.style.get_background().background_color;
        let bg_color = background_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();

        if bg_color != Color::TRANSPARENT {
            unsafe {
                // Create the brush with the background color
                let brush = self.context.create_solid_color_brush(rt, bg_color.to_d2d_color()).unwrap();
                
                // Use the frame's padding box directly for the rectangle
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.padding_box.width() as f32,
                    bottom: self.frame.padding_box.height() as f32,
                };
                
                rt.FillRectangle(&rect, &brush);
            }
        }
    }

    fn draw_gradient_frame(&self, rt: &mut ID2D1DeviceContext, gradient: &StyloGradient) {
        match gradient {
            // https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient
            style::values::generics::image::GenericGradient::Linear {
                direction,
                items,
                flags,
                // compat_mode,
                ..
            } => self.draw_linear_gradient(rt, direction, items, *flags),
            style::values::generics::image::GenericGradient::Radial {
                shape,
                position,
                items,
                flags,
                // compat_mode,
                ..
            } => self.draw_radial_gradient(rt, shape, position, items, *flags),
            style::values::generics::image::GenericGradient::Conic {
                angle,
                position,
                items,
                flags,
                ..
            } => self.draw_conic_gradient(rt, angle, position, items, *flags),
        };
    }

    fn draw_linear_gradient(
        &self,
        rt: &mut ID2D1DeviceContext,
        direction: &style::values::computed::LineDirection,
        items: &[style::values::generics::image::GenericGradientItem<GenericColor<style::values::computed::Percentage>, style::values::computed::LengthPercentage>],
        flags: GradientFlags,
    ) {
        let bb = vello::kurbo::Shape::bounding_box(&self.frame.border_box);
        let current_color = self.style.clone_color();
        let center:Point2D<f64, f64> = Point2D::new(bb.center().x, bb.center().y);
        let rect = self.frame.padding_box;
        
        // Calculate start and end points based on direction
        let (start, end) = match direction {
            style::values::computed::LineDirection::Angle(angle) => {
                let angle = -angle.radians64() + std::f64::consts::PI;
                let offset_length = rect.width() / 2.0 * angle.sin().abs()
                    + rect.height() / 2.0 * angle.cos().abs();
                let offset_vec_x = angle.sin() * offset_length;
                let offset_vec_y = angle.cos() * offset_length;
                let start_point: Point2D<f64, f64> = Point2D::new(center.x - offset_vec_x, center.y - offset_vec_y);
                let end_point: Point2D<f64, f64> = Point2D::new(center.x - offset_vec_x, center.y - offset_vec_y);
                (
                    start_point,
                    end_point
                )
            }
            style::values::computed::LineDirection::Horizontal(horizontal) => {
                let start = Point2D::new(
                    rect.x0,
                    rect.y0 + rect.height() / 2.0,
                );
                let end = Point2D::new(
                    rect.x1,
                    rect.y0 + rect.height() / 2.0,
                );
                match horizontal {
                    style::values::specified::position::HorizontalPositionKeyword::Right => (start, end),
                    style::values::specified::position::HorizontalPositionKeyword::Left => (end, start),
                }
            }
            style::values::computed::LineDirection::Vertical(vertical) => {
                let start = Point2D::new(
                    rect.x0 + rect.width() / 2.0,
                    rect.y0,
                );
                let end = Point2D::new(
                    rect.x0 + rect.width() / 2.0,
                    rect.y1,
                );
                match vertical {
                    style::values::specified::position::VerticalPositionKeyword::Bottom => (start, end),
                    style::values::specified::position::VerticalPositionKeyword::Top => (end, start),
                }
            }
            style::values::computed::LineDirection::Corner(horizontal, vertical) => {
                let (start_x, end_x) = match horizontal {
                    style::values::specified::position::HorizontalPositionKeyword::Right => (rect.x0, rect.x1),
                    style::values::specified::position::HorizontalPositionKeyword::Left => (rect.x1, rect.x0),
                };
                let (start_y, end_y) = match vertical {
                    style::values::specified::position::VerticalPositionKeyword::Bottom => (rect.y0, rect.y1),
                    style::values::specified::position::VerticalPositionKeyword::Top => (rect.y1, rect.y0),
                };
                (
                    Point2D::new(start_x, start_y),
                    Point2D::new(end_x, end_y)
                )
            }
        };

        let gradient_length = CSSPixelLength::new((start.distance_to(end) / self.scale) as f32);
        let repeating = flags.contains(GradientFlags::REPEATING);

        unsafe {
            // Create gradient stops for Direct2D
            let mut d2d_stops = Vec::new();
            
            // Helper function to process color stops, similar to resolve_length_color_stops
            let mut hint: Option<f32> = None;
            
            for (idx, item) in items.iter().enumerate() {
                let (color, offset) = match item {
                    style::values::generics::image::GenericGradientItem::SimpleColorStop(color) => {
                        let position = match idx {
                            0 => 0.0,
                            _ if idx == items.len() - 1 => 1.0,
                            _ => idx as f32 / (items.len() - 1) as f32,
                        };
                        (color.resolve_to_absolute(&current_color), position)
                    },
                    style::values::generics::image::GenericGradientItem::ComplexColorStop { color, position } => {
                        let pos = position
                            .resolve(gradient_length)
                            .px() / gradient_length.px();
                        (color.resolve_to_absolute(&current_color), pos)
                    },
                    style::values::generics::image::GenericGradientItem::InterpolationHint(position) => {
                        // Store hint and continue
                        hint = Some(position.resolve(gradient_length).px() / gradient_length.px());
                        continue;
                    }
                };

                // Add stop to collection
                d2d_stops.push(D2D1_GRADIENT_STOP {
                    position: offset,
                    color: color.as_srgb_color().to_d2d_color(),
                });
            }
            
            // Create D2D gradient stops collection
            let stops_collection = rt.CreateGradientStopCollection(
                &d2d_stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                if repeating { D2D1_EXTEND_MODE_WRAP } else { D2D1_EXTEND_MODE_CLAMP },
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT
            ).unwrap();
            
            // Convert points to D2D format
            let start_point = D2D_POINT_2F { 
                x: start.x as f32, 
                y: start.y as f32 
            };
            let end_point = D2D_POINT_2F { 
                x: end.x as f32, 
                y: end.y as f32 
            };
            
            // Create linear gradient brush
            let brush = rt.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: start_point,
                    endPoint: end_point,
                },
                None,
                &stops_collection,
            ).unwrap();
            
            // Draw rounded rectangle with gradient
            if self.frame.has_border_radius() {
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.padding_box.width() as f32,
                        bottom: self.frame.padding_box.height() as f32,
                    },
                    radiusX: self.frame.border_top_left_radius_width as f32,
                    radiusY: self.frame.border_top_left_radius_height as f32,
                };
                rt.FillRoundedRectangle(&rounded_rect, &brush);
            } else {
                // Simple rectangle
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.padding_box.width() as f32,
                    bottom: self.frame.padding_box.height() as f32,
                };
                rt.FillRectangle(&rect, &brush);
            }
        }
    }
    
    fn draw_radial_gradient(
        &self,
        rt: &mut ID2D1DeviceContext,
        shape: &style::values::generics::image::EndingShape<style::values::generics::NonNegative<CSSPixelLength>, style::values::generics::NonNegative<style::values::computed::LengthPercentage>>,
        position: &style::values::generics::position::GenericPosition<style::values::computed::LengthPercentage, style::values::computed::LengthPercentage>,
        items: &[style::values::generics::image::GenericGradientItem<GenericColor<style::values::computed::Percentage>, style::values::computed::LengthPercentage>],
        flags: GradientFlags,
    ) {
        let rect = self.frame.padding_box;
        let repeating = flags.contains(GradientFlags::REPEATING);
        let current_color = self.style.clone_color();

        unsafe {
            // Create gradient stops for Direct2D (similar to linear gradient)
            let mut d2d_stops = Vec::new();
            
            // Process color stops
            for (idx, item) in items.iter().enumerate() {
                let (color, offset) = match item {
                    style::values::generics::image::GenericGradientItem::SimpleColorStop(color) => {
                        let position = match idx {
                            0 => 0.0,
                            _ if idx == items.len() - 1 => 1.0,
                            _ => idx as f32 / (items.len() - 1) as f32,
                        };
                        (color.resolve_to_absolute(&current_color), position)
                    },
                    style::values::generics::image::GenericGradientItem::ComplexColorStop { color, position } => {
                        // Calculate a preliminary gradient radius based on the rect dimensions
                        let preliminary_radius = CSSPixelLength::new((rect.width().max(rect.height()) / 2.0) as f32);
                        let pos = position.resolve(preliminary_radius).px() / preliminary_radius.px();
                        (color.resolve_to_absolute(&current_color), pos)
                    },
                    _ => continue,
                };

                // Add stop to collection
                d2d_stops.push(D2D1_GRADIENT_STOP {
                    position: offset,
                    color: color.as_srgb_color().to_d2d_color(),
                });
            }
            
            // Create D2D gradient stops collection
            let stops_collection = rt.CreateGradientStopCollection(
                &d2d_stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                if repeating { D2D1_EXTEND_MODE_WRAP } else { D2D1_EXTEND_MODE_CLAMP },
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT
            ).unwrap();
            
            // Calculate center position
            let (width_px, height_px) = (
                position
                    .horizontal
                    .resolve(CSSPixelLength::new(rect.width() as f32))
                    .px() as f32,
                position
                    .vertical
                    .resolve(CSSPixelLength::new(rect.height() as f32))
                    .px() as f32,
            );
            
            // Calculate radius
            let radius_x;
            let radius_y;
            
            // Determine gradient radii based on shape
            match shape {
                GenericEndingShape::Circle(circle) => {
                    let scale = match circle {
                        // Simplified radius calculation
                        _ => rect.width().min(rect.height()) as f32 / 2.0,
                    };
                    radius_x = scale;
                    radius_y = scale;
                },
                GenericEndingShape::Ellipse(_) => {
                    // Simplified ellipse handling
                    radius_x = rect.width() as f32 / 2.0;
                    radius_y = rect.height() as f32 / 2.0;
                }
            }
            
            // Create radial gradient brush
            let brush = rt.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: D2D_POINT_2F { x: width_px, y: height_px },
                    gradientOriginOffset: D2D_POINT_2F { x: 0.0, y: 0.0 },
                    radiusX: radius_x,
                    radiusY: radius_y,
                },
                None,
                &stops_collection,
            ).unwrap();
            
            // Draw with the gradient
            if self.frame.has_border_radius() {
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.padding_box.width() as f32,
                        bottom: self.frame.padding_box.height() as f32,
                    },
                    radiusX: self.frame.border_top_left_radius_width as f32,
                    radiusY: self.frame.border_top_left_radius_height as f32,
                };
                rt.FillRoundedRectangle(&rounded_rect, &brush);
            } else {
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.padding_box.width() as f32,
                    bottom: self.frame.padding_box.height() as f32,
                };
                rt.FillRectangle(&rect, &brush);
            }
        }
    }
    
    fn draw_conic_gradient(
        &self,
        rt: &mut ID2D1DeviceContext,
        angle: &style::values::computed::Angle,
        position: &style::values::generics::position::GenericPosition<style::values::computed::LengthPercentage, style::values::computed::LengthPercentage>,
        items: &style::OwnedSlice<style::values::generics::image::GenericGradientItem<GenericColor<style::values::computed::Percentage>, style::values::computed::AngleOrPercentage>>,
        flags: GradientFlags,
    ) {
        let repeating = flags.contains(GradientFlags::REPEATING);
        // Direct2D doesn't have native conic gradient support
        // For a proper implementation, we'd need to either:
        // 1. Use a bitmap render and create the conic gradient manually
        // 2. Use Direct2D effects to simulate a conic gradient
        
        // This is a simplified fallback that draws a radial gradient instead
        unsafe {
            let rect = self.frame.padding_box;
            let current_color = self.style.clone_color();
            
            // Create gradient stops
            let mut d2d_stops = Vec::new();
            
            for (idx, item) in items.iter().enumerate() {
                let (color, offset) = match item {
                    style::values::generics::image::GenericGradientItem::SimpleColorStop(color) => {
                        let position = match idx {
                            0 => 0.0,
                            _ if idx == items.len() - 1 => 1.0,
                            _ => idx as f32 / (items.len() - 1) as f32,
                        };
                        (color.resolve_to_absolute(&current_color), position)
                    },
                    style::values::generics::image::GenericGradientItem::ComplexColorStop { color, position } => {
                        // Simplified offset calculation for angle/percentage
                        let pos = idx as f32 / (items.len() - 1) as f32;
                        (color.resolve_to_absolute(&current_color), pos)
                    },
                    _ => continue,
                };
                
                d2d_stops.push(D2D1_GRADIENT_STOP {
                    position: offset,
                    color: color.as_srgb_color().to_d2d_color(),
                });
            }
            
            // Calculate center position
            let (center_x, center_y) = (
                position
                    .horizontal
                    .resolve(CSSPixelLength::new(rect.width() as f32))
                    .px() as f32,
                position
                    .vertical
                    .resolve(CSSPixelLength::new(rect.height() as f32))
                    .px() as f32,
            );
            
            // Create stops collection and radial gradient as fallback
            let stops_collection = rt.CreateGradientStopCollection(
                &d2d_stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                if repeating { D2D1_EXTEND_MODE_WRAP } else { D2D1_EXTEND_MODE_CLAMP },
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT
            ).unwrap();
            
            // Use radial gradient as an approximation
            let radius = rect.width().max(rect.height()) as f32;
            
            let brush = rt.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: D2D_POINT_2F { x: center_x, y: center_y },
                    gradientOriginOffset: D2D_POINT_2F { x: 0.0, y: 0.0 },
                    radiusX: radius,
                    radiusY: radius,
                },
                None,
                &stops_collection,
            ).unwrap();
            
            // Draw with the gradient
            if self.frame.has_border_radius() {
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.padding_box.width() as f32,
                        bottom: self.frame.padding_box.height() as f32,
                    },
                    radiusX: self.frame.border_top_left_radius_width as f32,
                    radiusY: self.frame.border_top_left_radius_height as f32,
                };
                rt.FillRoundedRectangle(&rounded_rect, &brush);
            } else {
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.padding_box.width() as f32,
                    bottom: self.frame.padding_box.height() as f32,
                };
                rt.FillRectangle(&rect, &brush);
            }
            
            // Note: A complete implementation would use a custom effect or shader to create a true conic gradient
        }
    }

    #[inline]
    fn resolve_color_stops<T>(
        item_resolver: impl Fn(CSSPixelLength, &T) -> Option<f32>
    ) -> (f32, f32) {
        // Helper for gradient calculations
        (0.0, 1.0)
    }

    #[inline]
    fn resolve_length_color_stops(repeating: bool) -> (f32, f32) {
        // Helper for gradient calculations
        (0.0, 1.0)
    }

    #[inline]
    fn resolve_angle_color_stops(repeating: bool) -> (f32, f32) {
        // Helper for gradient calculations
        (0.0, 1.0)
    }

    fn draw_outset_box_shadow(&self, rt: &mut ID2D1DeviceContext) {
        let box_shadow = &self.style.get_effects().box_shadow.0;
        let current_color = self.style.clone_color();

        // Check if there are any outset shadows
        let has_outset_shadow = box_shadow.iter().any(|s| !s.inset);
        
        // Apply clipping as in the Vello implementation
        self.with_maybe_clip(
            rt,
            || has_outset_shadow,
            |elem_cx, rt| {
                for shadow in box_shadow.iter().filter(|s| !s.inset) {
                    let shadow_color = shadow
                        .base
                        .color
                        .resolve_to_absolute(&current_color)
                        .as_srgb_color();
                    
                    // Skip transparent shadows
                    if shadow_color == Color::TRANSPARENT {
                        continue;
                    }
                    
                    unsafe {
                        // Create shadow brush
                        let shadow_brush = elem_cx.context.create_solid_color_brush(
                            rt,
                            shadow_color.to_d2d_color()
                        ).unwrap();
                        
                        // Calculate shadow offset and apply shadow transform
                        let offset_x = shadow.base.horizontal.px() as f32;
                        let offset_y = shadow.base.vertical.px() as f32;
                        
                        // Save the current transform
                        let mut original_transform: Matrix3x2 = Default::default();
                        rt.GetTransform(&mut original_transform);
                        
                        // Apply shadow offset to transform
                        let shadow_transform = Matrix3x2 {
                            M11: original_transform.M11,
                            M12: original_transform.M12,
                            M21: original_transform.M21,
                            M22: original_transform.M22,
                            M31: original_transform.M31 + offset_x,
                            M32: original_transform.M32 + offset_y,
                        };
                        rt.SetTransform(&shadow_transform);
                        
                        // Get blur radius (similar to Vello implementation)
                        let blur_radius = shadow.base.blur.px() as f32;
                        
                        // Draw shadow - if we have border radius, use rounded rectangle
                        if elem_cx.frame.has_border_radius() {
                            // Draw a rounded rectangle for the shadow
                            let rounded_rect = D2D1_ROUNDED_RECT {
                                rect: D2D_RECT_F {
                                    left: 0.0,
                                    top: 0.0,
                                    right: elem_cx.frame.border_box.width() as f32,
                                    bottom: elem_cx.frame.border_box.height() as f32,
                                },
                                radiusX: (elem_cx.frame.border_top_left_radius_width + blur_radius as f64) as f32,
                                radiusY: (elem_cx.frame.border_top_left_radius_height + blur_radius as f64) as f32,
                            };
                            
                            // In a full implementation, we would:
                            // 1. Create a bitmap render target
                            // 2. Draw the shape into it
                            // 3. Apply a gaussian blur effect with the blur radius
                            // 4. Draw the resulting bitmap
                            
                            // For this simplified implementation, just draw the rounded rect
                            rt.FillRoundedRectangle(&rounded_rect, &shadow_brush);
                        } else {
                            // Draw a simple rectangle for the shadow
                            let rect = D2D_RECT_F {
                                left: 0.0,
                                top: 0.0,
                                right: elem_cx.frame.border_box.width() as f32,
                                bottom: elem_cx.frame.border_box.height() as f32,
                            };
                            rt.FillRectangle(&rect, &shadow_brush);
                        }
                        
                        // Restore original transform
                        rt.SetTransform(&original_transform);
                    }
                }
            },
        );
    }

    fn draw_inset_box_shadow(&self, rt: &mut ID2D1DeviceContext) {
        let box_shadow = &self.style.get_effects().box_shadow.0;
        let current_color = self.style.clone_color();
        
        // Check if there are any inset shadows
        let has_inset_shadow = box_shadow.iter().any(|s| s.inset);
        
        if has_inset_shadow {
            CLIPS_WANTED.fetch_add(1, atomic::Ordering::SeqCst);
            let clips_available = CLIPS_USED.load(atomic::Ordering::SeqCst) <= CLIP_LIMIT;
            
            if clips_available {
                unsafe {
                    // Create a layer for clipping the inset shadows
                    let clip_rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.border_box.width() as f32,
                        bottom: self.frame.border_box.height() as f32,
                    };
                    
                    // Create a layer for the inset shadow
                    let layer = rt.CreateLayer(None).unwrap();
                    CLIPS_USED.fetch_add(1, atomic::Ordering::SeqCst);
                    let depth = CLIP_DEPTH.fetch_add(1, atomic::Ordering::SeqCst) + 1;
                    CLIP_DEPTH_USED.fetch_max(depth, atomic::Ordering::SeqCst);
                    
                    let params = D2D1_LAYER_PARAMETERS1 {
                        contentBounds: clip_rect,
                        geometricMask: std::mem::ManuallyDrop::new(None),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: Matrix3x2::default(),
                        opacity: 1.0,
                        opacityBrush: std::mem::ManuallyDrop::new(None),
                        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                    };
                    
                    rt.PushLayer(&params, &layer);
                }
            }
        }
        
        // Draw each inset shadow
        for shadow in box_shadow.iter().filter(|s| s.inset) {
            let shadow_color = shadow
                .base
                .color
                .resolve_to_absolute(&current_color)
                .as_srgb_color();
                
            // Skip transparent shadows
            if shadow_color == Color::TRANSPARENT {
                continue;
            }
            
            unsafe {
                // Create shadow brush
                let shadow_brush = self.context.create_solid_color_brush(
                    rt,
                    shadow_color.to_d2d_color()
                ).unwrap();
                
                // Apply shadow offset to transform
                let transform = Matrix3x2 {
                    M11: self.scale as f32,
                    M12: 0.0,
                    M21: 0.0,
                    M22: self.scale as f32,
                    M31: (self.pos.x * self.scale) as f32,
                    M32: (self.pos.y * self.scale + shadow.base.vertical.px() as f64 * self.scale) as f32,
                };
                
                rt.SetTransform(&transform);
                
                // Calculate average border radius (similar to the Vello version)
                let radius = (self.frame.border_top_left_radius_width +
                             self.frame.border_top_right_radius_width +
                             self.frame.border_bottom_left_radius_width +
                             self.frame.border_bottom_right_radius_width) / 4.0;
                
                // Draw shadow with a rounded rectangle
                let shadow_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.padding_box.width() as f32,
                        bottom: self.frame.padding_box.height() as f32,
                    },
                    radiusX: radius as f32,
                    radiusY: radius as f32,
                };
                
                // For a proper blur effect, we would need to:
                // 1. Create an off-screen bitmap
                // 2. Draw the shadow shape to it
                // 3. Apply a Gaussian blur effect based on shadow.base.blur
                // 4. Draw the blurred result
                
                // For this simplified version, just draw the rounded rectangle with the shadow color
                rt.FillRoundedRectangle(&shadow_rect, &shadow_brush);
                
                // Reset transform
                let base_transform = Matrix3x2 {
                    M11: self.scale as f32,
                    M12: 0.0,
                    M21: 0.0,
                    M22: self.scale as f32,
                    M31: (self.pos.x * self.scale) as f32,
                    M32: (self.pos.y * self.scale) as f32,
                };
                rt.SetTransform(&base_transform);
            }
        }
        
        // Pop layer if we pushed one
        if has_inset_shadow && CLIPS_USED.load(atomic::Ordering::SeqCst) <= CLIP_LIMIT {
            unsafe {
                rt.PopLayer();
                CLIP_DEPTH.fetch_sub(1, atomic::Ordering::SeqCst);
            }
        }
    }

    fn stroke_border(&self, rt: &mut ID2D1DeviceContext) {
        // Stroke all four borders
        self.stroke_border_edge(rt, Edge::Top);
        self.stroke_border_edge(rt, Edge::Right);
        self.stroke_border_edge(rt, Edge::Bottom);
        self.stroke_border_edge(rt, Edge::Left);
    }

    fn stroke_border_edge(&self, rt: &mut ID2D1DeviceContext, edge: Edge) {
        let style = &*self.style;
        let border = style.get_border();
        
        // Get the path used to draw the edge border
        // This uses the same approach as in the Vello renderer
        let path = self.frame.border(edge);
        
        // Get the current color context
        let current_color = style.clone_color();
        
        // Get the color for this specific edge
        let color = match edge {
            Edge::Top => border
                .border_top_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color(),
            Edge::Right => border
                .border_right_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color(),
            Edge::Bottom => border
                .border_bottom_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color(),
            Edge::Left => border
                .border_left_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color(),
        };
        
        // Skip if border is not visible or transparent
        if color == Color::TRANSPARENT {
            return;
        }
        
        // Check if we need to draw the border at all
        let width = match edge {
            Edge::Top => border.border_top_width,
            Edge::Right => border.border_right_width,
            Edge::Bottom => border.border_bottom_width,
            Edge::Left => border.border_left_width,
        };
        
        let style_type = match edge {
            Edge::Top => border.border_top_style,
            Edge::Right => border.border_right_style,
            Edge::Bottom => border.border_bottom_style,
            Edge::Left => border.border_left_style,
        };
        
        if width.0 <= 0 || style_type == style::values::computed::BorderStyle::None || style_type == style::values::computed::BorderStyle::Hidden {
            return;
        }
        
        unsafe {
            // Create brush for the border color
            let brush = self.context.create_solid_color_brush(rt, color.to_d2d_color()).unwrap();
            
            // Create path geometry from the vello path
            let factory: ID2D1Factory = rt.GetFactory().unwrap();
            let path_geometry = factory.CreatePathGeometry().unwrap();
            let sink = path_geometry.Open().unwrap();
            
            // Convert vello path to Direct2D path
            // This would normally iterate through the vello path segments
            // For simplicity, we'll create a rectangle border for each edge
            match edge {
                Edge::Top => {
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.border_box.width() as f32,
                        bottom: width.0 as f32,
                    };
                    rt.FillRectangle(&rect, &brush);
                },
                Edge::Right => {
                    let rect = D2D_RECT_F {
                        left: self.frame.border_box.width() as f32 - width.0 as f32,
                        top: 0.0,
                        right: self.frame.border_box.width() as f32,
                        bottom: self.frame.border_box.height() as f32,
                    };
                    rt.FillRectangle(&rect, &brush);
                },
                Edge::Bottom => {
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: self.frame.border_box.height() as f32 - width.0 as f32,
                        right: self.frame.border_box.width() as f32,
                        bottom: self.frame.border_box.height() as f32,
                    };
                    rt.FillRectangle(&rect, &brush);
                },
                Edge::Left => {
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: width.0 as f32,
                        bottom: self.frame.border_box.height() as f32,
                    };
                    rt.FillRectangle(&rect, &brush);
                },
            }
            
            // For more complex border styles like dashed, dotted, etc.
            // we would create a custom stroke style using factory.CreateStrokeStyle()
        }
    }

    fn stroke_outline(&self, rt: &mut ID2D1DeviceContext) {
        let outline = self.style.get_outline();
        let width = outline.outline_width;
        
        if width.0 <= 0 || matches!(outline.outline_style, style::values::computed::OutlineStyle::BorderStyle(style::values::computed::BorderStyle::None)) {
            return;
        }
        
        let color = outline.outline_color.resolve_to_absolute(&self.style.clone_color());
        
        unsafe {
            let brush = self.context.create_solid_color_brush(rt, color.to_d2d_color()).unwrap();
            
            // Draw outline rectangle
            let rect = D2D_RECT_F {
                left: -width.0 as f32,
                top: -width.0 as f32,
                right: self.frame.border_box.width() as f32 + width.0 as f32,
                bottom: self.frame.border_box.height() as f32 + width.0 as f32,
            };
            
            rt.DrawRectangle(&rect, &brush, width.0 as f32, None);
            
            // For dashed/dotted outlines, you'd need to create a custom stroke style
            // Similar to borders
        }
    }

    fn stroke_effects(&self, _rt: &mut ID2D1DeviceContext) {
        // This would handle opacity, filters, etc.
        // Direct2D implementation would depend on specific effects needed
    }

    fn draw_input(&self, rt: &mut ID2D1DeviceContext) {
        if self.node.local_name() == "input" {
            let Some(checked) = self.element.checkbox_input_checked() else {
                return;
            };
            let disabled = self.node.attr(local_name!("disabled")).is_some();
    
            // TODO this should be coming from css accent-color, but I couldn't find how to retrieve it
            let accent_color = if disabled {
                Color::from_rgba8(209, 209, 209, 255)
            } else {
                self.style.clone_color().as_srgb_color()
            };
    
            let scale = (self.frame.border_box.width()
                .min(self.frame.border_box.height())
                - 4.0)
                .max(0.0)
                / 16.0;
    
            let attr_type = self.node.attr(local_name!("type"));
    
            unsafe {
                // Create brushes for drawing
                let accent_brush = self.context.create_solid_color_brush(rt, accent_color.to_d2d_color()).unwrap();
                let white_brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(255, 255, 255, 255).to_d2d_color()).unwrap();
    
                if attr_type == Some("checkbox") {
                    // Create rounded rectangle for checkbox
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.border_box.width() as f32,
                        bottom: self.frame.border_box.height() as f32
                    };
                    
                    let rounded_rect = D2D1_ROUNDED_RECT {
                        rect,
                        radiusX: (scale * 2.0) as f32,
                        radiusY: (scale * 2.0) as f32
                    };
                    
                    if checked {
                        // Fill the checkbox with accent color
                        rt.FillRoundedRectangle(&rounded_rect, &accent_brush);
                        
                        // Create checkmark
                        let factory: ID2D1Factory = rt.GetFactory().unwrap();
                        let path_geometry = factory.CreatePathGeometry().unwrap();
                        let sink = path_geometry.Open().unwrap();
                        
                        // Create checkmark path (equivalent to BezPath in Vello)
                        sink.BeginFigure(
                            D2D_POINT_2F { 
                                x: (2.0 + 2.0) * scale as f32, 
                                y: (9.0 + 1.0) * scale as f32 
                            },
                            D2D1_FIGURE_BEGIN_HOLLOW
                        );
                        
                        sink.AddLine(D2D_POINT_2F { 
                            x: (6.0 + 2.0) * scale as f32, 
                            y: (13.0 + 1.0) * scale as f32 
                        });
                        
                        sink.AddLine(D2D_POINT_2F { 
                            x: (14.0 + 2.0) * scale as f32, 
                            y: (2.0 + 1.0) * scale as f32 
                        });
                        
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        sink.Close().unwrap();
                        
                        // Create stroke style with round caps/joins (similar to Vello's Stroke)
                        let stroke_props = D2D1_STROKE_STYLE_PROPERTIES {
                            startCap: D2D1_CAP_STYLE_ROUND,
                            endCap: D2D1_CAP_STYLE_ROUND,
                            dashCap: D2D1_CAP_STYLE_ROUND,
                            lineJoin: D2D1_LINE_JOIN_ROUND,
                            miterLimit: 10.0,
                            dashStyle: D2D1_DASH_STYLE_SOLID,
                            dashOffset: 0.0,
                        };
                        
                        let stroke_style = factory.CreateStrokeStyle(&stroke_props, None).unwrap();
                        
                        // Draw white checkmark
                        rt.DrawGeometry(
                            &path_geometry, 
                            &white_brush, 
                            (2.0 * scale) as f32, 
                            &stroke_style
                        );
                    } else {
                        // Fill with white and stroke with accent color
                        rt.FillRoundedRectangle(&rounded_rect, &white_brush);
                        rt.DrawRoundedRectangle(&rounded_rect, &accent_brush, 1.0, None);
                    }
                } else if attr_type == Some("radio") {
                    // Calculate center of the radio button
                    let center_x = self.frame.border_box.width() as f32 / 2.0;
                    let center_y = self.frame.border_box.height() as f32 / 2.0;
                    let center = D2D_POINT_2F { x: center_x, y: center_y };
                    
                    // Create ellipses for the radio button (equivalent to Circle in Vello)
                    let outer_ellipse = D2D1_ELLIPSE {
                        point: center,
                        radiusX: (8.0 * scale) as f32,
                        radiusY: (8.0 * scale) as f32
                    };
                    
                    let gap_ellipse = D2D1_ELLIPSE {
                        point: center,
                        radiusX: (6.0 * scale) as f32,
                        radiusY: (6.0 * scale) as f32
                    };
                    
                    let inner_ellipse = D2D1_ELLIPSE {
                        point: center,
                        radiusX: (4.0 * scale) as f32,
                        radiusY: (4.0 * scale) as f32
                    };
                    
                    if checked {
                        // Draw checked radio button with concentric circles
                        rt.FillEllipse(&outer_ellipse, &accent_brush);
                        rt.FillEllipse(&gap_ellipse, &white_brush);
                        rt.FillEllipse(&inner_ellipse, &accent_brush);
                    } else {
                        // Draw unchecked radio button
                        let gray_brush = self.context.create_solid_color_brush(
                            rt, 
                            Color::from_rgba8(128, 128, 128, 255).to_d2d_color()
                        ).unwrap();
                        
                        rt.FillEllipse(&outer_ellipse, &gray_brush);
                        rt.FillEllipse(&gap_ellipse, &white_brush);
                    }
                }
            }
        }
    }
}

impl<'a> std::ops::Deref for ElementCx<'a> {
    type Target = D2dSceneGenerator<'a>;
    fn deref(&self) -> &Self::Target {
        self.context
    }
}
