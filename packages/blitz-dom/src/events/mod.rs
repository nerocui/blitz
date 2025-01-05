mod ime;
mod keyboard;
mod mouse;

pub(crate) use ime::handle_ime_event;
pub(crate) use keyboard::handle_keypress;
pub(crate) use mouse::handle_click;

use crate::Document;
use winit::event::{Ime, KeyEvent, Modifiers};

pub(crate) fn handle_event(doc: &mut Document, event: RendererEvent) {
    let target_node_id = event.target;

    match event.data {
        EventData::MouseDown { .. } | EventData::MouseUp { .. } => {}
        EventData::Hover => {}
        EventData::Click { x, y, .. } => {
            handle_click(doc, target_node_id, x, y);
        }
        EventData::KeyPress { event, mods } => {
            handle_keypress(doc, target_node_id, event, mods);
        }
        EventData::Ime(ime_event) => {
            handle_ime_event(doc, ime_event);
        }
    }
}

pub struct EventListener {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RendererEvent {
    pub target: usize,
    pub data: EventData,
}

impl RendererEvent {
    /// Returns the name of the event ("click", "mouseover", "keypress", etc)
    pub fn name(&self) -> &'static str {
        self.data.name()
    }
}

#[derive(Debug, Clone)]
pub enum EventData {
    MouseDown { x: f32, y: f32, mods: Modifiers },
    MouseUp { x: f32, y: f32, mods: Modifiers },
    Click { x: f32, y: f32, mods: Modifiers },
    KeyPress { event: KeyEvent, mods: Modifiers },
    Ime(Ime),
    Hover,
}

impl EventData {
    pub fn name(&self) -> &'static str {
        match self {
            EventData::MouseDown { .. } => "mousedown",
            EventData::MouseUp { .. } => "mouseup",
            EventData::Click { .. } => "click",
            EventData::KeyPress { .. } => "keypress",
            EventData::Ime { .. } => "input",
            EventData::Hover => "mouseover",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HitResult {
    /// The node_id of the node identified as the hit target
    pub node_id: usize,
    /// The x coordinate of the hit within the hit target's border-box
    pub x: f32,
    /// The y coordinate of the hit within the hit target's border-box
    pub y: f32,
}
