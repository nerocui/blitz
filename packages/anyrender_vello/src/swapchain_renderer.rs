use crate::{
    CustomPaintSource,
    wgpu_context::{DeviceHandle, WGPUContext},
    VelloScenePainter, DEFAULT_THREADS,
};
use rustc_hash::FxHashMap;
use std::sync::atomic::{self, AtomicU64};
use vello::{AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions, Scene as VelloScene};

static PAINT_SOURCE_ID: AtomicU64 = AtomicU64::new(0);

/// Experimental renderer targeting an externally managed DXGI swapchain.
/// This mirrors VelloWindowRenderer but does not own a wgpu::Surface. Instead,
/// the host is expected to supply a present path. We render into an internal
/// wgpu texture and expose hooks for copying into the swapchain backbuffer.
pub struct VelloSwapchainRenderer {
    // Vello core
    wgpu_context: WGPUContext,
    renderer: Option<VelloRenderer>,
    device_handle: Option<DeviceHandle>,

    // Intermediate target we render into (same as window renderer)
    target_texture: Option<wgpu::Texture>,
    pub target_view: Option<wgpu::TextureView>,
    pub target_format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    gpu_buffer: Option<wgpu::Buffer>,

    // Scene + custom paints
    scene: Option<VelloScene>,
    custom_paint_sources: FxHashMap<u64, Box<dyn crate::CustomPaintSource>>,
}

impl VelloSwapchainRenderer {
    pub fn new() -> Self {
        // Default to BGRA8, which matches DXGI_FORMAT_B8G8R8A8_UNORM
        Self {
            wgpu_context: WGPUContext::new(),
            renderer: None,
            device_handle: None,
            target_texture: None,
            target_view: None,
            target_format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            gpu_buffer: None,
            scene: Some(VelloScene::new()),
            custom_paint_sources: FxHashMap::default(),
        }
    }

    /// Initialize device and intermediate target without a surface.
    pub fn resume(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);

        // Find or create any compatible device (no surface)
        let dev_id = pollster::block_on(self.wgpu_context.find_or_create_device(None))
            .expect("No compatible device found for Vello");
        let device_handle = self.wgpu_context.device_pool[dev_id].clone();

        let options = RendererOptions {
            antialiasing_support: AaSupport::all(),
            use_cpu: false,
            num_init_threads: DEFAULT_THREADS,
            pipeline_cache: None,
        };
        let renderer = VelloRenderer::new(&device_handle.device, options).unwrap();

    // Create intermediate target
        let texture = device_handle.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("VelloSwapchainRenderer target"),
            size: wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, // Vello expects RGBA8 for storage
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create readback buffer sized to padded row width
        let padded_byte_width = (self.width * 4).next_multiple_of(256);
        let buffer_size = padded_byte_width as u64 * self.height as u64;
        let gpu_buffer = device_handle.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VelloSwapchainRenderer readback buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Publish state
        self.device_handle = Some(device_handle);
        self.renderer = Some(renderer);
    self.target_texture = Some(texture);
        self.target_view = Some(view);
    self.gpu_buffer = Some(gpu_buffer);

        // Notify custom paint sources we have an active device
        let instance = &self.wgpu_context.instance;
        if let Some(dev) = &self.device_handle {
            for source in self.custom_paint_sources.values_mut() {
                source.resume(instance, dev)
            }
        }
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height { return; }
        self.resume(width, height);
    }

    pub fn render<F: FnOnce(&mut VelloScenePainter<'_>)>(&mut self, draw_fn: F) {
        let (renderer, device_handle) = match (&mut self.renderer, &self.device_handle) {
            (Some(r), Some(d)) => (r, d),
            _ => return,
        };
        let target_view = match &self.target_view { Some(v) => v, None => return };

        // Regenerate the vello scene
        let mut scene = VelloScenePainter {
            inner: self.scene.take().unwrap(),
            renderer,
            custom_paint_sources: &mut self.custom_paint_sources,
        };
        draw_fn(&mut scene);
        self.scene = Some(scene.finish());

        let render_params = RenderParams {
            base_color: peniko::Color::WHITE,
            width: self.width,
            height: self.height,
            antialiasing_method: vello::AaConfig::Msaa16,
        };

        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                self.scene.as_ref().unwrap(),
                target_view,
                &render_params,
            )
            .expect("failed to render to intermediate texture");
        device_handle.device.poll(wgpu::Maintain::Wait);

        // Note: Present step is intentionally left to the host using the backbuffer API.
    }

    /// Read back the rendered RGBA pixels into the provided Vec (width*height*4).
    /// Returns (row_pitch in bytes) used for each row in the output buffer.
    pub fn readback_rgba(&mut self, out: &mut Vec<u8>) -> usize {
        let (device_handle, texture, gpu_buffer) = match (&self.device_handle, &self.target_texture, &self.gpu_buffer) {
            (Some(d), Some(t), Some(b)) => (d, t, b),
            _ => return 0,
        };

        // Copy texture -> buffer
        let mut encoder = device_handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("SwapchainRenderer Copy out") });
        let padded_byte_width = (self.width * 4).next_multiple_of(256);
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: gpu_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_byte_width),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        device_handle.queue.submit([encoder.finish()]);

        // Map and copy to output vec
        let slice = gpu_buffer.slice(..);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
        if let Some(res) = crate::wgpu_context::block_on_wgpu(&device_handle.device, receiver.receive()) { res.unwrap(); } else { return 0; }
        let data = slice.get_mapped_range();

        out.clear();
        out.reserve((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * padded_byte_width).try_into().unwrap();
            out.extend(&data[start..start + (self.width * 4) as usize]);
        }
        drop(data);
        gpu_buffer.unmap();

        (self.width as usize) * 4
    }

    pub fn register_custom_paint_source(&mut self, mut source: Box<dyn crate::CustomPaintSource>) -> u64 {
        if let Some(device_handle) = &self.device_handle {
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
            drop(source);
        }
    }
}
