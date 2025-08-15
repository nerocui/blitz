use anyrender::WindowRenderer as _;
use anyrender_d2d::D2DWindowRenderer;
use blitz_dom::{Document, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::bindings::ISwapChainAttacher;
use windows::core::{IInspectable, Interface};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    ID3D11Resource,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIFactory2, IDXGISwapChain1, DXGI_CREATE_FACTORY_FLAGS,
    DXGI_SWAP_CHAIN_DESC1, DXGI_USAGE_RENDER_TARGET_OUTPUT, DXGI_PRESENT,
    DXGI_SCALING_STRETCH, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;
use windows::core::PCSTR;

pub(crate) fn debug_log(msg: &str) {
    // Append newline for readability in DebugView.
    let mut bytes = msg.as_bytes().to_vec();
    if !bytes.ends_with(b"\n") { bytes.push(b'\n'); }
    bytes.push(0); // null terminator
    unsafe { OutputDebugStringA(PCSTR(bytes.as_ptr())); }
}

// Use generated ISwapChainAttacher from bindings.rs

/// Public host object backing the WinRT class. Keeps the document and renderer alive and exposes
/// methods called from C# to drive rendering and input.
pub struct BlitzHost {
    renderer: D2DWindowRenderer,
    doc: Box<dyn Document>,
    // Staging buffer for temporary CPU uploads (to bridge wgpu texture to D3D11 backbuffer)
    // TODO: Enable when implementing CPU-GPU texture bridge
    // cpu_staging: Vec<u8>,
    // SwapChainPanel interop (temporary D3D11 path until wgpu surface is implemented)
    d3d_device: Option<ID3D11Device>,
    d3d_context: Option<ID3D11DeviceContext>,
    swapchain: Option<IDXGISwapChain1>,
    attacher: Option<ISwapChainAttacher>,
    // Rendering control
    content_loaded: bool,
    // simple frame invalidation flag (best-effort; we still allow forced render)
    needs_render: bool,
    // If real content loaded before swapchain is ready, defer starting initial measurement until activation
    pending_content_measurement: bool,
    // Async panel attach workflow
    host_init_start: Option<std::time::Instant>,
    pending_swapchain: Option<IDXGISwapChain1>,
    // Queued attach timing
    attach_queue_start: Option<std::time::Instant>,
    attach_pending: bool,
    // First-frame placeholder control: ensure we never leave the panel transparent; draw exactly one placeholder frame if real content not yet loaded when attach completes.
    placeholder_drawn: bool,
}

impl BlitzHost {
    pub fn new_for_swapchain(_panel: crate::SwapChainPanelHandle, width: u32, height: u32, scale: f32) -> Result<Self, String> {
        // No HWND usage in WinUI path. We strictly render into the provided SwapChainPanel swapchain.

        // Minimal HTML doc placeholder; host can replace by calling load_html.
        // Start with an empty document so we don't flash placeholder content before real HTML loads.
        let mut doc = HtmlDocument::from_html(
            "<html><head></head><body style=\"margin:0;padding:0;background:transparent;\"></body></html>",
            DocumentConfig::default(),
        );

        // Initialize viewport so first swapchain uses real size instead of 1x1.
        let viewport = Viewport::new(width.max(1), height.max(1), scale, ColorScheme::Light);
        doc.set_viewport(viewport);

        let renderer = D2DWindowRenderer::new();
        Ok(Self { 
            renderer, 
            doc: Box::new(doc), 
            // cpu_staging: Vec::new(), // TODO: Enable when implementing CPU-GPU texture bridge
            d3d_device: None, 
            d3d_context: None, 
            swapchain: None, 
            attacher: None,
            content_loaded: false,
            needs_render: false,
            pending_content_measurement: false,
            host_init_start: None,
            pending_swapchain: None,
            attach_queue_start: None,
            attach_pending: false,
            placeholder_drawn: false,
        })
    }
    
    // New method that takes an attacher directly
    pub fn new_with_attacher(attacher: ISwapChainAttacher, width: u32, height: u32, scale: f32) -> Result<Self, String> {
        let mut host = Self::new_for_swapchain(crate::SwapChainPanelHandle { swapchain: 0 }, width, height, scale)?;
        host.attacher = Some(attacher);
        host.create_and_attach_swapchain();
        Ok(host)
    }
    
    // Method to get a reference to the attacher
    pub fn get_attacher(&self) -> Option<ISwapChainAttacher> {
        self.attacher.clone()
    }

    // Temporary mutable access for instrumentation augmentation; keep internal
    fn renderer_mut(&mut self) -> Option<&mut anyrender_d2d::D2DWindowRenderer> { Some(&mut self.renderer) }

    pub fn set_verbose_logging(&mut self, enabled: bool) {
        anyrender_d2d::set_verbose_logging(enabled);
        debug_log(&format!("SetVerboseLogging: enabled={}", enabled));
    }

    pub fn set_debug_overlay(&mut self, enabled: bool) {
        if let Some(r) = self.renderer_mut() { r.set_debug_overlay(enabled); }
        debug_log(&format!("SetDebugOverlay: enabled={}", enabled));
    }

    // SwapChainPanel interop: detect if the provided Object is an attacher callback; if so, store it and, if possible, create and attach swapchain now.
    pub fn set_panel(&mut self, panel: windows_core::Ref<'_, IInspectable>, _width: u32, _height: u32) {
        // Try casting to our attacher interface
        if let Some(insp) = panel.as_ref() {
        debug_log(&format!("set_panel: received panel object: {:?}", insp));
            match insp.cast::<ISwapChainAttacher>() {
                Ok(att) => {
            debug_log("set_panel: successfully cast to ISwapChainAttacher");
                    self.attacher = Some(att);
                    // Always create and attach the swapchain when we get an attacher
                    self.create_and_attach_swapchain();
                }
                Err(e) => {
            debug_log(&format!("set_panel: failed to cast to ISwapChainAttacher: {:?}", e));
                }
            }
        } else {
        debug_log("set_panel: no panel object received");
        }
    }

    fn create_and_attach_swapchain(&mut self) {
    debug_log("create_and_attach_swapchain: entering (async queued mode)");
    let host_t0 = std::time::Instant::now();
    self.host_init_start = Some(host_t0);
    let mut t_phase = host_t0;
        // Need an attacher to complete the hookup
        let attacher = match &self.attacher { 
            Some(a) => {
                debug_log("create_and_attach_swapchain: attacher found");
                a.clone()
            }, 
            None => {
                debug_log("create_and_attach_swapchain: no attacher available");
                return;
            } 
        };
        
        // First test the connection without a real pointer
        debug_log("create_and_attach_swapchain: Testing attacher connection...");
        match attacher.TestAttacherConnection() {
            Ok(true) => debug_log("create_and_attach_swapchain: TestAttacherConnection succeeded"),
            Ok(false) => debug_log("create_and_attach_swapchain: TestAttacherConnection returned false"),
            Err(e) => debug_log(&format!("create_and_attach_swapchain: TestAttacherConnection failed: {:?}", e)),
        }
        
        // Use current viewport size
        let (width, height) = self.doc.viewport().window_size;
        let width = width.max(1);
        let height = height.max(1);
    debug_log(&format!("create_and_attach_swapchain: using size {}x{}", width, height));
        unsafe {
            let acquire = crate::global_gfx::get_or_create_d3d_device();
            if acquire.is_none() { debug_log("create_and_attach_swapchain: failed to acquire global device"); return; }
            let acquire = acquire.unwrap();
            let device = acquire.device.clone();
            let context = acquire.context.clone();
            if acquire.created {
                let d3d_elapsed = t_phase.elapsed().as_secs_f32()*1000.0; t_phase = std::time::Instant::now();
                if let Some(r) = self.renderer_mut() { r.add_host_dxgi_d3d_ms(d3d_elapsed); }
                debug_log(&format!("create_and_attach_swapchain: created shared D3D device (feature {:?}) d3d_ms={:.2}", acquire.feature_level, d3d_elapsed));
            } else {
                debug_log(&format!("create_and_attach_swapchain: reused shared D3D device (feature {:?})", acquire.feature_level));
            }

            // Create swapchain for composition
            let factory: IDXGIFactory2 = match CreateDXGIFactory2::<IDXGIFactory2>(DXGI_CREATE_FACTORY_FLAGS(0)) {
                Ok(f) => {
                    debug_log("create_and_attach_swapchain: Created DXGI factory");
                    f
                },
                Err(e) => {
                    debug_log(&format!("create_and_attach_swapchain: CreateDXGIFactory2 failed: {:?}", e));
                    return;
                },
            };
            
            // Create a more robust swap chain for SwapChainPanel
            // Primary descriptor (premultiplied alpha, flip-sequential)
            let mut desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                // Use IGNORE initially to guarantee opaque composition (avoid transparent first frame issues in Release)
                AlphaMode: windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };
            debug_log(&format!(
                "create_and_attach_swapchain: Attempting swapchain ({}x{}, fmt={:?}, swap_effect={:?}, alpha={:?}, buffers={}, usage=0x{:X})",
                desc.Width, desc.Height, desc.Format, desc.SwapEffect, desc.AlphaMode, desc.BufferCount, desc.BufferUsage.0
            ));
            let mut sc_attempt: Option<IDXGISwapChain1> = match factory.CreateSwapChainForComposition(&device, &desc, None) {
                Ok(s) => Some(s),
                Err(e) => {
                    debug_log(&format!("create_and_attach_swapchain: initial CreateSwapChainForComposition failed: {:?}", e));
                    None
                }
            };

            if sc_attempt.is_none() {
                // Fallback 1: straight alpha
                desc.AlphaMode = windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_STRAIGHT;
                debug_log(&format!("create_and_attach_swapchain: retry with STRAIGHT alpha (alpha={:?})", desc.AlphaMode));
                sc_attempt = match factory.CreateSwapChainForComposition(&device, &desc, None) {
                    Ok(s) => Some(s),
                    Err(e) => { debug_log(&format!("fallback1 failed: {:?}", e)); None }
                };
            }
            if sc_attempt.is_none() {
                // Fallback 2: ignore alpha (opaque)
                desc.AlphaMode = windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_IGNORE;
                debug_log(&format!("create_and_attach_swapchain: retry with IGNORE alpha (alpha={:?})", desc.AlphaMode));
                sc_attempt = match factory.CreateSwapChainForComposition(&device, &desc, None) {
                    Ok(s) => Some(s),
                    Err(e) => { debug_log(&format!("fallback2 failed: {:?}", e)); None }
                };
            }
            if sc_attempt.is_none() {
                // Fallback 3: change swap effect to FLIP_DISCARD
                desc.SwapEffect = windows::Win32::Graphics::Dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD;
                desc.AlphaMode = windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_PREMULTIPLIED; // reset to premultiplied
                debug_log(&format!("create_and_attach_swapchain: retry with FLIP_DISCARD (swap_effect={:?}, alpha={:?})", desc.SwapEffect, desc.AlphaMode));
                sc_attempt = match factory.CreateSwapChainForComposition(&device, &desc, None) {
                    Ok(s) => Some(s),
                    Err(e) => { debug_log(&format!("fallback3 failed: {:?}", e)); None }
                };
            }
            let sc: IDXGISwapChain1 = match sc_attempt {
                Some(s) => {
                    debug_log("create_and_attach_swapchain: Created swap chain successfully (after possible fallbacks)");
                    s
                },
                None => {
                    debug_log("create_and_attach_swapchain: All swapchain creation attempts failed");
                    return;
                }
            };
            let sc_elapsed = t_phase.elapsed().as_secs_f32()*1000.0; t_phase = std::time::Instant::now();
            if let Some(r) = self.renderer_mut() { r.add_host_swapchain_ms(sc_elapsed); }
            debug_log(&format!("create_and_attach_swapchain: swapchain_ms={:.2}", sc_elapsed));

            // This is the critical part - getting the raw pointer correctly
            // 1. First clone to ensure we have a separate COM reference
            let sc_ptr = sc.clone();
            
            // 2. Get the raw pointer from the interface
            let raw_ptr = windows::core::Interface::as_raw(&sc_ptr);
            debug_log(&format!("create_and_attach_swapchain: Raw COM pointer: {:?}", raw_ptr));
            
            // 3. Convert to u64 for passing through WinRT boundary
            let ptr_u64 = raw_ptr as usize as u64;
            debug_log(&format!("create_and_attach_swapchain: Converted to u64: 0x{:X}", ptr_u64));
            
            // Store device + context now (these are immediately usable for layout text metrics etc.)
            self.d3d_device = Some(device);
            self.d3d_context = Some(context);
            self.pending_swapchain = Some(sc);
            self.renderer.set_size(width, height);
            // Mark attach as pending; actual AttachSwapChain will execute later (e.g. at next render/poll)
            self.attach_queue_start = Some(std::time::Instant::now());
            self.attach_pending = true;
            debug_log("create_and_attach_swapchain: queued panel AttachSwapChain (executing immediately to minimize wait)");
            // Execute immediately to keep queue_ms near-zero for better overlap accounting
            self.maybe_execute_queued_attach();
        }
    }

    // Execute pending panel attach when appropriate (first poll after queue) measuring queue vs exec time.
    fn maybe_execute_queued_attach(&mut self) {
        if !self.attach_pending { return; }
        if self.swapchain.is_some() { self.attach_pending = false; return; }
        // Safe to proceed now; measure queue_ms
        let queue_ms = self.attach_queue_start.map(|t| t.elapsed().as_secs_f32()*1000.0).unwrap_or(0.0);
        // Perform the real attach now
        let Some(attacher) = self.attacher.clone() else { debug_log("maybe_execute_queued_attach: no attacher (aborting)" ); self.attach_pending = false; return; };
        let Some(sc) = self.pending_swapchain.take() else { debug_log("maybe_execute_queued_attach: no pending swapchain" ); self.attach_pending = false; return; };
        // Recreate raw pointer for swapchain (COM pointer still valid)
        let raw_ptr = windows::core::Interface::as_raw(&sc) as usize as u64;
        let exec_start = std::time::Instant::now();
        let result = attacher.AttachSwapChain(raw_ptr);
        let exec_ms = exec_start.elapsed().as_secs_f32()*1000.0;
        if let Some(r) = self.renderer_mut() { r.add_host_panel_attach_queue_ms(queue_ms); r.add_host_panel_attach_exec_ms(exec_ms); }
        match result {
            Ok(_) => debug_log(&format!("maybe_execute_queued_attach: AttachSwapChain succeeded queue_ms={:.2} exec_ms={:.2}", queue_ms, exec_ms)),
            Err(e) => { debug_log(&format!("maybe_execute_queued_attach: AttachSwapChain failed queue_ms={:.2} exec_ms={:.2} err={:?}", queue_ms, exec_ms, e)); }
        }
        // Finalize swapchain into renderer
        let (w,h) = self.doc.viewport().window_size;
        self.renderer.set_swapchain(sc.clone(), w.max(1), h.max(1));
        self.swapchain = Some(sc);
        // Accumulate host init total after full attach completes, excluding queue wait (we only want non-overlapped exec + prior setup)
        if let Some(start) = self.host_init_start.take() {
            let total_elapsed = start.elapsed().as_secs_f32()*1000.0;
            let effective = (total_elapsed - queue_ms).max(0.0);
            if self.content_loaded && self.pending_content_measurement { 
                self.renderer.restart_initial_measurement();
                self.pending_content_measurement = false;
            }
            self.renderer.accumulate_host_init_ms(effective);
        }
        // If content already available, render immediately (first real frame). Otherwise draw a single placeholder frame if we have not yet.
        if self.content_loaded || !self.placeholder_drawn {
            // For real content OR first placeholder we schedule a render
            self.needs_render = true;
            self.render_once();
        }
        self.attach_pending = false;
    }

    // TODO: Enable when implementing CPU-GPU texture bridge
    // fn ensure_staging_capacity(&mut self, width: u32, height: u32) {
    //     let need = (width.max(1) * height.max(1) * 4) as usize;
    //     if self.cpu_staging.len() < need { self.cpu_staging.resize(need, 0); }
    // }

    // Alternative interop: host passes an already-created IDXGISwapChain1* pointer.
    // Safety: swapchain_ptr must be a valid, AddRef'd IDXGISwapChain1 pointer. We take ownership of a reference.
    pub fn set_swapchain(&mut self, swapchain_ptr: *mut core::ffi::c_void, width: u32, height: u32, scale: f32) {
        let _ = scale;
        if swapchain_ptr.is_null() { return; }
        unsafe {
            // Rebuild COM interface from raw pointer without transferring ownership (we take one ref).
            let sc: IDXGISwapChain1 = Interface::from_raw(swapchain_ptr);
            // Store swapchain and reset D3D device/context for render path that just clears/presents
            self.swapchain = Some(sc);
            // Update viewport and renderer size
            let viewport = Viewport::new(width, height, scale, ColorScheme::Light);
            self.doc.set_viewport(viewport);
            self.renderer.set_size(width, height);
            // Try an immediate resize to desired size in case buffers differ
            if let Some(sc) = &self.swapchain {
                let _ = sc.ResizeBuffers(0, width, height, DXGI_FORMAT(28), windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG(0));
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
    let viewport = Viewport::new(width, height, scale, ColorScheme::Light);
    self.doc.set_viewport(viewport);
    // Keep renderer sized to viewport; no window-handle surface used here
    self.renderer.set_size(width.max(1), height.max(1));
        // Resize DXGI swapchain if present
        if let Some(sc) = &self.swapchain {
            // Release all references (target + cached bitmap) so ResizeBuffers can succeed.
            self.renderer.release_backbuffer_resources();
            let mut hr = unsafe { sc.ResizeBuffers(0, width, height, DXGI_FORMAT(28), windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG(0)) };
            if !hr.is_ok() {
                debug_log(&format!("resize: first ResizeBuffers attempt failed hr={:?} ({}x{}); retrying after forced release", hr, width, height));
                // Extra safety: ensure target cleared again before retry
                self.renderer.release_backbuffer_resources();
                hr = unsafe { sc.ResizeBuffers(0, width, height, DXGI_FORMAT(28), windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG(0)) };
            }
            if hr.is_ok() { debug_log(&format!("resize: swapchain ResizeBuffers ok ({}x{})", width, height)); }
            else { debug_log(&format!("resize: ResizeBuffers failed hr={:?} ({}x{})", hr, width, height)); }
        }
    // Mark for redraw (layout may depend on viewport size)
    self.needs_render = true;
    // Eagerly render once to avoid blank gap after resize
    if self.content_loaded { self.render_once(); }
    }

    pub fn render_once(&mut self) {
        // Execute pending attach if any first
        self.maybe_execute_queued_attach();
        if !self.content_loaded && !self.needs_render { return; }
        if self.content_loaded && !self.needs_render { return; }
        debug_log(&format!("render_once: begin (dirty={}, content_loaded={})", self.needs_render, self.content_loaded));
        let (width, height) = self.doc.viewport().window_size;
        let scale = self.doc.viewport().scale_f64();
        if self.content_loaded { self.doc.resolve(); }

        if self.swapchain.is_none() && self.attacher.is_some() {
            debug_log("render_once: No swapchain yet; attempting lazy creation");
            self.create_and_attach_swapchain();
        }

    // Clone swapchain COM pointer out to avoid holding an immutable borrow of self during rendering
    if let Some(sc) = self.swapchain.clone() {
            let mut want_enable_test_pattern = false;
            let mut want_disable_test_pattern = false;
            if self.content_loaded { debug_log("render_once: Found swapchain, attempting to render"); }
            else { debug_log("render_once: Found swapchain, rendering placeholder (no content yet)"); }
            unsafe {
        match sc.GetBuffer::<ID3D11Texture2D>(0) {
                    Ok(tex) => {
                        if self.d3d_context.is_none() {
                            let res: &ID3D11Resource = (&tex).into();
                            if let Ok(device) = res.GetDevice() {
                                if let Ok(ctx) = device.GetImmediateContext() {
                                    self.d3d_device = Some(device);
                                    self.d3d_context = Some(ctx);
                                }
                            }
                        }
                        if self.d3d_context.is_none() { debug_log("render_once: No D3D context available"); return; }
                        let (w,h) = (width.max(1), height.max(1));
                        if self.content_loaded {
                            want_disable_test_pattern = true;
                            self.renderer.render(|scene| paint_scene(scene, &self.doc, scale, w, h));
                            debug_log(&format!("render_once: D2D command_count={} ({}x{})", self.renderer.last_command_count(), w, h));
                        } else if !self.placeholder_drawn {
                            want_enable_test_pattern = true;
                            self.renderer.render(|_scene| { /* placeholder test pattern */ });
                            self.placeholder_drawn = true;
                            debug_log("render_once: placeholder frame rendered (no content, test pattern)");
                        }
                    },
                    Err(e) => debug_log(&format!("render_once: Failed to get back buffer: {:?}", e)),
                }
                let sync_interval = if (!self.content_loaded && self.placeholder_drawn) || (self.content_loaded && self.placeholder_drawn) { 0 } else { 1 };
                let hr = sc.Present(sync_interval, DXGI_PRESENT(0));
                if hr.is_ok() { debug_log("render_once: presented"); } else { debug_log(&format!("render_once: Failed to present swapchain: {:?}", hr)); }
    }
    if want_enable_test_pattern { if let Some(r) = self.renderer_mut() { r.set_test_pattern(true); } }
    if want_disable_test_pattern { if let Some(r) = self.renderer_mut() { r.set_test_pattern(false); } }
    if self.content_loaded { self.needs_render = false; }
    return;
    }

        // Fallback path (should not normally trigger in WinUI panel scenario)
        if self.content_loaded {
            self.renderer.render(|scene| paint_scene(scene, &self.doc, scale, width, height));
            debug_log(&format!("render_once: D2D command_count={} (fallback path)", self.renderer.last_command_count()));
            self.needs_render = false;
        } else if !self.placeholder_drawn {
            self.renderer.render(|_scene| { /* placeholder fallback */ });
            self.placeholder_drawn = true;
            debug_log("render_once: placeholder frame rendered (fallback path, no content)");
        }
    }

    pub fn load_html(&mut self, html: &str) {
    // If swapchain active, restart initial metrics now so timings reflect real document; else defer until swapchain creation
    let swapchain_ready = self.swapchain.is_some();
    if swapchain_ready { self.renderer.restart_initial_measurement(); } else { self.pending_content_measurement = true; }
    let cfg = DocumentConfig::default();
    let new_doc = HtmlDocument::from_html(html, cfg);
        let scroll = self.doc.viewport_scroll();
        let viewport = self.doc.viewport().clone();
        self.doc = Box::new(new_doc);
        self.doc.set_viewport(viewport);
        self.doc.set_viewport_scroll(scroll);
    // Perform initial style/layout/shaping before first real frame so metrics capture them
    self.doc.resolve();
    debug_log(&format!("load_html: new document length={} chars", html.len()));
    self.content_loaded = true;
    if swapchain_ready {
        self.needs_render = true; // schedule first real paint now
        self.render_once();
    } else {
        // Will render automatically when swapchain attaches
        debug_log("load_html: swapchain not yet ready; deferring initial measurement start until attach");
    }
    }

    // Input bridging (to be called from C# event handlers)
    pub fn pointer_move(&mut self, x: f32, y: f32, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButtons, UiEvent};
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
    self.doc.handle_ui_event(UiEvent::MouseMove(BlitzMouseButtonEvent {
            x,
            y,
            button: Default::default(),
            buttons,
            mods,
        }));
    self.needs_render = true; // hover/scroll effects etc.
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButton, MouseEventButtons, UiEvent};
        let btn = match button {
            0 => MouseEventButton::Main,
            1 => MouseEventButton::Auxiliary,
            2 => MouseEventButton::Secondary,
            3 => MouseEventButton::Fourth,
            4 => MouseEventButton::Fifth,
            _ => MouseEventButton::Main,
        };
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
    self.doc.handle_ui_event(UiEvent::MouseDown(BlitzMouseButtonEvent {
            x,
            y,
            button: btn,
            buttons,
            mods,
        }));
    self.needs_render = true;
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButton, MouseEventButtons, UiEvent};
        let btn = match button {
            0 => MouseEventButton::Main,
            1 => MouseEventButton::Auxiliary,
            2 => MouseEventButton::Secondary,
            3 => MouseEventButton::Fourth,
            4 => MouseEventButton::Fifth,
            _ => MouseEventButton::Main,
        };
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
    self.doc.handle_ui_event(UiEvent::MouseUp(BlitzMouseButtonEvent {
            x,
            y,
            button: btn,
            buttons,
            mods,
        }));
    self.needs_render = true;
    }

    pub fn wheel_scroll(&mut self, dx: f64, dy: f64) {
        if let Some(hover_node_id) = self.doc.get_hover_node_id() {
            self.doc.scroll_node_by(hover_node_id, dx, dy);
        } else {
            self.doc.scroll_viewport_by(dx, dy);
        }
    self.needs_render = true;
    }

    pub fn key_down(&mut self, vk: u32, ch: u32, mods: u32, is_auto_repeating: bool) {
        use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
        let key = vk_or_char_to_key(vk, ch);
        let code = keyboard_types::Code::Unidentified;
        let modifiers = keyboard_types::Modifiers::from_bits_truncate(mods);
        let location = keyboard_types::Location::Standard;
        let text = char_from_u32(ch).map(|c| c.into());
        let evt = BlitzKeyEvent {
            key,
            code,
            modifiers,
            location,
            is_auto_repeating,
            is_composing: false,
            state: KeyState::Pressed,
            text,
        };
    self.doc.handle_ui_event(UiEvent::KeyDown(evt));
    self.needs_render = true;
    }

    pub fn key_up(&mut self, vk: u32, ch: u32, mods: u32) {
        use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
        let key = vk_or_char_to_key(vk, ch);
        let code = keyboard_types::Code::Unidentified;
        let modifiers = keyboard_types::Modifiers::from_bits_truncate(mods);
        let location = keyboard_types::Location::Standard;
        let text = char_from_u32(ch).map(|c| c.into());
        let evt = BlitzKeyEvent {
            key,
            code,
            modifiers,
            location,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Released,
            text,
        };
    self.doc.handle_ui_event(UiEvent::KeyUp(evt));
    self.needs_render = true;
    }

    // Receive sub-phase timing from C# attacher (kind codes: 1=UI add,2=SetSwapChain)
    pub fn report_attach_subphase(&mut self, kind: u8, ms: f32) {
        if let Some(r) = self.renderer_mut() {
            match kind {
                1 => r.add_host_panel_attach_sub_ui_add_ms(ms),
                2 => r.add_host_panel_attach_sub_set_swapchain_ms(ms),
                _ => {}
            }
        }
    }
}

fn char_from_u32(ch: u32) -> Option<String> {
    char::from_u32(ch).map(|c| c.to_string())
}

fn vk_or_char_to_key(vk: u32, ch: u32) -> keyboard_types::Key {
    use keyboard_types::Key;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    if let Some(s) = char_from_u32(ch) {
        return Key::Character(s);
    }
    let v = VIRTUAL_KEY(vk as u16);
    match v {
        VK_BACK => Key::Backspace,
        VK_TAB => Key::Tab,
        VK_RETURN => Key::Enter,
        VK_ESCAPE => Key::Escape,
        VK_SPACE => Key::Character(" ".into()),
        VK_LEFT => Key::ArrowLeft,
        VK_UP => Key::ArrowUp,
        VK_RIGHT => Key::ArrowRight,
        VK_DOWN => Key::ArrowDown,
        _ => Key::Unidentified,
    }
}
