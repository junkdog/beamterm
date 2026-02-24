mod atlas;
mod glyph;
pub(crate) mod serialization;

pub use atlas::{DebugSpacePattern, FontAtlasData, LineDecoration};
pub use glyph::{FontStyle, Glyph, GlyphEffect};
pub use serialization::SerializationError;
use serialization::*;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct FontAtlasDeserializationError {
    pub message: String,
}
