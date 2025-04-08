mod bindings;
mod d2drenderer;
mod iframe;

use windows::core::*;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Foundation::*;

// Simple struct for D2DRendererFactory implementation
struct D2DRendererFactoryImpl {
    ref_count: std::sync::atomic::AtomicU32,
}

// The vtable for the ID2DRendererFactory interface
#[repr(C)]
struct ID2DRendererFactoryVtbl {
    // IUnknown methods
    query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    
    // IInspectable methods
    get_iids: unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32, *mut *mut GUID) -> HRESULT,
    get_runtime_class_name: unsafe extern "system" fn(*mut std::ffi::c_void, *mut HSTRING) -> HRESULT,
    get_trust_level: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> HRESULT,
    
    // ID2DRendererFactory methods - must match the vtable layout in C#
    create_instance: unsafe extern "system" fn(*mut std::ffi::c_void, u64, *mut *mut std::ffi::c_void) -> HRESULT,
}

// The actual factory object with vtable
#[repr(C)]
struct D2DRendererFactory {
    vtbl: *const ID2DRendererFactoryVtbl,
    impl_ref: *mut D2DRendererFactoryImpl,
}

// Static factory instance and vtable
static mut FACTORY_VTBL: ID2DRendererFactoryVtbl = ID2DRendererFactoryVtbl {
    query_interface: factory_query_interface,
    add_ref: factory_add_ref,
    release: factory_release,
    get_iids: factory_get_iids,
    get_runtime_class_name: factory_get_runtime_class_name,
    get_trust_level: factory_get_trust_level,
    create_instance: factory_create_instance,
};

static mut FACTORY_IMPL: Option<D2DRendererFactoryImpl> = None;
static mut FACTORY: Option<D2DRendererFactory> = None;

// The vtable implementations
unsafe extern "system" fn factory_query_interface(
    this: *mut std::ffi::c_void, 
    riid: *const GUID, 
    ppv_object: *mut *mut std::ffi::c_void
) -> HRESULT {
    if ppv_object.is_null() {
        return E_POINTER;
    }

    let riid = &*riid;
    
    // IUnknown interface
    if riid == &IUnknown::IID {
        *ppv_object = this;
        factory_add_ref(this);
        return S_OK;
    }
    
    // IInspectable interface
    if riid == &IInspectable::IID {
        *ppv_object = this;
        factory_add_ref(this);
        return S_OK;
    }
    
    // ID2DRendererFactory interface - GUID from BlitzWinRT.cs
    let factory_iid = GUID::from_values(
        0xA61732F0, 0x1A1E, 0x55FE, 
        [0x8C, 0x25, 0x75, 0xEA, 0x15, 0xB0, 0x07, 0x3C]
    );
    
    if riid == &factory_iid {
        *ppv_object = this;
        factory_add_ref(this);
        return S_OK;
    }
    
    *ppv_object = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn factory_add_ref(this: *mut std::ffi::c_void) -> u32 {
    let factory = this as *mut D2DRendererFactory;
    let impl_ref = (*factory).impl_ref;
    (*impl_ref).ref_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

unsafe extern "system" fn factory_release(this: *mut std::ffi::c_void) -> u32 {
    let factory = this as *mut D2DRendererFactory;
    let impl_ref = (*factory).impl_ref;
    let prev = (*impl_ref).ref_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    
    // We never actually drop the factory since it's static
    prev - 1
}

unsafe extern "system" fn factory_get_iids(
    _this: *mut std::ffi::c_void,
    iid_count: *mut u32,
    iids: *mut *mut GUID
) -> HRESULT {
    // We don't need to implement this fully for basic functionality
    *iid_count = 0;
    *iids = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn factory_get_runtime_class_name(
    _this: *mut std::ffi::c_void,
    class_name: *mut HSTRING
) -> HRESULT {
    *class_name = HSTRING::from("BlitzWinRT.D2DRendererFactory");
    S_OK
}

unsafe extern "system" fn factory_get_trust_level(
    _this: *mut std::ffi::c_void,
    trust_level: *mut i32
) -> HRESULT {
    // BaseTrust = 0
    *trust_level = 0;
    S_OK
}

unsafe extern "system" fn factory_create_instance(
    _this: *mut std::ffi::c_void,
    device_context: u64,
    instance: *mut *mut std::ffi::c_void
) -> HRESULT {
    if instance.is_null() {
        return E_POINTER;
    }
    
    // Convert device context pointer
    let context = std::mem::transmute::<u64, ID2D1DeviceContext>(device_context);
    
    // Create our D2DRenderer implementation
    let renderer_impl = d2drenderer::D2DRenderer::new(context);
    
    // Create a WinRT-compatible implementation
    match create_d2drenderer_instance(renderer_impl) {
        Ok(ptr) => {
            *instance = ptr;
            S_OK
        },
        Err(e) => e.into()
    }
}

// Initialize the static factory
unsafe fn get_factory() -> *mut D2DRendererFactory {
    if FACTORY.is_none() {
        FACTORY_IMPL = Some(D2DRendererFactoryImpl {
            ref_count: std::sync::atomic::AtomicU32::new(1),
        });
        
        FACTORY = Some(D2DRendererFactory {
            vtbl: &FACTORY_VTBL,
            impl_ref: FACTORY_IMPL.as_mut().unwrap() as *mut D2DRendererFactoryImpl,
        });
    }
    
    FACTORY.as_mut().unwrap() as *mut D2DRendererFactory
}

// The DllGetActivationFactory function that WinRT needs
#[no_mangle]
pub unsafe extern "system" fn DllGetActivationFactory(
    activation_class_id: HSTRING,
    factory: *mut *mut std::ffi::c_void
) -> HRESULT {
    if factory.is_null() {
        return E_POINTER;
    }

    // Check if we're being asked for the D2DRenderer class
    if activation_class_id.to_string() == "BlitzWinRT.D2DRenderer" {
        // Get our singleton factory
        let factory_ptr = get_factory() as *mut std::ffi::c_void;
        *factory = factory_ptr;
        factory_add_ref(factory_ptr); // Increment ref count since we're returning it
        return S_OK;
    }
    
    // We don't support this class
    *factory = std::ptr::null_mut();
    REGDB_E_CLASSNOTREG
}

// D2D renderer vtable representation
#[repr(C)]
struct D2DRendererVtbl {
    // IUnknown methods
    query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    
    // IInspectable methods
    get_iids: unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32, *mut *mut GUID) -> HRESULT,
    get_runtime_class_name: unsafe extern "system" fn(*mut std::ffi::c_void, *mut HSTRING) -> HRESULT,
    get_trust_level: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> HRESULT,
    
    // ID2DRenderer interface methods - make sure set_logger is listed first to match the WinRT interface
    set_logger: unsafe extern "system" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> HRESULT,
    render: unsafe extern "system" fn(*mut std::ffi::c_void, HSTRING) -> HRESULT,
    resize: unsafe extern "system" fn(*mut std::ffi::c_void, u32, u32) -> HRESULT,
    on_pointer_moved: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32) -> HRESULT,
    on_pointer_pressed: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32, u32) -> HRESULT,
    on_pointer_released: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32, u32) -> HRESULT,
    on_mouse_wheel: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32) -> HRESULT,
    on_key_down: unsafe extern "system" fn(*mut std::ffi::c_void, u32, bool, bool, bool) -> HRESULT,
    on_key_up: unsafe extern "system" fn(*mut std::ffi::c_void, u32) -> HRESULT,
    on_text_input: unsafe extern "system" fn(*mut std::ffi::c_void, HSTRING) -> HRESULT,
    on_blur: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    on_focus: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    suspend: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    resume: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    set_theme: unsafe extern "system" fn(*mut std::ffi::c_void, bool) -> HRESULT,
    tick: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
}

// Actual WinRT implementation object
#[repr(C)]
struct D2DRendererImpl {
    vtbl: *const D2DRendererVtbl,
    inner: d2drenderer::D2DRenderer,
    ref_count: std::sync::atomic::AtomicU32,
}

// Static vtable instance
static D2DRENDERER_VTBL: D2DRendererVtbl = D2DRendererVtbl {
    // IUnknown
    query_interface: d2drenderer_query_interface,
    add_ref: d2drenderer_add_ref,
    release: d2drenderer_release,
    
    // IInspectable
    get_iids: d2drenderer_get_iids,
    get_runtime_class_name: d2drenderer_get_runtime_class_name,
    get_trust_level: d2drenderer_get_trust_level,
    
    // ID2DRenderer
    set_logger: d2drenderer_set_logger,
    render: d2drenderer_render,
    resize: d2drenderer_resize,
    on_pointer_moved: d2drenderer_on_pointer_moved,
    on_pointer_pressed: d2drenderer_on_pointer_pressed,
    on_pointer_released: d2drenderer_on_pointer_released,
    on_mouse_wheel: d2drenderer_on_mouse_wheel,
    on_key_down: d2drenderer_on_key_down,
    on_key_up: d2drenderer_on_key_up,
    on_text_input: d2drenderer_on_text_input,
    on_blur: d2drenderer_on_blur,
    on_focus: d2drenderer_on_focus,
    suspend: d2drenderer_suspend,
    resume: d2drenderer_resume,
    set_theme: d2drenderer_set_theme,
    tick: d2drenderer_tick,
};

// IUnknown implementation for D2DRenderer
unsafe extern "system" fn d2drenderer_query_interface(
    this: *mut std::ffi::c_void,
    riid: *const GUID,
    ppv_object: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if ppv_object.is_null() {
        return E_POINTER;
    }
    
    let riid = &*riid;
    
    // Handle standard interfaces
    if riid == &IUnknown::IID || riid == &IInspectable::IID {
        *ppv_object = this;
        d2drenderer_add_ref(this);
        return S_OK;
    }
    
    // Handle ID2DRenderer interface - GUID from bindings.rs
    let renderer_iid = GUID::from_values(
        0xDFF484B2, 0x94FA, 0x51D1, 
        [0xBA, 0x2D, 0xDC, 0x03, 0x32, 0x37, 0xEC, 0x1E]
    );
    
    if riid == &renderer_iid {
        *ppv_object = this;
        d2drenderer_add_ref(this);
        return S_OK;
    }
    
    *ppv_object = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn d2drenderer_add_ref(this: *mut std::ffi::c_void) -> u32 {
    let impl_ptr = this as *mut D2DRendererImpl;
    (*impl_ptr).ref_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

unsafe extern "system" fn d2drenderer_release(this: *mut std::ffi::c_void) -> u32 {
    let impl_ptr = this as *mut D2DRendererImpl;
    let prev = (*impl_ptr).ref_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    
    if prev == 1 {
        // Free the memory
        drop(Box::from_raw(impl_ptr));
        return 0;
    }
    
    prev - 1
}

// IInspectable implementation
unsafe extern "system" fn d2drenderer_get_iids(
    _this: *mut std::ffi::c_void,
    iid_count: *mut u32,
    iids: *mut *mut GUID
) -> HRESULT {
    // We don't need to fully implement this for basic functionality
    *iid_count = 0;
    *iids = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn d2drenderer_get_runtime_class_name(
    _this: *mut std::ffi::c_void,
    class_name: *mut HSTRING
) -> HRESULT {
    *class_name = HSTRING::from("BlitzWinRT.D2DRenderer");
    S_OK
}

unsafe extern "system" fn d2drenderer_get_trust_level(
    _this: *mut std::ffi::c_void,
    trust_level: *mut i32
) -> HRESULT {
    // 0 = BaseTrust in WinRT
    *trust_level = 0;
    S_OK
}

// ID2DRenderer implementation - delegate methods to the inner implementation

unsafe extern "system" fn d2drenderer_render(
    this: *mut std::ffi::c_void,
    markdown: HSTRING
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.render_markdown(&markdown.to_string()) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_resize(
    this: *mut std::ffi::c_void,
    width: u32,
    height: u32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    
    // Avoid resizing to zero dimensions which causes D2D errors
    if width == 0 || height == 0 {
        return S_OK; // Silently ignore invalid sizes
    }
    
    match (*impl_ptr).inner.iframe.resize(width, height) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_moved(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.pointer_moved(x, y) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_pressed(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32,
    button: u32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.pointer_pressed(x, y, button) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_released(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32,
    button: u32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.pointer_released(x, y, button) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_mouse_wheel(
    this: *mut std::ffi::c_void,
    delta_x: f32,
    delta_y: f32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.mouse_wheel(delta_x, delta_y) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_key_down(
    this: *mut std::ffi::c_void,
    key_code: u32,
    ctrl: bool,
    shift: bool,
    alt: bool
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.key_down(key_code, ctrl, shift, alt) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_key_up(
    this: *mut std::ffi::c_void,
    key_code: u32
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.key_up(key_code) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_text_input(
    this: *mut std::ffi::c_void,
    text: HSTRING
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.text_input(&text.to_string()) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_blur(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.on_blur() {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_on_focus(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.on_focus() {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_suspend(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.suspend() {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_resume(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.resume() {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_set_theme(
    this: *mut std::ffi::c_void,
    is_dark_mode: bool
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.iframe.set_theme(is_dark_mode) {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_tick(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let impl_ptr = this as *mut D2DRendererImpl;
    match (*impl_ptr).inner.tick() {
        Ok(_) => S_OK,
        Err(e) => e.into()
    }
}

unsafe extern "system" fn d2drenderer_set_logger(
    this: *mut std::ffi::c_void,
    logger: *mut std::ffi::c_void,
) -> HRESULT {
    if logger.is_null() {
        return E_POINTER;
    }

    let impl_ptr = this as *mut D2DRendererImpl;
    
    // Convert the raw pointer to an ILogger
    let logger = std::mem::transmute::<*mut std::ffi::c_void, crate::bindings::ILogger>(logger);

    // Pass the logger to our implementation
    match (*impl_ptr).inner.set_logger(logger) {
        Ok(_) => S_OK,
        Err(e) => {
            println!("Error setting logger: {:?}", e);
            e.into()
        }
    }
}

// Create a new D2DRenderer instance
fn create_d2drenderer_instance(inner: d2drenderer::D2DRenderer) -> Result<*mut std::ffi::c_void> {
    let instance = Box::new(D2DRendererImpl {
        vtbl: &D2DRENDERER_VTBL,
        inner,
        ref_count: std::sync::atomic::AtomicU32::new(1),
    });
    
    let ptr = Box::into_raw(instance) as *mut std::ffi::c_void;
    Ok(ptr)
}

// Required for COM unloading
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_OK
}

// For direct P/Invoke if needed
#[no_mangle]
pub unsafe extern "system" fn CreateD2DRenderer(
    device_context: u64,
    renderer: *mut *mut std::ffi::c_void
) -> HRESULT {
    if renderer.is_null() {
        return E_POINTER;
    }
    
    let context = std::mem::transmute::<u64, ID2D1DeviceContext>(device_context);
    let instance = d2drenderer::D2DRenderer::new(context);
    
    match create_d2drenderer_instance(instance) {
        Ok(ptr) => {
            *renderer = ptr;
            S_OK
        },
        Err(e) => e.into()
    }
}

// Clean up a renderer created via direct P/Invoke
#[no_mangle]
pub unsafe extern "system" fn DestroyD2DRenderer(renderer: *mut std::ffi::c_void) -> HRESULT {
    if !renderer.is_null() {
        // Let Rust reclaim the memory by wrapping in a Box that will be dropped
        drop(Box::from_raw(renderer as *mut d2drenderer::D2DRenderer));
    }
    S_OK
}
