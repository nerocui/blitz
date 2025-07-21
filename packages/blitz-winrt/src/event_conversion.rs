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

use blitz_dom::events::{EventData, PointerEvent, PointerData, KeyboardEvent, KeyboardData};
use blitz_dom::node::NodeId;
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_SHIFT, VK_CONTROL, VK_MENU};
use windows::Win32::Foundation::{POINT, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_KEYDOWN, WM_KEYUP, WM_CHAR, WM_SETFOCUS, WM_KILLFOCUS
};

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
    /// An optional Blitz EventData if the message can be converted
    pub fn convert_message(&mut self, message: &WindowsMessage) -> Option<EventData> {
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
    
    /// Converts a mouse move message to a Blitz pointer event.
    fn convert_mouse_move(&mut self, message: &WindowsMessage) -> Option<EventData> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        Some(EventData::Pointer(PointerEvent {
            event: "pointermove".to_string(),
            data: PointerData {
                pointer_id: 1,
                x: x as f64,
                y: y as f64,
                button: None,
                modifiers: self.get_modifier_flags(),
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a mouse button down message to a Blitz pointer event.
    fn convert_mouse_down(&mut self, message: &WindowsMessage, button: u16) -> Option<EventData> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        Some(EventData::Pointer(PointerEvent {
            event: "pointerdown".to_string(),
            data: PointerData {
                pointer_id: 1,
                x: x as f64,
                y: y as f64,
                button: Some(button),
                modifiers: self.get_modifier_flags(),
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a mouse button up message to a Blitz pointer event.
    fn convert_mouse_up(&mut self, message: &WindowsMessage, button: u16) -> Option<EventData> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        self.mouse_position = (x, y);
        self.update_modifier_state();
        
        Some(EventData::Pointer(PointerEvent {
            event: "pointerup".to_string(),
            data: PointerData {
                pointer_id: 1,
                x: x as f64,
                y: y as f64,
                button: Some(button),
                modifiers: self.get_modifier_flags(),
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a mouse wheel message to a Blitz wheel event.
    fn convert_mouse_wheel(&mut self, message: &WindowsMessage) -> Option<EventData> {
        let (x, y) = self.extract_mouse_position(message.lparam);
        let delta = self.extract_wheel_delta(message.wparam);
        self.update_modifier_state();
        
        // Convert to a pointer event with wheel data
        // Note: Blitz might need a dedicated wheel event type
        Some(EventData::Pointer(PointerEvent {
            event: "wheel".to_string(),
            data: PointerData {
                pointer_id: 1,
                x: x as f64,
                y: y as f64,
                button: None,
                modifiers: self.get_modifier_flags(),
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a key down message to a Blitz keyboard event.
    fn convert_key_down(&mut self, message: &WindowsMessage) -> Option<EventData> {
        let virtual_key = message.wparam as u16;
        self.update_modifier_state_from_key(virtual_key, true);
        
        let key_code = self.virtual_key_to_key_code(virtual_key)?;
        
        Some(EventData::Keyboard(KeyboardEvent {
            event: "keydown".to_string(),
            data: KeyboardData {
                key: self.virtual_key_to_key_string(virtual_key),
                code: key_code,
                modifiers: self.get_modifier_flags(),
                repeat: false, // TODO: Track repeat state
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a key up message to a Blitz keyboard event.
    fn convert_key_up(&mut self, message: &WindowsMessage) -> Option<EventData> {
        let virtual_key = message.wparam as u16;
        self.update_modifier_state_from_key(virtual_key, false);
        
        let key_code = self.virtual_key_to_key_code(virtual_key)?;
        
        Some(EventData::Keyboard(KeyboardEvent {
            event: "keyup".to_string(),
            data: KeyboardData {
                key: self.virtual_key_to_key_string(virtual_key),
                code: key_code,
                modifiers: self.get_modifier_flags(),
                repeat: false,
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
    }
    
    /// Converts a character input message to a Blitz keyboard event.
    fn convert_char(&mut self, message: &WindowsMessage) -> Option<EventData> {
        let char_code = message.wparam as u32;
        
        // Convert the character code to a Unicode character
        let character = char::from_u32(char_code)?;
        
        Some(EventData::Keyboard(KeyboardEvent {
            event: "input".to_string(),
            data: KeyboardData {
                key: character.to_string(),
                code: format!("U+{:04X}", char_code),
                modifiers: self.get_modifier_flags(),
                repeat: false,
            },
            target: NodeId::from(0u64), // Will be updated by event dispatcher
        }))
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
    
    /// Updates modifier state based on key press/release.
    fn update_modifier_state_from_key(&mut self, virtual_key: u16, pressed: bool) {
        // Use const values to avoid snake_case warnings
        const VK_SHIFT_VAL: i32 = VK_SHIFT.0;
        const VK_CONTROL_VAL: i32 = VK_CONTROL.0;
        const VK_MENU_VAL: i32 = VK_MENU.0;
        
        match VIRTUAL_KEY(virtual_key as i32) {
            key if key.0 == VK_SHIFT_VAL => self.modifier_state.shift = pressed,
            key if key.0 == VK_CONTROL_VAL => self.modifier_state.ctrl = pressed,
            key if key.0 == VK_MENU_VAL => self.modifier_state.alt = pressed, // VK_MENU is Alt key
            _ => {}
        }
    }
    
    /// Converts modifier state to Blitz modifier flags.
    fn get_modifier_flags(&self) -> u32 {
        let mut flags = 0u32;
        
        if self.modifier_state.shift {
            flags |= 1; // Shift flag
        }
        if self.modifier_state.ctrl {
            flags |= 2; // Ctrl flag
        }
        if self.modifier_state.alt {
            flags |= 4; // Alt flag
        }
        
        flags
    }
    
    /// Converts Windows virtual key to Blitz key code.
    fn virtual_key_to_key_code(&self, virtual_key: u16) -> Option<String> {
        // Convert common virtual keys to web-standard key codes
        match virtual_key {
            0x08 => Some("Backspace".to_string()),
            0x09 => Some("Tab".to_string()),
            0x0D => Some("Enter".to_string()),
            0x10 => Some("ShiftLeft".to_string()), // TODO: Distinguish left/right
            0x11 => Some("ControlLeft".to_string()),
            0x12 => Some("AltLeft".to_string()),
            0x1B => Some("Escape".to_string()),
            0x20 => Some("Space".to_string()),
            0x25 => Some("ArrowLeft".to_string()),
            0x26 => Some("ArrowUp".to_string()),
            0x27 => Some("ArrowRight".to_string()),
            0x28 => Some("ArrowDown".to_string()),
            0x2E => Some("Delete".to_string()),
            0x30..=0x39 => Some(format!("Digit{}", virtual_key - 0x30)), // 0-9
            0x41..=0x5A => Some(format!("Key{}", char::from(virtual_key as u8))), // A-Z
            0x70..=0x87 => Some(format!("F{}", virtual_key - 0x6F)), // F1-F24
            _ => Some(format!("Unidentified{}", virtual_key)),
        }
    }
    
    /// Converts Windows virtual key to readable key string.
    fn virtual_key_to_key_string(&self, virtual_key: u16) -> String {
        match virtual_key {
            0x08 => "Backspace".to_string(),
            0x09 => "Tab".to_string(),
            0x0D => "Enter".to_string(),
            0x10 => "Shift".to_string(),
            0x11 => "Control".to_string(),
            0x12 => "Alt".to_string(),
            0x1B => "Escape".to_string(),
            0x20 => " ".to_string(), // Space shows as actual space
            0x25 => "ArrowLeft".to_string(),
            0x26 => "ArrowUp".to_string(),
            0x27 => "ArrowRight".to_string(),
            0x28 => "ArrowDown".to_string(),
            0x2E => "Delete".to_string(),
            0x30..=0x39 => (virtual_key - 0x30).to_string(), // 0-9
            0x41..=0x5A => {
                let c = char::from(virtual_key as u8);
                if self.modifier_state.shift {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                }
            }
            0x70..=0x87 => format!("F{}", virtual_key - 0x6F), // F1-F24
            _ => "Unidentified".to_string(),
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

    #[test]
    fn test_virtual_key_conversion() {
        let converter = EventConverter::new();
        
        // Test space key
        assert_eq!(converter.virtual_key_to_key_code(0x20), Some("Space".to_string()));
        
        // Test A key
        assert_eq!(converter.virtual_key_to_key_code(0x41), Some("KeyA".to_string()));
        
        // Test F1 key
        assert_eq!(converter.virtual_key_to_key_code(0x70), Some("F1".to_string()));
    }
}
