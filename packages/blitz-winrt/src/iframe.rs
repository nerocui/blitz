use std::sync::atomic::{self, AtomicUsize, AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use std::time::{Instant, Duration};
use windows::Win32::Foundation::E_FAIL; // Add E_FAIL import

// Add the static variables for caching
static LAST_HOVER_NODE: AtomicUsize = AtomicUsize::new(0);
static FORCE_REDRAW: AtomicBool = AtomicBool::new(true);
static LAST_ACTIVE_NODE: AtomicUsize = AtomicUsize::new(0);
static LAST_SCROLL_X: AtomicUsize = AtomicUsize::new(0);
static LAST_SCROLL_Y: AtomicUsize = AtomicUsize::new(0);
static LAST_WIDTH: AtomicUsize = AtomicUsize::new(0);
static LAST_HEIGHT: AtomicUsize = AtomicUsize::new(0);
static RENDERING_COUNT: AtomicUsize = AtomicUsize::new(0);
static DROPPED_FRAMES: AtomicUsize = AtomicUsize::new(0);
static CONSECUTIVE_DROPS: AtomicUsize = AtomicUsize::new(0);
// Add a resize happened flag to ensure we render after resize
static RESIZE_HAPPENED: AtomicBool = AtomicBool::new(false);

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

// Direct2D imports
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Graphics::Direct2D::{D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE};
use windows_numerics::Matrix3x2;
use windows::core::*;

// Import ILogger directly from the bindings module
use crate::bindings::ILogger;
use comrak::{markdown_to_html_with_plugins, ExtensionOptions, Options, Plugins, RenderOptions};

// Import the d2drender module directly from blitz-renderer-vello
use blitz_renderer_vello::renderer::d2drender;

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
            needs_render: RefCell::new(true),
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
    
    /// Get a reference to the current logger
    pub fn get_logger(&self) -> Option<ILogger> {
        self.logger.borrow().clone()
    }
    
    /// Send a log message to the C# side if a logger is available
    pub fn log(&self, message: &str) {
        // if let Some(logger) = self.logger.borrow().as_ref() {
        //     // Use catch_unwind to prevent panics in logging from crashing the app
        //     let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        //         // Convert the &str to an HSTRING before passing to LogMessage
        //         let hstring_message = windows::core::HSTRING::from(message);
        //         logger.LogMessage(&hstring_message)
        //     }));
            
        //     if let Err(_) = result {
        //         // If logging itself panics, we can't do much but silently continue
        //         // In a debug build, we might still want to see these failures
        //         #[cfg(debug_assertions)]
        //         eprintln!("[IFRAME ERROR] Panic while trying to log: {}", message);
        //     }
        // } else {
        //     // Only fall back to eprintln in debug mode
        //     #[cfg(debug_assertions)]
        //     eprintln!("[IFRAME] No logger attached: {}", message);
        // }
    }
    
    /// Loads and renders markdown content
    pub fn render_markdown(&self, content: &str) -> Result<()> {
        // Log the attempt to render markdown
        self.log(&format!("render_markdown called with content length: {}", content.len()));
        
        let html = markdown_to_html(content.to_string());
        let mut stylesheets = Vec::new();
        
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
        
        // Update our document
        *self.doc.borrow_mut() = doc;
        *self.content_initialized.borrow_mut() = true;
        
        // IMPORTANT: Force an immediate render - explicitly set needs_render to true
        *self.needs_render.borrow_mut() = true;
        self.log("Document updated, forcing render");
        
        // Perform immediate render to avoid flashing
        match self.render_if_needed() {
            Ok(_) => self.log("Initial render of markdown content successful"),
            Err(e) => self.log(&format!("Initial render failed: {:?}", e)),
        }
        
        Ok(())
    }
    
    /// Update viewport dimensions and re-render
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        self.log(&format!("IFrame::resize({}, {})", width, height));
        
        if width == 0 || height == 0 {
            self.log("Ignoring resize with zero dimension");
            return Ok(());
        }
        
        // Set the resize flag to true - this will be checked in render_if_needed
        RESIZE_HAPPENED.store(true, Ordering::SeqCst);
        self.log("Setting RESIZE_HAPPENED flag");
        
        // Update viewport dimensions
        {
            let mut viewport = match self.viewport.try_lock() {
                Ok(viewport) => viewport,
                Err(_) => {
                    self.log("Failed to lock viewport for resize");
                    return Err(Error::new(E_FAIL, "Failed to lock viewport"));
                }
            };
            
            viewport.window_size = (width, height);
        }
        
        // Mark that we need to render with the new size
        *self.needs_render.borrow_mut() = true;
        
        // Update the document with the new size
        {
            let mut doc = match self.doc.try_borrow_mut() {
                Ok(doc) => doc,
                Err(_) => {
                    self.log("Failed to borrow document for resize");
                    return Err(Error::new(E_FAIL, "Failed to borrow document"));
                }
            };
            
            // Set the new viewport size in the document
            {
                let viewport = match self.viewport.try_lock() {
                    Ok(viewport) => viewport.clone(),
                    Err(_) => {
                        self.log("Failed to lock viewport for updating document");
                        return Err(Error::new(E_FAIL, "Failed to lock viewport"));
                    }
                };
                
                // Update the document with the new viewport dimensions
                doc.as_mut().set_viewport(viewport);
                doc.as_mut().resolve();
            }
        }
        
        self.log(&format!("Resize complete to {}x{}", width, height));
        Ok(())
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
                Err(_) => {
                    self.log("Error: Could not borrow document in pointer_moved");
                    return Ok(())
                }
            };
            
            let viewport = match self.viewport.try_lock() {
                Ok(v) => v,
                Err(_) => {
                    self.log("Error: Could not lock viewport in pointer_moved");
                    return Ok(())
                }
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
                Err(_) => {
                    self.log("Error: Could not borrow document for updating hover state");
                    return Ok(())
                }
            };
            
            // Catch any potential panic in set_hover_to
            changed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                doc.as_mut().set_hover_to(dom_x, dom_y)
            })) {
                Ok(result) => result,
                Err(_) => {
                    self.log("Panic in set_hover_to");
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
                    self.log("Panic in handle_event for MouseMove");
                }
            }
            
            changed
        };
        
        // Only render if something changed
        if should_render {
            match self.render() {
                Ok(_) => (),
                Err(e) => self.log(&format!("Error in render: {:?}", e)),
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
            self.log(&format!("Error in pointer_moved during pressed: {:?}", e));
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
                Err(_) => {
                    self.log("Error: Could not borrow document in pointer_pressed");
                    return Ok(())
                }
            };
            
            // Catch any potential panic
            if let Err(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                doc.as_mut().active_node();
            })) {
                self.log("Panic in active_node");
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
                    self.log("Panic in handle_event for MouseDown");
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
            self.log(&format!("Error in pointer_moved during released: {:?}", e));
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
                self.log("Panic in unactive_node");
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
                    self.log("Panic in handle_event for MouseUp");
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
                    self.log(&format!("Error in click handler: {:?}", e));
                }
            }
            
            Ok(())
        };
        
        if let Err(e) = result {
            self.log(&format!("Error in pointer_released: {:?}", e));
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
            self.log("Panic in mouse_wheel handler");
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
            self.log("Panic in text_input handler");
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
        // Optimize rendering with more comprehensive caching
        
        // 1. Check if we should do any rendering at all
        if !*self.active.borrow() || !*self.content_initialized.borrow() {
            self.log("Skipping render - inactive or content not initialized");
            return Ok(());
        }
        
        // 2. Frame dropping: Check if we're already rendering, and if so, drop this frame
        if *self.drawing_in_progress.borrow() {
            // Keep track of dropped frames
            let dropped = DROPPED_FRAMES.fetch_add(1, Ordering::SeqCst) + 1;
            let consecutive = CONSECUTIVE_DROPS.fetch_add(1, Ordering::SeqCst) + 1;
            
            // Log less frequently to avoid spam
            if consecutive == 1 || consecutive % 10 == 0 {
                self.log(&format!("Dropping frame - previous frame still rendering. Total dropped: {}, Consecutive: {}", 
                                  dropped, consecutive));
            }
            
            // Force a render if we've dropped too many consecutive frames
            // to avoid complete stalling in pathological cases
            if consecutive > 5 {
                self.log(&format!("Force rendering after {} consecutive dropped frames", consecutive));
                // We'll continue with the render below
            } else {
                // Skip this frame
                return Ok(());
            }
        } else {
            // Reset consecutive drops counter since we're rendering this frame
            CONSECUTIVE_DROPS.store(0, Ordering::SeqCst);
        }
        
        // 3. Check for resize event and handle special post-resize rendering
        let resize_happened = RESIZE_HAPPENED.load(Ordering::SeqCst);
        if resize_happened {
            // After a resize, we need to force continuous rendering for a short time
            // to ensure content is properly displayed (fixes white flash issue)
            self.log("Resize detected - forcing render");
            
            // Reset the flag after a few frames to avoid infinite rendering
            static RESIZE_FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);
            let counter = RESIZE_FRAME_COUNTER.fetch_add(1, Ordering::SeqCst);
            
            // Reset flag after 10 consecutive renders
            if counter >= 10 {
                RESIZE_HAPPENED.store(false, Ordering::SeqCst);
                RESIZE_FRAME_COUNTER.store(0, Ordering::SeqCst);
                self.log("Resize recovery completed - returning to normal rendering");
            }
            
            // Always force render during resize recovery
            *self.needs_render.borrow_mut() = true;
        }
        
        // 4. Evaluate if rendering is needed by checking state changes
        let should_render = {
            // Check if rendering was explicitly requested
            let needs_render = *self.needs_render.borrow();
            
            // Lock viewport to check dimensions and scroll position
            let viewport = match self.viewport.try_lock() {
                Ok(v) => v,
                Err(_) => {
                    self.log("Could not lock viewport for caching check");
                    return Ok(());
                }
            };
            
            // Get document info for additional caching checks
            let doc = match self.doc.try_borrow() {
                Ok(doc) => doc,
                Err(_) => {
                    self.log("Could not borrow document for caching check");
                    return Ok(());
                }
            };
            
            // Get current state values
            let current_size = (viewport.window_size.0 as usize, viewport.window_size.1 as usize);
            let viewport_scroll = doc.as_ref().viewport_scroll();
            let current_scroll = (viewport_scroll.x as usize, viewport_scroll.y as usize);
            let current_hover = match doc.as_ref().get_hover_node_id() {
                Some(id) => id,
                None => 0,
            };
            let current_active = match doc.as_ref().get_focussed_node_id() {
                Some(id) => id,
                None => 0,
            };
            
            // Load previous state from atomic variables
            let last_width = LAST_WIDTH.load(Ordering::SeqCst);
            let last_height = LAST_HEIGHT.load(Ordering::SeqCst);
            let last_scroll_x = LAST_SCROLL_X.load(Ordering::SeqCst);
            let last_scroll_y = LAST_SCROLL_Y.load(Ordering::SeqCst);
            let last_hover = LAST_HOVER_NODE.load(Ordering::SeqCst);
            let last_active = LAST_ACTIVE_NODE.load(Ordering::SeqCst);
            
            // Determine if we need to render
            let size_changed = current_size.0 != last_width || current_size.1 != last_height;
            let scroll_changed = current_scroll.0 != last_scroll_x || current_scroll.1 != last_scroll_y;
            let hover_changed = current_hover != last_hover;
            let active_changed = current_active != last_active;
            
            // Force redraw if too many renders have been skipped (safety net)
            let render_count = RENDERING_COUNT.fetch_add(1, Ordering::SeqCst);
            let force_periodic = render_count > 100; // Force render every 100 potential renders
            
            if force_periodic {
                RENDERING_COUNT.store(0, Ordering::SeqCst);
                self.log("Forcing periodic render to ensure content freshness");
            }
            
            // Update cached state regardless of render decision
            LAST_WIDTH.store(current_size.0, Ordering::SeqCst);
            LAST_HEIGHT.store(current_size.1, Ordering::SeqCst);
            LAST_SCROLL_X.store(current_scroll.0, Ordering::SeqCst);
            LAST_SCROLL_Y.store(current_scroll.1, Ordering::SeqCst);
            LAST_HOVER_NODE.store(current_hover, Ordering::SeqCst);
            LAST_ACTIVE_NODE.store(current_active, Ordering::SeqCst);
            
            // Log what triggered the render if we're going to render
            if needs_render || size_changed || scroll_changed || hover_changed || active_changed || force_periodic {
                let render_reason = if needs_render {
                    "explicit request"
                } else if size_changed {
                    "size change"
                } else if scroll_changed {
                    "scroll position change"
                } else if hover_changed {
                    "hover state change"
                } else if active_changed {
                    "active state change"
                } else {
                    "periodic refresh"
                };
                
                self.log(&format!("Render needed due to: {}", render_reason));
                true
            } else {
                self.log("No render needed - content unchanged");
                false
            }
        };
        
        // 5. Skip rendering if nothing changed
        if !should_render {
            // Make sure we reset the needs_render flag even if we skip rendering
            *self.needs_render.borrow_mut() = false;
            return Ok(());
        }
        
        // 6. Acquire device context lock
        let _device_lock = match self.device_context_lock.try_lock() {
            Ok(lock) => lock,
            Err(_) => {
                self.log("Device context already locked by another thread, skipping render");
                return Ok(());
            }
        };
        
        // 7. Reset needs_render flag and set drawing_in_progress flag
        *self.needs_render.borrow_mut() = false;
        *self.drawing_in_progress.borrow_mut() = true;
        
        // 8. Set up scope to ensure we always unset the drawing flag when done
        let result: Result<()> = {
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
                    *self.needs_render.borrow_mut() = true;
                    self.log("Could not borrow device context for rendering");
                    return Ok(());
                }
            };

            self.log("Starting D2D rendering process");
            self.log(&format!("Viewport size: {}x{}", viewport.window_size.0, viewport.window_size.1));
            self.log(&format!("Scale factor: {}", viewport.scale_f64()));
            
            // Set FORCE_REDRAW to true to ensure d2drender actually draws
            // Note: we already know a redraw is needed at this point
            FORCE_REDRAW.store(true, Ordering::SeqCst);
            
            // Use a safe approach to handle the Direct2D rendering
            unsafe {
                // Call the blitz-renderer-vello d2drender module directly
                // d2drender.rs now handles all BeginDraw/EndDraw internally
                d2drender::generate_d2d_scene(
                    &mut *device_context,
                    doc.as_ref(),
                    viewport.scale_f64(),
                    viewport.window_size.0, 
                    viewport.window_size.1,
                    devtools,
                );
                
                self.log("Successfully completed d2drender::generate_d2d_scene call");
            }
            
            Ok(())
        };
        
        // 9. ALWAYS unset the drawing flag when we're done, regardless of success or failure
        *self.drawing_in_progress.borrow_mut() = false;
        
        match &result {
            Ok(_) => self.log("Rendering completed successfully"),
            Err(e) => self.log(&format!("Rendering failed: 0x{:08X}", e.code().0)),
        }
        
        result
    }

    /// Tick function called by the rendering loop - performs rendering if needed
    pub fn tick(&self) -> Result<()> {
        self.log("D2DRenderer.tick called");
        
        // Use catch_unwind to safely handle any potential panics
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let result = self.render_if_needed();
            match &result {
                Ok(_) => self.log("D2DRenderer.tick - render_if_needed completed successfully"),
                Err(e) => self.log(&format!("D2DRenderer.tick - render_if_needed failed: 0x{:08X}", e.code().0)),
            }
            result
        }));
        
        // Handle the catch_unwind result
        match result {
            Ok(inner_result) => {
                self.log("d2drenderer_tick completed successfully");
                inner_result
            },
            Err(_) => {
                self.log("Panic occurred in tick function");
                Err(Error::new(windows::Win32::Foundation::E_FAIL, "Panic during tick"))
            }
        }
    }
}