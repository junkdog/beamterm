//! # Stability Policy
//!
//! beamterm-renderer's public API includes types from the following third-party crates:
//!
//! | Crate            | Types exposed                                                                | Re-exported as                                    |
//! |------------------|------------------------------------------------------------------------------|---------------------------------------------------|
//! | [`glow`]         | [`glow::Context`] via [`Terminal::gl()`]                                     | [`beamterm_renderer::glow`](glow)                 |
//! | [`compact_str`]  | [`CompactString`](compact_str::CompactString) in return types                | [`beamterm_renderer::compact_str`](compact_str)   |
//! | [`web_sys`]      | [`HtmlCanvasElement`](web_sys::HtmlCanvasElement) via [`Terminal::canvas()`] | [`beamterm_renderer::web_sys`](web_sys)           |
//! | [`js_sys`]       | [`Array`](js_sys::Array), [`Function`](js_sys::Function) in WASM bindings    | [`beamterm_renderer::js_sys`](js_sys)             |
//! | [`wasm_bindgen`] | [`JsValue`](wasm_bindgen::JsValue) in WASM bindings                          | [`beamterm_renderer::wasm_bindgen`](wasm_bindgen) |
//!
//! These crates are re-exported so that downstream users can depend on
//! beamterm-renderer's re-exports without adding separate dependencies
//! or worrying about version mismatches.
//!
//! **Semver policy**: A dependency version bump is only considered a
//! beamterm breaking change if the type signatures used in beamterm's
//! public API actually change. A version bump that preserves the same
//! type signatures is a compatible update.

mod error;
mod gl;
mod terminal;

pub(crate) mod js;

/// JavaScript/WASM bindings for the terminal renderer.
#[cfg(feature = "js-api")]
#[allow(
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value
)]
pub mod wasm;

pub mod mouse;

// Re-export third-party crates that appear in beamterm's public API.
// Downstream users can depend on these re-exports instead of adding
// separate dependencies or worrying about version mismatches.
// Re-export platform-agnostic types from beamterm-core
pub use ::beamterm_data::{DebugSpacePattern, GlyphEffect};
pub use beamterm_core::{
    CellSize, CursorPosition, FontAtlasData, FontStyle, GlslVersion, SerializationError,
    TerminalSize, UrlMatch, compact_str, find_url_at_cursor, glow, is_double_width, is_emoji,
};
pub use js_sys;
pub use terminal::*;
pub use wasm_bindgen;
pub use web_sys;

pub use crate::{error::Error, gl::*};

#[cfg(test)]
mod tests {
    use beamterm_data::FontAtlasData;

    #[test]
    fn test_font_atlas_config_deserialization() {
        let _ = FontAtlasData::default();
    }
}
