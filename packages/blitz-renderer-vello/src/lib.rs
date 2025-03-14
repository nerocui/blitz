//! A vello renderer for blitz-dom
//!
//! ## Feature flags
//!  - `default`: Enables the features listed below.
//!  - `tracing`: Enables tracing support.
mod renderer;
mod util;
pub mod d2drenderer;
pub use renderer::*;
pub use util::Color;
