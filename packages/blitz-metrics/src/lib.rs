//! Shared metric types for Blitz instrumentation.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Default, Debug, Clone)]
pub struct FrameTimings {
    pub html_parse_ms: f32,
    pub style_ms: f32,
    pub layout_ms: f32,
    pub text_shaping_ms: f32,
    pub scene_build_ms: f32,
    pub device_init_ms: f32,
    pub backbuffer_ms: f32,
    pub playback_ms: f32,
    pub frame_total_ms: f32,
}

impl FrameTimings {
    pub fn slowest_phase(&self) -> (&'static str, f32) {
        let mut pairs = [
            ("parse", self.html_parse_ms),
            ("style", self.style_ms),
            ("layout", self.layout_ms),
            ("shape", self.text_shaping_ms),
            ("scene", self.scene_build_ms),
            ("dev", self.device_init_ms),
            ("back", self.backbuffer_ms),
            ("play", self.playback_ms),
        ];
        pairs.sort_by(|a,b| b.1.total_cmp(&a.1));
        pairs[0]
    }
    pub fn add(&mut self, phase: &str, dur: Duration) {
        let ms = dur.as_secs_f32()*1000.0;
        match phase {
            // Accumulate all work for each phase during the init window.
            "parse" => self.html_parse_ms += ms,
            "style" => self.style_ms += ms,
            "layout" => self.layout_ms += ms,
            "shape" => self.text_shaping_ms += ms,
            "scene" => self.scene_build_ms += ms,
            _ => {}
        }
    }
}

static PHASE_TIMINGS: once_cell::sync::Lazy<Mutex<FrameTimings>> = once_cell::sync::Lazy::new(|| Mutex::new(FrameTimings::default()));
static FROZEN: AtomicBool = AtomicBool::new(false);

pub struct PhaseGuard { name: &'static str, start: Option<Instant> }
impl PhaseGuard {
    pub fn end(mut self) {
        if let Some(st) = self.start.take() {
            let dur = st.elapsed();
            if !FROZEN.load(Ordering::SeqCst) {
                let mut g = PHASE_TIMINGS.lock().unwrap();
                g.add(self.name, dur);
            }
        }
    }
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        if let Some(st) = self.start.take() {
            if !FROZEN.load(Ordering::SeqCst) {
                let dur = st.elapsed();
                let mut g = PHASE_TIMINGS.lock().unwrap();
                g.add(self.name, dur);
            }
        }
    }
}

pub fn start_phase(name: &'static str) -> PhaseGuard { PhaseGuard { name, start: Some(Instant::now()) } }
pub fn snapshot() -> FrameTimings { PHASE_TIMINGS.lock().unwrap().clone() }
pub fn reset_frame() { *PHASE_TIMINGS.lock().unwrap() = FrameTimings::default(); }
pub fn freeze() { FROZEN.store(true, Ordering::SeqCst); }
pub fn is_frozen() -> bool { FROZEN.load(Ordering::SeqCst) }
pub fn reset_for_testing() { FROZEN.store(false, Ordering::SeqCst); *PHASE_TIMINGS.lock().unwrap() = FrameTimings::default(); }
pub fn unfreeze_and_reset() { FROZEN.store(false, Ordering::SeqCst); *PHASE_TIMINGS.lock().unwrap() = FrameTimings::default(); }
pub fn begin_init_window(_start: Instant) { /* no-op with always-active gating */ }
pub fn end_init_window() { /* no-op; freeze() stops recording */ }
pub fn init_active() -> bool { !FROZEN.load(Ordering::SeqCst) }
