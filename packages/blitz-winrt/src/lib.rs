mod bindings;
mod d2drenderer;
mod iframe;

use windows::core::*;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Foundation::{S_OK, E_NOINTERFACE, E_POINTER};

// The DllGetActivationFactory function that WinRT needs
#[no_mangle]
pub unsafe extern "system" fn DllGetActivationFactory(
    activation_class_id: HSTRING,
    factory: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if factory.is_null() {
        return E_POINTER;
    }

    // We don't explicitly implement the factory interface,
    // as we'll use direct instantiation from C# instead
    E_NOINTERFACE
}

// Direct export to create D2DRenderer
#[no_mangle]
pub unsafe extern "system" fn CreateD2DRenderer(
    device_context: u64,
    renderer: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if renderer.is_null() {
        return E_POINTER;
    }
    
    // Convert device context pointer to ID2D1DeviceContext
    let context = std::mem::transmute::<u64, ID2D1DeviceContext>(device_context);
    
    // Create our D2DRenderer implementation
    let instance = d2drenderer::D2DRenderer::new(context);
    
    // Box it to keep it alive
    let boxed_instance = Box::new(instance);
    
    // Cast to void pointer
    let ptr = Box::into_raw(boxed_instance) as *mut std::ffi::c_void;
    
    // Return the pointer
    *renderer = ptr;
    S_OK
}

// Direct export to destroy a D2DRenderer
#[no_mangle]
pub unsafe extern "system" fn DestroyD2DRenderer(
    renderer: *mut std::ffi::c_void,
) -> HRESULT {
    if !renderer.is_null() {
        // Convert back to box and drop it
        let _ = Box::from_raw(renderer as *mut d2drenderer::D2DRenderer);
    }
    S_OK
}

// The DllCanUnloadNow function that COM needs
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    // For now, always allow unloading
    S_OK
}
