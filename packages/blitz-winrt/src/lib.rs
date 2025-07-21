//! # Blitz WinRT Package
//!
//! This package provides a Windows Runtime (WinRT) wrapper around the Blitz HTML/CSS rendering engine.
//! It serves a similar role to `blitz-shell` but instead of rendering to a full window, 
//! it renders to a SwapChainPanel control that can be embedded in WinUI/UWP applications.
//!
//! ## Architecture
//!
//! - **BlitzView**: The main WinRT component that handles rendering and event management
//! - **Surface Management**: Uses WGPU with SwapChainPanel instead of window handles
//! - **Event Handling**: Converts Windows events to Blitz-compatible events
//! - **Rendering**: Leverages anyrender_vello for GPU-accelerated vector graphics
//!
//! ## Usage
//!
//! The BlitzView can be instantiated from C#/WinUI with a SwapChainPanel and markdown content:
//!
//! ```csharp
//! var blitzView = new BlitzView((ulong)mySwapChainPanel.NativePtr, markdownContent);
//! blitzView.SetTheme(isDarkMode);
//! ```

// Import necessary modules
mod surface_manager;
mod event_conversion;
mod view_impl;

#[cfg(test)]
mod examples;

// Include the generated WinRT bindings
#[allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code,
    clippy::all
)]
mod bindings;
pub use bindings::*;

use windows_core::{Result, HSTRING};
use windows::core::implement;
use std::sync::{Arc, Mutex};

use surface_manager::SurfaceManager;
use event_conversion::{EventConverter, WindowsMessage};
use view_impl::BlitzViewImpl as CoreBlitzViewImpl;

/// State shared between WinRT interface and implementation
#[derive(Debug)]
pub struct BlitzViewState {
    /// Whether dark mode is currently enabled
    pub dark_mode: Arc<Mutex<bool>>,
    
    /// The SwapChainPanel pointer for rendering
    pub swap_chain_panel: *mut std::ffi::c_void,
    
    /// The markdown content being rendered
    pub markdown_content: String,
    
    /// The core implementation that handles rendering
    pub core_impl: Option<Arc<Mutex<CoreBlitzViewImpl>>>,
}

/// The main WinRT implementation struct that bridges COM/WinRT with Rust
///
/// This struct implements the WinRT interfaces defined in the IDL file and
/// delegates the actual work to the core implementation in view_impl.rs
#[derive(Debug)]
#[implement(IBlitzView, IBlitzViewFactory)]
pub struct BlitzViewImpl {
    /// Shared state between interface methods
    state: Arc<BlitzViewState>,
}

impl BlitzViewImpl {
    /// Creates a new BlitzViewImpl instance
    ///
    /// # Arguments
    ///
    /// * `swap_chain_panel` - Pointer to the SwapChainPanel for rendering
    /// * `markdown` - Initial markdown content to render
    ///
    /// # Returns
    ///
    /// A new BlitzViewImpl instance
    pub fn new(swap_chain_panel: *mut std::ffi::c_void, markdown: String) -> Self {
        let state = Arc::new(BlitzViewState {
            dark_mode: Arc::new(Mutex::new(false)),
            swap_chain_panel,
            markdown_content: markdown,
            core_impl: None,
        });
        
        BlitzViewImpl { state }
    }
    
    /// Initializes the core implementation asynchronously
    ///
    /// This method creates the actual rendering pipeline including:
    /// - WGPU surface creation from SwapChainPanel
    /// - Vello renderer initialization
    /// - Document parsing and layout
    ///
    /// # Returns
    ///
    /// Result indicating success or failure of initialization
    pub async fn initialize(&self) -> Result<()> {
        // Create the core implementation
        let core_impl = CoreBlitzViewImpl::new(self.state.swap_chain_panel).await?;
        
        // Initialize the renderer with the SwapChainPanel
        if let Ok(mut core_guard) = core_impl.lock() {
            core_guard.initialize_renderer(self.state.swap_chain_panel).await?;
        }
        
        // Store it in our state
        let mut state = Arc::get_mut(&mut self.state.clone()).unwrap();
        state.core_impl = Some(core_impl);
        
        // Load the initial markdown content
        if let Some(core) = &state.core_impl {
            if let Ok(mut core_guard) = core.lock() {
                // Convert markdown to HTML and load it
                let html = self.markdown_to_html(&state.markdown_content);
                core_guard.load_html(html)?;
            }
        }
        
        Ok(())
    }
    
    /// Converts markdown content to HTML
    ///
    /// # Arguments
    ///
    /// * `markdown` - The markdown content to convert
    ///
    /// # Returns
    ///
    /// HTML string ready for rendering
    fn markdown_to_html(&self, markdown: &str) -> String {
        // TODO: Implement proper markdown parsing using a crate like pulldown-cmark
        // For now, we'll wrap it in basic HTML structure
        let is_dark = self.state.dark_mode.lock().unwrap_or_else(|_| false.into());
        let theme_class = if *is_dark { "dark-theme" } else { "light-theme" };
        
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <style>
        body {{ 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui;
            margin: 20px;
            line-height: 1.6;
        }}
        .dark-theme {{ 
            background: #1e1e1e; 
            color: #ffffff; 
        }}
        .light-theme {{ 
            background: #ffffff; 
            color: #000000; 
        }}
        h1, h2, h3 {{ color: #0078d4; }}
        .dark-theme h1, .dark-theme h2, .dark-theme h3 {{ color: #60a5fa; }}
        code {{ 
            background: #f5f5f5; 
            padding: 2px 4px; 
            border-radius: 3px; 
        }}
        .dark-theme code {{ 
            background: #2d2d2d; 
            color: #f8f8f2; 
        }}
        pre {{ 
            background: #f8f8f8; 
            padding: 12px; 
            border-radius: 6px; 
            overflow-x: auto; 
        }}
        .dark-theme pre {{ 
            background: #2d2d2d; 
        }}
    </style>
</head>
<body class="{theme_class}">
{markdown}
</body>
</html>"#,
            theme_class = theme_class,
            markdown = markdown // TODO: Parse markdown to HTML properly
        )
    }
}

/// Implementation of the IBlitzView interface methods.
impl BlitzViewImpl {
    /// Sets the theme mode for the rendered content.
    ///
    /// # Arguments
    ///
    /// * `isDarkMode` - true for dark mode, false for light mode
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[allow(non_snake_case)] // WinRT method names are defined by IDL
    pub fn SetTheme(&self, isDarkMode: bool) -> Result<()> {
        // Update the internal theme state
        {
            let mut dark_mode = self.state.dark_mode.lock().unwrap();
            *dark_mode = isDarkMode;
        }
        
        // Regenerate HTML with new theme
        let html = self.markdown_to_html(&self.state.markdown_content);
        
        // Update the core implementation if it exists
        if let Some(core) = &self.state.core_impl {
            if let Ok(mut core_guard) = core.lock() {
                core_guard.set_dark_mode(isDarkMode);
                core_guard.load_html(html)?;
            }
        }
        
        Ok(())
    }
    
    /// Creates a new BlitzView instance with the specified SwapChainPanel and content.
    ///
    /// # Arguments
    ///
    /// * `swapChainPanel` - Pointer to the SwapChainPanel (cast as u64)
    /// * `markdown` - The markdown content to render
    ///
    /// # Returns
    ///
    /// A new BlitzView instance wrapped in the WinRT interface
    #[allow(non_snake_case)] // WinRT method names are defined by IDL
    pub fn CreateInstance(
        &self,
        swapChainPanel: u64,
        markdown: &HSTRING,
    ) -> Result<BlitzView> {
        // Convert the u64 back to a pointer
        // Note: This is unsafe but necessary for WinRT interop
        let swap_chain_panel_ptr = swapChainPanel as *mut std::ffi::c_void;
        
        // Convert HSTRING to Rust string
        let markdown_str = markdown.to_string();
        
        // Create the implementation
        let _impl_instance = Arc::new(BlitzViewImpl::new(swap_chain_panel_ptr, markdown_str));
        
        // TODO: Create proper BlitzView COM object 
        // For now, return an error until we implement proper COM object creation
        Err(windows_core::Error::from_hresult(windows_core::HRESULT(0x80004001))) // E_NOTIMPL
    }
}

// Ensure our implementation is thread-safe for WinRT
unsafe impl Send for BlitzViewImpl {}
unsafe impl Sync for BlitzViewImpl {}

/// Entry point for the WinRT component
///
/// This function is called when the component is loaded and should
/// register any necessary factories or interfaces.
#[no_mangle]
pub extern "C" fn DllCanUnloadNow() -> i32 {
    // TODO: Implement proper reference counting
    // Return S_FALSE to indicate the DLL should not be unloaded
    1 // S_FALSE
}

/// Gets the activation factory for the specified runtime class
///
/// # Arguments
///
/// * `activatable_class_id` - The ID of the class to create a factory for
/// * `factory` - Output parameter for the factory interface
///
/// # Returns
///
/// HRESULT indicating success or failure
#[no_mangle]
pub extern "C" fn DllGetActivationFactory(
    _activatable_class_id: *const u16,
    _factory: *mut *mut std::ffi::c_void,
) -> i32 {
    // TODO: Implement activation factory creation
    // This would typically create and return a factory for BlitzView
    0x80004001 // E_NOTIMPL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_to_html_conversion() {
        let impl_instance = BlitzViewImpl::new(std::ptr::null_mut(), "# Test".to_string());
        let html = impl_instance.markdown_to_html("# Hello World");
        
        assert!(html.contains("Hello World"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("light-theme")); // Default theme
    }

    #[test]
    fn test_theme_switching() {
        let impl_instance = BlitzViewImpl::new(std::ptr::null_mut(), "".to_string());
        
        // Test setting dark mode
        {
            let mut dark_mode = impl_instance.state.dark_mode.lock().unwrap();
            *dark_mode = true;
        }
        
        let html = impl_instance.markdown_to_html("Test");
        assert!(html.contains("dark-theme"));
    }
}