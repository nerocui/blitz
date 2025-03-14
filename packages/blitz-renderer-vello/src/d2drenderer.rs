use std::sync::Arc;
use blitz_dom::BaseDocument;
use blitz_traits::{BlitzWindowHandle, ColorScheme, Devtools, DocumentRenderer, Viewport};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Direct2D::*,
    Win32::Graphics::Direct2D::Common::*,
    Win32::Graphics::Direct3D::*,
    Win32::Graphics::Direct3D11::*,
    Win32::Graphics::Dxgi::*,
    Win32::Graphics::Dxgi::Common::*,
    Win32::System::Com::*,
};

use crate::renderer::d2drender::generate_d2d_scene;

/// Direct2D rendering state
pub struct D2DRenderState {
    factory: ID2D1Factory1,
    dxfactory: IDXGIFactory2,
    device_context: ID2D1DeviceContext,
    swapchain: IDXGISwapChain1,
    brush: Option<ID2D1SolidColorBrush>,
    dpi: f32,
}

/// Simple D2D renderer, similar to `BlitzVelloRenderer`.
pub struct BlitzD2DRenderer {
    /// Window handle for managing the OS-level surface
    window_handle: Arc<dyn BlitzWindowHandle>,
    /// D2D render state (when active)
    render_state: Option<D2DRenderState>,
}

impl BlitzD2DRenderer {
    /// Create the Direct2D factory
    fn create_factory() -> Result<ID2D1Factory1> {
        let mut options = D2D1_FACTORY_OPTIONS::default();
        if cfg!(debug_assertions) {
            options.debugLevel = D2D1_DEBUG_LEVEL_INFORMATION;
        }
        unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options)) }
    }

    /// Create the Direct3D device
    fn create_device() -> Result<ID3D11Device> {
        fn create_device_with_type(drive_type: D3D_DRIVER_TYPE) -> Result<ID3D11Device> {
            let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            if cfg!(debug_assertions) {
                flags |= D3D11_CREATE_DEVICE_DEBUG;
            }
            
            let mut device = None;
            unsafe {
                D3D11CreateDevice(
                    None,
                    drive_type,
                    HMODULE::default(),
                    flags,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    None,
                )
                .map(|()| device.unwrap())
            }
        }
        
        let mut result = create_device_with_type(D3D_DRIVER_TYPE_HARDWARE);
        if let Err(err) = &result {
            if err.code() == DXGI_ERROR_UNSUPPORTED {
                result = create_device_with_type(D3D_DRIVER_TYPE_WARP);
            }
        }
        result
    }

    /// Create the render target
    fn create_render_target(factory: &ID2D1Factory1, device: &ID3D11Device) -> Result<ID2D1DeviceContext> {
        unsafe {
            let d2device = factory.CreateDevice(&device.cast::<IDXGIDevice>()?)?;
            let target:ID2D1DeviceContext  = d2device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            target.SetUnitMode(D2D1_UNIT_MODE_DIPS);
            Ok(target)
        }
    }

    /// Create the swapchain
    fn create_swapchain(device: &ID3D11Device, window: HWND) -> Result<IDXGISwapChain1> {
        let dxdevice = device.cast::<IDXGIDevice>()?;
        let adapter = unsafe { dxdevice.GetAdapter()? };
        let factory: IDXGIFactory2 = unsafe { adapter.GetParent()? };
        
        let props = DXGI_SWAP_CHAIN_DESC1 {
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            ..Default::default()
        };
        
        unsafe { factory.CreateSwapChainForHwnd(device, window, &props, None, None) }
    }

    /// Create a bitmap for the swapchain
    fn create_swapchain_bitmap(swapchain: &IDXGISwapChain1, target: &ID2D1DeviceContext) -> Result<()> {
        let surface: IDXGISurface = unsafe { swapchain.GetBuffer(0)? };
        
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        
        unsafe {
            let bitmap = target.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            target.SetTarget(&bitmap);
        };
        
        Ok(())
    }
    
    /// Initialize Direct2D resources
    fn initialize_d2d(&self, hwnd: HWND) -> Result<D2DRenderState> {
        // Initialize Direct2D resources
        let factory = Self::create_factory()?;
        let device = Self::create_device()?;
        let device_context = Self::create_render_target(&factory, &device)?;
        let swapchain = Self::create_swapchain(&device, hwnd)?;
        Self::create_swapchain_bitmap(&swapchain, &device_context)?;
        
        // Get DPI and set on context
        let mut dpi = 96.0;
        let mut dpiy = 96.0;
        unsafe { factory.GetDesktopDpi(&mut dpi, &mut dpiy) };
        unsafe { device_context.SetDpi(dpi, dpi) };
        
        // Get factory for later use
        let dxdevice = device.cast::<IDXGIDevice>()?;
        let adapter = unsafe { dxdevice.GetAdapter()? };
        let dxfactory: IDXGIFactory2 = unsafe { adapter.GetParent()? };

        Ok(D2DRenderState {
            factory,
            dxfactory,
            device_context,
            swapchain,
            brush: None,
            dpi,
        })
    }
}

impl DocumentRenderer for BlitzD2DRenderer {
    type Doc = BaseDocument;

    /// Create a new Direct2D renderer
    fn new(window: Arc<dyn BlitzWindowHandle>) -> Self {
        Self {
            window_handle: window,
            render_state: None,
        }
    }

    /// Quickly check if renderer is active
    fn is_active(&self) -> bool {
        self.render_state.is_some()
    }

    /// Resume rendering (set up Direct2D resources, etc.)
    fn resume(&mut self, _viewport: &Viewport) {
        if self.render_state.is_some() {
            return;
        }
        
        // Get the HWND from the window handle
        let window_handle = self.window_handle.window_handle()
            .expect("Failed to get window handle");
        let raw_handle = window_handle.as_raw();
        
        let hwnd = match raw_handle {
            RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as _),
            _ => panic!("Expected Win32 window handle"),
        };
        
        match self.initialize_d2d(hwnd) {
            Ok(render_state) => {
                self.render_state = Some(render_state);
            },
            Err(e) => {
                eprintln!("Failed to initialize Direct2D: {:?}", e);
            }
        }
    }

    /// Suspend rendering (dispose/unbind Direct2D resources)
    fn suspend(&mut self) {
        self.render_state = None;
        // Resources will be dropped automatically
    }

    /// Handle window resizing
    fn set_size(&mut self, physical_width: u32, physical_height: u32) {
        if let Some(state) = &mut self.render_state {
            unsafe {
                // Release target
                state.device_context.SetTarget(None);
                
                // Resize the swapchain
                if state.swapchain.ResizeBuffers(
                    0,
                    physical_width,
                    physical_height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0)
                ).is_ok() {
                    // Create the swapchain bitmap again
                    if let Err(e) = Self::create_swapchain_bitmap(&state.swapchain, &state.device_context) {
                        eprintln!("Failed to resize swapchain bitmap: {:?}", e);
                        self.render_state = None;
                    }
                } else {
                    // If resizing fails, recreate the resources
                    self.render_state = None;
                }
            }
        }
    }

    /// Render a DOM document
    fn render(
        &mut self,
        doc: &BaseDocument,
        scale: f64,
        width: u32,
        height: u32,
        devtools: Devtools,
    ) {
        if let Some(state) = &mut self.render_state {
            unsafe {
                // Begin drawing
                state.device_context.BeginDraw();
                
                // Clear with white background
                state.device_context.Clear(Some(&D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }));
                
                // Generate the Direct2D scene
                generate_d2d_scene(
                    &mut state.device_context,
                    doc,
                    scale,
                    width,
                    height,
                    devtools,
                );
                
                // End drawing
                if let Err(e) = state.device_context.EndDraw(None, None) {
                    eprintln!("Failed to end drawing: {:?}", e);
                    self.render_state = None;
                    return;
                }
                
                // Present the swapchain
                let hr = state.swapchain.Present(1, DXGI_PRESENT(0));
                if hr == DXGI_STATUS_OCCLUDED {
                    // Window is occluded, can continue
                } else if hr == S_OK {
                    // Successful presentation - ensure brush is created for next frame if needed
                    if state.brush.is_none() {
                        let brush = state.device_context.CreateSolidColorBrush(
                            &D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
                            None
                        );
                        if let Ok(brush) = brush {
                            state.brush = Some(brush);
                        }
                    }
                    
                    // Optional: Add debug information in debug builds
                    #[cfg(debug_assertions)]
                    println!("Frame successfully rendered at {}x{}", width, height);
                } else {
                    // Handle other presentation errors
                    eprintln!("Failed to present swapchain: {:?}", hr);
                    self.render_state = None;
                }
            }
        } else if width > 0 && height > 0 {
            // Try to resume if we need to render but aren't active
            let viewport = Viewport::new(width, height, 1.0, ColorScheme::default());
            self.resume(&viewport);
        }
    }
}

impl Drop for BlitzD2DRenderer {
    fn drop(&mut self) {
        // Make sure to uninitialize COM when done
        unsafe {
            CoUninitialize();
        }
    }
}