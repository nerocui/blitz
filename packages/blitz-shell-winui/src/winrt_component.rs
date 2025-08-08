use std::sync::Arc;

use anyrender::WindowRenderer as _;
use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{Document, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::raw_handle::DxgiInteropHandle;
use windows::Win32::Foundation::HWND;

/// Public host object backing the WinRT class. Keeps the document and renderer alive and exposes
/// methods called from C# to drive rendering and input.
pub struct BlitzHost {
    renderer: VelloWindowRenderer,
    window: Arc<dyn anyrender::WindowHandle>,
    doc: Box<dyn Document>,
}

impl BlitzHost {
    pub fn new_for_swapchain(_panel: crate::SwapChainPanelHandle, width: u32, height: u32, scale: f32) -> Result<Self, String> {
    let _ = scale;
        // TODO: use panel.swapchain to get HWND or a surface target. For now, assume we can extract HWND somehow.
        // Placeholder: require caller to call SetHwnd before rendering.
    let hwnd: Option<HWND> = None;
    let window: Arc<dyn anyrender::WindowHandle> = Arc::new(DxgiInteropHandle::from(HWND(core::ptr::null_mut())));

        // Minimal HTML doc placeholder; host can replace by calling load_html.
        let doc = HtmlDocument::from_html(
            "<html><body><h1>Blitz WinUI host</h1><p>Initialize succeeded.</p></body></html>",
            DocumentConfig::default(),
        );

        let mut renderer = VelloWindowRenderer::new();
        if let Some(hwnd) = hwnd {
            let win = Arc::new(DxgiInteropHandle::from(hwnd)) as Arc<dyn anyrender::WindowHandle>;
            renderer.resume(win, width, height);
        }

        Ok(Self { renderer, window, doc: Box::new(doc) })
    }

    pub fn set_hwnd(&mut self, hwnd: isize, width: u32, height: u32) {
        // Create or re-create the wgpu surface against the new HWND
        let win = Arc::new(DxgiInteropHandle::from(hwnd)) as Arc<dyn anyrender::WindowHandle>;
        if self.renderer.is_active() {
            // suspend and resume on new window to recreate surface
            self.renderer.suspend();
        }
        self.renderer.resume(win.clone(), width, height);
        self.window = win;
    }

    // Placeholder for real SwapChainPanel interop; panel is a WinRT object reference.
    pub fn set_panel(&mut self, _panel: *mut core::ffi::c_void, width: u32, height: u32) {
        // TODO: Implement ISwapChainPanelNative interop to create/recreate the surface.
        // For now, keep the HWND path; this method exists so WinRT surface can forward here later.
        let _ = (width, height);
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
    let viewport = Viewport::new(width, height, scale, ColorScheme::Light);
        self.doc.set_viewport(viewport);
        self.renderer.set_size(width, height);
    }

    pub fn render_once(&mut self) {
        let (width, height) = self.doc.viewport().window_size;
        let scale = self.doc.viewport().scale_f64();
        self.doc.resolve();
        self.renderer
            .render(|scene| paint_scene(scene, &self.doc, scale, width, height));
    }

    pub fn load_html(&mut self, html: &str) {
        let cfg = DocumentConfig::default();
        let new_doc = HtmlDocument::from_html(html, cfg);
        let scroll = self.doc.viewport_scroll();
        let viewport = self.doc.viewport().clone();
        self.doc = Box::new(new_doc);
        self.doc.set_viewport(viewport);
        self.doc.set_viewport_scroll(scroll);
    }

    // Input bridging (to be called from C# event handlers)
    pub fn pointer_move(&mut self, x: f32, y: f32, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButtons, UiEvent};
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
        self.doc.handle_ui_event(UiEvent::MouseMove(BlitzMouseButtonEvent {
            x,
            y,
            button: Default::default(),
            buttons,
            mods,
        }));
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButton, MouseEventButtons, UiEvent};
        let btn = match button {
            0 => MouseEventButton::Main,
            1 => MouseEventButton::Auxiliary,
            2 => MouseEventButton::Secondary,
            3 => MouseEventButton::Fourth,
            4 => MouseEventButton::Fifth,
            _ => MouseEventButton::Main,
        };
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
        self.doc.handle_ui_event(UiEvent::MouseDown(BlitzMouseButtonEvent {
            x,
            y,
            button: btn,
            buttons,
            mods,
        }));
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, button: u8, buttons: u32, mods: u32) {
        use blitz_traits::events::{BlitzMouseButtonEvent, MouseEventButton, MouseEventButtons, UiEvent};
        let btn = match button {
            0 => MouseEventButton::Main,
            1 => MouseEventButton::Auxiliary,
            2 => MouseEventButton::Secondary,
            3 => MouseEventButton::Fourth,
            4 => MouseEventButton::Fifth,
            _ => MouseEventButton::Main,
        };
        let buttons = MouseEventButtons::from_bits_truncate(buttons as u8);
        let mods = keyboard_types::Modifiers::from_bits_truncate(mods);
        self.doc.handle_ui_event(UiEvent::MouseUp(BlitzMouseButtonEvent {
            x,
            y,
            button: btn,
            buttons,
            mods,
        }));
    }

    pub fn wheel_scroll(&mut self, dx: f64, dy: f64) {
        if let Some(hover_node_id) = self.doc.get_hover_node_id() {
            self.doc.scroll_node_by(hover_node_id, dx, dy);
        } else {
            self.doc.scroll_viewport_by(dx, dy);
        }
    }

    pub fn key_down(&mut self, vk: u32, ch: u32, mods: u32, is_auto_repeating: bool) {
        use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
        let key = vk_or_char_to_key(vk, ch);
        let code = keyboard_types::Code::Unidentified;
        let modifiers = keyboard_types::Modifiers::from_bits_truncate(mods);
        let location = keyboard_types::Location::Standard;
        let text = char_from_u32(ch).map(|c| c.into());
        let evt = BlitzKeyEvent {
            key,
            code,
            modifiers,
            location,
            is_auto_repeating,
            is_composing: false,
            state: KeyState::Pressed,
            text,
        };
        self.doc.handle_ui_event(UiEvent::KeyDown(evt));
    }

    pub fn key_up(&mut self, vk: u32, ch: u32, mods: u32) {
        use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
        let key = vk_or_char_to_key(vk, ch);
        let code = keyboard_types::Code::Unidentified;
        let modifiers = keyboard_types::Modifiers::from_bits_truncate(mods);
        let location = keyboard_types::Location::Standard;
        let text = char_from_u32(ch).map(|c| c.into());
        let evt = BlitzKeyEvent {
            key,
            code,
            modifiers,
            location,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Released,
            text,
        };
        self.doc.handle_ui_event(UiEvent::KeyUp(evt));
    }
}

fn char_from_u32(ch: u32) -> Option<String> {
    char::from_u32(ch).map(|c| c.to_string())
}

fn vk_or_char_to_key(vk: u32, ch: u32) -> keyboard_types::Key {
    use keyboard_types::Key;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    if let Some(s) = char_from_u32(ch) {
        return Key::Character(s);
    }
    let v = VIRTUAL_KEY(vk as u16);
    match v {
        VK_BACK => Key::Backspace,
        VK_TAB => Key::Tab,
        VK_RETURN => Key::Enter,
        VK_ESCAPE => Key::Escape,
        VK_SPACE => Key::Character(" ".into()),
        VK_LEFT => Key::ArrowLeft,
        VK_UP => Key::ArrowUp,
        VK_RIGHT => Key::ArrowRight,
        VK_DOWN => Key::ArrowDown,
        _ => Key::Unidentified,
    }
}
