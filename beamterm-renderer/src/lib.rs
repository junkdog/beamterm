mod error;
mod gl;
mod terminal;

pub(crate) mod js;

#[cfg(feature = "js-api")]
pub mod wasm;

pub mod mouse;

// Re-export platform-agnostic types from beamterm-core
pub use ::beamterm_data::{DebugSpacePattern, GlyphEffect};
pub use beamterm_core::{
    CursorPosition, FontAtlasData, FontStyle, GlslVersion, UrlMatch, find_url_at_cursor,
    is_double_width, is_emoji,
};
pub use terminal::*;

pub use crate::{error::Error, gl::*};

#[cfg(test)]
mod tests {
    use beamterm_data::FontAtlasData;

    #[test]
    fn test_font_atlas_config_deserialization() {
        let _ = FontAtlasData::default();
    }
}
