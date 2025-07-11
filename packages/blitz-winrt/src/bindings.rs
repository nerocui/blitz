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
    pub fn SetLogger<P0>(&self, logger: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<ILogger>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetLogger)(
                windows_core::Interface::as_raw(this),
                logger.param().abi(),
            )
            .ok()
        }
    }
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
    pub fn Resize(&self, width: u32, height: u32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).Resize)(
                windows_core::Interface::as_raw(this),
                width,
                height,
            )
            .ok()
        }
    }
    pub fn OnPointerMoved(&self, x: f32, y: f32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnPointerMoved)(
                windows_core::Interface::as_raw(this),
                x,
                y,
            )
            .ok()
        }
    }
    pub fn OnPointerPressed(&self, x: f32, y: f32, button: u32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnPointerPressed)(
                windows_core::Interface::as_raw(this),
                x,
                y,
                button,
            )
            .ok()
        }
    }
    pub fn OnPointerReleased(&self, x: f32, y: f32, button: u32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnPointerReleased)(
                windows_core::Interface::as_raw(this),
                x,
                y,
                button,
            )
            .ok()
        }
    }
    pub fn OnMouseWheel(&self, deltax: f32, deltay: f32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnMouseWheel)(
                windows_core::Interface::as_raw(this),
                deltax,
                deltay,
            )
            .ok()
        }
    }
    pub fn OnKeyDown(
        &self,
        keycode: u32,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnKeyDown)(
                windows_core::Interface::as_raw(this),
                keycode,
                ctrl,
                shift,
                alt,
            )
            .ok()
        }
    }
    pub fn OnKeyUp(&self, keycode: u32) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnKeyUp)(
                windows_core::Interface::as_raw(this),
                keycode,
            )
            .ok()
        }
    }
    pub fn OnTextInput(&self, text: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnTextInput)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(text),
            )
            .ok()
        }
    }
    pub fn OnBlur(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnBlur)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn OnFocus(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).OnFocus)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn Suspend(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).Suspend)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn Resume(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).Resume)(windows_core::Interface::as_raw(this))
                .ok()
        }
    }
    pub fn SetTheme(&self, isdarkmode: bool) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetTheme)(
                windows_core::Interface::as_raw(this),
                isdarkmode,
            )
            .ok()
        }
    }
    pub fn Tick(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).Tick)(windows_core::Interface::as_raw(this)).ok()
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
    0xe4dd8c6c_d185_5078_bb97_e7ed9464e31f
);
impl windows_core::RuntimeType for ID2DRenderer {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
impl windows_core::RuntimeName for ID2DRenderer {
    const NAME: &'static str = "BlitzWinRT.ID2DRenderer";
}
pub trait ID2DRenderer_Impl: windows_core::IUnknownImpl {
    fn SetLogger(&self, logger: windows_core::Ref<'_, ILogger>) -> windows_core::Result<()>;
    fn Render(&self, markdown: &windows_core::HSTRING) -> windows_core::Result<()>;
    fn Resize(&self, width: u32, height: u32) -> windows_core::Result<()>;
    fn OnPointerMoved(&self, x: f32, y: f32) -> windows_core::Result<()>;
    fn OnPointerPressed(&self, x: f32, y: f32, button: u32) -> windows_core::Result<()>;
    fn OnPointerReleased(&self, x: f32, y: f32, button: u32) -> windows_core::Result<()>;
    fn OnMouseWheel(&self, deltaX: f32, deltaY: f32) -> windows_core::Result<()>;
    fn OnKeyDown(
        &self,
        keyCode: u32,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> windows_core::Result<()>;
    fn OnKeyUp(&self, keyCode: u32) -> windows_core::Result<()>;
    fn OnTextInput(&self, text: &windows_core::HSTRING) -> windows_core::Result<()>;
    fn OnBlur(&self) -> windows_core::Result<()>;
    fn OnFocus(&self) -> windows_core::Result<()>;
    fn Suspend(&self) -> windows_core::Result<()>;
    fn Resume(&self) -> windows_core::Result<()>;
    fn SetTheme(&self, isDarkMode: bool) -> windows_core::Result<()>;
    fn Tick(&self) -> windows_core::Result<()>;
}
impl ID2DRenderer_Vtbl {
    pub const fn new<Identity: ID2DRenderer_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn SetLogger<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            logger: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::SetLogger(this, core::mem::transmute_copy(&logger)).into()
            }
        }
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
        unsafe extern "system" fn Resize<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            width: u32,
            height: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::Resize(this, width, height).into()
            }
        }
        unsafe extern "system" fn OnPointerMoved<
            Identity: ID2DRenderer_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            x: f32,
            y: f32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnPointerMoved(this, x, y).into()
            }
        }
        unsafe extern "system" fn OnPointerPressed<
            Identity: ID2DRenderer_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            x: f32,
            y: f32,
            button: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnPointerPressed(this, x, y, button).into()
            }
        }
        unsafe extern "system" fn OnPointerReleased<
            Identity: ID2DRenderer_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            x: f32,
            y: f32,
            button: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnPointerReleased(this, x, y, button).into()
            }
        }
        unsafe extern "system" fn OnMouseWheel<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            deltax: f32,
            deltay: f32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnMouseWheel(this, deltax, deltay).into()
            }
        }
        unsafe extern "system" fn OnKeyDown<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            keycode: u32,
            ctrl: bool,
            shift: bool,
            alt: bool,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnKeyDown(this, keycode, ctrl, shift, alt).into()
            }
        }
        unsafe extern "system" fn OnKeyUp<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            keycode: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnKeyUp(this, keycode).into()
            }
        }
        unsafe extern "system" fn OnTextInput<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            text: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnTextInput(this, core::mem::transmute(&text)).into()
            }
        }
        unsafe extern "system" fn OnBlur<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnBlur(this).into()
            }
        }
        unsafe extern "system" fn OnFocus<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::OnFocus(this).into()
            }
        }
        unsafe extern "system" fn Suspend<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::Suspend(this).into()
            }
        }
        unsafe extern "system" fn Resume<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::Resume(this).into()
            }
        }
        unsafe extern "system" fn SetTheme<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            isdarkmode: bool,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::SetTheme(this, isdarkmode).into()
            }
        }
        unsafe extern "system" fn Tick<Identity: ID2DRenderer_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ID2DRenderer_Impl::Tick(this).into()
            }
        }
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, ID2DRenderer, OFFSET>(),
            SetLogger: SetLogger::<Identity, OFFSET>,
            Render: Render::<Identity, OFFSET>,
            Resize: Resize::<Identity, OFFSET>,
            OnPointerMoved: OnPointerMoved::<Identity, OFFSET>,
            OnPointerPressed: OnPointerPressed::<Identity, OFFSET>,
            OnPointerReleased: OnPointerReleased::<Identity, OFFSET>,
            OnMouseWheel: OnMouseWheel::<Identity, OFFSET>,
            OnKeyDown: OnKeyDown::<Identity, OFFSET>,
            OnKeyUp: OnKeyUp::<Identity, OFFSET>,
            OnTextInput: OnTextInput::<Identity, OFFSET>,
            OnBlur: OnBlur::<Identity, OFFSET>,
            OnFocus: OnFocus::<Identity, OFFSET>,
            Suspend: Suspend::<Identity, OFFSET>,
            Resume: Resume::<Identity, OFFSET>,
            SetTheme: SetTheme::<Identity, OFFSET>,
            Tick: Tick::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<ID2DRenderer as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct ID2DRenderer_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub SetLogger: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Render: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Resize:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, u32) -> windows_core::HRESULT,
    pub OnPointerMoved:
        unsafe extern "system" fn(*mut core::ffi::c_void, f32, f32) -> windows_core::HRESULT,
    pub OnPointerPressed:
        unsafe extern "system" fn(*mut core::ffi::c_void, f32, f32, u32) -> windows_core::HRESULT,
    pub OnPointerReleased:
        unsafe extern "system" fn(*mut core::ffi::c_void, f32, f32, u32) -> windows_core::HRESULT,
    pub OnMouseWheel:
        unsafe extern "system" fn(*mut core::ffi::c_void, f32, f32) -> windows_core::HRESULT,
    pub OnKeyDown: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        bool,
        bool,
        bool,
    ) -> windows_core::HRESULT,
    pub OnKeyUp: unsafe extern "system" fn(*mut core::ffi::c_void, u32) -> windows_core::HRESULT,
    pub OnTextInput: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub OnBlur: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub OnFocus: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub Suspend: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub Resume: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub SetTheme: unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> windows_core::HRESULT,
    pub Tick: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
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
windows_core::imp::define_interface!(
    ILogger,
    ILogger_Vtbl,
    0xb9131dda_1428_539d_9a71_644070e26ebb
);
impl windows_core::RuntimeType for ILogger {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
windows_core::imp::interface_hierarchy!(
    ILogger,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl ILogger {
    pub fn LogMessage(&self, message: &windows_core::HSTRING) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).LogMessage)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(message),
            )
            .ok()
        }
    }
    pub fn LogWithCategory(
        &self,
        message: &windows_core::HSTRING,
        category: &windows_core::HSTRING,
        location: &windows_core::HSTRING,
    ) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).LogWithCategory)(
                windows_core::Interface::as_raw(this),
                core::mem::transmute_copy(message),
                core::mem::transmute_copy(category),
                core::mem::transmute_copy(location),
            )
            .ok()
        }
    }
}
impl windows_core::RuntimeName for ILogger {
    const NAME: &'static str = "BlitzWinRT.ILogger";
}
pub trait ILogger_Impl: windows_core::IUnknownImpl {
    fn LogMessage(&self, message: &windows_core::HSTRING) -> windows_core::Result<()>;
    fn LogWithCategory(
        &self,
        message: &windows_core::HSTRING,
        category: &windows_core::HSTRING,
        location: &windows_core::HSTRING,
    ) -> windows_core::Result<()>;
}
impl ILogger_Vtbl {
    pub const fn new<Identity: ILogger_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn LogMessage<Identity: ILogger_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            message: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ILogger_Impl::LogMessage(this, core::mem::transmute(&message)).into()
            }
        }
        unsafe extern "system" fn LogWithCategory<Identity: ILogger_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            message: *mut core::ffi::c_void,
            category: *mut core::ffi::c_void,
            location: *mut core::ffi::c_void,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                ILogger_Impl::LogWithCategory(
                    this,
                    core::mem::transmute(&message),
                    core::mem::transmute(&category),
                    core::mem::transmute(&location),
                )
                .into()
            }
        }
        Self {
            base__: windows_core::IInspectable_Vtbl::new::<Identity, ILogger, OFFSET>(),
            LogMessage: LogMessage::<Identity, OFFSET>,
            LogWithCategory: LogWithCategory::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<ILogger as windows_core::Interface>::IID
    }
}
#[repr(C)]
pub struct ILogger_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub LogMessage: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub LogWithCategory: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
}
