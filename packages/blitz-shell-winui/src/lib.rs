#![cfg_attr(docsrs, feature(doc_cfg))]

//! WinUI/WinAppSDK host shell for Blitz, exposing a WinRT component that can be consumed by C#.
//! This crate provides:
//! - A thin host that takes a WinUI SwapChainPanel (or a Direct3D11 back buffer) and creates a wgpu surface
//! - A minimal event bridge from WinUI pointer/keyboard events to Blitz DOM events
//! - A WinRT ABI surface to be used from a C# app. IDL is in `idl/Blitz.WinUI.idl`.
//!
//! Status: initial scaffold. Surface creation and event wiring are stubs that need real handles.

mod winrt_component;
mod bindings;

#[derive(Clone, Copy)]
pub struct SwapChainPanelHandle {
    pub swapchain: isize,
}
use crate::bindings::ISwapChainAttacher;

/// Use Direct2D window renderer implementation
pub use anyrender_d2d::D2DWindowRenderer as WindowRenderer;

/// High-level entry point: initialize the Blitz view for a host-provided surface.
///
/// Contract:
/// - Inputs: a platform handle (DXGI/D3D interop) describing the target surface + size/scale.
/// - Output: an opaque handle that the host can drive (resize, redraw, send input).
/// - Errors: returns a string on failure for easy marshaling across WinRT.
pub fn initialize_for_swapchain_panel(
    panel: SwapChainPanelHandle,
    width: u32,
    height: u32,
    scale: f32,
) -> Result<winrt_component::BlitzHost, String> {
    winrt_component::BlitzHost::new_for_swapchain(panel, width, height, scale)
}

// --- Optional C ABI for early interop testing (P/Invoke) ---
// Removed HWND-based C ABI: WinUI shell does not use raw window handles. Only WinRT activation is supported.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_resize(ptr: *mut winrt_component::BlitzHost, width: u32, height: u32, scale: f32) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.resize(width, height, scale);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_render(ptr: *mut winrt_component::BlitzHost) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.render_once();
    }
}

// Removed HWND setter: not supported in WinUI shell

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_load_html(ptr: *mut winrt_component::BlitzHost, bytes: *const u8, len: usize) {
    if let (Some(host), Some(slice)) = (unsafe { ptr.as_mut() }, unsafe { bytes.as_ref() }) {
        let s = unsafe { std::slice::from_raw_parts(slice, len) };
        if let Ok(html) = std::str::from_utf8(s) {
            host.load_html(html);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_destroy(ptr: *mut winrt_component::BlitzHost) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)); }
    }
}

// Input bridging C ABI (optional)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_pointer_move(ptr: *mut winrt_component::BlitzHost, x: f32, y: f32, buttons: u32, mods: u32) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.pointer_move(x, y, buttons, mods);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_pointer_down(ptr: *mut winrt_component::BlitzHost, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.pointer_down(x, y, button, buttons, mods);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_pointer_up(ptr: *mut winrt_component::BlitzHost, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.pointer_up(x, y, button, buttons, mods);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_wheel(ptr: *mut winrt_component::BlitzHost, dx: f64, dy: f64) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.wheel_scroll(dx, dy);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_key_down(ptr: *mut winrt_component::BlitzHost, vk: u32, ch: u32, mods: u32, is_auto_repeating: bool) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.key_down(vk, ch, mods, is_auto_repeating);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn blitz_winui_key_up(ptr: *mut winrt_component::BlitzHost, vk: u32, ch: u32, mods: u32) {
    if let Some(host) = unsafe { ptr.as_mut() } {
        host.key_up(vk, ch, mods);
    }
}

// --- WinRT component implementation (generated bindings -> concrete impl) ---
use core::ffi::c_void;
use windows::core::implement;
use windows_core::{IInspectable, HSTRING, Interface};
use windows_core::IUnknownImpl;
use crate::bindings::{IHost, IHostFactory, IHost_Impl, IHostFactory_Impl};
// Note: We expose a custom factory (IHostFactory) via DllGetActivationFactory.

#[implement(IHost, IHostFactory)]
pub struct HostRuntime {
    inner: std::sync::Mutex<Option<Box<winrt_component::BlitzHost>>>,
}

#[allow(non_snake_case)]
impl HostRuntime {
    fn new() -> HostRuntime {
        HostRuntime { inner: std::sync::Mutex::new(None) }
    }
}

// Implement the generated traits for the macro-generated identity type
#[allow(non_snake_case)]
impl IHost_Impl for HostRuntime_Impl {
    fn SetPanel(&self, panel: windows_core::Ref<'_, IInspectable>) -> windows_core::Result<()> {
        let imp = self.get_impl();
        if let Some(inner) = imp.inner.lock().unwrap().as_mut() {
            inner.set_panel(panel, 0, 0);
        }
        Ok(())
    }

    fn Resize(&self, width: u32, height: u32, scale: f32) -> windows_core::Result<()> {
        let imp = self.get_impl();
        if let Some(inner) = imp.inner.lock().unwrap().as_mut() {
            inner.resize(width, height, scale);
        }
        Ok(())
    }

    fn RenderOnce(&self) -> windows_core::Result<()> {
        let imp = self.get_impl();
        if let Some(inner) = imp.inner.lock().unwrap().as_mut() {
            inner.render_once();
        }
        Ok(())
    }

    fn LoadHtml(&self, html: &HSTRING) -> windows_core::Result<()> {
        let imp = self.get_impl();
        if let Some(inner) = imp.inner.lock().unwrap().as_mut() {
            let s: String = html.to_string();
            inner.load_html(&s);
        }
        Ok(())
    }

    fn TestAttacherConnection(&self) -> windows_core::Result<bool> {
        let imp = self.get_impl();
        if let Some(inner) = imp.inner.lock().unwrap().as_ref() {
            if let Some(attacher) = inner.get_attacher() {
                // Use the test method instead of trying to attach a fake pointer
                match attacher.TestAttacherConnection() {
                    Ok(result) => return Ok(result),
                    Err(_) => return Ok(false),
                }
            }
        }
        Ok(false)
    }
}

#[allow(non_snake_case)]
impl IHostFactory_Impl for HostRuntime_Impl {
    fn CreateInstance(
        &self,
        attacher: windows_core::Ref<'_, IInspectable>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> windows_core::Result<bindings::Host> {
        let runtime = HostRuntime::new();
        
        // Try to cast to ISwapChainAttacher
        if let Some(insp) = attacher.as_ref() {
            if let Ok(att) = insp.cast::<ISwapChainAttacher>() {
                // Create host with attacher directly
                if let Ok(shell) = winrt_component::BlitzHost::new_with_attacher(att, width, height, scale) {
                    *runtime.inner.lock().unwrap() = Some(Box::new(shell));
                    let insp: IInspectable = runtime.into();
                    let host: bindings::Host = Interface::cast(&insp)?;
                    return Ok(host);
                }
            }
        }
        
        // Fallback to old method if attacher casting failed
        let shell = winrt_component::BlitzHost::new_for_swapchain(
            SwapChainPanelHandle { swapchain: 0 },
            width,
            height,
            scale,
        )
        .map_err(|_| windows_core::Error::new(windows_core::HRESULT(0x80004005u32 as i32), "Host creation failed"))?;
        *runtime.inner.lock().unwrap() = Some(Box::new(shell));
        let insp: IInspectable = runtime.into();
        let host: bindings::Host = Interface::cast(&insp)?;
        host.SetPanel(attacher.as_ref())?;
        Ok(host)
    }
}

// --- WinRT Activation Factory ---
// Provide a factory object that implements IHostFactory; the runtime will QI for this interface.
#[implement(IHostFactory)]
pub struct HostActivationFactory;

#[allow(non_snake_case)]
impl IHostFactory_Impl for HostActivationFactory_Impl {
    fn CreateInstance(
        &self,
        attacher: windows_core::Ref<'_, IInspectable>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> windows_core::Result<bindings::Host> {
        let runtime = HostRuntime::new();
        
        // Try to cast to ISwapChainAttacher
        if let Some(insp) = attacher.as_ref() {
            if let Ok(att) = insp.cast::<ISwapChainAttacher>() {
                // Create host with attacher directly
                if let Ok(shell) = winrt_component::BlitzHost::new_with_attacher(att, width, height, scale) {
                    *runtime.inner.lock().unwrap() = Some(Box::new(shell));
                    let insp: IInspectable = runtime.into();
                    let host: bindings::Host = Interface::cast(&insp)?;
                    return Ok(host);
                }
            }
        }
        
        // Fallback to old method if attacher casting failed
        let shell = winrt_component::BlitzHost::new_for_swapchain(
            SwapChainPanelHandle { swapchain: 0 },
            width,
            height,
            scale,
        )
        .map_err(|_| windows_core::Error::new(windows_core::HRESULT(0x80004005u32 as i32), "Host creation failed"))?;
        *runtime.inner.lock().unwrap() = Some(Box::new(shell));
        let insp: IInspectable = runtime.into();
        let host: bindings::Host = Interface::cast(&insp)?;
        host.SetPanel(attacher.as_ref())?;
        Ok(host)
    }
}

// Exported activation entrypoint returning our activation factory for Blitz.WinUI.Host
#[unsafe(no_mangle)]
pub extern "system" fn DllGetActivationFactory(name: HSTRING, factory: *mut *mut c_void) -> windows_core::HRESULT {
    // E_INVALIDARG if no out parameter
    if factory.is_null() {
        return windows_core::HRESULT(0x80070057u32 as i32);
    }
    unsafe { *factory = core::ptr::null_mut(); }

    // Match the runtime class name defined in idl/Blitz.WinUI.idl
    let class_name = name.to_string();
    if class_name == "BlitzWinUI.Host" {
        // Create factory object and hand out IHostFactory
        let fac = HostActivationFactory;
        let insp: IInspectable = fac.into();
        match Interface::cast::<IHostFactory>(&insp) {
            Ok(host_factory) => {
                unsafe { *factory = host_factory.into_raw(); }
                windows_core::HRESULT(0) // S_OK
            }
            Err(err) => err.code(),
        }
    } else {
        // CLASS_E_CLASSNOTAVAILABLE
        windows_core::HRESULT(0x80040154u32 as i32)
    }
}
