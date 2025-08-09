//! Direct2D backend for the `anyrender` abstraction.
//!
//! This backend implements the `WindowRenderer` trait and renders into an existing IDXGISwapChain1
//! (composition swapchain for WinUI SwapChainPanel) using Direct2D 1.1.
//!
//! The path deliberately avoids wgpu/Vello, matching the WinUI requirement that we already own the
//! swapchain and must not rely on an HWND or create a new one.
//!
//! High level pipeline per frame:
//! 1. Acquire backbuffer (DXGI) as DXGI surface / D3D11 texture
//! 2. Create (or reuse) a Direct2D Bitmap wrapping that surface
//! 3. BeginDraw -> replay scene commands -> EndDraw
//! 4. Present swapchain (done by host after render_once)
//!
//! We map the anyrender PaintScene commands onto Direct2D primitives.
//! (Initial version implements a subset: fill rects, strokes, images, text placeholder.)

use std::sync::Arc;

use anyrender::{PaintScene, WindowHandle, WindowRenderer, Paint, Glyph, NormalizedCoord};
use kurbo::{Affine, Rect, Shape, Stroke, PathEl};
use peniko::{BlendMode, BrushRef, Color, Fill, Font, StyleRef};
use peniko::color; // for color space conversions
use rustc_hash::FxHashMap;
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::{*};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::{IDXGISwapChain1, IDXGISurface, IDXGIDevice};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

// NOTE: Do not rely on HWND in WinUI shell path

/// Scene representation for D2D backend: we store a lightweight command list then play it back.
#[derive(Default)]
struct D2DScene { commands: Vec<Command>, }

enum Command {
    PushLayer { rect: Rect },
    PopLayer,
    FillRect { rect: Rect, brush: RecordedBrush },
    StrokeRect { rect: Rect, brush: RecordedBrush, width: f64 },
    FillPath { path: Vec<PathEl>, brush: RecordedBrush },
    StrokePath { path: Vec<PathEl>, brush: RecordedBrush, width: f64 },
    BoxShadow { rect: Rect, color: Color },
    GlyphRun { text: String, size: f32, color: Color },
}

#[derive(Clone)]
enum RecordedBrush {
    Solid(Color),
    Gradient(RecordedGradient),
    Image(RecordedImage),
}

#[derive(Clone)]
struct RecordedGradient {
    kind: peniko::GradientKind,
    stops: Vec<(f32, Color)>,
}

#[derive(Clone)]
struct RecordedImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
    format: peniko::ImageFormat,
    alpha: f32,
}

impl D2DScene {
    fn reset(&mut self) { self.commands.clear(); }
}

pub struct D2DScenePainter<'a> {
    scene: &'a mut D2DScene,
}

impl<'a> PaintScene for D2DScenePainter<'a> {
    fn reset(&mut self) { self.scene.reset(); }
    fn push_layer(&mut self, _blend: impl Into<BlendMode>, _alpha: f32, _transform: Affine, clip: &impl Shape) {
        // Simplify: only rectangular clips supported for now
        if let Some(rect) = shape_as_rect(clip) {
            self.scene.commands.push(Command::PushLayer { rect });
        }
    }
    fn pop_layer(&mut self) { self.scene.commands.push(Command::PopLayer); }
    fn stroke<'b>(&mut self, style: &Stroke, _transform: Affine, brush: impl Into<BrushRef<'b>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        // Prefer rect fast path; otherwise record path elements
        if let Some(rect) = shape_as_rect(shape) {
            let brush_rec = record_brush(brush.into());
            self.scene.commands.push(Command::StrokeRect { rect, brush: brush_rec, width: style.width });
        } else {
            let brush_rec = record_brush(brush.into());
            let mut v = Vec::new();
            shape_to_path_elements(shape, &mut v);
            self.scene.commands.push(Command::StrokePath { path: v, brush: brush_rec, width: style.width });
        }
    }
    fn fill<'b>(&mut self, _style: Fill, _transform: Affine, brush: impl Into<anyrender::Paint<'b>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        if let Some(rect) = shape_as_rect(shape) {
            let brush_rec = record_paint(brush.into());
            self.scene.commands.push(Command::FillRect { rect, brush: brush_rec });
        } else {
            let brush_rec = record_paint(brush.into());
            let mut v = Vec::new();
            shape_to_path_elements(shape, &mut v);
            self.scene.commands.push(Command::FillPath { path: v, brush: brush_rec });
        }
    }
    fn draw_glyphs<'b, 's: 'b>(&'s mut self, _font: &'b Font, font_size: f32, _hint: bool, _norm: &'b [NormalizedCoord], _style: impl Into<StyleRef<'b>>, brush: impl Into<BrushRef<'b>>, brush_alpha: f32, _transform: Affine, _glyph_transform: Option<Affine>, glyphs: impl Iterator<Item = Glyph>) {
        // For now, convert glyphs into a simple string by char code if id <= 0x10FFFF.
        // Proper glyph run shaping requires DirectWrite font face creation.
        let mut s = String::new();
        for g in glyphs { if let Some(ch) = char::from_u32(g.id) { s.push(ch); } }
        let color = match brush.into() { BrushRef::Solid(c) => c.with_alpha(c.components[3] * brush_alpha), _ => Color::BLACK }; // only solid
        self.scene.commands.push(Command::GlyphRun { text: s, size: font_size, color });
    }
    fn draw_box_shadow(&mut self, _transform: Affine, rect: Rect, brush: Color, _radius: f64, _std_dev: f64) {
        self.scene.commands.push(Command::BoxShadow { rect, color: brush });
    }
}

fn shape_as_rect(shape: &impl Shape) -> Option<Rect> { let b = shape.bounding_box(); Some(b) }

fn shape_to_path_elements(shape: &impl Shape, out: &mut Vec<PathEl>) {
    // Use kurbo provided iterator; tolerance chosen arbitrarily for curves
    for el in shape.path_elements(0.25) { out.push(el); }
}

fn record_brush(b: BrushRef<'_>) -> RecordedBrush {
    match b {
        BrushRef::Solid(c) => RecordedBrush::Solid(c),
        BrushRef::Gradient(g) => RecordedBrush::Gradient(RecordedGradient { kind: g.kind, stops: g.stops.iter().map(|s| (s.offset, s.color.to_alpha_color::<color::Srgb>())).collect() }),
        BrushRef::Image(img) => RecordedBrush::Image(RecordedImage { width: img.width, height: img.height, data: img.data.as_ref().to_vec(), format: img.format, alpha: img.alpha }),
    }
}
fn record_paint(p: Paint<'_>) -> RecordedBrush {
    match p {
        Paint::Solid(c) => RecordedBrush::Solid(c),
        Paint::Gradient(g) => RecordedBrush::Gradient(RecordedGradient { kind: g.kind, stops: g.stops.iter().map(|s| (s.offset, s.color.to_alpha_color::<color::Srgb>())).collect() }),
        Paint::Image(img) => RecordedBrush::Image(RecordedImage { width: img.width, height: img.height, data: img.data.as_ref().to_vec(), format: img.format, alpha: img.alpha }),
        Paint::Custom(_) => RecordedBrush::Solid(Color::BLACK),
    }
}

/// Direct2D renderer bound to an existing DXGI swapchain (composition target).
pub struct D2DWindowRenderer {
    swapchain: Option<IDXGISwapChain1>,
    d3d_device: Option<ID3D11Device>,
    d2d_factory: Option<ID2D1Factory1>,
    d2d_device: Option<ID2D1Device>,
    d2d_ctx: Option<ID2D1DeviceContext>,
    dwrite_factory: Option<IDWriteFactory>,
    // caches
    gradient_cache: FxHashMap<u64, ID2D1Brush>,
    image_cache: FxHashMap<u64, ID2D1Bitmap>,
    text_format_cache: FxHashMap<u32, IDWriteTextFormat>,
    scene: D2DScene,
    width: u32,
    height: u32,
    active: bool,
}

impl D2DWindowRenderer {
    pub fn new() -> Self { Self { swapchain: None, d3d_device: None, d2d_factory: None, d2d_device: None, d2d_ctx: None, dwrite_factory: None, gradient_cache: FxHashMap::default(), image_cache: FxHashMap::default(), text_format_cache: FxHashMap::default(), scene: D2DScene::default(), width: 1, height: 1, active: false } }

    pub fn set_swapchain(&mut self, sc: IDXGISwapChain1, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.swapchain = Some(sc.clone());
        if self.d3d_device.is_none() { self.init_devices_from_swapchain(); }
        self.active = true;
    }

    fn init_devices_from_swapchain(&mut self) {
        if let Some(sc) = &self.swapchain {
            unsafe {
                // Get D3D11 device from swapchain
                if let Ok(tex) = sc.GetBuffer::<ID3D11Texture2D>(0) {
                    let res: &ID3D11Resource = (&tex).into();
                    if let Ok(dev) = res.GetDevice() {
                        self.d3d_device = Some(dev.clone());
                        // Create D2D device via DXGI device
                        if let Ok(dxgi_dev) = dev.cast::<IDXGIDevice>() {
                            // Create D2D factory
                            if let Ok(factory) = D2D1CreateFactory::<ID2D1Factory1>(D2D1_FACTORY_TYPE_MULTI_THREADED, None) {
                                self.d2d_factory = Some(factory.clone());
                                if let Ok(d2d_dev) = factory.CreateDevice(&dxgi_dev) {
                                    if let Ok(ctx) = d2d_dev.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) {
                                        self.d2d_device = Some(d2d_dev);
                                        self.d2d_ctx = Some(ctx);
                                        // DirectWrite factory
                                        if let Ok(dwf) = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) {
                                            self.dwrite_factory = Some(dwf);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn playback(&mut self, target: &ID2D1Bitmap1) {
        let ctx = match &self.d2d_ctx {
            Some(ctx) => ctx.clone(),
            None => return,
        };
        
        unsafe {
            ctx.BeginDraw();
            // SetTarget exists on ID2D1DeviceContext
            let _ = ctx.SetTarget(target);
            // Clear not on ID2D1DeviceContext in windows 0.58 feature set; emulate by filling full rect
            let size = target.GetSize();
            let full = D2D_RECT_F { left: 0.0, top: 0.0, right: size.width, bottom: size.height };
            let clear_brush = self.create_solid_brush(Color::TRANSPARENT);
            let _ = ctx.FillRectangle(&full, &clear_brush);
            
            // Collect commands to avoid borrow checker issues
            let commands = std::mem::take(&mut self.scene.commands);
            
            for cmd in commands {
                match cmd {
                    Command::FillRect { rect, brush } => {
                        let brush = self.get_or_create_brush(&brush);
                        let r = D2D_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                        let _ = ctx.FillRectangle(&r, &brush);
                    }
                    Command::StrokeRect { rect, brush, width } => {
                        let brush = self.get_or_create_brush(&brush);
                        let r = D2D_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                        let _ = ctx.DrawRectangle(&r, &brush, width as f32, None);
                    }
                    Command::FillPath { path, brush } => {
                        if let Some(geom) = self.build_path_geometry(&path) {
                            let brush = self.get_or_create_brush(&brush);
                            let _ = ctx.FillGeometry(&geom, &brush, None);
                        }
                    }
                    Command::StrokePath { path, brush, width } => {
                        if let Some(geom) = self.build_path_geometry(&path) {
                            let brush = self.get_or_create_brush(&brush);
                            let _ = ctx.DrawGeometry(&geom, &brush, width as f32, None);
                        }
                    }
                    Command::PushLayer { rect } => {
                        let r = D2D_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                        let _ = ctx.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    }
                    Command::PopLayer => { ctx.PopAxisAlignedClip(); }
                    Command::BoxShadow { rect, color } => {
                        // Approximate box shadow via drawing a filled rect with opacity and a blur effect applied to whole scene would be expensive.
                        // Simplified: fill pre-inflated rect with alpha half (placeholder). Real implementation should use a gaussian blur effect.
                        let brush = self.create_solid_brush(color.with_alpha(color.components[3] * 0.5));
                        let r = D2D_RECT_F { left: (rect.x0 - 4.0) as f32, top: (rect.y0 - 4.0) as f32, right: (rect.x1 + 4.0) as f32, bottom: (rect.y1 + 4.0) as f32 };
                        let _ = ctx.FillRectangle(&r, &brush);
                    }
                    Command::GlyphRun { text, size, color } => {
                        if let Some(dwf) = self.dwrite_factory.clone() {
                            let wide: Vec<u16> = text.encode_utf16().collect();
                            let text_format = self.get_or_create_text_format(size);
                            if let Ok(layout) = dwf.CreateTextLayout(&wide, text_format, f32::MAX, f32::MAX) {
                                let brush = self.create_solid_brush(color);
                                let _ = ctx.DrawTextLayout(D2D_POINT_2F { x: 0.0, y: 0.0 }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                            }
                        }
                    }
                }
            }
            // Note: SetTransform removed - not available in this windows-rs version
            let _ = ctx.EndDraw(None, None);
        }
    }

    fn create_solid_brush(&self, color: Color) -> ID2D1SolidColorBrush {
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            let col = D2D1_COLOR_F { r: color.components[0] as f32, g: color.components[1] as f32, b: color.components[2] as f32, a: color.components[3] as f32 };
            ctx.CreateSolidColorBrush(&col, None).unwrap()
        }
    }

    fn get_or_create_brush(&mut self, recorded: &RecordedBrush) -> ID2D1Brush {
        match recorded {
            RecordedBrush::Solid(c) => self.create_solid_brush(*c).cast().unwrap(),
            RecordedBrush::Gradient(g) => self.get_or_create_gradient_brush(g),
            RecordedBrush::Image(img) => self.get_or_create_image_brush(img),
        }
    }

    fn get_or_create_gradient_brush(&mut self, g: &RecordedGradient) -> ID2D1Brush {
    use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        // hash kind & stops
        (match &g.kind { peniko::GradientKind::Linear { .. } => 1u8, peniko::GradientKind::Radial { .. } => 2u8, peniko::GradientKind::Sweep { .. } => 3u8 }).hash(&mut hasher);
        for (o,c) in &g.stops { let comps = c.components; ((o.to_bits(), (comps[0].to_bits(), comps[1].to_bits(), comps[2].to_bits(), comps[3].to_bits()))).hash(&mut hasher); }
        let key = hasher.finish();
        if let Some(b) = self.gradient_cache.get(&key) { return b.clone(); }
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            // Build gradient stops
            let stops: Vec<D2D1_GRADIENT_STOP> = g.stops.iter().map(|(o,c)| {
                let comps = c.components;
                D2D1_GRADIENT_STOP { position: *o, color: D2D1_COLOR_F { r: comps[0], g: comps[1], b: comps[2], a: comps[3] } }
            }).collect();
            let stop_collection = ctx.CreateGradientStopCollection(&stops, D2D1_COLOR_SPACE_SRGB, D2D1_COLOR_SPACE_SRGB, D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_EXTEND_MODE_CLAMP, D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT).unwrap();
            let brush: ID2D1Brush = match g.kind {
                peniko::GradientKind::Linear { start, end } => {
                    let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: D2D_POINT_2F { x: start.x as f32, y: start.y as f32 },
                        endPoint: D2D_POINT_2F { x: end.x as f32, y: end.y as f32 },
                    };
                    ctx.CreateLinearGradientBrush(&props, None, &stop_collection).unwrap().cast().unwrap()
                }
                peniko::GradientKind::Radial { start_center, start_radius: _, end_center, end_radius } => {
                    let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                        center: D2D_POINT_2F { x: end_center.x as f32, y: end_center.y as f32 },
                        gradientOriginOffset: D2D_POINT_2F { x: (start_center.x - end_center.x) as f32, y: (start_center.y - end_center.y) as f32 },
                        radiusX: end_radius.max(0.1) as f32,
                        radiusY: end_radius.max(0.1) as f32,
                    };
                    ctx.CreateRadialGradientBrush(&props, None, &stop_collection).unwrap().cast().unwrap()
                }
                peniko::GradientKind::Sweep { .. } => {
                    // No native sweep; approximate by linear
                    let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: D2D_POINT_2F { x: 0.0, y: 0.0 }, endPoint: D2D_POINT_2F { x: 100.0, y: 0.0 } };
                    ctx.CreateLinearGradientBrush(&props, None, &stop_collection).unwrap().cast().unwrap()
                }
            };
            self.gradient_cache.insert(key, brush.clone());
            brush
        }
    }

    fn get_or_create_image_brush(&mut self, img: &RecordedImage) -> ID2D1Brush {
    use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        (img.width, img.height, img.alpha.to_bits()).hash(&mut hasher);
        // hash first 16 bytes for key (cheap)
        for b in img.data.iter().take(16) { b.hash(&mut hasher); }
        let key = hasher.finish();
        if let Some(existing) = self.image_cache.get(&key) { return existing.clone().cast().unwrap(); }
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            let format = match img.format { peniko::ImageFormat::Rgba8 => DXGI_FORMAT_R8G8B8A8_UNORM, _ => DXGI_FORMAT_R8G8B8A8_UNORM };
            let pf = D2D1_PIXEL_FORMAT { format, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
            let bp = D2D1_BITMAP_PROPERTIES1 { pixelFormat: pf, dpiX: 96.0, dpiY: 96.0, bitmapOptions: D2D1_BITMAP_OPTIONS_NONE, colorContext: std::mem::ManuallyDrop::new(None) };
            let pitch = (img.width * 4) as u32;
            let bitmap = ctx.CreateBitmap(D2D_SIZE_U { width: img.width, height: img.height }, Some(img.data.as_ptr() as *const _), pitch, &bp).unwrap();
            self.image_cache.insert(key, bitmap.clone().cast().unwrap());
            bitmap.cast().unwrap()
        }
    }

    fn get_or_create_text_format(&mut self, size: f32) -> &IDWriteTextFormat {
        let key = size.to_bits();
        if !self.text_format_cache.contains_key(&key) {
            if let Some(factory) = &self.dwrite_factory { unsafe {
                if let Ok(f) = factory.CreateTextFormat(windows::core::w!("Arial"), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, windows::core::w!("en-US")) {
                    self.text_format_cache.insert(key, f);
                }
            }}
        }
        // unwrap safe: either inserted or already present
        self.text_format_cache.get(&key).unwrap()
    }

    fn build_path_geometry(&self, path: &[PathEl]) -> Option<ID2D1PathGeometry> {
        let factory = self.d2d_factory.as_ref()?;
        unsafe {
            let geom = factory.CreatePathGeometry().ok()?;
            let sink = geom.Open().ok()?;
            {
                for el in path {
                    match el {
                        PathEl::MoveTo(p) => { sink.BeginFigure(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }, D2D1_FIGURE_BEGIN_FILLED); },
                        PathEl::LineTo(p) => { sink.AddLine(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }); },
                        PathEl::QuadTo(p1, p2) => { 
                            let bezier = D2D1_QUADRATIC_BEZIER_SEGMENT { 
                                point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 }, 
                                point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 } 
                            };
                            sink.AddQuadraticBezier(&bezier); 
                        },
                        PathEl::CurveTo(p1, p2, p3) => { sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 }, point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 }, point3: D2D_POINT_2F { x: p3.x as f32, y: p3.y as f32 } }); },
                        PathEl::ClosePath => { sink.EndFigure(D2D1_FIGURE_END_CLOSED); },
                    }
                }
            }
            sink.Close().ok();
            Some(geom.into())
        }
    }
}

impl WindowRenderer for D2DWindowRenderer {
    type ScenePainter<'a> = D2DScenePainter<'a> where Self: 'a;
    fn resume(&mut self, _window: Arc<dyn WindowHandle>, _width: u32, _height: u32) { /* unused: swapchain provided directly */ }
    fn suspend(&mut self) { self.active = false; }
    fn is_active(&self) -> bool { self.active }
    fn set_size(&mut self, width: u32, height: u32) { self.width = width; self.height = height; }
    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        if !self.active { return; }
        // Build scene
        {
            let mut painter = D2DScenePainter { scene: &mut self.scene };
            draw_fn(&mut painter);
        }
        // Acquire backbuffer and wrap in D2D bitmap
        if let Some(sc) = &self.swapchain { unsafe {
            if let Ok(surface) = sc.GetBuffer::<IDXGISurface>(0) {
                if let Some(ctx) = &self.d2d_ctx {
                    let bp = D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                        dpiX: 96.0, dpiY: 96.0,
                        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                        colorContext: std::mem::ManuallyDrop::new(None),
                    };
                    if let Ok(b) = ctx.CreateBitmapFromDxgiSurface(&surface, Some(&bp)) { self.playback(&b); }
                }
            }
        }}
    }
}
