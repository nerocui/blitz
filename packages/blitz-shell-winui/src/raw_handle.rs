use std::sync::Arc;

use anyrender::WindowHandle;
use raw_window_handle::{DisplayHandle, RawDisplayHandle, RawWindowHandle, WindowHandle as RawWH};
use std::num::NonZeroIsize;
use core::ffi::c_void;
use windows::Win32::Foundation::HWND;

/// Opaque wrapper around a WinUI SwapChainPanel. On WinUI, rendering is typically done by
/// creating a DXGI SwapChain and attaching it to the panel. wgpu can create a Surface from
/// a DXGI SwapChain or from the underlying HWND/COM objects depending on backend.
#[derive(Clone)]
pub struct SwapChainPanelHandle {
    /// A reference to the SwapChain associated with the panel.
    /// TODO: determine if wgpu supports creating a surface directly from SwapChain, or if we need HWND.
    pub swapchain: isize,
}

/// FFI-safe descriptor that allows anyrender/wgpu to acquire a surface.
/// For now, we implement HasWindowHandle/HasDisplayHandle for this type using
/// RawWindowHandle::Win32 and RawDisplayHandle::Windows to satisfy wgpu's surface creation.
#[derive(Clone)]
pub struct DxgiInteropHandle {
    pub hwnd: HWND,
}

impl raw_window_handle::HasWindowHandle for DxgiInteropHandle {
    fn window_handle(&self) -> Result<RawWH<'_>, raw_window_handle::HandleError> {
    // SAFETY: HWND is a transparent newtype over pointer; cast to isize and map 0 -> 1 for NonZero.
    let raw: isize = self.hwnd.0 as isize;
    let nz = unsafe { NonZeroIsize::new_unchecked(if raw == 0 { 1 } else { raw }) };
    let handle = RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(nz));
        Ok(unsafe { RawWH::borrow_raw(handle) })
    }
}

impl raw_window_handle::HasDisplayHandle for DxgiInteropHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new());
        Ok(unsafe { DisplayHandle::borrow_raw(handle) })
    }
}
// Marker trait is auto-implemented in anyrender for types meeting bounds; no local impl needed here.

impl From<isize> for DxgiInteropHandle {
    fn from(hwnd: isize) -> Self {
    Self { hwnd: HWND(hwnd as *mut c_void) }
    }
}

impl From<HWND> for DxgiInteropHandle {
    fn from(hwnd: HWND) -> Self {
        Self { hwnd }
    }
}

/// Helper to box and Arc the handle for consumption by anyrender_vello
pub fn arc_window_handle_from_hwnd(hwnd: isize) -> Arc<dyn WindowHandle> {
    Arc::new(DxgiInteropHandle::from(hwnd))
}

// SAFETY: HWND is an OS handle that can be copied between threads; using it across threads is
// acceptable for the purposes of creating and owning a wgpu surface. Synchronization of window
// message handling remains the host app's responsibility.
unsafe impl Send for DxgiInteropHandle {}
unsafe impl Sync for DxgiInteropHandle {}
