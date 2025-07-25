use std::fmt::Debug;

use compact_str::CompactString;

use crate::{Deserializer, FontAtlasDeserializationError, Glyph, Serializable};

/// Font atlas data for GPU-accelerated terminal rendering.
///
/// Contains a pre-rasterized font atlas stored as a 2D texture array, where each layer
/// holds 16 glyphs in a 16×1 grid. The atlas includes multiple font styles (normal, bold,
/// italic, bold+italic) and full Unicode support including emoji.
#[derive(PartialEq)]
pub struct FontAtlasData {
    /// The name of the font
    pub font_name: CompactString,
    /// The font size in points
    pub font_size: f32,
    /// Width, height and depth of the texture in pixels
    pub texture_dimensions: (i32, i32, i32),
    /// Width and height of each character cell
    pub cell_size: (i32, i32),
    /// Underline configuration
    pub underline: LineDecoration,
    /// Strikethrough configuration
    pub strikethrough: LineDecoration,
    /// The glyphs in the font
    pub glyphs: Vec<Glyph>,
    /// The 3d texture data containing the font glyphs
    pub texture_data: Vec<u8>,
}

impl Debug for FontAtlasData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontAtlasData")
            .field("font_name", &self.font_name)
            .field("font_size", &self.font_size)
            .field("texture_dimensions", &self.texture_dimensions)
            .field("cell_size", &self.cell_size)
            .field("glyphs_count", &self.glyphs.len())
            .field("texture_data_kb", &(self.texture_data.len() * 4 / 1024))
            .finish()
    }
}

impl FontAtlasData {
    pub const PADDING: i32 = 1;
    pub const CELLS_PER_SLICE: i32 = 16;

    pub fn from_binary(serialized: &[u8]) -> Result<Self, FontAtlasDeserializationError> {
        let mut deserializer = Deserializer::new(serialized);
        FontAtlasData::deserialize(&mut deserializer).map_err(|e| FontAtlasDeserializationError {
            message: format!("Failed to deserialize font atlas: {}", e.message),
        })
    }

    pub fn to_binary(&self) -> Vec<u8> {
        self.serialize()
    }

    pub fn terminal_size(&self, viewport_width: i32, viewport_height: i32) -> (i32, i32) {
        (
            viewport_width / self.cell_size.0,
            viewport_height / self.cell_size.1,
        )
    }

    pub fn cell_size(&self) -> (i32, i32) {
        self.cell_size
    }
}

impl Default for FontAtlasData {
    fn default() -> Self {
        Self::from_binary(include_bytes!("../atlas/bitmap_font.atlas")).unwrap()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LineDecoration {
    /// 0.0 to 1.0, where 0.0 is the top of the text line and 1.0 is the bottom.
    pub position: f32,
    /// Thickness of the line as a fraction of the cell height (0.0 to 1.0)
    pub thickness: f32,
}

impl LineDecoration {
    pub fn new(position: f32, thickness: f32) -> Self {
        Self {
            position: position.clamp(0.0, 1.0),
            thickness: thickness.clamp(0.0, 1.0),
        }
    }
}
