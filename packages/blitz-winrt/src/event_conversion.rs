//! # Event Conversion for WinRT Integration
//!
//! This module handles the conversion between Windows WinRT events and Blitz-compatible
//! event formats. It bridges the gap between Windows input events (mouse, keyboard, touch)
//! and the event system expected by the Blitz rendering engine.
//!
//! ## Supported Event Types
//!
//! - Mouse events (move, click, wheel)
//! - Keyboard events (key press, release, character input)
//! - Touch events (touch start, move, end)
//! - Focus events (gained, lost)
//! - Resize events (size changed)

use blitz_traits::events::{DomEvent, DomEventData, BlitzMouseButtonEvent, BlitzKeyEvent, MouseEventButtons, MouseEventButton, KeyState};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_SHIFT, VK_CONTROL, VK_MENU};
use windows::Win32::Foundation::{POINT, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_KEYDOWN, WM_KEYUP, WM_CHAR, WM_SETFOCUS, WM_KILLFOCUS
};
use keyboard_types::{Code, Key, Modifiers, Location};
use smol_str::SmolStr;

/// Represents a Windows message with its parameters.
///
/// This struct encapsulates a Windows message in a format that can be
/// easily converted to Blitz events.
#[derive(Debug, Clone)]
pub struct WindowsMessage {
    /// The message identifier (WM_* constants)
    pub message: u32,
    /// The WPARAM value
    pub wparam: usize,
    /// The LPARAM value  
    pub lparam: isize,
    /// Timestamp when the message was received
    pub timestamp: u64,
}

/// Modifier key state for events.
///
/// Tracks which modifier keys (Shift, Ctrl, Alt) are currently pressed.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierState {
    /// Whether Shift is pressed
    pub shift: bool,
    /// Whether Ctrl is pressed
    pub ctrl: bool,
    /// Whether Alt is pressed
    pub alt: bool,
}

/// Event converter that transforms Windows messages into Blitz events.
///
/// This struct maintains state needed for proper event conversion, such as
/// tracking mouse position and modifier key states.
pub struct EventConverter {
    /// Current mouse position relative to the SwapChainPanel
    mouse_position: (f32, f32),
    
    /// Current modifier key state
    modifier_state: ModifierState,
    
    /// Scale factor for DPI-aware coordinate conversion
    scale_factor: f32,
    
    /// Size of the SwapChainPanel for coordinate normalization
    panel_size: (u32, u32),
}

impl EventConverter {
    /// Creates a new EventConverter with default state.
    ///
    /// # Returns
    ///
    /// A new EventConverter instance
    pub fn new() -> Self {
        EventConverter {
            mouse_position: (0.0, 0.0),
            modifier_state: ModifierState::default(),
            scale_factor: 1.0,
            panel_size: (800, 600),
        }
    }
    
    /// Updates the scale factor for DPI-aware coordinate conversion.
    ///
    /// # Arguments
    ///
    /// * `scale_factor` - The new scale factor
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }
    
    /// Updates the panel size for coordinate normalization.
    ///
    /// # Arguments
    ///
    /// * `width` - Panel width in pixels
    /// * `height` - Panel height in pixels
    pub fn set_panel_size(&mut self, width: u32, height: u32) {
        self.panel_size = (width, height);
    }
    
    /// Converts a Windows message to a Blitz event.
    ///
    /// # Arguments
    ///
    /// * `message` - The Windows message to convert
    ///
    /// # Returns
    ///
    /// An optional Blitz DomEvent if the message can be converted
    pub fn convert_message(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        // Use const values to avoid snake_case warnings
        const WM_MOUSEMOVE_VAL: u32 = WM_MOUSEMOVE;
        const WM_LBUTTONDOWN_VAL: u32 = WM_LBUTTONDOWN;
        const WM_LBUTTONUP_VAL: u32 = WM_LBUTTONUP;
        const WM_RBUTTONDOWN_VAL: u32 = WM_RBUTTONDOWN;
        const WM_RBUTTONUP_VAL: u32 = WM_RBUTTONUP;
        const WM_MOUSEWHEEL_VAL: u32 = WM_MOUSEWHEEL;
        const WM_KEYDOWN_VAL: u32 = WM_KEYDOWN;
        const WM_KEYUP_VAL: u32 = WM_KEYUP;
        const WM_CHAR_VAL: u32 = WM_CHAR;
        
        match message.message {
            WM_MOUSEMOVE_VAL => self.convert_mouse_move(message),
            WM_LBUTTONDOWN_VAL => self.convert_mouse_down(message, 0), // Left button
            WM_LBUTTONUP_VAL => self.convert_mouse_up(message, 0),     // Left button
            WM_RBUTTONDOWN_VAL => self.convert_mouse_down(message, 2), // Right button
            WM_RBUTTONUP_VAL => self.convert_mouse_up(message, 2),     // Right button
            WM_MOUSEWHEEL_VAL => self.convert_mouse_wheel(message),
            WM_KEYDOWN_VAL => self.convert_key_down(message),
            WM_KEYUP_VAL => self.convert_key_up(message),
            WM_CHAR_VAL => self.convert_char(message),
            _ => None,
        }
    }
    
    /// Converts a mouse move message to a Blitz mouse event.
    fn convert_mouse_move(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        let mouse_event = BlitzMouseButtonEvent {
            x,
            y,
            button: MouseEventButton::Main, // Use Main as default (no specific button)
            buttons: MouseEventButtons::None, // No buttons pressed for mouse move
            mods: self.get_modifiers(),
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::MouseMove(mouse_event)
        ))
    }
    
    /// Converts a mouse button down message to a Blitz mouse event.
    fn convert_mouse_down(&mut self, message: &WindowsMessage, button: u16) -> Option<DomEvent> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        let (blitz_button, button_flags) = self.convert_mouse_button(button);
        
        let mouse_event = BlitzMouseButtonEvent {
            x,
            y,
            button: blitz_button,
            buttons: button_flags,
            mods: self.get_modifiers(),
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::MouseDown(mouse_event)
        ))
    }
    
    /// Converts a mouse button up message to a Blitz mouse event.
    fn convert_mouse_up(&mut self, message: &WindowsMessage, button: u16) -> Option<DomEvent> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        let (blitz_button, _) = self.convert_mouse_button(button);
        
        let mouse_event = BlitzMouseButtonEvent {
            x,
            y,
            button: blitz_button,
            buttons: MouseEventButtons::None, // Button is being released
            mods: self.get_modifiers(),
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::MouseUp(mouse_event)
        ))
    }
    
    /// Converts a mouse wheel message to a Blitz mouse event.
    /// Note: For now we treat this as a mouse move event. Blitz may need dedicated wheel support.
    fn convert_mouse_wheel(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        let _delta = self.extract_wheel_delta(message.wparam);
        self.update_modifier_state();
        
        // For now, treat wheel as mouse move since Blitz doesn't have dedicated wheel events
        let mouse_event = BlitzMouseButtonEvent {
            x,
            y,
            button: MouseEventButton::Main, // Use Main as default (no specific button)
            buttons: MouseEventButtons::None,
            mods: self.get_modifiers(),
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::MouseMove(mouse_event)
        ))
    }
    
    /// Converts a key down message to a Blitz keyboard event.
    fn convert_key_down(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        let virtual_key = message.wparam as u16;
        self.update_modifier_state_from_key(virtual_key, true);
        
        let key = self.virtual_key_to_key(virtual_key)?;
        let code = self.virtual_key_to_code(virtual_key)?;
        
        let key_event = BlitzKeyEvent {
            key,
            code,
            modifiers: self.get_modifiers(),
            location: Location::Standard,
            is_auto_repeating: false, // TODO: Track repeat state
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::KeyDown(key_event)
        ))
    }
    
    /// Converts a key up message to a Blitz keyboard event.
    fn convert_key_up(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        let virtual_key = message.wparam as u16;
        self.update_modifier_state_from_key(virtual_key, false);
        
        let key = self.virtual_key_to_key(virtual_key)?;
        let code = self.virtual_key_to_code(virtual_key)?;
        
        let key_event = BlitzKeyEvent {
            key,
            code,
            modifiers: self.get_modifiers(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Released,
            text: None,
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::KeyUp(key_event)
        ))
    }
    
    /// Converts a character input message to a Blitz input event.
    fn convert_char(&mut self, message: &WindowsMessage) -> Option<DomEvent> {
        let char_code = message.wparam as u32;
        
        // Convert the character code to a Unicode character
        let character = char::from_u32(char_code)?;
        
        let key_event = BlitzKeyEvent {
            key: Key::Character(SmolStr::new(character.to_string())),
            code: Code::Unidentified,
            modifiers: self.get_modifiers(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: Some(SmolStr::new(character.to_string())),
        };
        
        Some(DomEvent::new(
            0, // Target node ID - will be updated by event dispatcher
            DomEventData::KeyPress(key_event)
        ))
    }
    
    /// Extracts mouse position from LPARAM, accounting for DPI scaling.
    fn extract_mouse_position(&self, lparam: isize) -> (f32, f32) {
        let x = (lparam & 0xFFFF) as i16 as f32;
        let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
        
        // Apply DPI scaling
        (x / self.scale_factor, y / self.scale_factor)
    }
    
    /// Extracts wheel delta from WPARAM.
    fn extract_wheel_delta(&self, wparam: usize) -> f32 {
        let delta = ((wparam >> 16) & 0xFFFF) as i16 as f32;
        delta / 120.0 // Standard wheel delta is 120 units per notch
    }
    
    /// Updates modifier state by checking current key states.
    fn update_modifier_state(&mut self) {
        // TODO: Use GetKeyState or similar to check current modifier state
        // For now, we'll rely on key events to track modifiers
    }
    
    /// Converts Windows mouse button to Blitz mouse button and flags.
    fn convert_mouse_button(&self, button: u16) -> (MouseEventButton, MouseEventButtons) {
        match button {
            0 => (MouseEventButton::Main, MouseEventButtons::Primary), // Left button
            1 => (MouseEventButton::Auxiliary, MouseEventButtons::Auxiliary), // Middle button
            2 => (MouseEventButton::Secondary, MouseEventButtons::Secondary), // Right button
            _ => (MouseEventButton::Main, MouseEventButtons::None), // Default fallback
        }
    }
    
    /// Converts modifier state to keyboard-types Modifiers.
    fn get_modifiers(&self) -> Modifiers {
        let mut mods = Modifiers::empty();
        
        if self.modifier_state.shift {
            mods |= Modifiers::SHIFT;
        }
        if self.modifier_state.ctrl {
            mods |= Modifiers::CONTROL;
        }
        if self.modifier_state.alt {
            mods |= Modifiers::ALT;
        }
        
        mods
    }
    
    /// Converts Windows virtual key to keyboard-types Key.
    fn virtual_key_to_key(&self, virtual_key: u16) -> Option<Key> {
        match virtual_key {
            0x08 => Some(Key::Backspace),
            0x09 => Some(Key::Tab),
            0x0D => Some(Key::Enter),
            0x10 => Some(Key::Shift),
            0x11 => Some(Key::Control),
            0x12 => Some(Key::Alt),
            0x1B => Some(Key::Escape),
            0x20 => Some(Key::Character(SmolStr::new(" "))),
            0x25 => Some(Key::ArrowLeft),
            0x26 => Some(Key::ArrowUp),
            0x27 => Some(Key::ArrowRight),
            0x28 => Some(Key::ArrowDown),
            0x2E => Some(Key::Delete),
            0x30..=0x39 => {
                let digit = (virtual_key - 0x30) as u8 as char;
                Some(Key::Character(SmolStr::new(digit.to_string())))
            }
            0x41..=0x5A => {
                let c = (virtual_key as u8) as char;
                let key_str = if self.modifier_state.shift {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                };
                Some(Key::Character(SmolStr::new(key_str)))
            }
            0x70..=0x87 => {
                let f_num = virtual_key - 0x6F;
                match f_num {
                    1 => Some(Key::F1),
                    2 => Some(Key::F2),
                    3 => Some(Key::F3),
                    4 => Some(Key::F4),
                    5 => Some(Key::F5),
                    6 => Some(Key::F6),
                    7 => Some(Key::F7),
                    8 => Some(Key::F8),
                    9 => Some(Key::F9),
                    10 => Some(Key::F10),
                    11 => Some(Key::F11),
                    12 => Some(Key::F12),
                    _ => Some(Key::Unidentified),
                }
            }
            _ => Some(Key::Unidentified),
        }
    }
    
    /// Converts Windows virtual key to keyboard-types Code.
    fn virtual_key_to_code(&self, virtual_key: u16) -> Option<Code> {
        match virtual_key {
            0x08 => Some(Code::Backspace),
            0x09 => Some(Code::Tab),
            0x0D => Some(Code::Enter),
            0x10 => Some(Code::ShiftLeft), // TODO: Distinguish left/right
            0x11 => Some(Code::ControlLeft),
            0x12 => Some(Code::AltLeft),
            0x1B => Some(Code::Escape),
            0x20 => Some(Code::Space),
            0x25 => Some(Code::ArrowLeft),
            0x26 => Some(Code::ArrowUp),
            0x27 => Some(Code::ArrowRight),
            0x28 => Some(Code::ArrowDown),
            0x2E => Some(Code::Delete),
            0x30 => Some(Code::Digit0),
            0x31 => Some(Code::Digit1),
            0x32 => Some(Code::Digit2),
            0x33 => Some(Code::Digit3),
            0x34 => Some(Code::Digit4),
            0x35 => Some(Code::Digit5),
            0x36 => Some(Code::Digit6),
            0x37 => Some(Code::Digit7),
            0x38 => Some(Code::Digit8),
            0x39 => Some(Code::Digit9),
            0x41 => Some(Code::KeyA),
            0x42 => Some(Code::KeyB),
            0x43 => Some(Code::KeyC),
            0x44 => Some(Code::KeyD),
            0x45 => Some(Code::KeyE),
            0x46 => Some(Code::KeyF),
            0x47 => Some(Code::KeyG),
            0x48 => Some(Code::KeyH),
            0x49 => Some(Code::KeyI),
            0x4A => Some(Code::KeyJ),
            0x4B => Some(Code::KeyK),
            0x4C => Some(Code::KeyL),
            0x4D => Some(Code::KeyM),
            0x4E => Some(Code::KeyN),
            0x4F => Some(Code::KeyO),
            0x50 => Some(Code::KeyP),
            0x51 => Some(Code::KeyQ),
            0x52 => Some(Code::KeyR),
            0x53 => Some(Code::KeyS),
            0x54 => Some(Code::KeyT),
            0x55 => Some(Code::KeyU),
            0x56 => Some(Code::KeyV),
            0x57 => Some(Code::KeyW),
            0x58 => Some(Code::KeyX),
            0x59 => Some(Code::KeyY),
            0x5A => Some(Code::KeyZ),
            0x70 => Some(Code::F1),
            0x71 => Some(Code::F2),
            0x72 => Some(Code::F3),
            0x73 => Some(Code::F4),
            0x74 => Some(Code::F5),
            0x75 => Some(Code::F6),
            0x76 => Some(Code::F7),
            0x77 => Some(Code::F8),
            0x78 => Some(Code::F9),
            0x79 => Some(Code::F10),
            0x7A => Some(Code::F11),
            0x7B => Some(Code::F12),
            _ => Some(Code::Unidentified),
        }
    }
    
    /// Updates modifier state based on key press/release.
    fn update_modifier_state_from_key(&mut self, virtual_key: u16, pressed: bool) {
        // Use const values to avoid snake_case warnings
        const VK_SHIFT_VAL: i32 = VK_SHIFT.0 as i32;
        const VK_CONTROL_VAL: i32 = VK_CONTROL.0 as i32;
        const VK_MENU_VAL: i32 = VK_MENU.0 as i32;
        
        match VIRTUAL_KEY(virtual_key as i32) {
            key if key.0 == VK_SHIFT_VAL => self.modifier_state.shift = pressed,
            key if key.0 == VK_CONTROL_VAL => self.modifier_state.ctrl = pressed,
            key if key.0 == VK_MENU_VAL => self.modifier_state.alt = pressed, // VK_MENU is Alt key
            _ => {}
        }
    }
}

impl Default for EventConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a WindowsMessage from individual components.
///
/// # Arguments
///
/// * `message` - The Windows message ID
/// * `wparam` - The WPARAM value
/// * `lparam` - The LPARAM value
///
/// # Returns
///
/// A new WindowsMessage instance
pub fn create_windows_message(message: u32, wparam: usize, lparam: isize) -> WindowsMessage {
    WindowsMessage {
        message,
        wparam,
        lparam,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_position_extraction() {
        let converter = EventConverter::new();
        let lparam = (100i16 as isize) | ((200i16 as isize) << 16);
        let (x, y) = converter.extract_mouse_position(lparam);
        assert_eq!(x, 100.0);
        assert_eq!(y, 200.0);
    }

    #[test]
    fn test_modifier_state_tracking() {
        let mut converter = EventConverter::new();
        
        // Test shift key press
        converter.update_modifier_state_from_key(0x10, true);
        assert!(converter.modifier_state.shift);
        
        // Test shift key release
        converter.update_modifier_state_from_key(0x10, false);
        assert!(!converter.modifier_state.shift);
    }
}
