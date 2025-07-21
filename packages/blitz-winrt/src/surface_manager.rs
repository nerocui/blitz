//! # Surface Manager for SwapChainPanel Integration
//!
//! This module handles the creation and management of WGPU surfaces that render to
//! Windows SwapChainPanel controls instead of traditional window handles.
//!
//! ## Key Responsibilities
//!
//! - Create WGPU surfaces from SwapChainPanel pointers
//! - Manage surface configuration and resizing
//! - Handle the bridge between Windows DirectX and WGPU
//! - Provide surface information for viewport management

use std::ptr::NonNull;
use windows_core::Result;
use wgpu::{Instance, Surface, SurfaceTarget, Adapter, Device, Queue};

/// Information about the current surface state.
///
/// This includes dimensions, scale factor, and other properties needed
/// for proper rendering setup.
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    /// Width of the surface in pixels
    pub width: u32,
    /// Height of the surface in pixels  
    pub height: u32,
    /// Scale factor for high-DPI displays
    pub scale_factor: f32,
    /// Whether the surface supports alpha blending
    pub supports_alpha: bool,
}

/// Manages WGPU surface creation and lifecycle for SwapChainPanel rendering.
///
/// This struct encapsulates the complex process of creating a WGPU surface
/// from a Windows SwapChainPanel control, handling the necessary DirectX
/// integration and surface configuration.
#[derive(Debug)]
pub struct SurfaceManager {
    /// The WGPU instance used for surface creation
    instance: Instance,
    
    /// The created surface for rendering
    surface: Option<Surface<'static>>,
    
    /// Pointer to the SwapChainPanel control
    swap_chain_panel: NonNull<std::ffi::c_void>,
    
    /// Current surface information
    surface_info: SurfaceInfo,
    
    /// WGPU adapter for this surface
    adapter: Option<Adapter>,
    
    /// WGPU device for rendering
    device: Option<Device>,
    
    /// WGPU queue for command submission
    queue: Option<Queue>,
}

impl SurfaceManager {
    /// Creates a new SurfaceManager for the given SwapChainPanel.
    ///
    /// # Arguments
    ///
    /// * `swap_chain_panel` - Pointer to the SwapChainPanel control
    ///
    /// # Returns
    ///
    /// A new SurfaceManager instance
    ///
    /// # Safety
    ///
    /// The `swap_chain_panel` pointer must be valid and point to a valid
    /// SwapChainPanel control that will remain alive for the lifetime of
    /// this SurfaceManager.
    pub fn new(swap_chain_panel: *mut std::ffi::c_void) -> Result<Self> {
        // Validate the pointer
        let panel_ptr = NonNull::new(swap_chain_panel)
            .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(0x80070057u32 as i32)))?; // E_INVALIDARG
        
        // Create WGPU instance with DX12 backend for Windows
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12, // Use DX12 for SwapChainPanel compatibility
            flags: wgpu::InstanceFlags::default(),
            ..Default::default()
        });
        
        // Initialize with default surface info - will be updated when surface is created
        let surface_info = SurfaceInfo {
            width: 800,
            height: 600,
            scale_factor: 1.0,
            supports_alpha: true,
        };
        
        let mut manager = SurfaceManager {
            instance,
            surface: None,
            swap_chain_panel: panel_ptr,
            surface_info,
            adapter: None,
            device: None,
            queue: None,
        };
        
        // Create the surface immediately
        manager.create_surface()?;
        
        Ok(manager)
    }
    
    /// Creates a WGPU surface from the SwapChainPanel.
    ///
    /// This method uses the unsafe WGPU surface creation API to create a surface
    /// from the SwapChainPanel pointer. It handles the DirectX integration
    /// necessary for proper rendering.
    fn create_surface(&mut self) -> Result<()> {
        // Create surface target for SwapChainPanel
        let surface_target = wgpu::SurfaceTargetUnsafe::SwapChainPanel(self.swap_chain_panel.as_ptr());
        
        // Create the surface
        // SAFETY: We've validated that the SwapChainPanel pointer is non-null
        // and we assume it points to a valid SwapChainPanel control
        let surface = unsafe {
            self.instance.create_surface_unsafe(surface_target)
                .map_err(|e| windows_core::Error::from_hresult(windows_core::HRESULT(0x80004005u32 as i32)))? // E_FAIL
        };
        
        self.surface = Some(surface);
        
        // TODO: Get actual surface dimensions from the SwapChainPanel
        // For now, we'll use defaults and update them later
        self.update_surface_info();
        
        Ok(())
    }
    
    /// Updates the surface information by querying the SwapChainPanel.
    ///
    /// This method should be called when the SwapChainPanel is resized
    /// or when DPI changes occur.
    fn update_surface_info(&mut self) {
        // TODO: Query the actual SwapChainPanel for its current size and properties
        // This would involve calling into Windows APIs to get the panel's dimensions
        // For now, we'll use placeholder values
        
        self.surface_info = SurfaceInfo {
            width: 800,
            height: 600,
            scale_factor: 1.0,
            supports_alpha: true,
        };
    }
    
    /// Gets the current surface information.
    ///
    /// # Returns
    ///
    /// A copy of the current SurfaceInfo
    pub fn get_surface_info(&self) -> SurfaceInfo {
        self.surface_info.clone()
    }
    
    /// Gets a reference to the WGPU surface.
    ///
    /// # Returns
    ///
    /// An optional reference to the surface, None if not yet created
    pub fn get_surface(&self) -> Option<&Surface<'static>> {
        self.surface.as_ref()
    }
    
    /// Gets a reference to the WGPU instance.
    ///
    /// # Returns
    ///
    /// A reference to the WGPU instance
    pub fn get_instance(&self) -> &Instance {
        &self.instance
    }
    
    /// Initializes the WGPU adapter, device, and queue for this surface.
    ///
    /// This method must be called before rendering can begin. It finds
    /// a compatible adapter, creates a device, and sets up the command queue.
    ///
    /// # Returns
    ///
    /// Result indicating success or failure of initialization
    pub async fn initialize_device(&mut self) -> Result<()> {
        let surface = self.surface.as_ref()
            .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(0x80004005u32 as i32)))?; // E_FAIL
        
        // Find a compatible adapter
        let adapter = self.instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(0x80004005u32 as i32)))?; // E_FAIL
        
        // Create device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Blitz WinRT Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|_| windows_core::Error::from_hresult(windows_core::HRESULT(0x80004005u32 as i32)))?; // E_FAIL
        
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        
        Ok(())
    }
    
    /// Gets references to the device and queue.
    ///
    /// # Returns
    ///
    /// Optional tuple of (device, queue) references
    pub fn get_device_and_queue(&self) -> Option<(&Device, &Queue)> {
        if let (Some(device), Some(queue)) = (&self.device, &self.queue) {
            Some((device, queue))
        } else {
            None
        }
    }
    
    /// Resizes the surface to new dimensions.
    ///
    /// This method should be called when the SwapChainPanel is resized
    /// to ensure the surface matches the new size.
    ///
    /// # Arguments
    ///
    /// * `width` - New width in pixels
    /// * `height` - New height in pixels
    /// * `scale_factor` - New scale factor for DPI changes
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) -> Result<()> {
        self.surface_info.width = width;
        self.surface_info.height = height;
        self.surface_info.scale_factor = scale_factor;
        
        // TODO: Notify the surface about the size change
        // This might involve reconfiguring the surface or recreating it
        
        Ok(())
    }
}

impl Drop for SurfaceManager {
    /// Cleanup when the SurfaceManager is dropped.
    ///
    /// This ensures proper cleanup of WGPU resources.
    fn drop(&mut self) {
        // WGPU resources will be automatically cleaned up
        // The SwapChainPanel pointer is not owned by us, so we don't free it
    }
}

// Ensure SurfaceManager can be safely used across threads
// This is necessary for the WinRT threading model
unsafe impl Send for SurfaceManager {}
unsafe impl Sync for SurfaceManager {}
