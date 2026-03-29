//! # Stability Policy
//!
//! beamterm-core's public API includes types from the following third-party crates:
//!
//! | Crate           | Types exposed                                                                   | Re-exported as                              |
//! |-----------------|---------------------------------------------------------------------------------|---------------------------------------------|
//! | [`glow`]        | [`glow::Context`] in method parameters and [`RenderContext::gl`]                | [`beamterm_core::glow`](glow)               |
//! | [`compact_str`] | [`CompactString`](compact_str::CompactString) in return types and struct fields | [`beamterm_core::compact_str`](compact_str) |
//!
//! These crates are re-exported so that downstream users can depend on
//! `beamterm_core::glow` and `beamterm_core::compact_str` without adding
//! separate dependencies or worrying about version mismatches.
//!
//! **Semver policy**: A dependency version bump (e.g. glow 0.17 to 0.18) is
//! only considered a beamterm breaking change if the type signatures used in
//! beamterm's public API actually change. A version bump that preserves the
//! same type signatures is a compatible update.

pub(crate) mod error;
pub mod gl;
mod mat4;
mod position;
mod url;

// Re-export third-party crates that appear in beamterm-core's public API.
// This allows downstream users to use `beamterm_core::glow` and
// `beamterm_core::compact_str` without adding separate dependencies
// or worrying about version mismatches.
pub use ::beamterm_data::{
    CellSize, DebugSpacePattern, FontAtlasData, GlyphEffect, SerializationError, TerminalSize,
};
pub use beamterm_data::FontStyle;
pub use beamterm_unicode::{is_double_width, is_emoji};
pub use compact_str;
pub use error::Error;
pub use gl::{
    Atlas, CellData, CellDynamic, CellIterator, CellQuery, Drawable, FontAtlas, GlState, GlyphSlot,
    GlyphTracker, RenderContext, SelectionMode, SelectionTracker, StaticFontAtlas, TerminalGrid,
    select,
};
#[cfg(feature = "native-dynamic-atlas")]
pub use gl::{NativeDynamicAtlas, NativeGlyphRasterizer};
pub use glow;
pub use position::CursorPosition;
pub use url::{UrlMatch, find_url_at_cursor};

/// GL shader language target for version injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlslVersion {
    /// WebGL2 / OpenGL ES 3.0: `#version 300 es`
    Es300,
    /// OpenGL 3.3 Core: `#version 330 core`
    Gl330,
}

impl GlslVersion {
    #[must_use]
    pub fn vertex_preamble(&self) -> &'static str {
        match self {
            Self::Es300 => "#version 300 es\nprecision highp float;\n",
            Self::Gl330 => "#version 330 core\n",
        }
    }

    #[must_use]
    pub fn fragment_preamble(&self) -> &'static str {
        match self {
            Self::Es300 => "#version 300 es\nprecision mediump float;\nprecision highp int;\n",
            Self::Gl330 => "#version 330 core\n",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_atlas_config_deserialization() {
        let _ = FontAtlasData::default();
    }
}
