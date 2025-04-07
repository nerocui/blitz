use std::sync::Arc;
use crate::bindings;
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

fn markdown_to_html(contents: String) -> String {
    let plugins = Plugins::default();
    // let syntax_highligher = CustomSyntectAdapter(SyntectAdapter::new(Some("InspiredGitHub")));
    // plugins.render.codefence_syntax_highlighter = Some(&syntax_highligher as _);

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

#[derive(Clone)]
#[implement(bindings::D2DRenderer)]
pub struct D2DRenderer {
    iframe: Arc<IFrame>,
}

impl D2DRenderer {
    pub fn new(device_context: ID2D1DeviceContext) -> Self {
        Self {
            iframe: Arc::new(IFrame::new(device_context)),
        }
    }
}

impl bindings::ID2DRenderer_Impl for D2DRenderer_Impl {
    fn Render(&self, markdown: &HSTRING) -> Result<()> {
        // Convert HSTRING to &str and pass by reference
        self.iframe.render_markdown(&markdown.to_string_lossy())
    }
    
    fn Resize(&self, width: u32, height: u32) -> Result<()> {
        self.iframe.resize(width, height)
    }
    
    fn OnPointerMoved(&self, x: f32, y: f32) -> Result<()> {
        self.iframe.pointer_moved(x, y)
    }
    
    fn OnPointerPressed(&self, x: f32, y: f32, button: u32) -> Result<()> {
        self.iframe.pointer_pressed(x, y, button)
    }
    
    fn OnPointerReleased(&self, x: f32, y: f32, button: u32) -> Result<()> {
        self.iframe.pointer_released(x, y, button)
    }
    
    fn OnMouseWheel(&self, delta_x: f32, delta_y: f32) -> Result<()> {
        self.iframe.mouse_wheel(delta_x, delta_y)
    }
    
    fn OnKeyDown(&self, key_code: u32, ctrl: bool, shift: bool, alt: bool) -> Result<()> {
        self.iframe.key_down(key_code, ctrl, shift, alt)
    }
    
    fn OnKeyUp(&self, key_code: u32) -> Result<()> {
        self.iframe.key_up(key_code)
    }
    
    fn OnTextInput(&self, text: &HSTRING) -> Result<()> {
        // Convert HSTRING to &str and pass by reference
        self.iframe.text_input(&text.to_string_lossy())
    }
    
    fn OnBlur(&self) -> Result<()> {
        self.iframe.on_blur()
    }
    
    fn OnFocus(&self) -> Result<()> {
        self.iframe.on_focus()
    }
    
    fn Suspend(&self) -> Result<()> {
        self.iframe.suspend()
    }
    
    fn Resume(&self) -> Result<()> {
        self.iframe.resume()
    }
    
    fn SetTheme(&self, is_dark_mode: bool) -> Result<()> {
        self.iframe.set_theme(is_dark_mode)
    }
}