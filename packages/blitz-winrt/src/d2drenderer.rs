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

// Our D2DRenderer is now just a simple wrapper around IFrame
pub struct D2DRenderer {
    pub iframe: Arc<IFrame>,
}

impl D2DRenderer {
    pub fn new(device_context: ID2D1DeviceContext) -> Self {
        Self {
            iframe: Arc::new(IFrame::new(device_context)),
        }
    }
    
    // Set logger to use for debug output
    pub fn set_logger(&self, logger: ILogger) -> Result<()> {
        self.iframe.set_logger(logger)
    }
    
    // Called by the host application's render loop to perform any pending render operations
    pub fn tick(&self) -> Result<()> {
        self.iframe.render_if_needed()
    }
}