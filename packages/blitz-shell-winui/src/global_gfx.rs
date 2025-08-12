use std::sync::OnceLock;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_DEBUG, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use crate::winrt_component::debug_log;

struct GlobalDevice {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    feature_level: D3D_FEATURE_LEVEL,
    _thread_id: std::thread::ThreadId,
}

static GLOBAL_DEVICE: OnceLock<GlobalDevice> = OnceLock::new();

pub(crate) struct DeviceAcquireResult {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub feature_level: D3D_FEATURE_LEVEL,
    pub created: bool,
    pub create_ms: f32,
}

pub(crate) fn get_or_create_d3d_device() -> Option<DeviceAcquireResult> {
    if let Some(glob) = GLOBAL_DEVICE.get() {
        return Some(DeviceAcquireResult { device: glob.device.clone(), context: glob.context.clone(), feature_level: glob.feature_level, created: false, create_ms: 0.0 });
    }
    let start = std::time::Instant::now();
    unsafe {
        let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut chosen: D3D_FEATURE_LEVEL = D3D_FEATURE_LEVEL_11_0;
        let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        #[cfg(debug_assertions)]
        { flags |= D3D11_CREATE_DEVICE_DEBUG; }
    let mut try_create = |flags| {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut chosen),
                Some(&mut context),
            )
        };
        let hr = try_create(flags);
        if hr.is_err() {
            #[cfg(debug_assertions)]
            {
                if (flags & D3D11_CREATE_DEVICE_DEBUG) == D3D11_CREATE_DEVICE_DEBUG {
                    debug_log("global_gfx: retry without DEBUG layer");
                    let fallback = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
                    if try_create(fallback).is_err() { return None; }
                } else { return None; }
            }
            #[cfg(not(debug_assertions))]
            { return None; }
        }
        let device = device.unwrap();
        let context = context.unwrap();
        let create_ms = start.elapsed().as_secs_f32()*1000.0;
        let _thread_id = std::thread::current().id();
        let _ = GLOBAL_DEVICE.set(GlobalDevice { device: device.clone(), context: context.clone(), feature_level: chosen, _thread_id });
        debug_log(&format!("global_gfx: created shared D3D device (feature {:?}) in {:.2} ms", chosen, create_ms));
        Some(DeviceAcquireResult { device, context, feature_level: chosen, created: true, create_ms })
    }
}
