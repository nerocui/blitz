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
use windows::Win32::Foundation::E_FAIL; // Import the E_FAIL constant

// Import ILogger directly from the bindings module
use crate::bindings::ILogger;
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
    
    /// Lock to ensure exclusive access to the device context during rendering
    device_context_lock: Mutex<()>,

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
    
    /// Content has been initialized
    content_initialized: RefCell<bool>,
    
    /// Flag to track if content needs redrawing
    needs_render: RefCell<bool>,
    
    /// Add a flag to track if drawing is in progress
    /// This helps prevent BeginDraw/EndDraw mismatches
    drawing_in_progress: RefCell<bool>,
    
    /// Logger for sending debug messages to the C# side
    logger: RefCell<Option<ILogger>>,
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
            device_context_lock: Mutex::new(()),
            viewport: Mutex::new(viewport),
            buttons: RefCell::new(MouseEventButtons::None),
            mouse_pos: RefCell::new((0.0, 0.0)),
            dom_mouse_pos: RefCell::new((0.0, 0.0)),
            mouse_down_node: RefCell::new(None),
            devtools: RefCell::new(Devtools::default()),
            active: RefCell::new(true),
            content_initialized: RefCell::new(false),
            needs_render: RefCell::new(false),
            drawing_in_progress: RefCell::new(false),
            logger: RefCell::new(None), // Initialize logger as None
        }
    }
    
    /// Sets the logger for debugging output
    pub fn set_logger(&self, logger: ILogger) -> Result<()> {
        *self.logger.borrow_mut() = Some(logger);
        self.log("Logger attached to IFrame");
        Ok(())
    }
    
    /// Send a log message to the C# side if a logger is available
    pub fn log(&self, message: &str) {
        if let Some(logger) = self.logger.borrow().as_ref() {
            // Use catch_unwind to prevent panics in logging from crashing the app
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Convert the &str to an HSTRING before passing to LogMessage
                let hstring_message = windows::core::HSTRING::from(message);
                let _ = logger.LogMessage(&hstring_message);
            }));
        } else {
            // Fall back to println if no logger is available
            println!("[IFRAME] {}", message);
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
        *self.content_initialized.borrow_mut() = true;
        
        // Force rendering by setting needs_render directly
        *self.needs_render.borrow_mut() = true;
        
        // Signal that we want to render, but don't actually render yet
        // The actual rendering will happen in render_if_needed
        Ok(())
    }
    
    /// Update viewport dimensions and re-render
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        // If no content has been initialized, just update viewport size without rendering
        if !*self.content_initialized.borrow() {
            let mut viewport = self.viewport.lock().unwrap();
            viewport.window_size = (width, height);
            return Ok(());
        }
        
        // Update viewport dimensions
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
        
        // Render with updated dimensions
        self.render()
    }
    
    /// Handle mouse move events, dispatch to DOM
    pub fn pointer_moved(&self, x: f32, y: f32) -> Result<()> {
        if !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        // Store the raw mouse position
        *self.mouse_pos.borrow_mut() = (x, y);
        
        let dom_x;
        let dom_y;
        let changed;
        
        // Use a scope to ensure the viewport lock is released before further operations
        {
            // Calculate DOM position (adjusted for scroll) - Use scoped access to avoid holding locks across function calls
            let doc_ref = match self.doc.try_borrow() {
                Ok(doc) => doc,
                Err(_) => return Ok(()), // If we can't borrow the document, just return without doing anything
            };
            
            let viewport = match self.viewport.try_lock() {
                Ok(v) => v,
                Err(_) => return Ok(()),
            };
            
            let viewport_scroll = doc_ref.as_ref().viewport_scroll();
            
            dom_x = x + viewport_scroll.x as f32 / viewport.zoom();
            dom_y = y + viewport_scroll.y as f32 / viewport.zoom();
            *self.dom_mouse_pos.borrow_mut() = (dom_x, dom_y);
        }
        
        // Update hover state in DOM - use a separate scope to minimize lock duration
        let should_render = {
            let mut doc = match self.doc.try_borrow_mut() {
                Ok(doc) => doc,
                Err(_) => return Ok(()),
            };
            
            // Catch any potential panic in set_hover_to
            changed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                doc.as_mut().set_hover_to(dom_x, dom_y)
            })) {
                Ok(result) => result,
                Err(_) => {
                    println!("Panic in set_hover_to");
                    false
                }
            };
            
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
                        mods: Default::default(),
                    }),
                );
                
                // Again, catch any potential panic
                if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    doc.handle_event(&mut event);
                })) {
                    println!("Panic in handle_event for MouseMove");
                }
            }
            
            changed
        };
        
        // Only render if something changed
        if should_render {
            match self.render() {
                Ok(_) => (),
                Err(e) => println!("Error in render: {:?}", e),
            }
        }
        
        Ok(())
    }
    
    /// Handle mouse down events, dispatch to DOM
    pub fn pointer_pressed(&self, x: f32, y: f32, button_code: u32) -> Result<()> {
        if !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        // Convert button code to MouseEventButton
        let button = match button_code {
            0 => MouseEventButton::Main,     // Left button
            1 => MouseEventButton::Secondary, // Right button
            _ => return Ok(()),              // Other buttons not handled
        };
        
        // Update pointer position first - safely handling errors
        if let Err(e) = self.pointer_moved(x, y) {
            println!("Error in pointer_moved during pressed: {:?}", e);
        }
        
        // Update button state
        {
            let mut buttons = self.buttons.borrow_mut();
            *buttons |= button.into();
        }
        
        // Get hover node and dispatch event
        {
            let mut doc = match self.doc.try_borrow_mut() {
                Ok(doc) => doc,
                Err(_) => return Ok(()),
            };
            
            // Catch any potential panic
            if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                doc.as_mut().active_node();
            })) {
                println!("Panic in active_node");
                return Ok(());
            }
            
            if let Some(node_id) = doc.as_ref().get_hover_node_id() {
                let (dom_x, dom_y) = *self.dom_mouse_pos.borrow();
                let buttons = *self.buttons.borrow();
                
                // Catch any potential panic in handle_event
                if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    doc.handle_event(&mut DomEvent::new(
                        node_id,
                        DomEventData::MouseDown(BlitzMouseButtonEvent {
                            x: dom_x,
                            y: dom_y,
                            button,
                            buttons,
                            mods: Default::default(),
                        }),
                    ));
                })) {
                    println!("Panic in handle_event for MouseDown");
                }
                
                *self.mouse_down_node.borrow_mut() = Some(node_id);
            }
        }
        
        self.render()
    }
    
    /// Handle mouse up events, dispatch to DOM
    pub fn pointer_released(&self, x: f32, y: f32, button_code: u32) -> Result<()> {
        if !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        // Convert button code to MouseEventButton
        let button = match button_code {
            0 => MouseEventButton::Main,     // Left button
            1 => MouseEventButton::Secondary, // Right button
            _ => return Ok(()),              // Other buttons not handled
        };
        
        // Update pointer position first - safely handling errors
        if let Err(e) = self.pointer_moved(x, y) {
            println!("Error in pointer_moved during released: {:?}", e);
            // Continue execution even if pointer_moved fails
        }
        
        // Update button state
        {
            let mut buttons = self.buttons.borrow_mut();
            *buttons ^= button.into();
        }
        
        // Get hover node and dispatch event
        let result: Result<()> = {
            let mut doc = match self.doc.try_borrow_mut() {
                Ok(doc) => doc,
                Err(_) => return self.render(), // Just trigger a render and return if can't access doc
            };
            
            // Catch any potential panic
            if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                doc.as_mut().unactive_node();
            })) {
                println!("Panic in unactive_node");
                return self.render();
            }
            
            if let Some(node_id) = doc.as_ref().get_hover_node_id() {
                let (dom_x, dom_y) = *self.dom_mouse_pos.borrow();
                let buttons = *self.buttons.borrow();
                
                // Dispatch mouse up event - catch any potential panic
                if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    doc.handle_event(&mut DomEvent::new(
                        node_id,
                        DomEventData::MouseUp(BlitzMouseButtonEvent {
                            x: dom_x,
                            y: dom_y,
                            button,
                            buttons,
                            mods: Default::default(),
                        }),
                    ));
                })) {
                    println!("Panic in handle_event for MouseUp");
                }
                
                // Handle click if this is the same node where mouse down occurred
                let mouse_down_node = *self.mouse_down_node.borrow();
                
                // Use a result to safely propagate any errors from click
                let click_result = if mouse_down_node == Some(node_id) {
                    self.click(node_id, dom_x, dom_y, button, buttons, &mut doc)
                } else if let Some(mouse_down_id) = mouse_down_node {
                    // Check if non-anonymous ancestors match (for stability)
                    if doc.as_ref().non_anon_ancestor_if_anon(mouse_down_id)
                        == doc.as_ref().non_anon_ancestor_if_anon(node_id)
                    {
                        self.click(node_id, dom_x, dom_y, button, buttons, &mut doc)
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                };
                
                if let Err(e) = click_result {
                    println!("Error in click handler: {:?}", e);
                }
            }
            
            Ok(())
        };
        
        if let Err(e) = result {
            println!("Error in pointer_released: {:?}", e);
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
        if !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        let mut doc = match self.doc.try_borrow_mut() {
            Ok(doc) => doc,
            Err(_) => return Ok(()),
        };
        
        // Scale deltas to match typical scrolling behavior
        let scroll_x = delta_x as f64 * 20.0;
        let scroll_y = delta_y as f64 * 20.0;
        
        // Use catch_unwind to handle potential panics
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Scroll the hovered node if there is one, otherwise scroll viewport
            if let Some(hover_node_id) = doc.as_ref().get_hover_node_id() {
                doc.as_mut().scroll_node_by(hover_node_id, scroll_x, scroll_y);
            } else {
                doc.as_mut().scroll_viewport_by(scroll_x, scroll_y);
            }
        }));
        
        if let Err(_) = result {
            println!("Panic in mouse_wheel handler");
        }
        
        self.render()
    }
    
    /// Handle keyboard key down events
    pub fn key_down(&self, _key_code: u32, _ctrl: bool, _shift: bool, _alt: bool) -> Result<()> {
        // Implementation
        Ok(())
    }
    
    /// Handle keyboard key up events
    pub fn key_up(&self, _key_code: u32) -> Result<()> {
        // Key up events might not need specific handling in this case
        Ok(())
    }
    
    /// Handle text input events (IME, etc.)
    pub fn text_input(&self, text: &str) -> Result<()> {
        if !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        let mut doc = match self.doc.try_borrow_mut() {
            Ok(doc) => doc,
            Err(_) => return Ok(()),
        };
        
        // Use catch_unwind to handle potential panics
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(target) = doc.as_ref().get_focussed_node_id() {
                doc.handle_event(&mut DomEvent::new(
                    target, 
                    DomEventData::Ime(blitz_traits::BlitzImeEvent::Commit(text.to_string())),
                ));
            }
        }));
        
        if let Err(_) = result {
            println!("Panic in text_input handler");
        }
        
        self.render()
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
        if *self.content_initialized.borrow() {
            self.render()?;
        }
        Ok(())
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
        
        // Only update DOM if content has been initialized
        if *self.content_initialized.borrow() {
            // Update the DOM with new viewport
            {
                let mut doc = self.doc.borrow_mut();
                let viewport = self.viewport.lock().unwrap();
                doc.as_mut().set_viewport(viewport.clone());
                doc.as_mut().resolve();
            }
            
            self.render()?;
        }
        
        Ok(())
    }
    
    /// Internal function to render the current document
    fn render(&self) -> Result<()> {
        // Skip rendering if inactive or no content has been initialized
        if !*self.active.borrow() || !*self.content_initialized.borrow() {
            return Ok(());
        }
        
        // Mark that we need rendering
        *self.needs_render.borrow_mut() = true;
        Ok(())
    }
    
    /// Performs the actual rendering if needed
    pub fn render_if_needed(&self) -> Result<()> {
        // Skip if we don't need to render
        if !*self.needs_render.borrow() {
            return Ok(());
        }

        if !*self.active.borrow() {
            self.log("Skipping render - not active");
            return Ok(());
        }

        if !*self.content_initialized.borrow() {
            self.log("Skipping render - content not initialized");
            return Ok(());
        }
        
        // Acquire an exclusive lock on the device context to prevent multiple threads
        // from rendering simultaneously
        let _device_lock = match self.device_context_lock.try_lock() {
            Ok(lock) => lock,
            Err(_) => {
                // Already locked, another thread is rendering
                self.log("Device context already locked by another thread, skipping render");
                return Ok(());
            }
        };
        
        // Check if drawing is already in progress
        if *self.drawing_in_progress.borrow() {
            // If drawing is already in progress, don't try to render again
            self.log("Drawing already in progress, skipping render");
            return Ok(());
        }

        // Reset needs_render flag
        *self.needs_render.borrow_mut() = false;
        
        let doc = match self.doc.try_borrow() {
            Ok(doc) => doc,
            Err(_) => {
                self.log("Could not borrow document for rendering");
                return Ok(());
            }
        };
        
        let viewport = match self.viewport.try_lock() {
            Ok(v) => v,
            Err(_) => {
                self.log("Could not lock viewport for rendering");
                return Ok(());
            }
        };
        
        let devtools = self.devtools.borrow().clone();

        // Skip rendering if viewport dimensions are invalid
        if viewport.window_size.0 == 0 || viewport.window_size.1 == 0 {
            self.log(&format!("Invalid viewport dimensions: {}x{}", viewport.window_size.0, viewport.window_size.1));
            return Ok(());
        }
        
        // Now try to borrow the device context
        let mut device_context = match self.device_context.try_borrow_mut() {
            Ok(ctx) => ctx,
            Err(_) => {
                // If we can't borrow the device context, mark that we need to render again later
                *self.needs_render.borrow_mut() = true;
                self.log("Could not borrow device context for rendering");
                return Ok(());
            }
        };

        // Set drawing in progress flag
        *self.drawing_in_progress.borrow_mut() = true;
        
        // Check if D2D is already drawing, and if so, end the previous draw first
        let mut tag1: u64 = 0;
        let mut tag2: u64 = 0;
        
        // Instead of using a non-existent CheckWindowState method, we'll try to
        // detect active drawing sessions using a safer approach
        unsafe {
            // Try to end any potentially existing drawing session
            // If no drawing session exists, this will fail with D2DERR_WRONG_STATE
            // which is fine - we can safely ignore that error
            self.log("Checking for active drawing session...");
            
            let end_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let end_hr = device_context.EndDraw(Some(&mut tag1), Some(&mut tag2));
                if end_hr.is_ok() {
                    self.log("Found and ended previous drawing session");
                    true // Indicates we found an active session
                } else {
                    // This is the expected case when no drawing is in progress
                    self.log("No active drawing session detected");
                    false
                }
            }));
            
            // Handle the result, ignoring panics (just means no session was active)
            let had_previous_drawing = match end_result {
                Ok(found_session) => found_session,
                Err(_) => {
                    self.log("Error checking for drawing session, assuming none active");
                    false
                }
            };
            
            // If we had a previous drawing session, wait a bit before starting a new one
            if had_previous_drawing {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        
        self.log("Starting D2D rendering process");
        self.log(&format!("Viewport size: {}x{}", viewport.window_size.0, viewport.window_size.1));
        self.log(&format!("Scale factor: {}", viewport.scale_f64()));
        
        // Use this scope to ensure we properly handle results and reset flags
        let render_result: windows::core::Result<()> = unsafe {
            // Make sure the device context is not in a bad state
            let mut tag1: u64 = 0;
            let mut tag2: u64 = 0;

            // Always catch any potential panic that could happen during BeginDraw/EndDraw
            let draw_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Start fresh with a new BeginDraw - BeginDraw returns void, not a Result
                device_context.BeginDraw();
                self.log("BeginDraw called successfully");
                
                // Clear background with white (fully opaque)
                device_context.Clear(Some(&D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }));
                self.log("Background cleared to white");
                
                // Generate D2D scene
                let scene_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.log("Generating D2D scene...");
                    blitz_renderer_vello::renderer::d2drender::generate_d2d_scene(
                        &mut *device_context,
                        doc.as_ref(),
                        viewport.scale_f64(),
                        viewport.window_size.0,
                        viewport.window_size.1,
                        devtools,
                    );
                    self.log("D2D scene generation completed");
                }));
                
                // Handle any panic in scene generation
                if let Err(panic) = scene_result {
                    self.log("PANIC during D2D scene generation");
                    if let Some(panic_msg) = panic.downcast_ref::<&str>() {
                        self.log(&format!("Panic message: {}", panic_msg));
                    } else if let Some(panic_string) = panic.downcast_ref::<String>() {
                        self.log(&format!("Panic message: {}", panic_string));
                    }
                    
                    // Still need to end drawing even if scene generation failed
                    self.log("Calling EndDraw after panic...");
                    let end_hr = device_context.EndDraw(Some(&mut tag1), Some(&mut tag2));
                    if let Err(error) = end_hr {
                        self.log(&format!("EndDraw failed with error code: 0x{:08X}", error.code().0));
                    } else {
                        self.log("EndDraw succeeded after panic");
                    }
                    
                    // Re-throw the panic to be caught by the outer catch_unwind
                    std::panic::resume_unwind(panic);
                }
                
                // End drawing (only reached if no panic occurred)
                self.log("Calling EndDraw...");
                let end_hr = device_context.EndDraw(Some(&mut tag1), Some(&mut tag2));
                if let Err(error) = end_hr {
                    self.log(&format!("EndDraw failed with error code: 0x{:08X}", error.code().0));
                    Err(error)
                } else {
                    self.log("EndDraw successful");
                    Ok(())
                }
            }));
            
            // Handle panic from the entire Draw cycle
            match draw_result {
                Ok(inner_result) => inner_result,
                Err(_) => {
                    self.log("PANIC occurred during drawing cycle");
                    // Try to end drawing just in case, but ignore any errors
                    let _ = device_context.EndDraw(Some(&mut tag1), Some(&mut tag2));
                    Err(Error::new(E_FAIL, "Panic during drawing"))
                }
            }
        };
        
        // Always reset the drawing flag, regardless of result
        *self.drawing_in_progress.borrow_mut() = false;
        
        if render_result.is_ok() {
            self.log("Rendering completed successfully");
        } else if let Err(e) = &render_result {
            self.log(&format!("Rendering failed: 0x{:08X}", e.code().0));
        }
        
        render_result
    }
}