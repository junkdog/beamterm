mod error;
mod gl;
mod mat4;
mod terminal;

pub(crate) mod js;

#[cfg(feature = "js-api")]
pub mod wasm;

pub mod mouse;

pub use ::beamterm_data::{DebugSpacePattern, FontAtlasData, GlyphEffect};
pub use beamterm_data::FontStyle;
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
