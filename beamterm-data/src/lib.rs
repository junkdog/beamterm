mod atlas;
mod glyph;
mod serialization;

pub use atlas::FontAtlasData;
pub use glyph::{FontStyle, Glyph, GlyphEffect};
use serialization::*;

#[derive(Debug)]
pub struct FontAtlasDeserializationError {
    pub message: String,
}
