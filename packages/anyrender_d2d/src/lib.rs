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
use std::hash::{Hash, Hasher};

// Cache key for blurred shadow bitmaps (quantized params to limit variety)
#[derive(Clone, Copy, Eq)]
struct ShadowKey {
    w: u32,
    h: u32,
    radius_q: u16,
    stddev_q: u16,
    rgba: u32, // packed
}
impl PartialEq for ShadowKey { fn eq(&self, other: &Self) -> bool { self.w==other.w && self.h==other.h && self.radius_q==other.radius_q && self.stddev_q==other.stddev_q && self.rgba==other.rgba } }
impl Hash for ShadowKey { fn hash<H: Hasher>(&self, state: &mut H) { state.write_u32(self.w); state.write_u32(self.h); state.write_u16(self.radius_q); state.write_u16(self.stddev_q); state.write_u32(self.rgba); } }
impl ShadowKey { fn new(rect: &Rect, radius: f64, std_dev: f64, color: Color) -> Self { let w = rect.width().round().max(0.0) as u32; let h = rect.height().round().max(0.0) as u32; let radius_q = (radius.clamp(0.0,655.0)*100.0).round() as u16; let stddev_q = (std_dev.clamp(0.0,655.0)*100.0).round() as u16; let r = (color.components[0].clamp(0.0,1.0)*255.0).round() as u8; let g=(color.components[1].clamp(0.0,1.0)*255.0).round() as u8; let b=(color.components[2].clamp(0.0,1.0)*255.0).round() as u8; let a=(color.components[3].clamp(0.0,1.0)*255.0).round() as u8; let rgba = u32::from_le_bytes([r,g,b,a]); ShadowKey { w,h,radius_q,stddev_q,rgba } } }

// NOTE: Do not rely on HWND in WinUI shell path

/// Scene representation for D2D backend: we store a lightweight command list then play it back.
#[derive(Default)]
struct D2DScene { commands: Vec<Command>, }

enum Command {
    PushLayer { rect: Rect },
    PopLayer,
    FillPath { path: Vec<PathEl>, brush: RecordedBrush },
    StrokePath { path: Vec<PathEl>, brush: RecordedBrush, width: f64 },
    BoxShadow { rect: Rect, color: Color, radius: f64, std_dev: f64, inset: bool },
    GlyphRun { glyph_indices: Vec<u16>, advances: Vec<f32>, origin: (f32,f32), size: f32, style: GlyphRenderStyle, font: FontKey, var_coords: Vec<NormalizedCoord> },
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

// Key identifying a font face request (initially only default Segoe UI is used until full plumbing).
#[derive(Clone, Hash, PartialEq, Eq)]
struct FontKey {
    family: String,
    weight: u16,   // 100-900 CSS weights
    stretch: u8,   // map to DWRITE_FONT_STRETCH_* (1..=9)
    italic: bool,
}

impl FontKey {
    fn default() -> Self { Self { family: "Segoe UI".to_string(), weight: 400, stretch: 5, italic: false } } // stretch=5 -> normal
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
// Lightweight macro to avoid repeating VERBOSE_LOG.load boilerplate while preserving
// ability to skip formatting cost when verbose logging is off.
macro_rules! vlog { ($($t:tt)*) => { if VERBOSE_LOG.load(Ordering::Relaxed) { debug_log_d2d(&format!($($t)*)); } } }

impl<'a> PaintScene for D2DScenePainter<'a> {
    fn reset(&mut self) { self.scene.reset(); }
    fn push_layer(&mut self, _blend: impl Into<BlendMode>, _alpha: f32, transform: Affine, clip: &impl Shape) {
        // Only rectangular clips supported for now; approximate by bounding box + translation.
        if let Some(mut rect) = shape_as_rect(clip) {
            let t = transform.as_coeffs();
            // If transform is (approximately) a pure translation, bake it into the rect.
            if t[0] == 1.0 && t[1] == 0.0 && t[2] == 0.0 && t[3] == 1.0 {
                rect = rect + kurbo::Vec2::new(t[4], t[5]);
            }
            self.scene.commands.push(Command::PushLayer { rect });
        }
    }
    fn pop_layer(&mut self) { self.scene.commands.push(Command::PopLayer); }
    fn stroke<'b>(&mut self, style: &Stroke, transform: Affine, brush: impl Into<BrushRef<'b>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        let brush_rec = record_brush(brush.into());
    // Removed rect fast path so rounded rectangles (and other shapes) retain corner geometry.
        // Fallback: record full path with translation baked in (ignore non-translation components for now).
        let mut v = Vec::new();
        shape_to_path_elements(shape, &mut v);
        let t = transform.as_coeffs();
        if t[4] != 0.0 || t[5] != 0.0 {
            for el in &mut v {
                match el {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => { p.x += t[4]; p.y += t[5]; }
                    PathEl::QuadTo(p1, p2) => { p1.x += t[4]; p1.y += t[5]; p2.x += t[4]; p2.y += t[5]; }
                    PathEl::CurveTo(p1, p2, p3) => { p1.x += t[4]; p1.y += t[5]; p2.x += t[4]; p2.y += t[5]; p3.x += t[4]; p3.y += t[5]; }
                    PathEl::ClosePath => {}
                }
            }
        }
        self.scene.commands.push(Command::StrokePath { path: v, brush: brush_rec, width: style.width });
    }
    fn fill<'b>(&mut self, _style: Fill, transform: Affine, brush: impl Into<anyrender::Paint<'b>>, _brush_transform: Option<Affine>, shape: &impl Shape) {
        let brush_rec = record_paint(brush.into());
    // Removed rect fast path to allow rounded rect path elements to be recorded.
        let mut v = Vec::new();
        shape_to_path_elements(shape, &mut v);
        let t = transform.as_coeffs();
        if t[4] != 0.0 || t[5] != 0.0 {
            for el in &mut v {
                match el {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => { p.x += t[4]; p.y += t[5]; }
                    PathEl::QuadTo(p1, p2) => { p1.x += t[4]; p1.y += t[5]; p2.x += t[4]; p2.y += t[5]; }
                    PathEl::CurveTo(p1, p2, p3) => { p1.x += t[4]; p1.y += t[5]; p2.x += t[4]; p2.y += t[5]; p3.x += t[4]; p3.y += t[5]; }
                    PathEl::ClosePath => {}
                }
            }
        }
        self.scene.commands.push(Command::FillPath { path: v, brush: brush_rec });
    if self.scene.commands.len() == 1 { vlog!("first command recorded (FillPath)"); }
    }
    fn draw_glyphs<'b, 's: 'b>(&'s mut self, _font: &'b Font, font_family: &str, font_size: f32, font_weight: u16, _hint: bool, _norm: &'b [NormalizedCoord], style: impl Into<StyleRef<'b>>, brush: impl Into<BrushRef<'b>>, brush_alpha: f32, transform: Affine, _glyph_transform: Option<Affine>, glyphs: impl Iterator<Item = Glyph>) {
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
        let mut fk = FontKey::default();
    // Map CSS generic families to concrete Windows fonts.
    let lower = font_family.to_ascii_lowercase();
    let resolved_family = match lower.as_str() {
            "monospace" => "Consolas", // or Cascadia Mono if desired
            "serif" => "Times New Roman",
            "sans-serif" => "Segoe UI",
            "system-ui" => "Segoe UI",
            "cursive" => "Comic Sans MS",
            "fantasy" => "Segoe UI", // placeholder
            fam if fam.is_empty() => "Segoe UI",
            other => other,
        };
        fk.family = resolved_family.to_string();
        fk.weight = if (100..=900).contains(&font_weight) { font_weight } else { 400 } as u16;
    self.scene.commands.push(Command::GlyphRun { glyph_indices, advances, origin: (origin_x, origin_y), size: font_size, style: glyph_style, font: fk, var_coords: Vec::new() });
    }
    fn draw_box_shadow(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) {
        // Apply only translation components of the transform (common case in current usage).
        let coeffs = transform.as_coeffs();
        let tx = coeffs[4];
        let ty = coeffs[5];
        let translated = rect + kurbo::Vec2::new(tx, ty);
        let inset = std_dev < 0.0;
        let std_dev = std_dev.abs();
        self.scene.commands.push(Command::BoxShadow { rect: translated, color: brush, radius, std_dev, inset });
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
    font_face_cache: FxHashMap<FontKey, IDWriteFontFace>,
    // caches
    gradient_cache: FxHashMap<u64, ID2D1Brush>,
    image_cache: FxHashMap<u64, ID2D1Bitmap>,
    // shadow blur cache (bitmap of blurred rounded rect); separate from image_cache to control eviction separately
    shadow_cache: FxHashMap<ShadowKey, ID2D1Bitmap1>,
    shadow_cache_order: std::collections::VecDeque<ShadowKey>,
    gaussian_blur_effect: Option<ID2D1Effect>,
    scene: D2DScene,
    width: u32,
    height: u32,
    active: bool,
    debug_shadow_logs: u32,
    last_command_count: u32,
    backbuffer_bitmap: Option<ID2D1Bitmap1>,
}

impl D2DWindowRenderer {
    pub fn new() -> Self { Self { swapchain: None, d3d_device: None, d2d_factory: None, d2d_device: None, d2d_ctx: None, dwrite_factory: None, dwrite_font_face: None, font_face_cache: FxHashMap::default(), gradient_cache: FxHashMap::default(), image_cache: FxHashMap::default(), shadow_cache: FxHashMap::default(), shadow_cache_order: std::collections::VecDeque::new(), gaussian_blur_effect: None, scene: D2DScene::default(), width: 1, height: 1, active: false, debug_shadow_logs: 0, last_command_count: 0, backbuffer_bitmap: None } }

    pub fn last_command_count(&self) -> u32 { self.last_command_count }

    pub fn set_swapchain(&mut self, sc: IDXGISwapChain1, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.swapchain = Some(sc);
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
                colorContext: std::mem::ManuallyDrop::new(None::<ID2D1ColorContext>),
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
                colorContext: std::mem::ManuallyDrop::new(None::<ID2D1ColorContext>),
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
        // Allow enabling verbose logging dynamically via environment.
    if std::env::var("BLITZ_VERBOSE").map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) {
            set_verbose_logging(true);
        }
        
        unsafe {
            ctx.BeginDraw();
            // SetTarget exists on ID2D1DeviceContext
            let _ = ctx.SetTarget(target);
            // Clear: previously we filled with transparent which caused full window transparency when scene content lacked opaque background.
            // Use an opaque fallback (white) so something is always visible; later we can sample actual page background color.
            let size = target.GetSize();
            let full = D2D_RECT_F { left: 0.0, top: 0.0, right: size.width, bottom: size.height };
            let fallback_bg_brush = self.create_solid_brush(Color::WHITE); // TODO: replace with document root background
            let _ = ctx.FillRectangle(&full, &fallback_bg_brush);
            vlog!("fallback bg {}x{}", size.width as u32, size.height as u32);
            // (Removed always-on debug rect; keep codebase clean. Use VERBOSE logs for diagnostics.)
            // Reset per-frame debug counters
            self.debug_shadow_logs = 0;
            
            // Collect commands to avoid borrow checker issues
            let commands = std::mem::take(&mut self.scene.commands);
            let command_count = commands.len();
            self.last_command_count = command_count as u32;
            if command_count == 0 { vlog!("playback: 0 cmds"); } else { vlog!("playback: {} cmds", command_count); }
            // Optional debug background (disabled by default). Enable only under verbose logging to diagnose alpha issues.
            if VERBOSE_LOG.load(Ordering::Relaxed) {
                let dbg = self.create_solid_brush(Color::new([0.92,0.92,0.95,1.0]));
                let _ = ctx.FillRectangle(&full, &dbg);
            }
            let shadow_count = commands.iter().filter(|c| matches!(c, Command::BoxShadow { .. })).count();
            if shadow_count > 0 { vlog!("shadows: {}", shadow_count); }
            
            // Playback counters
            let mut fill_path_count = 0u32;
            let mut stroke_path_count = 0u32;
            let mut clip_depth: i32 = 0;
            let mut max_clip_depth: i32 = 0;
            // Isolation flags
            // Pruned experimental env toggles; retain only minimal isolation switches.
            let disable_clips = false; // clip stack stable
            let disable_text  = false; // glyph runs stable
            let recreate_effect_per_shadow = false; // effect reused
            let disable_inset_shadows = false; // inset stable
            for (cmd_index, cmd) in commands.into_iter().enumerate() {
                // max command limit feature removed (kept simpler playback path)
                vlog!("cmd {} {}", cmd_index, match &cmd { Command::FillPath{..}=>"FillPath", Command::StrokePath{..}=>"StrokePath", Command::PushLayer{..}=>"PushLayer", Command::PopLayer=>"PopLayer", Command::BoxShadow{inset,..}=> if *inset {"BoxShadowInset"} else {"BoxShadow"}, Command::GlyphRun{..}=>"GlyphRun" });
                match cmd {
                    Command::FillPath { path, brush } => {
                        fill_path_count += 1;
                        if let Some(geom) = self.build_path_geometry(&path) {
                            let brush = self.get_or_create_brush(&brush);
                            if fill_path_count <= 8 { if let Ok(sol) = brush.cast::<ID2D1SolidColorBrush>() { let col = sol.GetColor(); vlog!("FillPath idx={} cmd={} rgba=({:.3},{:.3},{:.3},{:.3})", fill_path_count, cmd_index, col.r, col.g, col.b, col.a); } else { vlog!("FillPath idx={} cmd={} (non-solid)", fill_path_count, cmd_index); } }
                            let _ = ctx.FillGeometry(&geom, &brush, None);
                            if fill_path_count <= 8 { let mut minx = f32::INFINITY; let mut miny = f32::INFINITY; let mut maxx = f32::NEG_INFINITY; let mut maxy = f32::NEG_INFINITY; for el in &path { match el { PathEl::MoveTo(p) | PathEl::LineTo(p) => { minx=minx.min(p.x as f32); miny=miny.min(p.y as f32); maxx=maxx.max(p.x as f32); maxy=maxy.max(p.y as f32); }, PathEl::QuadTo(p1,p2) => { for q in [p1,p2] { minx=minx.min(q.x as f32); miny=miny.min(q.y as f32); maxx=maxx.max(q.x as f32); maxy=maxy.max(q.y as f32); } }, PathEl::CurveTo(p1,p2,p3) => { for q in [p1,p2,p3] { minx=minx.min(q.x as f32); miny=miny.min(q.y as f32); maxx=maxx.max(q.x as f32); maxy=maxy.max(q.y as f32); } }, PathEl::ClosePath => {} } } vlog!("FillPath idx={} elems={} bbox=({}, {}, {}, {})", fill_path_count, path.len(), minx, miny, maxx, maxy); }
                        }
                    }
                    Command::StrokePath { path, brush, width } => {
                        stroke_path_count += 1;
                        if let Some(geom) = self.build_path_geometry(&path) {
                            let brush = self.get_or_create_brush(&brush);
                            let _ = ctx.DrawGeometry(&geom, &brush, width as f32, None);
                        }
                    }
                    Command::PushLayer { rect } => {
                        if disable_clips { continue; }
                        let r = D2D_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
                        let _ = ctx.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        clip_depth += 1; if clip_depth > max_clip_depth { max_clip_depth = clip_depth; }
                        vlog!("PushLayer depth={} rect=({}, {}, {}, {})", clip_depth, rect.x0, rect.y0, rect.x1, rect.y1);
                    }
                    Command::PopLayer => {
                        if disable_clips { continue; }
                        if clip_depth <= 0 { vlog!("PopLayer underflow"); }
                        else { clip_depth -= 1; }
                        ctx.PopAxisAlignedClip();
                        vlog!("PopLayer depth={}", clip_depth);
                    }
                    Command::BoxShadow { rect, color, radius, std_dev, inset } => {
                        // Allow disabling shadows for isolation (BLITZ_DISABLE_SHADOWS=1)
                        if std::env::var("BLITZ_DISABLE_SHADOWS").map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) { continue; } // keep one isolation flag
                        if inset && disable_inset_shadows { continue; }
                        if self.debug_shadow_logs < 8 { vlog!("BoxShadow{} rect=({}, {}, {}, {}) r={} sd={} a={:.3}", if inset {"(inset)"} else {""}, rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev, color.components[3]); self.debug_shadow_logs += 1; }
                        if inset { self.draw_inset_gaussian_box_shadow(&ctx, rect, color, radius, std_dev); }
                        else {
                            if recreate_effect_per_shadow { self.gaussian_blur_effect = None; }
                            self.draw_gaussian_box_shadow(&ctx, rect, color, radius, std_dev);
                        }
                    }
                    Command::GlyphRun { glyph_indices, advances, origin, size, style, font, var_coords: _ } => {
                        if disable_text { continue; }
                        // Resolve font face via cache, fallback to default face if not yet available.
                        let face_opt = self.get_or_create_font_face(&font).or_else(|| self.dwrite_font_face.clone());
                        if let Some(face) = face_opt {
                            if !glyph_indices.is_empty() && advances.len() == glyph_indices.len() {
                                let (color, stroke_width_opt) = match style {
                                    GlyphRenderStyle::Fill { color } => (color, None),
                                    GlyphRenderStyle::Stroke { color, width } => (color, Some(width)),
                                };
                                let brush = self.create_solid_brush(color);
                                if let Some(stroke_width) = stroke_width_opt {
                                    if let Some(geom) = self.build_glyph_outline_geometry(&face, size, &glyph_indices, &advances) {
                                        let _ = ctx.DrawGeometry(&geom, &brush, stroke_width, None);
                                    } else {
                                        // Fallback: fill if outline extraction fails
                                        let run = DWRITE_GLYPH_RUN { fontFace: std::mem::ManuallyDrop::new(Some(face.clone())), fontEmSize: size, glyphCount: glyph_indices.len() as u32, glyphIndices: glyph_indices.as_ptr(), glyphAdvances: advances.as_ptr(), glyphOffsets: std::ptr::null(), isSideways: false.into(), bidiLevel: 0 };
                                        let origin_pt = D2D_POINT_2F { x: origin.0, y: origin.1 };
                                        let _ = ctx.DrawGlyphRun(origin_pt, &run, None, &brush, DWRITE_MEASURING_MODE_NATURAL);
                                    }
                                } else {
                                    let run = DWRITE_GLYPH_RUN { fontFace: std::mem::ManuallyDrop::new(Some(face.clone())), fontEmSize: size, glyphCount: glyph_indices.len() as u32, glyphIndices: glyph_indices.as_ptr(), glyphAdvances: advances.as_ptr(), glyphOffsets: std::ptr::null(), isSideways: false.into(), bidiLevel: 0 };
                                    let origin_pt = D2D_POINT_2F { x: origin.0, y: origin.1 };
                                    let _ = ctx.DrawGlyphRun(origin_pt, &run, None, &brush, DWRITE_MEASURING_MODE_NATURAL);
                                }
                            }
                        }
                    }
                }
            }
            if clip_depth != 0 { while clip_depth > 0 { ctx.PopAxisAlignedClip(); clip_depth -= 1; } }
            vlog!("counts fp={} sp={} cmds={} shadows={}", fill_path_count, stroke_path_count, command_count, shadow_count);
            // If no commands, fallback bg already drawn earlier.
            // Note: SetTransform removed - not available in this windows-rs version
            let end_res = ctx.EndDraw(None, None);
            if let Err(e) = end_res { debug_log_d2d(&format!("EndDraw error {:?}", e)); } else { vlog!("EndDraw ok"); }
        }
    }

    fn create_solid_brush(&self, color: Color) -> ID2D1SolidColorBrush {
        let ctx = self.d2d_ctx.as_ref().unwrap();
        unsafe {
            let col = D2D1_COLOR_F { r: color.components[0] as f32, g: color.components[1] as f32, b: color.components[2] as f32, a: color.components[3] as f32 };
            ctx.CreateSolidColorBrush(&col, None).unwrap()
        }
    }

    // Resolve (and cache) a font face for the provided key using DirectWrite system collection.
    fn get_or_create_font_face(&mut self, key: &FontKey) -> Option<IDWriteFontFace> {
        if let Some(face) = self.font_face_cache.get(key) { return Some(face.clone()); }
        let factory = self.dwrite_factory.clone()?;
        unsafe {
            let mut collection_opt: Option<IDWriteFontCollection> = None;
            if factory.GetSystemFontCollection(&mut collection_opt, false).is_ok() {
                if let Some(collection) = collection_opt {
                    let mut idx = 0u32; let mut exists = false.into();
                    if collection.FindFamilyName(&windows::core::HSTRING::from(&key.family), &mut idx, &mut exists).is_ok() && exists.as_bool() {
                        if let Ok(family) = collection.GetFontFamily(idx) {
                            let weight = DWRITE_FONT_WEIGHT(key.weight as i32);
                            // Map stretch (1..=9) directly; default normal (5)
                            let stretch = DWRITE_FONT_STRETCH(key.stretch as i32);
                            let style = if key.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL };
                            if let Ok(font) = family.GetFirstMatchingFont(weight, stretch, style) {
                                if let Ok(face) = font.CreateFontFace() {
                                    self.font_face_cache.insert(key.clone(), face.clone());
                                    return Some(face);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    // Build outline geometry for glyph run; returns a path geometry or None on failure.
    fn build_glyph_outline_geometry(&self, face: &IDWriteFontFace, em_size: f32, glyph_indices: &[u16], advances: &[f32]) -> Option<ID2D1PathGeometry> {
        if glyph_indices.is_empty() { return None; }
        let factory = self.d2d_factory.as_ref()?;
        unsafe {
            let path_geom1 = factory.CreatePathGeometry().ok()?;
            let path_geom: ID2D1PathGeometry = path_geom1.cast().ok()?;
            let sink: ID2D1GeometrySink = path_geom.Open().ok()?; // implements simplified
            let simple: ID2D1SimplifiedGeometrySink = sink.cast().ok()?;
            let hr = face.GetGlyphRunOutline(
                em_size,
                glyph_indices.as_ptr(),
                Some(advances.as_ptr()),
                None,
                glyph_indices.len() as u32,
                false,
                false,
                &simple,
            );
            if hr.is_ok() { let _ = sink.Close(); return Some(path_geom); }
        }
        None
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
            let mut figure_open = false;
            for el in path {
                match el {
                    PathEl::MoveTo(p) => {
                        if figure_open { sink.EndFigure(D2D1_FIGURE_END_OPEN); }
                        sink.BeginFigure(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }, D2D1_FIGURE_BEGIN_FILLED);
                        figure_open = true;
                    }
                    PathEl::LineTo(p) => { sink.AddLine(D2D_POINT_2F { x: p.x as f32, y: p.y as f32 }); }
                    PathEl::QuadTo(p1, p2) => {
                        let bezier = D2D1_QUADRATIC_BEZIER_SEGMENT {
                            point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 },
                            point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 }
                        };
                        sink.AddQuadraticBezier(&bezier);
                    }
                    PathEl::CurveTo(p1, p2, p3) => {
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: D2D_POINT_2F { x: p1.x as f32, y: p1.y as f32 }, point2: D2D_POINT_2F { x: p2.x as f32, y: p2.y as f32 }, point3: D2D_POINT_2F { x: p3.x as f32, y: p3.y as f32 } });
                    }
            PathEl::ClosePath => { if figure_open { sink.EndFigure(D2D1_FIGURE_END_CLOSED); figure_open = false; } }
                }
            }
    if figure_open { sink.EndFigure(D2D1_FIGURE_END_OPEN); }
            if let Err(e) = sink.Close() { debug_log_d2d(&format!("build_path_geometry: sink.Close error hr={:?}", e)); }
            Some(geom)
        }
    }

    fn draw_gaussian_box_shadow(&mut self, ctx: &ID2D1DeviceContext, rect: Rect, color: Color, radius: f64, std_dev: f64) {
        // Clamp / sanitize inputs
        if rect.width() <= 0.0 || rect.height() <= 0.0 { return; }
    debug_log_d2d(&format!("draw_gaussian_box_shadow: begin rect=({}, {}, {}, {}) radius={} sd={} color_a={:.3}", rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev, color.components[3]));
        let std_dev = std_dev.clamp(0.5, 200.0);
        let corner_radius = radius.max(0.0);
        // Determine padding (rough heuristic similar to Vello: 2.5 * std_dev)
        let pad = (std_dev * 2.5).ceil().max(1.0);
        let key = ShadowKey::new(&rect, corner_radius, std_dev, color);
        // Try cache
        if let Some(bmp) = self.shadow_cache.get(&key) {
            self.blit_cached_shadow(ctx, bmp, &rect, pad as f32);
            return;
        }
        // Create offscreen target sized to rect + padding both sides
        let ow = (rect.width() + pad * 2.0).ceil().max(1.0) as u32;
        let oh = (rect.height() + pad * 2.0).ceil().max(1.0) as u32;
        if ow == 0 || oh == 0 { return; }
        unsafe {
            // Lazily create blur effect (will be used on main context)
            if self.gaussian_blur_effect.is_none() {
                if let Ok(effect) = ctx.CreateEffect(&CLSID_D2D1GaussianBlur) { self.gaussian_blur_effect = Some(effect); }
            }
            // We cannot call SetTarget on the primary context during an active BeginDraw (causes D2DERR_WRONG_STATE).
            // Instead create a temporary device context to rasterize the solid shape into an offscreen bitmap.
            let d2d_device = match &self.d2d_device { Some(d) => d.clone(), None => return };
            let temp_ctx = match d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) { Ok(c) => c, Err(_) => return };
            let pf = D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
            let bp = D2D1_BITMAP_PROPERTIES1 { pixelFormat: pf, dpiX: 96.0, dpiY: 96.0, bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET, colorContext: std::mem::ManuallyDrop::new(None) };
            let size = D2D_SIZE_U { width: ow, height: oh };
            let offscreen = match temp_ctx.CreateBitmap(size, None, 0, &bp) { Ok(b) => b, Err(_) => return };
            let _ = temp_ctx.SetTarget(&offscreen);
            temp_ctx.BeginDraw();
            temp_ctx.Clear(Some(&D2D1_COLOR_F { r:0.0,g:0.0,b:0.0,a:0.0 }));
            let solid_brush = {
                let col = D2D1_COLOR_F { r: color.components[0] as f32, g: color.components[1] as f32, b: color.components[2] as f32, a: color.components[3] as f32 };
                temp_ctx.CreateSolidColorBrush(&col, None).unwrap()
            };
            let local_rect = D2D_RECT_F { left: pad as f32, top: pad as f32, right: pad as f32 + rect.width() as f32, bottom: pad as f32 + rect.height() as f32 };
            if corner_radius > 0.0 {
                let r_clamped = corner_radius.min((rect.width()*0.5).min(rect.height()*0.5)) as f32;
                let rr = D2D1_ROUNDED_RECT { rect: local_rect, radiusX: r_clamped, radiusY: r_clamped };
                temp_ctx.FillRoundedRectangle(&rr, &solid_brush);
            } else {
                temp_ctx.FillRectangle(&local_rect, &solid_brush);
            }
            let _ = temp_ctx.EndDraw(None, None);
            // Apply Gaussian blur effect: Feed offscreen -> blur -> draw to original target.
            if let Some(effect) = &self.gaussian_blur_effect {
                // Provide offscreen bitmap directly as input 0
                let _ = effect.SetInput(0, &offscreen, true);
                let sigma = std_dev as f32; // Direct2D uses standard deviation
                // SetValue expects raw bytes; provide proper property types
                let sigma_bytes: &[u8] = std::slice::from_raw_parts((&sigma) as *const f32 as *const u8, std::mem::size_of::<f32>());
                let _ = effect.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, sigma_bytes);
                let border_val: u32 = D2D1_BORDER_MODE_SOFT.0 as u32;
                let border_bytes: &[u8] = std::slice::from_raw_parts((&border_val) as *const u32 as *const u8, std::mem::size_of::<u32>());
                let _ = effect.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_UINT32, border_bytes);
                if let Ok(effect_img) = effect.cast::<ID2D1Image>() {
                    let offset = D2D_POINT_2F { x: (rect.x0 - pad) as f32, y: (rect.y0 - pad) as f32 };
                    ctx.DrawImage(&effect_img, Some(&offset), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
                } else {
                    // Fallback copy via DrawBitmap (no blur)
                    let dest = D2D_RECT_F { left: (rect.x0 - pad) as f32, top: (rect.y0 - pad) as f32, right: (rect.x0 - pad) as f32 + ow as f32, bottom: (rect.y0 - pad) as f32 + oh as f32 };
                    ctx.DrawBitmap(&offscreen, Some(&dest), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, None, None);
                }
            } else {
                // Fallback: no effect, just draw offscreen directly (unblurred)
                let dest = D2D_RECT_F { left: (rect.x0 - pad) as f32, top: (rect.y0 - pad) as f32, right: (rect.x0 - pad) as f32 + ow as f32, bottom: (rect.y0 - pad) as f32 + oh as f32 };
                ctx.DrawBitmap(&offscreen, Some(&dest), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, None, None);
            }
            // Insert into cache: need blurred result if effect exists; capture by drawing effect into a bitmap.
            // Avoid mutating the main device context target mid-frame (suspected contributor to D2DERR_WRONG_STATE)
            // by performing the capture in a temporary device context.
            if let Some(effect) = &self.gaussian_blur_effect {
                if let Ok(effect_img) = effect.cast::<ID2D1Image>() {
                    if let Some(d2d_device) = &self.d2d_device {
                        if let Ok(temp_ctx_cache) = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) {
                            if let Ok(desc_bitmap) = temp_ctx_cache.CreateBitmap(D2D_SIZE_U { width: ow, height: oh }, None, 0, &bp) {
                                let _ = temp_ctx_cache.SetTarget(&desc_bitmap);
                                temp_ctx_cache.BeginDraw();
                                let offset0 = D2D_POINT_2F { x: 0.0, y: 0.0 };
                                let copy_rect = D2D_RECT_F { left: 0.0, top: 0.0, right: ow as f32, bottom: oh as f32 };
                                temp_ctx_cache.Clear(Some(&D2D1_COLOR_F { r:0.0,g:0.0,b:0.0,a:0.0 }));
                                temp_ctx_cache.DrawImage(&effect_img, Some(&offset0), Some(&copy_rect), D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_COPY);
                                let _ = temp_ctx_cache.EndDraw(None, None);
                                self.insert_shadow_cache(key, desc_bitmap.clone());
                            }
                        }
                    }
                } else {
                    self.insert_shadow_cache(key, offscreen.clone());
                }
            } else {
                self.insert_shadow_cache(key, offscreen.clone());
            }
            debug_log_d2d("draw_gaussian_box_shadow: end");
        }
    }

    fn draw_inset_gaussian_box_shadow(&mut self, ctx: &ID2D1DeviceContext, rect: Rect, color: Color, radius: f64, std_dev: f64) {
        // Revised inset shadow: create a thin ring just inside the element rect and blur inward.
        let std_dev = std_dev.clamp(0.5, 64.0);
        if rect.width() <= 0.0 || rect.height() <= 0.0 { return; }
    debug_log_d2d(&format!("draw_inset_gaussian_box_shadow: begin rect=({}, {}, {}, {}) radius={} sd={} a={:.3}", rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev, color.components[3]));
        let ring_thickness = 1.5_f64.max(std_dev * 0.4).min(rect.width().min(rect.height()) * 0.5 - 0.5);
        let pad = (std_dev * 1.5).ceil().max(1.0); // inward spread
        let off_w = (rect.width() + pad * 2.0).ceil() as u32;
        let off_h = (rect.height() + pad * 2.0).ceil() as u32;
        if off_w == 0 || off_h == 0 { return; }
        // Safety: bail if something went wrong causing enormous allocation ( > 16k )
        if off_w > 16384 || off_h > 16384 { debug_log_d2d(&format!("draw_inset_gaussian_box_shadow: dimensions too large off_w={} off_h={} (bail)", off_w, off_h)); return; }
        let factory = match &self.d2d_factory { Some(f) => f.clone(), None => return };
        unsafe {
            // Use a temporary device context to draw the inner ring to avoid SetTarget on primary context.
            let d2d_device = match &self.d2d_device { Some(d) => d.clone(), None => return };
            let temp_ctx = match d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) { Ok(c) => c, Err(_) => return };
            let bmp_props = D2D1_BITMAP_PROPERTIES1 { pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 96.0, dpiY: 96.0, bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET, colorContext: std::mem::ManuallyDrop::new(None::<ID2D1ColorContext>) };
            let off_bmp = match temp_ctx.CreateBitmap(D2D_SIZE_U { width: off_w, height: off_h }, None, 0, &bmp_props) { Ok(b) => b, Err(_) => return };
            let _ = temp_ctx.SetTarget(&off_bmp);
            temp_ctx.BeginDraw();
            temp_ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            // Build ring using geometry group of two rounded rects (outer + inner hole).
            let rr = radius; // preserve original radius
            let outer_rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: pad as f32, top: pad as f32, right: (pad + rect.width()) as f32, bottom: (pad + rect.height()) as f32 }, radiusX: rr as f32, radiusY: rr as f32 };
            let inner_rr = {
                let shrink = ring_thickness as f32;
                let mut r_inner = radius as f32 - shrink;
                if r_inner < 0.0 { r_inner = 0.0; }
                D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: pad as f32 + shrink, top: pad as f32 + shrink, right: (pad + rect.width()) as f32 - shrink, bottom: (pad + rect.height()) as f32 - shrink }, radiusX: r_inner, radiusY: r_inner }
            };
            let outer_geom1 = match factory.CreateRoundedRectangleGeometry(&outer_rr) { Ok(g) => g, Err(_) => { return; } };
            let inner_geom1 = match factory.CreateRoundedRectangleGeometry(&inner_rr) { Ok(g) => g, Err(_) => { return; } };
            let geoms_vec: [Option<ID2D1Geometry>; 2] = [Some(outer_geom1.cast().unwrap()), Some(inner_geom1.cast().unwrap())];
            if let Ok(group) = factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &geoms_vec) {
                let mut comps = color.components; comps[3] *= 0.9;
                let col = D2D1_COLOR_F { r: comps[0] as f32, g: comps[1] as f32, b: comps[2] as f32, a: comps[3] as f32 };
                if let Ok(ring_brush) = temp_ctx.CreateSolidColorBrush(&col, None) {
                    temp_ctx.FillGeometry(&group, &ring_brush, None);
                }
            }
            let _ = temp_ctx.EndDraw(None, None);
            // Blur ring using main context effect
            let effect = if let Some(e) = &self.gaussian_blur_effect { e.clone() } else { match ctx.CreateEffect(&CLSID_D2D1GaussianBlur) { Ok(e) => { self.gaussian_blur_effect = Some(e.clone()); e }, Err(_) => { return; } } };
            // Provide ring bitmap to blur effect
            let _ = effect.SetInput(0, &off_bmp, true);
            let sigma = std_dev as f32;
            let sigma_bytes = std::slice::from_raw_parts((&sigma) as *const f32 as *const u8, std::mem::size_of::<f32>());
            let _ = effect.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, sigma_bytes);
            let border_mode = D2D1_BORDER_MODE_SOFT; let border_u32: u32 = border_mode.0 as u32;
            let border_bytes = std::slice::from_raw_parts((&border_u32) as *const u32 as *const u8, std::mem::size_of::<u32>());
            let _ = effect.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_UINT32, border_bytes);
            // Clip to element rect and draw
            let clip = D2D_RECT_F { left: rect.x0 as f32, top: rect.y0 as f32, right: rect.x1 as f32, bottom: rect.y1 as f32 };
            ctx.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            if let Ok(effect_img) = effect.cast::<ID2D1Image>() {
                let offset = D2D_POINT_2F { x: (rect.x0 - pad) as f32, y: (rect.y0 - pad) as f32 };
                ctx.DrawImage(&effect_img, Some(&offset), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            }
            ctx.PopAxisAlignedClip();
            debug_log_d2d(&format!("draw_inset_gaussian_box_shadow: drew inset ring rect=({}, {}, {}, {}) radius={} sd={} pad={} ring_thickness={}", rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev, pad, ring_thickness));
            debug_log_d2d("draw_inset_gaussian_box_shadow: end");
        }
    }

    fn blit_cached_shadow(&self, ctx: &ID2D1DeviceContext, bmp: &ID2D1Bitmap1, rect: &Rect, pad: f32) {
        unsafe {
            let sz = bmp.GetSize();
            let dest = D2D_RECT_F { left: (rect.x0 - pad as f64) as f32, top: (rect.y0 - pad as f64) as f32, right: (rect.x0 - pad as f64) as f32 + sz.width, bottom: (rect.y0 - pad as f64) as f32 + sz.height };
            ctx.DrawBitmap(bmp, Some(&dest), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, None, None);
        }
    }

    fn insert_shadow_cache(&mut self, key: ShadowKey, bmp: ID2D1Bitmap1) {
        const MAX_SHADOWS: usize = 64;
        if self.shadow_cache.contains_key(&key) { return; }
        if self.shadow_cache_order.len() >= MAX_SHADOWS {
            if let Some(old) = self.shadow_cache_order.pop_front() { self.shadow_cache.remove(&old); }
        }
        self.shadow_cache_order.push_back(key);
        self.shadow_cache.insert(key, bmp);
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
