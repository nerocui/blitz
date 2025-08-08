use std::sync::Arc;

use anyrender::WindowRenderer as _;
use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{Document, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::raw_handle::DxgiInteropHandle;
use crate::bindings::ISwapChainAttacher;
use windows::Win32::Foundation::HWND;
use windows::core::{IInspectable, Interface};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
    ID3D11Resource, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RENDER_TARGET_VIEW_DESC,
    D3D11_RTV_DIMENSION_TEXTURE2D, D3D11_TEX2D_RTV,
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
    DXGI_FORMAT, DXGI_SAMPLE_DESC, DXGI_ALPHA_MODE,
};

// Use generated ISwapChainAttacher from bindings.rs

/// Public host object backing the WinRT class. Keeps the document and renderer alive and exposes
/// methods called from C# to drive rendering and input.
pub struct BlitzHost {
    renderer: VelloWindowRenderer,
    window: Arc<dyn anyrender::WindowHandle>,
    doc: Box<dyn Document>,
    // SwapChainPanel interop (temporary D3D11 path until wgpu surface is implemented)
    d3d_device: Option<ID3D11Device>,
    d3d_context: Option<ID3D11DeviceContext>,
    swapchain: Option<IDXGISwapChain1>,
    attacher: Option<ISwapChainAttacher>,
}

impl BlitzHost {
    pub fn new_for_swapchain(_panel: crate::SwapChainPanelHandle, width: u32, height: u32, scale: f32) -> Result<Self, String> {
        let _ = scale;
        // TODO: use panel.swapchain to get HWND or a surface target. For now, assume we can extract HWND somehow.
        // Placeholder: require caller to call SetHwnd before rendering.
        let hwnd: Option<HWND> = None;
        let window: Arc<dyn anyrender::WindowHandle> = Arc::new(DxgiInteropHandle::from(HWND(core::ptr::null_mut())));

        // Minimal HTML doc placeholder; host can replace by calling load_html.
        let doc = HtmlDocument::from_html(
            "<html><body><h1>Blitz WinUI host</h1><p>Initialize succeeded.</p></body></html>",
            DocumentConfig::default(),
        );

        let mut renderer = VelloWindowRenderer::new();
        if let Some(hwnd) = hwnd {
            let win = Arc::new(DxgiInteropHandle::from(hwnd)) as Arc<dyn anyrender::WindowHandle>;
            renderer.resume(win, width, height);
        }

        Ok(Self { renderer, window, doc: Box::new(doc), d3d_device: None, d3d_context: None, swapchain: None, attacher: None })
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

    pub fn set_hwnd(&mut self, hwnd: isize, width: u32, height: u32) {
        // Create or re-create the wgpu surface against the new HWND
        let win = Arc::new(DxgiInteropHandle::from(hwnd)) as Arc<dyn anyrender::WindowHandle>;
        if self.renderer.is_active() {
            // suspend and resume on new window to recreate surface
            self.renderer.suspend();
        }
        self.renderer.resume(win.clone(), width, height);
        self.window = win;
    }

    // SwapChainPanel interop: detect if the provided Object is an attacher callback; if so, store it and, if possible, create and attach swapchain now.
    pub fn set_panel(&mut self, panel: windows_core::Ref<'_, IInspectable>, _width: u32, _height: u32) {
        // Try casting to our attacher interface
        if let Some(insp) = panel.as_ref() {
            println!("set_panel: received panel object: {:?}", insp);
            match insp.cast::<ISwapChainAttacher>() {
                Ok(att) => {
                    println!("set_panel: successfully cast to ISwapChainAttacher");
                    self.attacher = Some(att);
                    // Always create and attach the swapchain when we get an attacher
                    self.create_and_attach_swapchain();
                }
                Err(e) => {
                    println!("set_panel: failed to cast to ISwapChainAttacher: {:?}", e);
                }
            }
        } else {
            println!("set_panel: no panel object received");
        }
    }

    fn create_and_attach_swapchain(&mut self) {
        println!("create_and_attach_swapchain: entering");
        // Need an attacher to complete the hookup
        let attacher = match &self.attacher { 
            Some(a) => {
                println!("create_and_attach_swapchain: attacher found");
                a.clone()
            }, 
            None => {
                println!("create_and_attach_swapchain: no attacher available");
                return;
            } 
        };
        
        // First test the connection without a real pointer
        println!("create_and_attach_swapchain: Testing attacher connection...");
        match attacher.TestAttacherConnection() {
            Ok(true) => println!("create_and_attach_swapchain: TestAttacherConnection succeeded"),
            Ok(false) => println!("create_and_attach_swapchain: TestAttacherConnection returned false"),
            Err(e) => println!("create_and_attach_swapchain: TestAttacherConnection failed: {:?}", e),
        }
        
        // Use current viewport size
        let (width, height) = self.doc.viewport().window_size;
        let width = width.max(1);
        let height = height.max(1);
        println!("create_and_attach_swapchain: using size {}x{}", width, height);
        unsafe {
            // Create D3D11 device/context
            let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut chosen: D3D_FEATURE_LEVEL = D3D_FEATURE_LEVEL_11_0;
            let hr = D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&feature_levels),
                0,
                Some(&mut device),
                Some(&mut chosen),
                Some(&mut context),
            );
            if hr.is_err() { 
                println!("create_and_attach_swapchain: D3D11CreateDevice failed: {:?}", hr);
                return; 
            }
            let device = device.unwrap();
            let context = context.unwrap();
            println!("create_and_attach_swapchain: D3D11 device created successfully");

            // Create swapchain for composition
            let factory: IDXGIFactory2 = match CreateDXGIFactory2::<IDXGIFactory2>(DXGI_CREATE_FACTORY_FLAGS(0)) {
                Ok(f) => {
                    println!("create_and_attach_swapchain: Created DXGI factory");
                    f
                },
                Err(e) => {
                    println!("create_and_attach_swapchain: CreateDXGIFactory2 failed: {:?}", e);
                    return;
                },
            };
            
            // Create a more robust swap chain for SwapChainPanel
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT(87), // DXGI_FORMAT_B8G8R8A8_UNORM
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE(2), // DXGI_ALPHA_MODE_PREMULTIPLIED (2)
                Flags: 0,
            };
            println!("create_and_attach_swapchain: Creating swap chain with size {}x{}", width, height);
            let sc: IDXGISwapChain1 = match factory.CreateSwapChainForComposition(&device, &desc, None) {
                Ok(s) => {
                    println!("create_and_attach_swapchain: Created swap chain successfully");
                    s
                },
                Err(e) => {
                    println!("create_and_attach_swapchain: CreateSwapChainForComposition failed: {:?}", e);
                    return;
                },
            };

            // This is the critical part - getting the raw pointer correctly
            // 1. First clone to ensure we have a separate COM reference
            let sc_ptr = sc.clone();
            
            // 2. Get the raw pointer from the interface
            let raw_ptr = windows::core::Interface::as_raw(&sc_ptr);
            println!("create_and_attach_swapchain: Raw COM pointer: {:?}", raw_ptr);
            
            // 3. Convert to u64 for passing through WinRT boundary
            let ptr_u64 = raw_ptr as usize as u64;
            println!("create_and_attach_swapchain: Converted to u64: 0x{:X}", ptr_u64);
            
            // 4. Call the attacher with the real pointer
            println!("create_and_attach_swapchain: Calling AttachSwapChain with real pointer...");
            let result = attacher.AttachSwapChain(ptr_u64);
            
            // 5. Check result
            match result {
                Ok(_) => println!("create_and_attach_swapchain: AttachSwapChain call succeeded"),
                Err(e) => println!("create_and_attach_swapchain: AttachSwapChain call failed: {:?}", e),
            }

            // Store for rendering
            self.d3d_device = Some(device);
            self.d3d_context = Some(context);
            self.swapchain = Some(sc);
            println!("create_and_attach_swapchain: Successfully stored device, context, and swapchain");
            
            // Test the rendering path immediately
            println!("create_and_attach_swapchain: Testing immediate render path");
            self.render_once();
        }
    }

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
        self.renderer.set_size(width, height);
        // Resize DXGI swapchain if present
        if let Some(sc) = &self.swapchain {
            let _ = unsafe { sc.ResizeBuffers(0, width, height, DXGI_FORMAT(28), windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG(0)) };
        }
    }

    pub fn render_once(&mut self) {
        println!("render_once: Starting rendering...");
        let (width, height) = self.doc.viewport().window_size;
        let scale = self.doc.viewport().scale_f64();
        self.doc.resolve();
        // If we have a panel-backed swapchain, clear and present it (temporary path)
        if let Some(sc) = &self.swapchain {
            println!("render_once: Found swapchain, attempting to render");
            unsafe {
                // Get back buffer
                match sc.GetBuffer::<ID3D11Texture2D>(0) {
                    Ok(tex) => {
                        println!("render_once: Successfully got back buffer texture");
                        
                        // If we don't have a context yet, derive device/context from the texture
                        if self.d3d_context.is_none() {
                            println!("render_once: Need to get D3D context from texture");
                            let res: &ID3D11Resource = (&tex).into();
                            match res.GetDevice() {
                                Ok(device) => {
                                    match device.GetImmediateContext() {
                                        Ok(ctx) => {
                                            println!("render_once: Successfully got device and context from texture");
                                            self.d3d_device = Some(device);
                                            self.d3d_context = Some(ctx);
                                        },
                                        Err(e) => println!("render_once: Failed to get immediate context: {:?}", e),
                                    }
                                },
                                Err(e) => println!("render_once: Failed to get device from resource: {:?}", e),
                            }
                        }
                        
                        if self.d3d_context.is_none() {
                            println!("render_once: No D3D context available, can't render");
                            return; // can't render without a context
                        }
                        
                        let ctx = self.d3d_context.as_ref().unwrap();
                        println!("render_once: Got D3D context, creating render target view");
                        
                        // Create RTV
                        let mut rtv_desc = D3D11_RENDER_TARGET_VIEW_DESC::default();
                        rtv_desc.Format = DXGI_FORMAT(28); // DXGI_FORMAT_R8G8B8A8_UNORM
                        rtv_desc.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
                        rtv_desc.Anonymous.Texture2D = D3D11_TEX2D_RTV { MipSlice: 0 };
                        
                        if let Some(dev) = &self.d3d_device {
                            let mut rtv: Option<ID3D11RenderTargetView> = None;
                            match dev.CreateRenderTargetView(&tex, Some(&rtv_desc), Some(&mut rtv)) {
                                Ok(_) => {
                                    if let Some(rtv) = &rtv {
                                        println!("render_once: Created RTV, clearing to blue color");
                                        // Use a distinct blue color to clearly see if rendering works
                                        let color = [0.1f32, 0.2f32, 0.8f32, 1.0f32];
                                        ctx.ClearRenderTargetView(rtv, &color);
                                    } else {
                                        println!("render_once: RTV is None even though creation succeeded");
                                    }
                                },
                                Err(e) => println!("render_once: Failed to create RTV: {:?}", e),
                            }
                        } else {
                            println!("render_once: No D3D device available");
                        }
                    },
                    Err(e) => println!("render_once: Failed to get back buffer: {:?}", e),
                }
                
                // Present the swapchain
                let hr = sc.Present(1, DXGI_PRESENT(0));
                if hr.is_ok() {
                    println!("render_once: Successfully presented swapchain");
                } else {
                    println!("render_once: Failed to present swapchain: {:?}", hr);
                }
            }
            return;
        } else {
            println!("render_once: No swapchain found, trying fallback renderer");
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
