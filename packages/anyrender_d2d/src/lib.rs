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
use std::sync::atomic::{AtomicBool, Ordering};

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
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;
use windows::core::PCSTR;

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
    BoxShadow { rect: Rect, color: Color, radius: f64, std_dev: f64 },
    // Store raw glyph indices + per-glyph advances + origin (baseline start). Advances derived
    // from provided absolute x positions so we maintain original shaping/spacing from layout.
    GlyphRun { glyph_indices: Vec<u16>, advances: Vec<f32>, origin: (f32,f32), size: f32, style: GlyphRenderStyle },
}

#[derive(Clone)]
enum GlyphRenderStyle {
    Fill { color: Color },
    Stroke { color: Color, width: f32 },
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

fn debug_log_d2d(msg: &str) {
    let mut bytes = msg.as_bytes().to_vec();
    if !bytes.ends_with(b"\n") { bytes.push(b'\n'); }
    bytes.push(0);
    unsafe { OutputDebugStringA(PCSTR(bytes.as_ptr())); }
}

// Runtime-switchable verbose logging (disabled by default for perf)
static VERBOSE_LOG: AtomicBool = AtomicBool::new(false);
pub fn set_verbose_logging(enabled: bool) { VERBOSE_LOG.store(enabled, Ordering::Relaxed); }
#[inline] fn verbose_log_d2d(msg: &str) { if VERBOSE_LOG.load(Ordering::Relaxed) { debug_log_d2d(msg); } }

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
        if self.scene.commands.len() == 1 {
            debug_log_d2d("D2DScenePainter: first command recorded (kind=Fill/Stroke) - painting pipeline active");
        }
    }
    fn draw_glyphs<'b, 's: 'b>(&'s mut self, _font: &'b Font, font_size: f32, _hint: bool, _norm: &'b [NormalizedCoord], style: impl Into<StyleRef<'b>>, brush: impl Into<BrushRef<'b>>, brush_alpha: f32, transform: Affine, _glyph_transform: Option<Affine>, glyphs: impl Iterator<Item = Glyph>) {
    let style_ref: StyleRef<'b> = style.into();
    let brush_color = match brush.into() { BrushRef::Solid(c) => c.with_alpha(c.components[3] * brush_alpha), _ => Color::BLACK };
        let glyph_style = match style_ref {
            StyleRef::Fill(_) => GlyphRenderStyle::Fill { color: brush_color },
            StyleRef::Stroke(stroke) => {
                GlyphRenderStyle::Stroke { color: brush_color, width: stroke.width as f32 }
            }
        };
        // Collect glyphs first.
    let collected: Vec<Glyph> = glyphs.collect();
        if collected.is_empty() { return; }
        // Single run: upstream stroke_text already iterates lines; we no longer split heuristically here.
        let origin_x = collected.first().unwrap().x as f32 + transform.as_coeffs()[4] as f32; // e (translation x)
        let origin_y = collected.first().unwrap().y as f32 + transform.as_coeffs()[5] as f32; // f (translation y)
        let mut glyph_indices: Vec<u16> = Vec::with_capacity(collected.len());
        let mut advances: Vec<f32> = Vec::with_capacity(collected.len());
        for (i, g) in collected.iter().enumerate() {
            glyph_indices.push(g.id as u16);
            if i + 1 < collected.len() {
                let mut adv = (collected[i+1].x - g.x) as f32;
                if adv < 0.0 { adv = 0.0; }
                let max_reasonable = font_size * 2.0;
                if adv > max_reasonable { adv = font_size * 0.6; }
                advances.push(adv);
            }
        }
        let last_adv = if advances.is_empty() { font_size * 0.6 } else { (advances.iter().copied().sum::<f32>() / advances.len() as f32).max(1.0) };
        advances.push(last_adv);
        self.scene.commands.push(Command::GlyphRun { glyph_indices, advances, origin: (origin_x, origin_y), size: font_size, style: glyph_style });
    }
    fn draw_box_shadow(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) {
        // Apply only translation components of the transform (common case in current usage).
        let coeffs = transform.as_coeffs();
        let tx = coeffs[4];
        let ty = coeffs[5];
        let translated = rect + kurbo::Vec2::new(tx, ty);
        self.scene.commands.push(Command::BoxShadow { rect: translated, color: brush, radius, std_dev });
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
    dwrite_font_face: Option<IDWriteFontFace>,
    // caches
    gradient_cache: FxHashMap<u64, ID2D1Brush>,
    image_cache: FxHashMap<u64, ID2D1Bitmap>,
    scene: D2DScene,
    width: u32,
    height: u32,
    active: bool,
    debug_shadow_logs: u32,
    last_command_count: u32,
    backbuffer_bitmap: Option<ID2D1Bitmap1>,
}

impl D2DWindowRenderer {
    pub fn new() -> Self { Self { swapchain: None, d3d_device: None, d2d_factory: None, d2d_device: None, d2d_ctx: None, dwrite_factory: None, dwrite_font_face: None, gradient_cache: FxHashMap::default(), image_cache: FxHashMap::default(), scene: D2DScene::default(), width: 1, height: 1, active: false, debug_shadow_logs: 0, last_command_count: 0, backbuffer_bitmap: None } }

    pub fn last_command_count(&self) -> u32 { self.last_command_count }

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
                                            self.dwrite_factory = Some(dwf.clone());
                                            // Create a default font face (Segoe UI) for glyph runs.
                                            let mut collection: Option<IDWriteFontCollection> = None;
                                            if dwf.GetSystemFontCollection(&mut collection, false).is_ok() {
                                                if let Some(collection) = collection {
                                                    let mut idx = 0u32;
                                                    let mut exists = false.into();
                                                    if collection.FindFamilyName(windows::core::w!("Segoe UI"), &mut idx, &mut exists).is_ok() && exists.as_bool() {
                                                        if let Ok(family) = collection.GetFontFamily(idx) {
                                                            if let Ok(font) = family.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL) {
                                                                if let Ok(face) = font.CreateFontFace() { self.dwrite_font_face = Some(face); }
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
                }
            }
        }
    }

    /// Release any bound D2D target (backbuffer bitmap) so the swapchain can ResizeBuffers.
    pub fn release_backbuffer_target(&self) {
        if let Some(ctx) = &self.d2d_ctx { unsafe { let _ = ctx.SetTarget(None::<&ID2D1Image>); } }
    }

    /// Release target and cached backbuffer bitmap so the swapchain buffers can be resized.
    pub fn release_backbuffer_resources(&mut self) {
        if let Some(ctx) = &self.d2d_ctx { unsafe { let _ = ctx.SetTarget(None::<&ID2D1Image>); } }
        if self.backbuffer_bitmap.is_some() {
            debug_log_d2d("release_backbuffer_resources: dropping cached backbuffer bitmap");
        }
        self.backbuffer_bitmap = None;
    }

    fn recreate_backbuffer_bitmap(&mut self, surface: &IDXGISurface) -> bool {
        self.backbuffer_bitmap = None;
        let ctx = match &self.d2d_ctx { Some(c) => c, None => { debug_log_d2d("recreate_backbuffer_bitmap: no D2D ctx"); return false; } };
        unsafe {
            // Improvement: try context DPI first, then inherit, then 96dpi fallback to mitigate E_INVALIDARG.
            let mut dpi_x = 0.0f32; let mut dpi_y = 0.0f32; ctx.GetDpi(&mut dpi_x, &mut dpi_y);
            if let Ok(desc) = surface.GetDesc() { verbose_log_d2d(&format!("recreate_backbuffer_bitmap: surface desc fmt={:?} w={} h={}", desc.Format, desc.Width, desc.Height)); }
            let props_ctx = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: dpi_x, dpiY: dpi_y,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            let attempt_ctx = ctx.CreateBitmapFromDxgiSurface(surface, Some(&props_ctx));
            match attempt_ctx {
                Ok(bmp) => { verbose_log_d2d("recreate_backbuffer_bitmap: created with context DPI props"); self.backbuffer_bitmap = Some(bmp); return true; }
                Err(e_ctx) => { verbose_log_d2d(&format!("recreate_backbuffer_bitmap: context DPI props failed hr={:?}; trying inherit", e_ctx)); }
            }
            if let Ok(bmp_inherit) = ctx.CreateBitmapFromDxgiSurface(surface, None) {
                verbose_log_d2d("recreate_backbuffer_bitmap: created with inherited props (None)");
                self.backbuffer_bitmap = Some(bmp_inherit); return true;
            }
            // Final fallback: explicit 96dpi
            let props_96 = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0, dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            match ctx.CreateBitmapFromDxgiSurface(surface, Some(&props_96)) {
                Ok(bmp3) => { debug_log_d2d("recreate_backbuffer_bitmap: created with 96dpi fallback props"); self.backbuffer_bitmap = Some(bmp3); true }
                Err(e3) => { debug_log_d2d(&format!("recreate_backbuffer_bitmap: all creation attempts failed e3={:?}", e3)); false }
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
            // Clear not on ID2D1DeviceContext in windows 0.58 feature set; emulate by filling full rect with transparent (composition decides backdrop).
            let size = target.GetSize();
            let full = D2D_RECT_F { left: 0.0, top: 0.0, right: size.width, bottom: size.height };
            let clear_brush = self.create_solid_brush(Color::TRANSPARENT);
            let _ = ctx.FillRectangle(&full, &clear_brush);
            // Reset per-frame debug counters
            self.debug_shadow_logs = 0;
            
            // Collect commands to avoid borrow checker issues
            let commands = std::mem::take(&mut self.scene.commands);
            let command_count = commands.len();
            self.last_command_count = command_count as u32;
            if command_count == 0 { debug_log_d2d("D2D: playback with 0 commands (expect fallback bg)"); }
            else { debug_log_d2d(&format!("D2D: playback command_count={}", command_count)); }
            // Always draw an opaque debug background to detect if contents not visible due to alpha blending.
            let dbg = self.create_solid_brush(Color::new([0.92,0.92,0.95,1.0]));
            let _ = ctx.FillRectangle(&full, &dbg);
            let shadow_count = commands.iter().filter(|c| matches!(c, Command::BoxShadow { .. })).count();
            if shadow_count > 0 {
                debug_log_d2d(&format!("D2D: {} box shadow commands", shadow_count));
            }
            
            let had_commands = !commands.is_empty();
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
                    Command::BoxShadow { rect, color, radius, std_dev } => {
                        // Temporary debug: log first few shadows to stderr so we can verify coordinates.
                        if self.debug_shadow_logs < 16 {
                            debug_log_d2d(&format!(
                                "D2D BoxShadow rect=({}, {}, {}, {}) radius={} sd={} color rgba=({:.3},{:.3},{:.3},{:.3})",
                                rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev,
                                color.components[0], color.components[1], color.components[2], color.components[3]
                            ));
                            self.debug_shadow_logs += 1;
                        }
                        self.draw_gaussian_box_shadow(&ctx, rect, color, radius, std_dev);
                    }
                    Command::GlyphRun { glyph_indices, advances, origin, size, style } => {
                        if let Some(face) = &self.dwrite_font_face {
                            if !glyph_indices.is_empty() {
                                // Stroke not yet implemented (outline path); keep width for future use.
                                let (color, _stroke_width_opt) = match style {
                                    GlyphRenderStyle::Fill { color } => (color, None),
                                    GlyphRenderStyle::Stroke { color, width } => (color, Some(width)),
                                };
                                let brush = self.create_solid_brush(color);
                                // Use recorded advances (already derived). Fallback: if lengths mismatch, bail.
                                if advances.len() != glyph_indices.len() { continue; }
                                let run = DWRITE_GLYPH_RUN {
                                    fontFace: std::mem::ManuallyDrop::new(Some(face.clone())),
                                    fontEmSize: size,
                                    glyphCount: glyph_indices.len() as u32,
                                    glyphIndices: glyph_indices.as_ptr(),
                                    glyphAdvances: advances.as_ptr(),
                                    glyphOffsets: std::ptr::null(),
                                    isSideways: false.into(),
                                    bidiLevel: 0,
                                };
                                let origin_pt = D2D_POINT_2F { x: origin.0, y: origin.1 };
                                // Stroke variant currently falls back to fill rendering until outline support is added.
                                let _ = ctx.DrawGlyphRun(origin_pt, &run, None, &brush, DWRITE_MEASURING_MODE_NATURAL);
                            }
                        }
                    }
                }
            }
            if !had_commands {
                // Fallback background (single rect) for visibility when nothing recorded.
                let bg = self.create_solid_brush(Color::WHITE);
                let _ = ctx.FillRectangle(&full, &bg);
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

    // Removed legacy text_format_cache based path; glyph runs now used directly.

    fn build_path_geometry(&self, path: &[PathEl]) -> Option<ID2D1PathGeometry> {
        let factory = self.d2d_factory.as_ref()?;
        unsafe {
            // CreatePathGeometry returns a PathGeometry1 in newer SDKs; cast to base interface.
            let geom1 = factory.CreatePathGeometry().ok()?; // ID2D1PathGeometry1
            let geom: ID2D1PathGeometry = geom1.cast().unwrap_or_else(|_| panic!("PathGeometry cast failed"));
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
            Some(geom)
        }
    }

    fn draw_gaussian_box_shadow(&mut self, ctx: &ID2D1DeviceContext, rect: Rect, color: Color, radius: f64, _std_dev: f64) {
        // Temporary simplified shadow: draw a single rounded (if radius>0) translucent expanded rect.
        unsafe {
            let alpha_color = color.with_alpha(color.components[3] * 0.35);
            let brush = self.create_solid_brush(alpha_color);
            let expand = 4.0_f32; // fixed feather
            let base_rect = D2D_RECT_F { left: rect.x0 as f32 - expand, top: rect.y0 as f32 - expand, right: rect.x1 as f32 + expand, bottom: rect.y1 as f32 + expand };
            if radius > 0.0 {
                let r = radius.min((rect.width()*0.5).min(rect.height()*0.5)) as f32;
                let rr = D2D1_ROUNDED_RECT { rect: base_rect, radiusX: r + expand, radiusY: r + expand };
                let _ = ctx.FillRoundedRectangle(&rr, &brush);
            } else {
                let _ = ctx.FillRectangle(&base_rect, &brush);
            }
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
            let before = painter.scene.commands.len();
            verbose_log_d2d(&format!("D2DWindowRenderer::render: before draw_fn commands={}", before));
            draw_fn(&mut painter);
            let after = painter.scene.commands.len();
            verbose_log_d2d(&format!("D2DWindowRenderer::render: after draw_fn commands={}", after));
        }
        // Acquire backbuffer and wrap in D2D bitmap
        if let Some(sc) = &self.swapchain { unsafe {
            if let Ok(surface) = sc.GetBuffer::<IDXGISurface>(0) {
                if self.d2d_ctx.is_none() {
                    verbose_log_d2d("D2DWindowRenderer::render: d2d_ctx missing; attempting lazy initialization");
                    self.init_devices_from_swapchain();
                    if self.d2d_ctx.is_none() { debug_log_d2d("D2DWindowRenderer::render: lazy init failed (no D2D context)"); }
                    else { verbose_log_d2d("D2DWindowRenderer::render: lazy init succeeded"); }
                }
                if self.d2d_ctx.is_none() { return; }
                // (Re)create backbuffer bitmap if absent or size changed
                let need_new = match &self.backbuffer_bitmap {
                    Some(bmp) => {
                        let sz = bmp.GetSize();
                        (sz.width as u32) != self.width || (sz.height as u32) != self.height
                    }
                    None => true,
                };
                if need_new {
                    if !self.recreate_backbuffer_bitmap(&surface) {
                        debug_log_d2d("D2DWindowRenderer::render: cannot create backbuffer bitmap; skipping frame");
                        return;
                    }
                }
                if let Some(bmp) = self.backbuffer_bitmap.take() {
                    self.playback(&bmp);
                    self.backbuffer_bitmap = Some(bmp);
                }
            }
        }}
    }
}
