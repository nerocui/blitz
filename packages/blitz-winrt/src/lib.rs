mod bindings;
mod d2drenderer;
mod iframe;

use windows::core::*;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Foundation::*;

// Core WinRT activation function
#[no_mangle]
pub unsafe extern "system" fn DllGetActivationFactory(
    activation_class_id: HSTRING,
    factory: *mut *mut std::ffi::c_void,
) -> HRESULT {
    // Standard error checks
    if factory.is_null() {
        return E_POINTER;
    }
    
    // Clear the factory pointer first
    *factory = std::ptr::null_mut();
    
    // Check if we're being asked for our D2DRenderer class
    if activation_class_id.to_string() == "BlitzWinRT.D2DRenderer" {
        // The factory pointer requested is for ID2DRendererFactory
        // Since we can't properly implement the factory with interface inheritance,
        // we'll provide a direct implementation of the COM interface
        
        // Create a D2DRendererFactory instance with vtable
        let factory_instance = create_d2drenderer_factory();
        
        // Return the factory pointer
        *factory = factory_instance;
        return S_OK;
    }
    
    // Class not supported
    REGDB_E_CLASSNOTREG
}

// COM vtable for ID2DRendererFactory
#[repr(C)]
struct ID2DRendererFactoryVtbl {
    // IUnknown methods
    QueryInterface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    Release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    
    // IInspectable methods
    GetIids: unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32, *mut *mut GUID) -> HRESULT,
    GetRuntimeClassName: unsafe extern "system" fn(*mut std::ffi::c_void, *mut HSTRING) -> HRESULT,
    GetTrustLevel: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> HRESULT,
    
    // ID2DRendererFactory methods
    CreateInstance: unsafe extern "system" fn(*mut std::ffi::c_void, u64, *mut *mut std::ffi::c_void) -> HRESULT,
}

// COM implementation of ID2DRendererFactory
#[repr(C)]
struct D2DRendererFactoryImpl {
    vtbl: *const ID2DRendererFactoryVtbl,
    ref_count: std::sync::atomic::AtomicU32,
}

// Static factory VTable
static D2DRENDERER_FACTORY_VTBL: ID2DRendererFactoryVtbl = ID2DRendererFactoryVtbl {
    // IUnknown methods
    QueryInterface: d2drenderer_factory_query_interface,
    AddRef: d2drenderer_factory_add_ref,
    Release: d2drenderer_factory_release,
    
    // IInspectable methods
    GetIids: d2drenderer_factory_get_iids,
    GetRuntimeClassName: d2drenderer_factory_get_runtime_class_name,
    GetTrustLevel: d2drenderer_factory_get_trust_level,
    
    // ID2DRendererFactory methods
    CreateInstance: d2drenderer_factory_create_instance,
};

// Implementation of IUnknown for D2DRendererFactory
unsafe extern "system" fn d2drenderer_factory_query_interface(
    this: *mut std::ffi::c_void,
    iid: *const GUID,
    out: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    
    let iid = &*iid;
    
    // Handle standard interfaces
    if iid == &IUnknown::IID || iid == &IInspectable::IID {
        *out = this;
        d2drenderer_factory_add_ref(this);
        return S_OK;
    }
    
    // Handle ID2DRendererFactory interface - GUID from BlitzWinRT.cs
    let factory_iid = GUID::from_values(
        0xA61732F0, 0x1A1E, 0x55FE, 
        [0x8C, 0x25, 0x75, 0xEA, 0x15, 0xB0, 0x07, 0x3C]
    );
    
    if iid == &factory_iid {
        *out = this;
        d2drenderer_factory_add_ref(this);
        return S_OK;
    }
    
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn d2drenderer_factory_add_ref(this: *mut std::ffi::c_void) -> u32 {
    let factory = this as *mut D2DRendererFactoryImpl;
    let prev = (*factory).ref_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    prev + 1
}

unsafe extern "system" fn d2drenderer_factory_release(this: *mut std::ffi::c_void) -> u32 {
    let factory = this as *mut D2DRendererFactoryImpl;
    let prev = (*factory).ref_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    
    if prev == 1 {
        // Free the memory (we don't actually do this for static instances)
        // Box::from_raw(factory);
    }
    
    prev - 1
}

unsafe extern "system" fn d2drenderer_factory_get_iids(
    _this: *mut std::ffi::c_void,
    count: *mut u32,
    ids: *mut *mut GUID,
) -> HRESULT {
    // We don't need to implement this completely for basic functionality
    *count = 0;
    *ids = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn d2drenderer_factory_get_runtime_class_name(
    _this: *mut std::ffi::c_void,
    class_name: *mut HSTRING,
) -> HRESULT {
    *class_name = HSTRING::from("BlitzWinRT.D2DRendererFactory");
    S_OK
}

unsafe extern "system" fn d2drenderer_factory_get_trust_level(
    _this: *mut std::ffi::c_void,
    trust_level: *mut i32,
) -> HRESULT {
    // 0 = BaseTrust in WinRT
    *trust_level = 0;
    S_OK
}

// Implementation of ID2DRendererFactory.CreateInstance
unsafe extern "system" fn d2drenderer_factory_create_instance(
    _this: *mut std::ffi::c_void,
    device_context: u64,
    instance: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if instance.is_null() {
        return E_POINTER;
    }
    
    // Convert device context to ID2D1DeviceContext
    let context = std::mem::transmute::<u64, ID2D1DeviceContext>(device_context);
    
    // Create our D2DRenderer implementation
    let renderer = d2drenderer::D2DRenderer::new(context);
    
    // Create a WinRT-compatible wrapper
    let wrapper = create_d2drenderer_instance(renderer);
    
    // Return the instance pointer
    *instance = wrapper;
    S_OK
}

// Create a static factory instance to return
fn create_d2drenderer_factory() -> *mut std::ffi::c_void {
    // We use a static variable to avoid allocating each time
    static mut FACTORY: Option<D2DRendererFactoryImpl> = None;
    static INIT: std::sync::Once = std::sync::Once::new();
    
    unsafe {
        INIT.call_once(|| {
            FACTORY = Some(D2DRendererFactoryImpl {
                vtbl: &D2DRENDERER_FACTORY_VTBL,
                ref_count: std::sync::atomic::AtomicU32::new(1),
            });
        });
        
        FACTORY.as_mut().unwrap() as *mut D2DRendererFactoryImpl as *mut std::ffi::c_void
    }
}

// COM vtable for ID2DRenderer
#[repr(C)]
struct ID2DRendererVtbl {
    // IUnknown methods
    QueryInterface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    Release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    
    // IInspectable methods
    GetIids: unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32, *mut *mut GUID) -> HRESULT,
    GetRuntimeClassName: unsafe extern "system" fn(*mut std::ffi::c_void, *mut HSTRING) -> HRESULT,
    GetTrustLevel: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> HRESULT,
    
    // ID2DRenderer methods
    Render: unsafe extern "system" fn(*mut std::ffi::c_void, HSTRING) -> HRESULT,
    Resize: unsafe extern "system" fn(*mut std::ffi::c_void, u32, u32) -> HRESULT,
    OnPointerMoved: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32) -> HRESULT,
    OnPointerPressed: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32, u32) -> HRESULT,
    OnPointerReleased: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32, u32) -> HRESULT,
    OnMouseWheel: unsafe extern "system" fn(*mut std::ffi::c_void, f32, f32) -> HRESULT,
    OnKeyDown: unsafe extern "system" fn(*mut std::ffi::c_void, u32, bool, bool, bool) -> HRESULT,
    OnKeyUp: unsafe extern "system" fn(*mut std::ffi::c_void, u32) -> HRESULT,
    OnTextInput: unsafe extern "system" fn(*mut std::ffi::c_void, HSTRING) -> HRESULT,
    OnBlur: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    OnFocus: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    Suspend: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    Resume: unsafe extern "system" fn(*mut std::ffi::c_void) -> HRESULT,
    SetTheme: unsafe extern "system" fn(*mut std::ffi::c_void, bool) -> HRESULT,
}

// COM implementation of ID2DRenderer
#[repr(C)]
struct D2DRendererImpl {
    vtbl: *const ID2DRendererVtbl,
    renderer: d2drenderer::D2DRenderer,
    ref_count: std::sync::atomic::AtomicU32,
}

// Static renderer VTable
static D2DRENDERER_VTBL: ID2DRendererVtbl = ID2DRendererVtbl {
    // IUnknown methods
    QueryInterface: d2drenderer_query_interface,
    AddRef: d2drenderer_add_ref,
    Release: d2drenderer_release,
    
    // IInspectable methods
    GetIids: d2drenderer_get_iids,
    GetRuntimeClassName: d2drenderer_get_runtime_class_name,
    GetTrustLevel: d2drenderer_get_trust_level,
    
    // ID2DRenderer methods
    Render: d2drenderer_render,
    Resize: d2drenderer_resize,
    OnPointerMoved: d2drenderer_on_pointer_moved,
    OnPointerPressed: d2drenderer_on_pointer_pressed,
    OnPointerReleased: d2drenderer_on_pointer_released,
    OnMouseWheel: d2drenderer_on_mouse_wheel,
    OnKeyDown: d2drenderer_on_key_down,
    OnKeyUp: d2drenderer_on_key_up,
    OnTextInput: d2drenderer_on_text_input,
    OnBlur: d2drenderer_on_blur,
    OnFocus: d2drenderer_on_focus,
    Suspend: d2drenderer_suspend,
    Resume: d2drenderer_resume,
    SetTheme: d2drenderer_set_theme,
};

// Implementation of IUnknown for D2DRenderer
unsafe extern "system" fn d2drenderer_query_interface(
    this: *mut std::ffi::c_void,
    iid: *const GUID,
    out: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    
    let iid = &*iid;
    
    // Handle standard interfaces
    if iid == &IUnknown::IID || iid == &IInspectable::IID {
        *out = this;
        d2drenderer_add_ref(this);
        return S_OK;
    }
    
    // Handle ID2DRenderer interface - GUID from BlitzWinRT.cs
    let renderer_iid = GUID::from_values(
        0xDFF484B2, 0x94FA, 0x51D1, 
        [0xBA, 0x2D, 0xDC, 0x03, 0x32, 0x37, 0xEC, 0x1E]
    );
    
    if iid == &renderer_iid {
        *out = this;
        d2drenderer_add_ref(this);
        return S_OK;
    }
    
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn d2drenderer_add_ref(this: *mut std::ffi::c_void) -> u32 {
    let renderer = this as *mut D2DRendererImpl;
    let prev = (*renderer).ref_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    prev + 1
}

unsafe extern "system" fn d2drenderer_release(this: *mut std::ffi::c_void) -> u32 {
    let renderer = this as *mut D2DRendererImpl;
    let prev = (*renderer).ref_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    
    if prev == 1 {
        // Free the memory
        Box::from_raw(renderer);
    }
    
    prev - 1
}

unsafe extern "system" fn d2drenderer_get_iids(
    _this: *mut std::ffi::c_void,
    count: *mut u32,
    ids: *mut *mut GUID,
) -> HRESULT {
    // We don't need to implement this completely for basic functionality
    *count = 0;
    *ids = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn d2drenderer_get_runtime_class_name(
    _this: *mut std::ffi::c_void,
    class_name: *mut HSTRING,
) -> HRESULT {
    *class_name = HSTRING::from("BlitzWinRT.D2DRenderer");
    S_OK
}

unsafe extern "system" fn d2drenderer_get_trust_level(
    _this: *mut std::ffi::c_void,
    trust_level: *mut i32,
) -> HRESULT {
    // 0 = BaseTrust in WinRT
    *trust_level = 0;
    S_OK
}

// Delegate method implementations to the actual D2DRenderer instance

unsafe extern "system" fn d2drenderer_render(
    this: *mut std::ffi::c_void, 
    markdown: HSTRING
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.render_markdown(&markdown.to_string_lossy()) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_resize(
    this: *mut std::ffi::c_void,
    width: u32,
    height: u32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.resize(width, height) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_moved(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.pointer_moved(x, y) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_pressed(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32,
    button: u32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.pointer_pressed(x, y, button) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_pointer_released(
    this: *mut std::ffi::c_void,
    x: f32,
    y: f32,
    button: u32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.pointer_released(x, y, button) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_mouse_wheel(
    this: *mut std::ffi::c_void,
    delta_x: f32,
    delta_y: f32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.mouse_wheel(delta_x, delta_y) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_key_down(
    this: *mut std::ffi::c_void,
    key_code: u32,
    ctrl: bool,
    shift: bool,
    alt: bool
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.key_down(key_code, ctrl, shift, alt) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_key_up(
    this: *mut std::ffi::c_void,
    key_code: u32
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.key_up(key_code) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_text_input(
    this: *mut std::ffi::c_void,
    text: HSTRING
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.text_input(&text.to_string_lossy()) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_blur(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.on_blur() {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_on_focus(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.on_focus() {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_suspend(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.suspend() {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_resume(
    this: *mut std::ffi::c_void
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.resume() {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

unsafe extern "system" fn d2drenderer_set_theme(
    this: *mut std::ffi::c_void,
    is_dark_mode: bool
) -> HRESULT {
    let renderer = this as *mut D2DRendererImpl;
    match (*renderer).renderer.iframe.set_theme(is_dark_mode) {
        Ok(_) => S_OK,
        Err(err) => err.into()
    }
}

// Create a WinRT-compatible wrapper for our D2DRenderer implementation
fn create_d2drenderer_instance(renderer: d2drenderer::D2DRenderer) -> *mut std::ffi::c_void {
    let instance = Box::new(D2DRendererImpl {
        vtbl: &D2DRENDERER_VTBL,
        renderer,
        ref_count: std::sync::atomic::AtomicU32::new(1),
    });
    
    Box::into_raw(instance) as *mut std::ffi::c_void
}

// Required for COM unloading
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_OK
}

// Legacy exports for direct C# P/Invoke
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

// Legacy function to destroy a D2DRenderer created with CreateD2DRenderer
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
