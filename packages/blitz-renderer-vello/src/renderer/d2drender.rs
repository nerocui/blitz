use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
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

use color::DynamicColor;
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

#[cfg(feature = "svg")]
use vello_svg::usvg;

const CLIP_LIMIT: usize = 1024;
static CLIPS_USED: AtomicUsize = AtomicUsize::new(0);
static CLIP_DEPTH: AtomicUsize = AtomicUsize::new(0);
static CLIP_DEPTH_USED: AtomicUsize = AtomicUsize::new(0);
static CLIPS_WANTED: AtomicUsize = AtomicUsize::new(0);

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
    fn node_position(&self, node: usize, location: Point2D<f64, UnknownUnit>) -> (Layout, Point2D<f64, UnknownUnit>) {
        let layout = self.layout(node);
        let pos = Point2D::new(
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
            let scale_matrix = D2D_MATRIX_3X2_F {
                m11: self.scale as f32,
                m12: 0.0,
                m21: 0.0,
                m22: self.scale as f32,
                dx: 0.0,
                dy: 0.0,
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
            unsafe {
                let brush = self.create_solid_color_brush(rt, color);
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
            rt.SetTransform(&D2D1_MATRIX_3X2_F {
                m11: 1.0,
                m12: 0.0,
                m21: 0.0,
                m22: 1.0,
                dx: 0.0,
                dy: 0.0,
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

            let fill_brush = self.create_solid_color_brush(rt, fill_color).unwrap();
            let padding_brush = self.create_solid_color_brush(rt, padding_color).unwrap();
            let border_brush = self.create_solid_color_brush(rt, border_color).unwrap();
            let margin_brush = self.create_solid_color_brush(rt, margin_color).unwrap();

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

    fn render_element(&self, rt: &mut ID2D1DeviceContext, node_id: usize, location: Point2D<f64, UnknownUnit>) {
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
        let scaled_pb = (padding + border).map(f64::from);
        let content_position = Point2D::new(
            box_position.x + scaled_pb.left,
            box_position.y + scaled_pb.top
        );
        let content_box_size = euclid::Size2D::new(
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
            let transform = D2D1_MATRIX_3X2_F {
                m11: self.scale as f32,
                m12: 0.0,
                m21: 0.0,
                m22: self.scale as f32,
                dx: (content_position.x * self.scale) as f32,
                dy: (content_position.y * self.scale) as f32,
            };
            rt.SetTransform(&transform);
        }

        // Set up clipping if needed
        let mut layer_params = None;
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
                    maskTransform: D2D1_MATRIX_3X2_F::default(),
                    opacity: 1.0,
                    opacityBrush: ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                };
                layer_params = Some(params);
                
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
            let transform = D2D1_MATRIX_3X2_F {
                m11: self.scale as f32,
                m12: 0.0,
                m21: 0.0,
                m22: self.scale as f32,
                dx: (content_position.x * self.scale) as f32,
                dy: (content_position.y * self.scale) as f32,
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
        
        // Pop layer if we pushed one
        if should_clip && clips_available {
            unsafe {
                rt.PopLayer();
            }
        }
        
        // Draw any child nodes
        if let NodeData::Element(ref data) = node.data {
            for &child_id in &data.paint_children {
                self.render_node(rt, child_id, box_position);
            }
        }
    }

    fn render_node(&self, rt: &mut ID2D1DeviceContext, node_id: usize, location: Point2D<f64, UnknownUnit>) {
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

    fn element_cx<'w>(&'w self, 
        node: &'w Node, 
        layout: Layout, 
        box_position: Point2D<f64, UnknownUnit>
    ) -> ElementCx<'w> {
        let element = match &node.data {
            NodeData::Element(element) => element,
            _ => panic!("Node is not an element")
        };
        
        // Extract other data from the node
        let text_input = match &node.data {
            NodeData::Element(el) => {
                match &el.data {
                    ElementNodeData::TextInput(data) => Some(data),
                    _ => None
                }
            },
            _ => None
        };
        
        let list_item = match &node.data {
            NodeData::Element(el) => {
                el.list_item_layout.as_ref()
            },
            _ => None
        };
        
        #[cfg(feature = "svg")]
        let svg = if node.local_name() == "svg" {
            node.svg.as_ref().map(|s| &**s)
        } else {
            None
        };
        
        // Create frame with border radii
        let frame = ElementFrame::new(&node, layout, box_position);
        
        ElementCx {
            context: self,
            frame,
            style: node.primary_styles().unwrap().clone(),
            pos: box_position,
            scale: self.scale,
            node,
            element,
            transform: Transform3D::identity(),
            #[cfg(feature = "svg")]
            svg,
            text_input,
            list_item,
            devtools: &self.devtools,
        }
    }
    
    // Helper function to create D2D solid color brush
    fn create_solid_color_brush(&self, rt: &ID2D1DeviceContext, color: Color) -> Result<ID2D1SolidColorBrush> {
        let color_f = D2D1_COLOR_F {
            r: color.r as f32 / 255.0,
            g: color.g as f32 / 255.0,
            b: color.b as f32 / 255.0,
            a: color.a as f32 / 255.0,
        };
        
        let properties = D2D1_BRUSH_PROPERTIES {
            opacity: 1.0,
            transform: D2D1_MATRIX_3X2_F::default(),
        };
        
        unsafe { rt.CreateSolidColorBrush(&color_f, Some(&properties)) }
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
    transform: Transform3D<f64, UnknownUnit, UnknownUnit>,
    #[cfg(feature = "svg")]
    svg: Option<&'a usvg::Tree>,
    text_input: Option<&'a TextInputData>,
    list_item: Option<&'a ListItemLayout>,
    devtools: &'a Devtools,
}

impl ElementCx<'_> {
    fn with_maybe_clip(&self, rt: &mut ID2D1DeviceContext, mut cb: impl FnMut(&ElementCx<'_>, &mut ID2D1DeviceContext)) {
        // Create a layer for clipping
        unsafe {
            // Create a layer for clipping if needed
            let current_clip_depth = CLIP_DEPTH.fetch_add(1, atomic::Ordering::SeqCst) + 1;
            CLIP_DEPTH_USED.store(current_clip_depth.max(CLIP_DEPTH_USED.load(atomic::Ordering::SeqCst)), atomic::Ordering::SeqCst);
            
            if current_clip_depth < CLIP_LIMIT {
                let clip_rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.width as f32,
                    bottom: self.frame.height as f32,
                };
                
                let layer = rt.CreateLayer(None).unwrap();
                let params = D2D1_LAYER_PARAMETERS {
                    contentBounds: clip_rect,
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: D2D1_MATRIX_3X2_F::default(),
                    opacity: 1.0,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                
                rt.PushLayer(&params, &layer);
                
                // Execute the callback
                cb(self, rt);
                
                // Pop the layer
                rt.PopLayer();
            } else {
                // Just execute the callback without clipping
                cb(self, rt);
            }
            
            CLIP_DEPTH.fetch_sub(1, atomic::Ordering::SeqCst);
        }
    }

    fn draw_inline_layout(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        // Find text nodes in this element
        if let NodeData::Element(element) = &self.node.data {
            // Only render text if we have inline layout information
            if let Some(inline_layout) = &element.inline_layout {
                unsafe {
                    // Create text format with DirectWrite (would require DirectWrite setup)
                    // For now, just draw rectangles representing text blocks
                    let text_brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
                    
                    // For each line in the inline layout
                    for line in inline_layout.lines.iter() {
                        for item in line.items.iter() {
                            if let PositionedLayoutItem::Text(text_item) = item {
                                // Create a rectangle for each text item
                                let text_rect = D2D_RECT_F {
                                    left: text_item.x as f32,
                                    top: text_item.y as f32,
                                    right: (text_item.x + text_item.width) as f32,
                                    bottom: (text_item.y + text_item.height) as f32,
                                };
                                
                                // Draw text (simplified to a filled rectangle for now)
                                rt.FillRectangle(&text_rect, &text_brush);
                                
                                // In a real implementation, we'd use DirectWrite for proper text rendering
                                // rt.DrawText(...) with a properly configured IDWriteTextFormat
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_text_input_text(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        if let Some(text_input) = self.text_input {
            unsafe {
                // Create a solid color brush for text
                let text_brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
                
                // Draw a rectangle representing the text input area
                let input_rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.width as f32,
                    bottom: self.frame.height as f32,
                };
                
                // In a real implementation, we'd use DirectWrite for text rendering
                // For now, just draw a placeholder rectangle
                rt.DrawRectangle(&input_rect, &text_brush, 1.0, None);
                
                // If there's placeholder text and no value, display it in gray
                if text_input.value.is_empty() && !text_input.placeholder.is_empty() {
                    // Draw placeholder (simplified)
                    let placeholder_brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(128, 128, 128, 255)).unwrap();
                    rt.DrawRectangle(&D2D_RECT_F { left: 5.0, top: 5.0, right: input_rect.right - 5.0, bottom: input_rect.bottom - 5.0 }, 
                                    &placeholder_brush, 1.0, None);
                }
            }
        }
    }

    fn draw_marker(&self, rt: &mut ID2D1DeviceContext, pos: Point2D<f64, f64>) {
        if let Some(list_item) = self.list_item {
            if let Some(marker) = &list_item.marker {
                match marker {
                    Marker::Bullet => {
                        // Draw a bullet marker
                        unsafe {
                            let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
                            
                            // Get marker position
                            let marker_pos = match &list_item.layout {
                                Some(layout) => match layout.position {
                                    ListItemLayoutPosition::Inside => Point2D::new(10.0, 10.0), // Simplified
                                    ListItemLayoutPosition::Outside => Point2D::new(0.0, 10.0), // Simplified
                                },
                                None => Point2D::new(0.0, 10.0) // Default position
                            };
                            
                            // Draw bullet (circle)
                            let ellipse = D2D1_ELLIPSE {
                                point: D2D_POINT_2F {
                                    x: marker_pos.x as f32,
                                    y: marker_pos.y as f32,
                                },
                                radiusX: 3.0,
                                radiusY: 3.0,
                            };
                            
                            rt.FillEllipse(&ellipse, &brush);
                        }
                    },
                    Marker::Decimal(n) => {
                        // Draw a decimal number marker (simplified)
                        unsafe {
                            let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
                            
                            // In a real implementation, we'd render text with DirectWrite
                            let rect = D2D_RECT_F {
                                left: 0.0,
                                top: 0.0,
                                right: 20.0,
                                bottom: 20.0,
                            };
                            
                            rt.DrawRectangle(&rect, &brush, 1.0, None);
                        }
                    }
                }
            }
        }
    }

    fn draw_children(&self, rt: &mut ID2D1DeviceContext) {
        // Iterate through child nodes and render them
        for &child_id in &self.element.children {
            self.context.render_node(rt, child_id, self.pos);
        }
    }

    fn stroke_text<'a>(
        pos: Point2D<f64, f64>,
        // Other parameters would be needed for actual implementation
    ) {
        // This would require DirectWrite setup
        // Would be implemented using IDWriteTextFormat and DrawText
    }

    #[cfg(feature = "svg")]
    fn draw_svg(&self, rt: &mut ID2D1DeviceContext) {
        // SVG rendering in Direct2D would require complex implementation
        // Basic approach would be to convert SVG paths to Direct2D geometries
        if let Some(svg) = self.svg {
            // This is a simplified placeholder
            unsafe {
                let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
                
                // Draw a rectangle as a placeholder for SVG
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.width as f32,
                    bottom: self.frame.height as f32,
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
        // Draw image if this node has one
        if let NodeData::Element(element) = &self.node.data {
            if let Some(image_data) = &element.data.as_any().downcast_ref::<ImageData>() {
                if let Some(data) = &image_data.data {
                    match data {
                        RasterImageData::Raster(data) => {
                            // Load image data into Direct2D bitmap
                            unsafe {
                                // This is simplified - in reality you'd need to create a bitmap from the image data
                                let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(200, 200, 200, 255)).unwrap();
                                
                                // Draw rectangle as placeholder for image
                                let rect = D2D_RECT_F {
                                    left: 0.0,
                                    top: 0.0,
                                    right: self.frame.width as f32,
                                    bottom: self.frame.height as f32,
                                };
                                
                                rt.FillRectangle(&rect, &brush);
                                
                                // In a real implementation:
                                // 1. Create a Direct2D bitmap from image data
                                // 2. Draw the bitmap using DrawBitmap
                            }
                        },
                        // Handle other image types
                        _ => {}
                    }
                }
            }
        }
    }

    fn draw_raster_bg_image(&self, rt: &mut ID2D1DeviceContext, idx: usize) {
        // Similar to draw_image, but for background images
        // Implementation would follow the same pattern
    }

    fn stroke_devtools(&self, rt: &mut ID2D1DeviceContext) {
        // Draw debugging overlays if devtools are enabled
        if self.devtools.show_layout_boxes {
            unsafe {
                let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(255, 0, 0, 128)).unwrap();
                
                let rect = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.frame.width as f32,
                    bottom: self.frame.height as f32,
                };
                
                rt.DrawRectangle(&rect, &brush, 1.0, None);
            }
        }
    }

    fn draw_background(&self, rt: &mut ID2D1DeviceContext) {
        let bg = self.style.get_background();
        
        // Handle solid background color
        let color = bg.background_color;
        if color != GenericColor::TRANSPARENT_BLACK {
            let resolved_color = color.resolve_to_absolute(&self.style.clone_color());
            unsafe {
                let brush = self.context.create_solid_color_brush(rt, resolved_color.to_color_color()).unwrap();
                
                // Create rounded rectangle if we have border radius
                if self.frame.has_border_radius() {
                    // Create rounded rectangle geometry
                    let factory: ID2D1Factory = rt.GetFactory().unwrap();
                    let rounded_rect = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: 0.0,
                            top: 0.0,
                            right: self.frame.width as f32,
                            bottom: self.frame.height as f32,
                        },
                        radiusX: self.frame.top_left_radius as f32,
                        radiusY: self.frame.top_left_radius as f32, // Using same radius for simplicity
                    };
                    
                    let geometry = unsafe { factory.CreateRoundedRectangleGeometry(&rounded_rect).unwrap() };
                    rt.FillGeometry(&geometry, &brush, None);
                } else {
                    // Simple rectangle for non-rounded corners
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.width as f32,
                        bottom: self.frame.height as f32,
                    };
                    
                    rt.FillRectangle(&rect, &brush);
                }
            }
        }
        
        // Handle background images and gradients
        for (idx, image) in bg.background_image.0.iter().enumerate() {
            match image {
                style::values::computed::image::Image::Gradient(gradient) => {
                    self.draw_gradient_frame(rt, gradient);
                },
                _ => {
                    // Handle other background image types
                    // This is more complex and would require specific Direct2D implementations
                }
            }
        }
    }

    fn draw_gradient_frame(&self, rt: &mut ID2D1DeviceContext, gradient: &StyloGradient) {
        match gradient {
            StyloGradient::Linear(linear) => {
                // Create a linear gradient brush
                unsafe {
                    // We need gradient stops
                    let mut stops = Vec::new();
                    
                    for (idx, stop) in linear.items.iter().enumerate() {
                        let color = stop.color.resolve_to_absolute(&self.style.clone_color());
                        let position = stop.position.map(|pos| pos as f32).unwrap_or(idx as f32 / (linear.items.len() - 1) as f32);
                        
                        let stop = D2D1_GRADIENT_STOP {
                            position,
                            color: D2D1_COLOR_F {
                                r: color.r as f32 / 255.0,
                                g: color.g as f32 / 255.0,
                                b: color.b as f32 / 255.0,
                                a: color.a as f32 / 255.0,
                            },
                        };
                        
                        stops.push(stop);
                    }
                    
                    // Create gradient stops collection
                    let factory: ID2D1Factory = rt.GetFactory().unwrap();
                    let stops_collection = factory.CreateGradientStopCollection(
                        &stops,
                        D2D1_GAMMA_2_2,
                        D2D1_EXTEND_MODE_CLAMP,
                    ).unwrap();
                    
                    // Create linear gradient brush
                    // This is simplified - we'd need to handle the gradient line direction properly
                    let start_point = D2D_POINT_2F { x: 0.0, y: 0.0 };
                    let end_point = D2D_POINT_2F { 
                        x: self.frame.width as f32, 
                        y: self.frame.height as f32 
                    };
                    
                    let brush = rt.CreateLinearGradientBrush(
                        &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: start_point,
                            endPoint: end_point,
                        },
                        None,
                        &stops_collection,
                    ).unwrap();
                    
                    // Draw the gradient
                    let rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: self.frame.width as f32,
                        bottom: self.frame.height as f32,
                    };
                    
                    rt.FillRectangle(&rect, &brush);
                }
            },
            StyloGradient::Radial(radial) => {
                // Similar implementation for radial gradients
                // Would use CreateRadialGradientBrush
            },
            _ => {
                // Handle other gradient types
            }
        }
    }

    fn draw_linear_gradient(flags: GradientFlags) {
        // Helper method for linear gradients
        // Implementation would depend on the specific gradient parameters
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
        let shadow = self.style.get_effects().box_shadow;
        
        for shadow_item in shadow.0.iter() {
            if shadow_item.inset {
                continue; // Skip inset shadows here
            }
            
            unsafe {
                // Create shadow effect
                let shadow_color = shadow_item.color.resolve_to_absolute(&self.style.clone_color());
                let color_brush = self.context.create_solid_color_brush(rt, shadow_color.to_color_color()).unwrap();
                
                // Shadow parameters
                let offset_x = shadow_item.offset_x.px() as f32;
                let offset_y = shadow_item.offset_y.px() as f32;
                let blur = shadow_item.blur.px() as f32;
                
                // Create shadow rectangle
                let shadow_rect = D2D_RECT_F {
                    left: offset_x - blur,
                    top: offset_y - blur,
                    right: self.frame.width as f32 + offset_x + blur,
                    bottom: self.frame.height as f32 + offset_y + blur,
                };
                
                // Draw shadow with alpha for blur effect
                // This is a simplified approximation - proper shadow would need more complex effects
                rt.FillRectangle(&shadow_rect, &color_brush);
                
                // In a more sophisticated implementation:
                // 1. Create a bitmap of the element
                // 2. Apply a gaussian blur effect
                // 3. Draw the blurred bitmap offset by the shadow parameters
            }
        }
    }

    fn draw_inset_box_shadow(&self, rt: &mut ID2D1DeviceContext) {
        let shadow = self.style.get_effects().box_shadow;
        
        for shadow_item in shadow.0.iter() {
            if !shadow_item.inset {
                continue; // Skip outset shadows here
            }
            
            // Inset shadows are more complex and would require a sophisticated implementation
            // using layer masks and effects
        }
    }

    fn draw_solid_frame(&self, rt: &mut ID2D1DeviceContext) {
        // Draw a solid frame for the element
        unsafe {
            let brush = self.context.create_solid_color_brush(rt, Color::from_rgba8(0, 0, 0, 255)).unwrap();
            
            let rect = D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: self.frame.width as f32,
                bottom: self.frame.height as f32,
            };
            
            rt.DrawRectangle(&rect, &brush, 1.0, None);
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
        let border = self.style.get_border();
        
        // Get border properties for this edge
        let (width, style, color) = match edge {
            Edge::Top => (
                border.border_top_width.px() as f32,
                border.border_top_style,
                border.border_top_color.resolve_to_absolute(&self.style.clone_color()),
            ),
            Edge::Right => (
                border.border_right_width.px() as f32,
                border.border_right_style,
                border.border_right_color.resolve_to_absolute(&self.style.clone_color()),
            ),
            Edge::Bottom => (
                border.border_bottom_width.px() as f32,
                border.border_bottom_style,
                border.border_bottom_color.resolve_to_absolute(&self.style.clone_color()),
            ),
            Edge::Left => (
                border.border_left_width.px() as f32,
                border.border_left_style,
                border.border_left_color.resolve_to_absolute(&self.style.clone_color()),
            ),
        };
        
        // Skip if border is not visible
        if width <= 0.0 || style == BorderStyle::None || style == BorderStyle::Hidden {
            return;
        }
        
        unsafe {
            let brush = self.context.create_solid_color_brush(rt, color.to_color_color()).unwrap();
            
            // Define the border line
            let (start, end) = match edge {
                Edge::Top => (
                    D2D_POINT_2F { x: 0.0, y: 0.0 },
                    D2D_POINT_2F { x: self.frame.width as f32, y: 0.0 },
                ),
                Edge::Right => (
                    D2D_POINT_2F { x: self.frame.width as f32, y: 0.0 },
                    D2D_POINT_2F { x: self.frame.width as f32, y: self.frame.height as f32 },
                ),
                Edge::Bottom => (
                    D2D_POINT_2F { x: 0.0, y: self.frame.height as f32 },
                    D2D_POINT_2F { x: self.frame.width as f32, y: self.frame.height as f32 },
                ),
                Edge::Left => (
                    D2D_POINT_2F { x: 0.0, y: 0.0 },
                    D2D_POINT_2F { x: 0.0, y: self.frame.height as f32 },
                ),
            };
            
            // Draw the border line
            rt.DrawLine(start, end, &brush, width, None);
            
            // For dashed/dotted borders, you'd need to create a custom stroke style
            // See D2D1CreateStrokeStyle for complex border styles
        }
    }

    fn stroke_outline(&self, rt: &mut ID2D1DeviceContext) {
        let outline = self.style.get_outline();
        let width = outline.outline_width.px() as f32;
        
        if width <= 0.0 || outline.outline_style == OutlineStyle::None {
            return;
        }
        
        let color = outline.outline_color.resolve_to_absolute(&self.style.clone_color());
        
        unsafe {
            let brush = self.context.create_solid_color_brush(rt, color.to_color_color()).unwrap();
            
            // Draw outline rectangle
            let rect = D2D_RECT_F {
                left: -width,
                top: -width,
                right: self.frame.width as f32 + width,
                bottom: self.frame.height as f32 + width,
            };
            
            rt.DrawRectangle(&rect, &brush, width, None);
            
            // For dashed/dotted outlines, you'd need to create a custom stroke style
            // Similar to borders
        }
    }

    fn stroke_effects(&self, _rt: &mut ID2D1DeviceContext) {
        // This would handle opacity, filters, etc.
        // Direct2D implementation would depend on specific effects needed
    }
}

impl<'a> std::ops::Deref for ElementCx<'a> {
    type Target = D2dSceneGenerator<'a>;
    fn deref(&self) -> &Self::Target {
        self.context
    }
}
