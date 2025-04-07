use std::sync::{Arc, Mutex};
use std::cell::RefCell;

use blitz_dom::BaseDocument;
use blitz_html::HtmlDocument;
use blitz_traits::{
    BlitzMouseButtonEvent, ColorScheme, Devtools, Document, MouseEventButton, MouseEventButtons, Viewport, 
    KeyState, BlitzKeyEvent, BlitzImeEvent
};
use blitz_traits::DomEvent;
use blitz_traits::DomEventData;
use blitz_traits::net::DummyNetProvider;
use blitz_traits::navigation::DummyNavigationProvider;
use keyboard_types::{Code, Key, Location, Modifiers};

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::*;
use windows::core::*;

use crate::bindings;
use comrak::{markdown_to_html_with_plugins, ExtensionOptions, Options, Plugins, RenderOptions};

/// Converts markdown text to HTML with GitHub-style formatting
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

/// Represents a rendered iframe-like component with its own event handling
pub struct IFrame {
    /// The document to render
    doc: RefCell<HtmlDocument>,
    
    /// The Direct2D device context used for rendering
    device_context: RefCell<ID2D1DeviceContext>,
    
    /// The physical dimensions of the viewport
    viewport: Mutex<Viewport>,
    
    /// Current state of mouse buttons
    buttons: RefCell<MouseEventButtons>,
    
    /// Current mouse position relative to the iframe
    mouse_pos: RefCell<(f32, f32)>,
    
    /// Current mouse position relative to the document (accounting for scroll)
    dom_mouse_pos: RefCell<(f32, f32)>,
    
    /// Node where the mouse button was pressed (for click detection)
    mouse_down_node: RefCell<Option<usize>>,
    
    /// Devtools state for debugging
    devtools: RefCell<Devtools>,
    
    /// Whether the iframe is active or suspended
    active: RefCell<bool>,
}

impl IFrame {
    pub fn new(device_context: ID2D1DeviceContext) -> Self {
        let viewport = Viewport::new(720, 1080, 1.0, ColorScheme::Light);
        let empty_html = "<html><body></body></html>";
        let net_provider = DummyNetProvider::default();
        let navigation_provider = DummyNavigationProvider {};
        
        // Create an empty HTML document
        let doc = HtmlDocument::from_html(
            empty_html,
            None,
            vec![],
            Arc::new(net_provider),
            None,
            Arc::new(navigation_provider),
        );
        
        Self {
            doc: RefCell::new(doc),
            device_context: RefCell::new(device_context),
            viewport: Mutex::new(viewport),
            buttons: RefCell::new(MouseEventButtons::None),
            mouse_pos: RefCell::new((0.0, 0.0)),
            dom_mouse_pos: RefCell::new((0.0, 0.0)),
            mouse_down_node: RefCell::new(None),
            devtools: RefCell::new(Devtools::default()),
            active: RefCell::new(true),
        }
    }
    
    /// Loads and renders markdown content
    pub fn render_markdown(&self, content: &str) -> Result<()> {
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

        // Set viewport, resolve layout and store document
        {
            let viewport = self.viewport.lock().unwrap();
            doc.as_mut().set_viewport(viewport.clone());
        }
        doc.as_mut().resolve();
        *self.doc.borrow_mut() = doc;
        
        // Render the document
        self.render()
    }
    
    /// Update viewport dimensions and re-render
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        {
            let mut viewport = self.viewport.lock().unwrap();
            viewport.window_size = (width, height);
        }
        
        // Update the DOM with new viewport
        {
            let mut doc = self.doc.borrow_mut();
            let viewport = self.viewport.lock().unwrap();
            doc.as_mut().set_viewport(viewport.clone());
            doc.as_mut().resolve();
        }
        
        self.render()
    }
    
    /// Handle mouse move events, dispatch to DOM
    pub fn pointer_moved(&self, x: f32, y: f32) -> Result<()> {
        // Store the raw mouse position
        *self.mouse_pos.borrow_mut() = (x, y);
        
        // Calculate DOM position (adjusted for scroll)
        let doc = self.doc.borrow();
        let viewport = self.viewport.lock().unwrap();
        let viewport_scroll = doc.as_ref().viewport_scroll();
        
        let dom_x = x + viewport_scroll.x as f32 / viewport.zoom();
        let dom_y = y + viewport_scroll.y as f32 / viewport.zoom();
        *self.dom_mouse_pos.borrow_mut() = (dom_x, dom_y);
        
        // Update hover state in DOM
        let mut doc = self.doc.borrow_mut();
        let changed = doc.as_mut().set_hover_to(dom_x, dom_y);
        
        // If a node is hovered, dispatch mouse move event
        if let Some(node_id) = doc.as_ref().get_hover_node_id() {
            let buttons = *self.buttons.borrow();
            let mut event = DomEvent::new(
                node_id,
                DomEventData::MouseMove(BlitzMouseButtonEvent {
                    x: dom_x,
                    y: dom_y,
                    button: Default::default(),
                    buttons,
                    mods: Default::default(), // TODO: Add modifier support
                }),
            );
            doc.handle_event(&mut event);
        }
        
        if changed {
            self.render()?;
        }
        
        Ok(())
    }
    
    /// Handle mouse down events, dispatch to DOM
    pub fn pointer_pressed(&self, x: f32, y: f32, button_code: u32) -> Result<()> {
        // Convert button code to MouseEventButton
        let button = match button_code {
            0 => MouseEventButton::Main,     // Left button
            1 => MouseEventButton::Secondary, // Right button
            _ => return Ok(()),              // Other buttons not handled
        };
        
        // Update pointer position first
        self.pointer_moved(x, y)?;
        
        // Update button state
        {
            let mut buttons = self.buttons.borrow_mut();
            *buttons |= button.into();
        }
        
        // Get hover node and dispatch event
        {
            let mut doc = self.doc.borrow_mut();
            doc.as_mut().active_node();
            
            if let Some(node_id) = doc.as_ref().get_hover_node_id() {
                let (dom_x, dom_y) = *self.dom_mouse_pos.borrow();
                let buttons = *self.buttons.borrow();
                
                doc.handle_event(&mut DomEvent::new(
                    node_id,
                    DomEventData::MouseDown(BlitzMouseButtonEvent {
                        x: dom_x,
                        y: dom_y,
                        button,
                        buttons,
                        mods: Default::default(), // TODO: Add modifier support
                    }),
                ));
                
                *self.mouse_down_node.borrow_mut() = Some(node_id);
            }
        }
        
        self.render()
    }
    
    /// Handle mouse up events, dispatch to DOM
    pub fn pointer_released(&self, x: f32, y: f32, button_code: u32) -> Result<()> {
        // Convert button code to MouseEventButton
        let button = match button_code {
            0 => MouseEventButton::Main,     // Left button
            1 => MouseEventButton::Secondary, // Right button
            _ => return Ok(()),              // Other buttons not handled
        };
        
        // Update pointer position first
        self.pointer_moved(x, y)?;
        
        // Update button state
        {
            let mut buttons = self.buttons.borrow_mut();
            *buttons ^= button.into();
        }
        
        // Get hover node and dispatch event
        {
            let mut doc = self.doc.borrow_mut();
            doc.as_mut().unactive_node();
            
            if let Some(node_id) = doc.as_ref().get_hover_node_id() {
                let (dom_x, dom_y) = *self.dom_mouse_pos.borrow();
                let buttons = *self.buttons.borrow();
                
                // Dispatch mouse up event
                doc.handle_event(&mut DomEvent::new(
                    node_id,
                    DomEventData::MouseUp(BlitzMouseButtonEvent {
                        x: dom_x,
                        y: dom_y,
                        button,
                        buttons,
                        mods: Default::default(), // TODO: Add modifier support
                    }),
                ));
                
                // Handle click if this is the same node where mouse down occurred
                let mouse_down_node = *self.mouse_down_node.borrow();
                if mouse_down_node == Some(node_id) {
                    self.click(node_id, dom_x, dom_y, button, buttons, &mut doc)?;
                } else if let Some(mouse_down_id) = mouse_down_node {
                    // Check if non-anonymous ancestors match (for stability)
                    if doc.as_ref().non_anon_ancestor_if_anon(mouse_down_id)
                        == doc.as_ref().non_anon_ancestor_if_anon(node_id)
                    {
                        self.click(node_id, dom_x, dom_y, button, buttons, &mut doc)?;
                    }
                }
            }
        }
        
        *self.mouse_down_node.borrow_mut() = None;
        self.render()
    }
    
    /// Handle click events internally
    fn click(&self, node_id: usize, x: f32, y: f32, button: MouseEventButton, 
             buttons: MouseEventButtons, doc: &mut HtmlDocument) -> Result<()> {
        if button == MouseEventButton::Main {
            doc.handle_event(&mut DomEvent::new(
                node_id,
                DomEventData::Click(BlitzMouseButtonEvent {
                    x,
                    y,
                    button,
                    buttons,
                    mods: Default::default(), // TODO: Add modifier support
                }),
            ));
        }
        
        Ok(())
    }
    
    /// Handle mouse wheel events
    pub fn mouse_wheel(&self, delta_x: f32, delta_y: f32) -> Result<()> {
        let mut doc = self.doc.borrow_mut();
        
        // Scale deltas to match typical scrolling behavior
        let scroll_x = delta_x as f64 * 20.0;
        let scroll_y = delta_y as f64 * 20.0;
        
        // Scroll the hovered node if there is one, otherwise scroll viewport
        if let Some(hover_node_id) = doc.as_ref().get_hover_node_id() {
            doc.as_mut().scroll_node_by(hover_node_id, scroll_x, scroll_y);
        } else {
            doc.as_mut().scroll_viewport_by(scroll_x, scroll_y);
        }
        
        self.render()
    }
    
    /// Handle keyboard key down events
    pub fn key_down(&self, key_code: u32, ctrl: bool, shift: bool, alt: bool) -> Result<()> {
        let mut doc = self.doc.borrow_mut();
        
        // Let the document handle key events if there's a focused node
        if let Some(focus_node_id) = doc.as_ref().get_focussed_node_id() {
            // Convert the key code to a DomEventData
            // This is simplified and would need proper conversion in a real implementation
            doc.handle_event(&mut DomEvent::new(
                focus_node_id,
                DomEventData::KeyPress(blitz_traits::BlitzKeyEvent {
                    key: Key::Character("".into()),
                    code: Code::KeyA, // Using a placeholder code since we can't convert directly from string
                    modifiers: Modifiers::empty()
                        .union(if alt { Modifiers::ALT } else { Modifiers::empty() })
                        .union(if ctrl { Modifiers::CONTROL } else { Modifiers::empty() })
                        .union(if shift { Modifiers::SHIFT } else { Modifiers::empty() }),
                    location: Location::Standard,
                    is_auto_repeating: false,
                    is_composing: false,
                    state: KeyState::Pressed,
                    text: None,
                }),
            ));
        }
        
        self.render()
    }
    
    /// Handle keyboard key up events
    pub fn key_up(&self, _key_code: u32) -> Result<()> {
        // Key up events might not need specific handling in this case
        Ok(())
    }
    
    /// Handle text input events (IME, etc.)
    pub fn text_input(&self, text: &str) -> Result<()> {
        let mut doc = self.doc.borrow_mut();
        
        if let Some(target) = doc.as_ref().get_focussed_node_id() {
            doc.handle_event(&mut DomEvent::new(
                target, 
                DomEventData::Ime(blitz_traits::BlitzImeEvent::Commit(text.to_string())),
            ));
            self.render()?;
        }
        
        Ok(())
    }
    
    /// Handle focus events
    pub fn on_focus(&self) -> Result<()> {
        // Implementation would depend on how focus should be handled
        Ok(())
    }
    
    /// Handle blur events
    pub fn on_blur(&self) -> Result<()> {
        // Implementation would depend on how blur should be handled
        Ok(())
    }
    
    /// Suspend the iframe (save state, etc.)
    pub fn suspend(&self) -> Result<()> {
        *self.active.borrow_mut() = false;
        Ok(())
    }
    
    /// Resume the iframe
    pub fn resume(&self) -> Result<()> {
        *self.active.borrow_mut() = true;
        self.render()
    }
    
    /// Set theme (light/dark mode)
    pub fn set_theme(&self, is_dark_mode: bool) -> Result<()> {
        let color_scheme = if is_dark_mode {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        
        {
            let mut viewport = self.viewport.lock().unwrap();
            viewport.color_scheme = color_scheme;
        }
        
        // Update the DOM with new viewport
        {
            let mut doc = self.doc.borrow_mut();
            let viewport = self.viewport.lock().unwrap();
            doc.as_mut().set_viewport(viewport.clone());
            doc.as_mut().resolve();
        }
        
        self.render()
    }
    
    /// Internal function to render the current document
    fn render(&self) -> Result<()> {
        if !*self.active.borrow() {
            return Ok(());
        }
        
        let mut device_context = self.device_context.borrow_mut();
        let doc = self.doc.borrow();
        let viewport = self.viewport.lock().unwrap();
        let devtools = self.devtools.borrow().clone();

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
                doc.as_ref(),
                viewport.scale_f64(),
                viewport.window_size.0,
                viewport.window_size.1,
                devtools,
            );
            
            // End drawing
            device_context.EndDraw(None, None)?;
        }
        
        Ok(())
    }
}