//! SwapChainPanel renderer for Windows Runtime integration
//!
//! This module provides rendering support for Windows SwapChainPanel controls,
//! allowing Vello to render directly to WinRT UI components.

use crate::{
    CustomPaintSource, DebugTimer,
    wgpu_context::{DeviceHandle, WGPUContext},
};
use peniko::Color;
use rustc_hash::FxHashMap;
use std::sync::{
    Arc,
    atomic::{self, AtomicU64},
};
use vello::{
    AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions, Scene as VelloScene,
};
use wgpu::{CommandEncoderDescriptor, Features, Limits, PresentMode, Surface, SurfaceConfiguration, TextureViewDescriptor, TextureFormat, SurfaceTarget};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};

use crate::{DEFAULT_THREADS, VelloScenePainter};

static PAINT_SOURCE_ID: AtomicU64 = AtomicU64::new(0);

/// A handle to a Windows SwapChainPanel that can be used for rendering
pub struct SwapChainPanelHandle {
    panel_ptr: *mut std::ffi::c_void,
}

impl SwapChainPanelHandle {
    /// Creates a new SwapChainPanelHandle from a raw pointer
    ///
    /// # Safety
    ///
    /// The caller must ensure that `panel_ptr` is a valid pointer to a SwapChainPanel
    /// and that the panel remains valid for the lifetime of this handle.
    pub unsafe fn new(panel_ptr: *mut std::ffi::c_void) -> Self {
        Self { panel_ptr }
    }
}

impl HasWindowHandle for SwapChainPanelHandle {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let mut handle = Win32WindowHandle::new(std::num::NonZeroIsize::new(self.panel_ptr as isize).unwrap());
        let raw = RawWindowHandle::Win32(handle);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(raw)) }
    }
}

impl HasDisplayHandle for SwapChainPanelHandle {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = WindowsDisplayHandle::new();
        let raw = RawDisplayHandle::Windows(handle);
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(raw)) }
    }
}

// Safety: SwapChainPanel operations are thread-safe on Windows
unsafe impl Send for SwapChainPanelHandle {}
unsafe impl Sync for SwapChainPanelHandle {}

/// Custom surface structure for SwapChainPanel rendering
pub struct SwapChainSurface {
    pub surface: Surface<'static>,
    pub config: SurfaceConfiguration,
    pub device_handle: DeviceHandle,
    pub target_texture: wgpu::Texture,
    pub target_view: wgpu::TextureView,
    pub blitter: wgpu::util::TextureBlitter,
    panel_handle: Arc<SwapChainPanelHandle>,
}

impl SwapChainSurface {
    pub async fn new(
        panel_handle: Arc<SwapChainPanelHandle>,
        device_handle: DeviceHandle,
        width: u32,
        height: u32,
        present_mode: PresentMode,
        instance: &wgpu::Instance,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create surface from the SwapChainPanel handle
        let surface_target = SurfaceTarget::from(panel_handle.clone());
        let surface = instance.create_surface(surface_target)?;

        // Get surface capabilities
        let surface_caps = surface.get_capabilities(&device_handle.adapter);
        
        // Select format - prefer Bgra8Unorm for Windows
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| **f == TextureFormat::Bgra8Unorm)
            .or_else(|| surface_caps.formats.iter().find(|f| **f == TextureFormat::Rgba8Unorm))
            .copied()
            .ok_or("No supported surface format found")?;

        // Configure the surface
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        
        surface.configure(&device_handle.device, &config);

        // Create intermediate texture for Vello rendering
        let (target_texture, target_view) = Self::create_intermediate_texture(width, height, &device_handle.device);

        // Create blitter for copying from intermediate texture to surface
        let blitter = wgpu::util::TextureBlitter::new(&device_handle.device, surface_format);
        Ok(Self {
            surface,
            config,
            device_handle,
            target_texture,
            target_view,
            blitter,
            panel_handle,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device_handle.device, &self.config);
        
        // Recreate intermediate texture
        let (target_texture, target_view) = Self::create_intermediate_texture(width, height, &self.device_handle.device);
        self.target_texture = target_texture;
        self.target_view = target_view;
    }

    fn create_intermediate_texture(width: u32, height: u32, device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
        let target_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vello Intermediate Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            format: TextureFormat::Rgba8Unorm,
            view_formats: &[],
        });
        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());
        (target_texture, target_view)
    }
}

/// Render state for SwapChainPanel renderer
enum SwapChainRenderState {
    Active {
        renderer: VelloRenderer,
        surface: SwapChainSurface,
        panel_handle: Arc<SwapChainPanelHandle>,
    },
    Suspended,
}

impl SwapChainRenderState {
    fn current_device_handle(&self) -> Option<&DeviceHandle> {
        match self {
            SwapChainRenderState::Active { surface, .. } => Some(&surface.device_handle),
            SwapChainRenderState::Suspended => None,
        }
    }
}

/// Vello renderer for Windows SwapChainPanel controls
pub struct VelloSwapChainRenderer {
    render_state: SwapChainRenderState,
    wgpu_context: WGPUContext,
    scene: Option<VelloScene>,
    custom_paint_sources: FxHashMap<u64, Box<dyn CustomPaintSource>>,
}

impl VelloSwapChainRenderer {
    /// Creates a new SwapChainRenderer
    pub fn new() -> Self {
        Self::with_features_and_limits(None, None)
    }

    pub fn with_features_and_limits(features: Option<Features>, limits: Option<Limits>) -> Self {
        let features =
            features.unwrap_or_default() | Features::CLEAR_TEXTURE | Features::PIPELINE_CACHE;
        Self {
            wgpu_context: WGPUContext::with_features_and_limits(Some(features), limits),
            render_state: SwapChainRenderState::Suspended,
            scene: Some(VelloScene::new()),
            custom_paint_sources: FxHashMap::default(),
        }
    }

    /// Resumes rendering to the specified SwapChainPanel
    ///
    /// # Safety
    ///
    /// The caller must ensure that `panel_ptr` is a valid pointer to a SwapChainPanel
    /// and that the panel remains valid for the lifetime of the renderer.
    pub async unsafe fn resume_with_panel(
        &mut self,
        panel_ptr: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let panel_handle = Arc::new(unsafe { SwapChainPanelHandle::new(panel_ptr) });
        
        // Find or create a suitable device
        let dev_id = self
            .wgpu_context
            .find_or_create_device(None)
            .await
            .ok_or("No compatible device found")?;
        let device_handle = self.wgpu_context.device_pool[dev_id].clone();

        // Create the surface
        let surface = SwapChainSurface::new(
            panel_handle.clone(),
            device_handle,
            width,
            height,
            PresentMode::AutoVsync,
            &self.wgpu_context.instance,
        ).await?;

        // Create the renderer
        let options = RendererOptions {
            antialiasing_support: AaSupport::all(),
            use_cpu: false,
            num_init_threads: DEFAULT_THREADS,
            pipeline_cache: None,
        };
        let renderer = VelloRenderer::new(&surface.device_handle.device, options)?;

        self.render_state = SwapChainRenderState::Active {
            renderer,
            surface,
            panel_handle,
        };

        // Resume custom paint sources
        let device_handle = self.render_state.current_device_handle().unwrap();
        let instance = &self.wgpu_context.instance;
        for source in self.custom_paint_sources.values_mut() {
            source.resume(instance, device_handle);
        }

        Ok(())
    }

    pub fn suspend(&mut self) {
        for source in self.custom_paint_sources.values_mut() {
            source.suspend();
        }
        self.render_state = SwapChainRenderState::Suspended;
    }

    pub fn is_active(&self) -> bool {
        matches!(self.render_state, SwapChainRenderState::Active { .. })
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        if let SwapChainRenderState::Active { surface, .. } = &mut self.render_state {
            surface.resize(width, height);
        }
    }

    pub fn current_device_handle(&self) -> Option<&DeviceHandle> {
        self.render_state.current_device_handle()
    }

    pub fn register_custom_paint_source(&mut self, mut source: Box<dyn CustomPaintSource>) -> u64 {
        if let Some(device_handle) = self.render_state.current_device_handle() {
            let instance = &self.wgpu_context.instance;
            source.resume(instance, device_handle);
        }
        let id = PAINT_SOURCE_ID.fetch_add(1, atomic::Ordering::SeqCst);
        self.custom_paint_sources.insert(id, source);
        id
    }

    pub fn unregister_custom_paint_source(&mut self, id: u64) {
        if let Some(mut source) = self.custom_paint_sources.remove(&id) {
            source.suspend();
        }
    }

    /// Renders the scene to the SwapChainPanel
    pub fn render<F: FnOnce(&mut VelloScenePainter<'_>)>(&mut self, draw_fn: F) -> Result<(), Box<dyn std::error::Error>> {
        let SwapChainRenderState::Active { renderer, surface, .. } = &mut self.render_state else {
            return Ok(());
        };

        let device_handle = &surface.device_handle;
        let mut timer = DebugTimer::init();

        let render_params = RenderParams {
            base_color: Color::WHITE,
            width: surface.config.width,
            height: surface.config.height,
            antialiasing_method: vello::AaConfig::Msaa16,
        };

        // Regenerate the vello scene
        let mut scene_painter = VelloScenePainter {
            inner: self.scene.take().unwrap(),
            renderer,
            custom_paint_sources: &mut self.custom_paint_sources,
        };
        draw_fn(&mut scene_painter);
        self.scene = Some(scene_painter.finish());
        timer.record_time("cmd");

        // Render to intermediate texture
        renderer.render_to_texture(
            &device_handle.device,
            &device_handle.queue,
            self.scene.as_ref().unwrap(),
            &surface.target_view,
            &render_params,
        )?;
        timer.record_time("render");

        // Get surface texture and copy from intermediate
        let surface_texture = surface.surface.get_current_texture()?;
        let mut encoder = device_handle.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("SwapChain Surface Blit"),
        });

        surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &surface.target_view,
            &surface_texture.texture.create_view(&TextureViewDescriptor::default()),
        );
        
        device_handle.queue.submit([encoder.finish()]);
        surface_texture.present();
        timer.record_time("present");

        device_handle.device.poll(wgpu::Maintain::Wait);
        timer.record_time("wait");
        timer.print_times("SwapChain Frame time: ");

        // Reset scene for next frame
        self.scene.as_mut().unwrap().reset();

        Ok(())
    }
}

impl Default for VelloSwapChainRenderer {
    fn default() -> Self {
        Self::new()
    }
}
