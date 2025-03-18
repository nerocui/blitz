use std::sync::Arc;
use crate::bindings;
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

#[implement(bindings::D2DRenderer)]
pub struct D2DRenderer {
    device_context: RefCell<ID2D1DeviceContext>
}

impl D2DRenderer {
    pub fn new(device_context: ID2D1DeviceContext) -> Self {
        Self {
            device_context: RefCell::new(device_context)
        }
    }
}

impl bindings::ID2DRenderer_Impl for D2DRenderer_Impl {
    fn Render(&self, content: &HSTRING) -> Result<()> {
        let mut device_context = self.device_context.borrow_mut();
        let mut html = content.to_string();
        let mut stylesheets = Vec::new();
        html = markdown_to_html(html);
        stylesheets.push(String::from(GITHUB_MD_STYLES));
        stylesheets.push(String::from(BLITZ_MD_STYLES));

        let net_provider = DummyNetProvider::default();
        let navigation_provider = DummyNavigationProvider {};

        let mut doc = HtmlDocument::from_html(
            &html,
            None,
            stylesheets,
            Arc::new(net_provider),
            None,
            Arc::new(navigation_provider),
        );

        let scroll = doc.as_ref().viewport_scroll();
        doc.as_mut().set_viewport_scroll(scroll);

        unsafe {
            // Begin drawing
            device_context.BeginDraw();
            
            // Clear with white background
            device_context.Clear(Some(&D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }));
            
            // Generate the Direct2D scene
            blitz_renderer_vello::renderer::d2drender::generate_d2d_scene(
                &mut *device_context,
                &doc.as_ref(),
                1.0,
                480,
                640,
                blitz_traits::Devtools {
                    show_layout: false,
                    highlight_hover: false,
                    show_style: false,
                    print_hover: false,
                },
            );
            
            // // End drawing
            device_context.EndDraw(None, None);
            Ok(())
        }
    }
}