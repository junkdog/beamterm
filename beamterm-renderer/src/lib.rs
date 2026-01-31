mod error;
mod gl;
mod mat4;
mod position;
mod terminal;

pub(crate) mod js;

#[cfg(feature = "js-api")]
pub mod wasm;

pub mod mouse;
mod url;

pub use ::beamterm_data::{DebugSpacePattern, FontAtlasData, GlyphEffect};
pub use beamterm_data::FontStyle;
pub use position::CursorPosition;
pub use terminal::*;
pub use url::UrlMatch;

pub use crate::{error::Error, gl::*};

#[cfg(test)]
mod tests {
    use beamterm_data::FontAtlasData;

    #[test]
    fn test_font_atlas_config_deserialization() {
        let _ = FontAtlasData::default();
    }
}
