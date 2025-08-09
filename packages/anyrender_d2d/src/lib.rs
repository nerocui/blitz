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

use anyrender::{PaintScene, WindowHandle, WindowRenderer, Paint};
use kurbo::{Affine, Rect, Shape, Stroke, PathEl};
use peniko::{BlendMode, BrushRef, Color, Fill, Font, Image, StyleRef};
use rustc_hash::FxHashMap;
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::{Common::D2D1_COLOR_F, *};
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::{IDXGISwapChain1, IDXGISurface};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Foundation::HWND;

/// Scene representation for D2D backend: we store a lightweight command list then play it back.
#[derive(Default)]
struct D2DScene { commands: Vec<Command>, }

enum Command {
    PushLayer { blend: BlendMode, alpha: f32, transform: Affine, rect: Rect },
    PopLayer,
    FillRect { rect: Rect, brush: RecordedBrush, transform: Affine },
    StrokeRect { rect: Rect, brush: RecordedBrush, width: f64, transform: Affine },
    FillPath { path: Vec<PathEl>, brush: RecordedBrush, transform: Affine },
    StrokePath { path: Vec<PathEl>, brush: RecordedBrush, width: f64, transform: Affine },
    BoxShadow { rect: Rect, color: Color, radius: f64, std_dev: f64, transform: Affine },
    GlyphRun { text: String, font_key: u64, size: f32, color: Color, transform: Affine },
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
    extend: peniko::Extend,
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

struct D2DScenePainter<'a> {
    scene: &'a mut D2DScene,
}

impl<'a> PaintScene for D2DScenePainter<'a> {
    fn reset(&mut self) { self.scene.reset(); }
    fn push_layer(&mut self, blend: impl Into<BlendMode>, alpha: f32, transform: Affine, clip: &impl Shape) {
        // Simplify: only rectangular clips supported for now
        if let Some(rect) = clip_as_rect(clip) {
            self.scene.commands.push(Command::PushLayer { blend: blend.into(), alpha, transform, rect });
        }
    }
    fn pop_layer(&mut self) { self.scene.commands.push(Command::PopLayer); }
    fn stroke(&mut self, style: &Stroke, transform: Affine, brush: impl Into<BrushRef<'_>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        // Prefer rect fast path; otherwise record path elements
        if let Some(rect) = shape_as_rect(shape) {
            let brush_rec = record_brush(brush.into());
            self.scene.commands.push(Command::StrokeRect { rect, brush: brush_rec, width: style.width, transform });
        } else {
            let brush_rec = record_brush(brush.into());
            let mut v = Vec::new();
            shape_to_path_elements(shape, &mut v);
            self.scene.commands.push(Command::StrokePath { path: v, brush: brush_rec, width: style.width, transform });
        }
    }
    fn fill(&mut self, _style: Fill, transform: Affine, brush: impl Into<anyrender::Paint<'_>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        if let Some(rect) = shape_as_rect(shape) {
            let brush_rec = record_paint(brush.into());
            self.scene.commands.push(Command::FillRect { rect, brush: brush_rec, transform });
        } else {
            let brush_rec = record_paint(brush.into());
            let mut v = Vec::new();
            shape_to_path_elements(shape, &mut v);
            self.scene.commands.push(Command::FillPath { path: v, brush: brush_rec, transform });
        }
    }
    fn draw_glyphs<'b, 's: 'b>(&'s mut self, _font: &'b Font, _font_size: f32, _hint: bool, _norm: &'b [peniko::NormalizedCoord], _style: impl Into<StyleRef<'b>>, _brush: impl Into<BrushRef<'b>>, _brush_alpha: f32, _transform: Affine, _glyph_transform: Option<Affine>, _glyphs: impl Iterator<Item = peniko::Glyph>) {}
    fn draw_glyphs<'b, 's: 'b>(&'s mut self, font: &'b Font, font_size: f32, _hint: bool, _norm: &'b [peniko::NormalizedCoord], _style: impl Into<StyleRef<'b>>, brush: impl Into<BrushRef<'b>>, brush_alpha: f32, transform: Affine, _glyph_transform: Option<Affine>, glyphs: impl Iterator<Item = peniko::Glyph>) {
        // For now, convert glyphs into a simple string by char code if id <= 0x10FFFF.
        // Proper glyph run shaping requires DirectWrite font face creation.
        let mut s = String::new();
        for g in glyphs { if let Some(ch) = char::from_u32(g.id) { s.push(ch); } }
        let color = match brush.into() { BrushRef::Solid(c) => c.with_alpha(c.a * brush_alpha), _ => Color::BLACK }; // only solid
        // Hash font data pointer + index
        let font_key = font.data.as_ptr() as u64 ^ ((font.index as u64) << 32);
        self.scene.commands.push(Command::GlyphRun { text: s, font_key, size: font_size, color, transform });
    }
    fn draw_box_shadow(&mut self, _transform: Affine, _rect: Rect, _brush: Color, _radius: f64, _std_dev: f64) {}
    fn draw_box_shadow(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) {
        self.scene.commands.push(Command::BoxShadow { rect, color: brush, radius, std_dev, transform });
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
        BrushRef::Gradient(g) => RecordedBrush::Gradient(RecordedGradient { kind: g.kind, extend: g.extend, stops: g.stops.iter().map(|s| (s.offset, s.color.to_alpha_color::<color::Srgb>())).collect() }),
        BrushRef::Image(img) => RecordedBrush::Image(RecordedImage { width: img.width, height: img.height, data: img.data.as_ref().to_vec(), format: img.format, alpha: img.alpha }),
    }
}
fn record_paint(p: Paint<'_>) -> RecordedBrush {
    match p {
        Paint::Solid(c) => RecordedBrush::Solid(c),
        Paint::Gradient(g) => RecordedBrush::Gradient(RecordedGradient { kind: g.kind, extend: g.extend, stops: g.stops.iter().map(|s| (s.offset, s.color.to_alpha_color::<color::Srgb>())).collect() }),
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
                            let mut d2d_factory: Option<ID2D1Factory1> = None;
                            if D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, &ID2D1Factory1::IID, std::ptr::null(), std::mem::transmute(&mut d2d_factory as *mut _ as *mut _)).is_ok() {
                                if let Some(factory) = d2d_factory { self.d2d_factory = Some(factory.clone());
                                    if let Ok(d2d_dev) = factory.CreateDevice(&dxgi_dev) {
                                        if let Ok(ctx) = d2d_dev.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) {
                                            self.d2d_device = Some(d2d_dev);
                                            self.d2d_ctx = Some(ctx);
                                            // DirectWrite factory
                                            let mut dwf: Option<IDWriteFactory> = None;
                                            if DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, &IDWriteFactory::IID, std::mem::transmute(&mut dwf as *mut _ as *mut _)).is_ok() {
                                                self.dwrite_factory = dwf;
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
    }

    fn playback(&mut self, target: &ID2D1Bitmap1) {
        if let Some(ctx) = &self.d2d_ctx {
            unsafe {
                ctx.BeginDraw();
                ctx.SetTarget(target);
                ctx.Clear(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.0 });
                for cmd in self.scene.commands.drain(..) {
                    match cmd {
                        Command::FillRect { rect, brush, transform } => {
                            let brush = self.get_or_create_brush(&brush);
                            let r = D2D1_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                            ctx.SetTransform(&affine_to_matrix(transform));
                            ctx.FillRectangle(&r, &brush);
                        }
                        Command::StrokeRect { rect, brush, width, transform } => {
                            let brush = self.get_or_create_brush(&brush);
                            let r = D2D1_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                            ctx.SetTransform(&affine_to_matrix(transform));
                            ctx.DrawRectangle(&r, &brush, width as f32, None);
                        }
                        Command::FillPath { path, brush, transform } => {
                            if let Some(geom) = self.build_path_geometry(&path) {
                                let brush = self.get_or_create_brush(&brush);
                                ctx.SetTransform(&affine_to_matrix(transform));
                                ctx.FillGeometry(&geom, &brush, None);
                            }
                        }
                        Command::StrokePath { path, brush, width, transform } => {
                            if let Some(geom) = self.build_path_geometry(&path) {
                                let brush = self.get_or_create_brush(&brush);
                                ctx.SetTransform(&affine_to_matrix(transform));
                                ctx.DrawGeometry(&geom, &brush, width as f32, None);
                            }
                        }
                        Command::PushLayer { rect, .. } => {
                            let r = D2D1_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                            ctx.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        }
                        Command::PopLayer => { ctx.PopAxisAlignedClip(); }
                        Command::BoxShadow { rect, color, radius: _, std_dev: _, transform } => {
                            // Approximate box shadow via drawing a filled rect with opacity and a blur effect applied to whole scene would be expensive.
                            // Simplified: fill pre-inflated rect with alpha half (placeholder). Real implementation should use a gaussian blur effect.
                            let brush = self.create_solid_brush(color.with_alpha(color.a * 0.5));
                            let r = D2D1_RECT_F { left: (rect.x0 - 4.0) as f32, top: (rect.y0 - 4.0) as f32, right: (rect.x1 + 4.0) as f32, bottom: (rect.y1 + 4.0) as f32 };
                            ctx.SetTransform(&affine_to_matrix(transform));
                            ctx.FillRectangle(&r, &brush);
                        }
                        Command::GlyphRun { text, font_key: _, size, color, transform } => {
                            if let Some(dwf) = &self.dwrite_factory {
                                unsafe {
                                    let mut text_layout: Option<IDWriteTextLayout> = None;
                                    let wide: Vec<u16> = text.encode_utf16().collect();
                                    if dwf.CreateTextLayout(wide.as_ptr(), wide.len() as u32, self.get_or_create_text_format(size), f32::MAX, f32::MAX, &mut text_layout).is_ok() {
                                        if let Some(layout) = text_layout {
                                            let brush = self.create_solid_brush(color);
                                            ctx.SetTransform(&affine_to_matrix(transform));
                                            ctx.DrawTextLayout(D2D_POINT_2F { x: 0.0, y: 0.0 }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Reset transform
                ctx.SetTransform(&identity_matrix());
                let _ = ctx.EndDraw(std::ptr::null_mut(), std::ptr::null_mut());
            }
        }
    }

    fn create_solid_brush(&self, color: Color) -> ID2D1SolidColorBrush {
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            let props = D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: D2D_MATRIX_3X2_F { matrix: [[1.0,0.0],[0.0,1.0],[0.0,0.0]] } };
            let col = D2D1_COLOR_F { r: color.r as f32, g: color.g as f32, b: color.b as f32, a: color.a as f32 };
            let mut brush: Option<ID2D1SolidColorBrush> = None;
            ctx.CreateSolidColorBrush(&col, Some(&props), Some(&mut brush)).ok();
            brush.unwrap()
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
        let mut hasher = fxhash::FxHasher::default();
        // hash kind & stops
        (match g.kind { peniko::GradientKind::Linear(_) => 1u8, peniko::GradientKind::Radial(_) => 2u8, peniko::GradientKind::Sweep(_) => 3u8 }).hash(&mut hasher);
        for (o,c) in &g.stops { ((o.to_bits(), (c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()))).hash(&mut hasher); }
        let key = hasher.finish();
        if let Some(b) = self.gradient_cache.get(&key) { return b.clone(); }
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            // Build gradient stops
            let stops: Vec<D2D1_GRADIENT_STOP> = g.stops.iter().map(|(o,c)| D2D1_GRADIENT_STOP { position: *o, color: D2D1_COLOR_F { r: c.r as f32, g: c.g as f32, b: c.b as f32, a: c.a as f32 } }).collect();
            let mut stop_collection: Option<ID2D1GradientStopCollection> = None;
            ctx.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP, &mut stop_collection).ok();
            let stop_collection = stop_collection.unwrap();
            let brush: ID2D1Brush = match g.kind {
                peniko::GradientKind::Linear(lin) => {
                    let mut linear: Option<ID2D1LinearGradientBrush> = None;
                    let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: D2D_POINT_2F { x: lin.start.x as f32, y: lin.start.y as f32 },
                        endPoint: D2D_POINT_2F { x: lin.end.x as f32, y: lin.end.y as f32 },
                    };
                    ctx.CreateLinearGradientBrush(&props, None, &stop_collection, &mut linear).ok();
                    linear.unwrap().cast().unwrap()
                }
                peniko::GradientKind::Radial(rad) => {
                    let mut radial: Option<ID2D1RadialGradientBrush> = None;
                    let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                        center: D2D_POINT_2F { x: rad.end_center.x as f32, y: rad.end_center.y as f32 },
                        gradientOriginOffset: D2D_POINT_2F { x: (rad.start_center.x - rad.end_center.x) as f32, y: (rad.start_center.y - rad.end_center.y) as f32 },
                        radiusX: rad.end_radius.max(0.1),
                        radiusY: rad.end_radius.max(0.1),
                    };
                    ctx.CreateRadialGradientBrush(&props, None, &stop_collection, &mut radial).ok();
                    radial.unwrap().cast().unwrap()
                }
                peniko::GradientKind::Sweep(_) => {
                    // No native sweep; approximate by linear
                    let mut linear: Option<ID2D1LinearGradientBrush> = None;
                    let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: D2D_POINT_2F { x: 0.0, y: 0.0 }, endPoint: D2D_POINT_2F { x: 100.0, y: 0.0 } };
                    ctx.CreateLinearGradientBrush(&props, None, &stop_collection, &mut linear).ok();
                    linear.unwrap().cast().unwrap()
                }
            };
            self.gradient_cache.insert(key, brush.clone());
            brush
        }
    }

    fn get_or_create_image_brush(&mut self, img: &RecordedImage) -> ID2D1Brush {
        use std::hash::{Hash, Hasher};
        let mut hasher = fxhash::FxHasher::default();
        (img.width, img.height, img.alpha.to_bits()).hash(&mut hasher);
        // hash first 16 bytes for key (cheap)
        for b in img.data.iter().take(16) { b.hash(&mut hasher); }
        let key = hasher.finish();
        if let Some(existing) = self.image_cache.get(&key) { return existing.clone().cast().unwrap(); }
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            let format = match img.format { peniko::ImageFormat::Rgba8 => DXGI_FORMAT_R8G8B8A8_UNORM, _ => DXGI_FORMAT_R8G8B8A8_UNORM };
            let pf = D2D1_PIXEL_FORMAT { format, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
            let bp = D2D1_BITMAP_PROPERTIES1 { pixelFormat: pf, dpiX: 96.0, dpiY: 96.0, bitmapOptions: D2D1_BITMAP_OPTIONS_NONE, colorContext: None };
            let pitch = (img.width * 4) as u32;
            let mut bitmap: Option<ID2D1Bitmap1> = None;
            ctx.CreateBitmap(D2D_SIZE_U { width: img.width, height: img.height }, Some(&img.data), pitch, &bp, &mut bitmap).ok();
            let bitmap = bitmap.unwrap();
            self.image_cache.insert(key, bitmap.clone().cast().unwrap());
            bitmap.cast().unwrap()
        }
    }

    fn get_or_create_text_format(&mut self, size: f32) -> &IDWriteTextFormat {
        let key = size.to_bits();
        if !self.text_format_cache.contains_key(&key) {
            if let Some(factory) = &self.dwrite_factory { unsafe {
                let mut fmt: Option<IDWriteTextFormat> = None;
                // Use default font Arial
                if factory.CreateTextFormat(windows::core::w!("Arial"), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, windows::core::w!("en-US"), &mut fmt).is_ok() {
                    if let Some(f) = fmt { self.text_format_cache.insert(key, f); }
                }
            }}
        }
        // unwrap safe: either inserted or already present
        self.text_format_cache.get(&key).unwrap()
    }

    fn build_path_geometry(&self, path: &[PathEl]) -> Option<ID2D1PathGeometry> {
        let factory = self.d2d_factory.as_ref()?;
        unsafe {
            let mut geom: Option<ID2D1PathGeometry> = None;
            if factory.CreatePathGeometry(&mut geom).is_err() { return None; }
            let geom = geom?;
            let mut sink: Option<ID2D1GeometrySink> = None;
            if geom.Open(&mut sink).is_err() { return None; }
            if let Some(sink) = sink {
                for el in path {
                    match el {
                        PathEl::MoveTo(p) => sink.BeginFigure(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }, D2D1_FIGURE_BEGIN_FILLED),
                        PathEl::LineTo(p) => sink.AddLine(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }),
                        PathEl::QuadTo(p1, p2) => sink.AddQuadraticBezier(&[D2D1_QUADRATIC_BEZIER_SEGMENT { point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 }, point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 } }]),
                        PathEl::CurveTo(p1, p2, p3) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 },
                            point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 },
                            point3: D2D_POINT_2F { x: p3.x as f32, y: p3.y as f32 },
                        }),
                        PathEl::ClosePath => sink.EndFigure(D2D1_FIGURE_END_CLOSED),
                    }
                }
                sink.Close().ok();
            }
            Some(geom)
        }
    }
}

fn affine_to_matrix(a: Affine) -> D2D_MATRIX_3X2_F {
    let c = a.as_coeffs();
    D2D_MATRIX_3X2_F { matrix: [ [c[0] as f32, c[1] as f32], [c[2] as f32, c[3] as f32], [c[4] as f32, c[5] as f32] ] }
}
fn identity_matrix() -> D2D_MATRIX_3X2_F { D2D_MATRIX_3X2_F { matrix: [[1.0,0.0],[0.0,1.0],[0.0,0.0]] } }

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
                        colorContext: None,
                    };
                    let mut bmp: Option<ID2D1Bitmap1> = None;
                    ctx.CreateBitmapFromDxgiSurface(&surface, &bp, Some(&mut bmp)).ok();
                    if let Some(b) = bmp { self.playback(&b); }
                }
            }
        }}
    }
}
