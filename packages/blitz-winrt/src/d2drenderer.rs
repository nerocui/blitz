use std::sync::Arc;
use crate::iframe::IFrame;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::*;
use comrak::{markdown_to_html_with_plugins, ExtensionOptions, Options, Plugins, RenderOptions};
use blitz_html::HtmlDocument;
use blitz_traits::net::{DummyNetProvider, NetProvider};
use blitz_traits::navigation::DummyNavigationProvider;
use windows::core::*;
use std::cell::RefCell;

// Import ILogger directly from the bindings module
use crate::bindings::ILogger;

fn markdown_to_html(contents: String) -> String {
    let plugins = Plugins::default();

    let body_html = markdown_to_html_with_plugins(
        &contents,
        &Options {
            extension: ExtensionOptions {
                strikethrough: true,
                tagfilter: false,
                table: true,
                autolink: true,
                tasklist: true,
                superscript: false,
                header_ids: None,
                footnotes: false,
                description_lists: false,
                front_matter_delimiter: None,
                multiline_block_quotes: false,
                alerts: true,
                ..ExtensionOptions::default()
            },
            render: RenderOptions {
                unsafe_: true,
                tasklist_classes: true,
                ..RenderOptions::default()
            },
            ..Options::default()
        },
        &plugins,
    );

    // Strip trailing newlines in code blocks
    let body_html = body_html.replace("\n</code", "</code");

    format!(
        r#"
        <!DOCTYPE html>
        <html>
        <body>
        <div class="markdown-body">{}</div>
        </body>
        </html>
    "#,
        body_html
    )
}

const GITHUB_MD_STYLES: &str = include_str!("../assets/github-markdown.css");
const BLITZ_MD_STYLES: &str = include_str!("../assets/blitz-markdown-overrides.css");

/// Wraps an IFrame and provides a D2D-based renderer implementation
pub struct D2DRenderer {
    /// The iframe that manages document and rendering
    pub iframe: IFrame,
}

impl D2DRenderer {
    /// Create a new D2DRenderer with the given Direct2D device context
    pub fn new(device_context: ID2D1DeviceContext) -> Self {
        // Log through logger if available - will fall back to println if not
        Self {
            iframe: IFrame::new(device_context),
        }
    }

    /// Pass the logger from C# to the iframe
    pub fn set_logger(&self, logger: ILogger) -> Result<()> {
        self.iframe.log("D2DRenderer::set_logger() - Setting logger");
        self.iframe.set_logger(logger)
    }

    /// Tick function called by the rendering loop - delegates to the iframe
    pub fn tick(&self) -> Result<()> {
        self.iframe.log("D2DRenderer::tick() - Forwarding to IFrame rendering pipeline");
        
        // We need to make sure we call render_if_needed() to actually perform rendering
        self.iframe.render_if_needed()
    }
    
    /// Render markdown content - delegates to the iframe
    pub fn render_markdown(&self, markdown: &str) -> Result<()> {
        self.iframe.log(&format!("D2DRenderer::render_markdown() with {} bytes", markdown.len()));
        
        // Forward to IFrame implementation
        let result = self.iframe.render_markdown(markdown);
        
        // Log the result for debugging
        match &result {
            Ok(_) => self.iframe.log("D2DRenderer::render_markdown() succeeded"),
            Err(e) => self.iframe.log(&format!("D2DRenderer::render_markdown() failed with error: {:?}", e)),
        }
        
        result
    }
    
    /// Resize the renderer
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        self.iframe.log(&format!("D2DRenderer::resize({}, {})", width, height));
        self.iframe.resize(width, height)
    }
    
    /// Handle pointer move events
    pub fn pointer_moved(&self, x: f32, y: f32) -> Result<()> {
        self.iframe.pointer_moved(x, y)
    }
    
    /// Handle pointer press events
    pub fn pointer_pressed(&self, x: f32, y: f32, button: u32) -> Result<()> {
        self.iframe.pointer_pressed(x, y, button)
    }
    
    /// Handle pointer release events
    pub fn pointer_released(&self, x: f32, y: f32, button: u32) -> Result<()> {
        self.iframe.pointer_released(x, y, button)
    }
    
    /// Handle mouse wheel events
    pub fn mouse_wheel(&self, delta_x: f32, delta_y: f32) -> Result<()> {
        self.iframe.mouse_wheel(delta_x, delta_y)
    }
    
    /// Handle key down events
    pub fn key_down(&self, key_code: u32, ctrl: bool, shift: bool, alt: bool) -> Result<()> {
        self.iframe.key_down(key_code, ctrl, shift, alt)
    }
    
    /// Handle key up events
    pub fn key_up(&self, key_code: u32) -> Result<()> {
        self.iframe.key_up(key_code)
    }
    
    /// Handle text input events
    pub fn text_input(&self, text: &str) -> Result<()> {
        self.iframe.text_input(text)
    }
    
    /// Handle blur events
    pub fn on_blur(&self) -> Result<()> {
        self.iframe.on_blur()
    }
    
    /// Handle focus events
    pub fn on_focus(&self) -> Result<()> {
        self.iframe.on_focus()
    }
    
    /// Suspend the renderer
    pub fn suspend(&self) -> Result<()> {
        self.iframe.suspend()
    }
    
    /// Resume the renderer
    pub fn resume(&self) -> Result<()> {
        self.iframe.resume()
    }
    
    /// Set the theme (light/dark mode)
    pub fn set_theme(&self, is_dark_mode: bool) -> Result<()> {
        self.iframe.set_theme(is_dark_mode)
    }
}