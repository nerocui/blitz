mod bindings;
mod d2drenderer;
mod iframe;

use std::sync::{Once, Mutex};
use std::collections::HashMap;
use windows::core::*;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Foundation::{S_OK, S_FALSE, E_NOINTERFACE};

// A simple factory method-based approach instead of using the WinRT macros
// This avoids the multiple windows_core version issues
static INIT: Once = Once::new();
static mut FACTORY: Option<FactoryState> = None;

struct FactoryState {
    instances: Mutex<HashMap<u64, d2drenderer::D2DRenderer>>
}

// Export the DllGetActivationFactory function that WinRT needs
#[no_mangle]
unsafe extern "system" fn DllGetActivationFactory(
    activation_class_id: HSTRING,
    factory: *mut *mut std::ffi::c_void,
) -> HRESULT {
    // Initialize the factory once
    INIT.call_once(|| {
        FACTORY = Some(FactoryState {
            instances: Mutex::new(HashMap::new())
        });
    });
    
    // Check if we're being asked for the D2DRenderer factory
    let class_name = bindings::D2DRenderer::NAME;
    let activation_class = activation_class_id.to_string();
    
    // If the requested class doesn't match our renderer, return an error
    if activation_class != class_name {
        return E_NOINTERFACE;
    }
    
    // Instead of trying to access the private factory method,
    // create a new instance directly via CreateInstance
    match create_factory_instance() {
        Ok(factory_instance) => {
            *factory = std::mem::transmute(factory_instance);
            S_OK
        },
        Err(e) => e.into()
    }
}

// Helper function to create a factory instance
unsafe fn create_factory_instance() -> Result<*mut std::ffi::c_void> {
    // This is a simplified approach - we should use proper COM infrastructure
    // but for this fix we're focusing on getting a valid interface pointer
    
    // Create our D2DRenderer instance factory directly
    let instance = bindings::D2DRenderer::CreateInstance(0)?;
    
    // Return the raw pointer (this is unsafe and simplified)
    Ok(std::mem::transmute(instance))
}

// Create the actual renderer
pub fn create_renderer(device_context_ptr: u64) -> Result<bindings::D2DRenderer> {
    unsafe {
        // Convert numeric pointer to ID2D1DeviceContext
        let context: ID2D1DeviceContext = std::mem::transmute(device_context_ptr);
        let context = context.clone(); // Clone to ensure ownership
        
        // Create the renderer
        let renderer = d2drenderer::D2DRenderer::new(context);
        
        // If we have a factory, store the instance
        if let Some(factory) = &FACTORY {
            let mut instances = factory.instances.lock().unwrap();
            instances.insert(device_context_ptr, renderer.clone());
        }
        
        // Return the WinRT wrapper - using the proper factory method
        bindings::D2DRenderer::CreateInstance(device_context_ptr)
    }
}

// The DllCanUnloadNow function that COM needs
#[no_mangle]
unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    // Allow DLL to unload if no instances are still in use
    if let Some(factory) = &FACTORY {
        let instances = factory.instances.lock().unwrap();
        if !instances.is_empty() {
            return S_FALSE;
        }
    }
    
    S_OK
}
