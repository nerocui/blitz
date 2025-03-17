// Bindings generated by `windows-bindgen` 0.60.0

#![allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code,
    clippy::all
)]

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct D2DRenderer(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    D2DRenderer,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl D2DRenderer {
    pub fn Render(&self, markdown: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).Render)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(markdown),
            )
            .ok()
        }
    }
    pub fn CreateInstance(d2ddevicecontext: u64) -> windows_core::Result<D2DRenderer> {
        Self::ID2DRendererFactory(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).CreateInstance)(
                windows_core::Interface::as_raw(this),
                d2ddevicecontext,
                &mut result__,
            )
            .and_then(|| windows_core::Type::from_abi(result__))
        })
    }
    fn ID2DRendererFactory<R, F: FnOnce(&ID2DRendererFactory) -> windows_core::Result<R>>(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<D2DRenderer, ID2DRendererFactory> =
            windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeType for D2DRenderer {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, ID2DRenderer>();
}
unsafe impl windows_core::Interface for D2DRenderer {
    type Vtable = <ID2DRenderer as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <ID2DRenderer as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for D2DRenderer {
    const NAME: &'static str = "BlitzWinRT.D2DRenderer";
}
unsafe impl Send for D2DRenderer {}
unsafe impl Sync for D2DRenderer {}
windows_core::imp::define_interface!(
    ID2DRenderer,
    ID2DRenderer_Vtbl,
    0x65e142b2_e90a_5daa_87c0_69688748b7af
);
impl windows_core::RuntimeType for ID2DRenderer {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
impl windows_core::RuntimeName for ID2DRenderer {
    const NAME: &'static str = "BlitzWinRT.ID2DRenderer";
}
pub trait ID2DRenderer_Impl: windows_core::IUnknownImpl {
    fn Render(&self, markdown: &windows_core::HSTRING) -> windows_core::Result<()>;
}
impl ID2DRenderer_Vtbl {
    pub const fn new<Identity: ID2DRenderer_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn Render<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            markdown: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::Render(this, core::mem::transmute(&markdown)).into()
            }
        }
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, ID2DRenderer, OFFSET>(),
            Render: Render::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<ID2DRenderer as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct ID2DRenderer_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub Render: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    ID2DRendererFactory,
    ID2DRendererFactory_Vtbl,
    0xa61732f0_1a1e_55fe_8c25_75ea15b0073c
);
impl windows_core::RuntimeType for ID2DRendererFactory {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
impl windows_core::RuntimeName for ID2DRendererFactory {
    const NAME: &'static str = "BlitzWinRT.ID2DRendererFactory";
}
pub trait ID2DRendererFactory_Impl: windows_core::IUnknownImpl {
    fn CreateInstance(&self, d2dDeviceContext: u64) -> windows_core::Result<D2DRenderer>;
}
impl ID2DRendererFactory_Vtbl {
    pub const fn new<Identity: ID2DRendererFactory_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn CreateInstance<
            Identity: ID2DRendererFactory_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            d2ddevicecontext: u64,
            result__: *mut *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match ID2DRendererFactory_Impl::CreateInstance(this, d2ddevicecontext) {
                    Ok(ok__) => {
                        result__.write(core::mem::transmute_copy(&ok__));
                        core::mem::forget(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, ID2DRendererFactory, OFFSET>(),
            CreateInstance: CreateInstance::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<ID2DRendererFactory as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct ID2DRendererFactory_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub CreateInstance: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u64,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
