use anyrender::WindowRenderer as _;
use anyrender_d2d::D2DWindowRenderer;
use blitz_dom::{Document, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::bindings::ISwapChainAttacher;
use windows::core::{IInspectable, Interface};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    ID3D11Resource, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
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
}

impl BlitzHost {
    pub fn new_for_swapchain(_panel: crate::SwapChainPanelHandle, width: u32, height: u32, scale: f32) -> Result<Self, String> {
        // No HWND usage in WinUI path. We strictly render into the provided SwapChainPanel swapchain.

        // Minimal HTML doc placeholder; host can replace by calling load_html.
        let mut doc = HtmlDocument::from_html(
            "<html><body><h1>Blitz WinUI host</h1><p>Initialize succeeded.</p></body></html>",
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
            attacher: None 
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
    debug_log("create_and_attach_swapchain: entering");
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
            // Create D3D11 device/context
            let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut chosen: D3D_FEATURE_LEVEL = D3D_FEATURE_LEVEL_11_0;
            debug_log(&format!(
                "create_and_attach_swapchain: Calling D3D11CreateDevice feature_level_count={} (debug build: {})",
                feature_levels.len(),
                cfg!(debug_assertions)
            ));
            // Try with debug layer in debug builds; fallback without if it fails (e.g. Graphics Tools not installed).
            let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            #[cfg(debug_assertions)]
            { flags |= D3D11_CREATE_DEVICE_DEBUG; }

            let mut try_create = |flags| {
                // windows-rs 0.58 signature:
                // D3D11CreateDevice(padapter, drivertype, software, flags,
                //   pfeaturelevels: Option<&[D3D_FEATURE_LEVEL]>, sdkversion: u32,
                //   ppdevice: Option<*mut Option<ID3D11Device>>,
                //   pfeaturelevel: Option<*mut D3D_FEATURE_LEVEL>,   <== NOTE: windows 0.58 expects Option here
                //   ppimmediatecontext: Option<*mut Option<ID3D11DeviceContext>>)
                // Differences vs native: slice + implicit count; Option wrappers for ALL out params including feature level.
                // IMPORTANT: Do NOT remove the Some(...) wrappers around device, chosen feature level, or context.
                // Previous mistakes removed Some(&mut chosen) causing E0308 (expected Option<*mut D3D_FEATURE_LEVEL>).
                // NOTE: No nested `unsafe {}` here; we are already inside an outer unsafe block.
                // Adding another unsafe block would trigger `unused_unsafe` warning.
                D3D11CreateDevice(
                    None,                              // adapter
                    D3D_DRIVER_TYPE_HARDWARE,          // driver type
                    None,                              // software module
                    flags,                             // flags
                    Some(&feature_levels),             // feature level candidates
                    D3D11_SDK_VERSION,                 // sdk version constant
                    Some(&mut device),                 // out: device (Option wrapper required)
                    Some(&mut chosen),                 // out: chosen feature level (MUST stay wrapped in Some)
                    Some(&mut context),                // out: immediate context (Option wrapper required)
                )
            };

            let hr = try_create(flags);
            if hr.is_err() {
                debug_log(&format!("create_and_attach_swapchain: D3D11CreateDevice initial attempt failed (flags={:?}) hr={:?}", flags, hr));
                #[cfg(debug_assertions)]
                {
                    if (flags & D3D11_CREATE_DEVICE_DEBUG) == D3D11_CREATE_DEVICE_DEBUG {
                        let fallback_flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT; // drop debug
                        debug_log("create_and_attach_swapchain: retrying D3D11CreateDevice without DEBUG layer");
                        let hr2 = try_create(fallback_flags);
                        if hr2.is_err() {
                            debug_log(&format!("create_and_attach_swapchain: D3D11CreateDevice fallback failed hr={:?}", hr2));
                            return;
                        }
                    } else {
                        return;
                    }
                }
                #[cfg(not(debug_assertions))]
                { return; }
            }
            let device = device.unwrap();
            let context = context.unwrap();
            debug_log(&format!("create_and_attach_swapchain: D3D11 device created (feature level {:?})", chosen));
            debug_log("create_and_attach_swapchain: D3D11 device created successfully");

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
                AlphaMode: windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_PREMULTIPLIED, // correct enum value for premultiplied
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

            // This is the critical part - getting the raw pointer correctly
            // 1. First clone to ensure we have a separate COM reference
            let sc_ptr = sc.clone();
            
            // 2. Get the raw pointer from the interface
            let raw_ptr = windows::core::Interface::as_raw(&sc_ptr);
            debug_log(&format!("create_and_attach_swapchain: Raw COM pointer: {:?}", raw_ptr));
            
            // 3. Convert to u64 for passing through WinRT boundary
            let ptr_u64 = raw_ptr as usize as u64;
            debug_log(&format!("create_and_attach_swapchain: Converted to u64: 0x{:X}", ptr_u64));
            
            // 4. Call the attacher with the real pointer
            debug_log("create_and_attach_swapchain: Calling AttachSwapChain with real pointer...");
            let result = attacher.AttachSwapChain(ptr_u64);
            
            // 5. Check result
            match result {
                Ok(_) => debug_log("create_and_attach_swapchain: AttachSwapChain call succeeded"),
                Err(e) => debug_log(&format!("create_and_attach_swapchain: AttachSwapChain call failed: {:?}", e)),
            }

            // Store for rendering
            self.d3d_device = Some(device);
            self.d3d_context = Some(context);
            self.swapchain = Some(sc);
            debug_log("create_and_attach_swapchain: Successfully stored device, context, and swapchain");
            if let Some(sc_ref) = &self.swapchain { self.renderer.set_swapchain(sc_ref.clone(), width, height); }
            
            // Ensure our renderer matches the current size
            self.renderer.set_size(width, height);
            
            // Test the rendering path immediately
            debug_log("create_and_attach_swapchain: Testing immediate render path");
            self.render_once();
        }
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
            let _ = unsafe { sc.ResizeBuffers(0, width, height, DXGI_FORMAT(28), windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG(0)) };
        }
    }

    pub fn render_once(&mut self) {
    debug_log("render_once: Starting rendering...");
        let (width, height) = self.doc.viewport().window_size;
        let scale = self.doc.viewport().scale_f64();
        self.doc.resolve();

        // Lazy attach fallback: if we have an attacher but no swapchain yet, attempt creation now.
        if self.swapchain.is_none() && self.attacher.is_some() {
            debug_log("render_once: No swapchain yet; attempting lazy creation");
            self.create_and_attach_swapchain();
        }
        
    // If we have a panel-backed swapchain, render via Vello (GPU) into an intermediate texture,
    // then upload/copy into the D3D11 backbuffer.
        if let Some(sc) = &self.swapchain {
            debug_log("render_once: Found swapchain, attempting to render");
            unsafe {
                // Get back buffer
                match sc.GetBuffer::<ID3D11Texture2D>(0) {
                    Ok(tex) => {
                        debug_log("render_once: Successfully got back buffer texture");
                        
                        // If we don't have a context yet, derive device/context from the texture
                        if self.d3d_context.is_none() {
                            debug_log("render_once: Need to get D3D context from texture");
                            let res: &ID3D11Resource = (&tex).into();
                            match res.GetDevice() {
                                Ok(device) => {
                                    match device.GetImmediateContext() {
                                        Ok(ctx) => {
                                            debug_log("render_once: Successfully got device and context from texture");
                                            self.d3d_device = Some(device);
                                            self.d3d_context = Some(ctx);
                                        },
                                        Err(e) => debug_log(&format!("render_once: Failed to get immediate context: {:?}", e)),
                                    }
                                },
                                Err(e) => debug_log(&format!("render_once: Failed to get device from resource: {:?}", e)),
                            }
                        }
                        
                        if self.d3d_context.is_none() {
                            debug_log("render_once: No D3D context available, can't render");
                            return; // can't render without a context
                        }
                        
                        let _ctx = self.d3d_context.as_ref().unwrap();
                        let (w, h) = (width.max(1), height.max(1));

                        // Render HTML scene with Vello to its intermediate GPU texture
                        // Set swapchain into D2D renderer (only once)
                        if self.d3d_device.is_some() && self.swapchain.is_some() {
                            // Already set
                        }
                        // Render via D2D backend directly into the backbuffer
                        self.renderer.render(|scene| paint_scene(scene, &self.doc, scale, w, h));
                    },
                    Err(e) => debug_log(&format!("render_once: Failed to get back buffer: {:?}", e)),
                }
                
                // Present the swapchain
                let hr = sc.Present(1, DXGI_PRESENT(0));
                if hr.is_ok() { debug_log("render_once: Successfully presented swapchain"); }
                else { debug_log(&format!("render_once: Failed to present swapchain: {:?}", hr)); }
            }
            return;
        } else {
            debug_log("render_once: No swapchain found, trying fallback renderer");
        }

        // Fallback to anyrender/vello window path if active (when HWND path is used)
        self.renderer
            .render(|scene| paint_scene(scene, &self.doc, scale, width, height));
    }

    pub fn load_html(&mut self, html: &str) {
        let cfg = DocumentConfig::default();
        let new_doc = HtmlDocument::from_html(html, cfg);
        let scroll = self.doc.viewport_scroll();
        let viewport = self.doc.viewport().clone();
        self.doc = Box::new(new_doc);
        self.doc.set_viewport(viewport);
        self.doc.set_viewport_scroll(scroll);
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
    }

    pub fn wheel_scroll(&mut self, dx: f64, dy: f64) {
        if let Some(hover_node_id) = self.doc.get_hover_node_id() {
            self.doc.scroll_node_by(hover_node_id, dx, dy);
        } else {
            self.doc.scroll_viewport_by(dx, dy);
        }
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
