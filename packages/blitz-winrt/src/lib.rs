mod bindings;
mod d2drenderer;
use std::{mem::{transmute, ManuallyDrop}, ops::DerefMut};
use d2drenderer::{D2DRenderer, D2DRenderer_Impl};
use windows::{core::*, Win32::{Foundation::*, Graphics::Direct2D::ID2D1DeviceContext, System::WinRT::*}};


#[implement(IActivationFactory, bindings::ID2DRendererFactory)]
struct D2DRendererFactory;

impl IActivationFactory_Impl for D2DRendererFactory_Impl {
    fn ActivateInstance(&self) -> Result<IInspectable> {
        Err(E_NOTIMPL.into())
    }
}

impl DerefMut for D2DRenderer_Impl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self
    }
}

impl ComObjectInner for D2DRendererFactory_Impl {
    type Outer = D2DRendererFactory;

    fn into_object(self) -> ComObject<Self> {
        todo!()
    }
}

impl windows_core::IUnknownImpl for D2DRendererFactory {
    type Impl = D2DRendererFactory_Impl;

    fn get_impl(&self) -> &Self::Impl {
        todo!()
    }

    fn get_impl_mut(&mut self) -> &mut Self::Impl {
        todo!()
    }

    fn into_inner(self) -> Self::Impl {
        todo!()
    }

    unsafe fn QueryInterface(&self, iid: *const GUID, interface: *mut *mut std::ffi::c_void) -> HRESULT {
        todo!()
    }

    fn AddRef(&self) -> u32 {
        todo!()
    }

    unsafe fn Release(self_: *mut Self) -> u32 {
        todo!()
    }

    fn is_reference_count_one(&self) -> bool {
        todo!()
    }

    unsafe fn GetTrustLevel(&self, value: *mut i32) -> HRESULT {
        todo!()
    }

    unsafe fn from_inner_ref(inner: &Self::Impl) -> &Self {
        todo!()
    }

    fn to_object(&self) -> ComObject<Self::Impl> {
        todo!()
    }

    const INNER_OFFSET_IN_POINTERS: usize = 0;
}

impl bindings::ID2DRendererFactory_Impl for D2DRendererFactory_Impl {
    fn CreateInstance(&self, device_context_ptr: u64) -> Result<bindings::D2DRenderer> {
        unsafe {
            // Convert numeric pointer to ID2D1DeviceContext
            let context: ID2D1DeviceContext = std::mem::transmute(device_context_ptr);
            let context = context.clone(); // Clone to ensure ownership
            
            Ok(D2DRenderer::new(context).into())
        }
    }
}

#[no_mangle]
unsafe extern "system" fn DllGetActivationFactory(
    device_context_ptr: u64,
    result: *mut *mut std::ffi::c_void,
) -> HRESULT {
    unsafe {
        // Convert numeric pointer to ID2D1DeviceContext
        let context: ID2D1DeviceContext = std::mem::transmute(device_context_ptr);
        let context = context.clone(); // Clone to ensure ownership
        let instance: bindings::D2DRenderer = crate::d2drenderer::D2DRenderer::new(context).into();

    
        *result = transmute(instance);
        S_OK
    }
}
