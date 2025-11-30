use std::fmt::Debug;

use compact_str::CompactString;

use crate::{Deserializer, FontAtlasDeserializationError, Glyph, Serializable};

/// Font atlas data for GPU-accelerated terminal rendering.
///
/// Contains a pre-rasterized font atlas stored as a 2D texture array, where each layer
/// holds 32 glyphs in a 32×1 grid. The atlas includes multiple font styles (normal, bold,
/// italic, bold+italic) and full Unicode support including emoji.
#[derive(PartialEq)]
pub struct FontAtlasData {
    /// The name of the font
    pub font_name: CompactString,
    /// The font size in points
    pub font_size: f32,
    /// The number of single-cell (halfwidth) glyphs per layer, before fullwidth glyphs begin.
    ///
    /// Fullwidth glyphs (e.g., CJK characters) are assigned IDs starting from this value,
    /// aligned to even boundaries. This allows the renderer to distinguish halfwidth from
    /// fullwidth glyphs by comparing against this threshold.
    pub max_halfwidth_base_glyph_id: u16,
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
    pub const CELLS_PER_SLICE: i32 = 32;

    /// Deserializes a font atlas from binary format.
    ///
    /// # Arguments
    /// * `serialized` - Binary data containing the serialized font atlas
    ///
    /// # Returns
    /// The deserialized font atlas or an error if deserialization fails
    pub fn from_binary(serialized: &[u8]) -> Result<Self, FontAtlasDeserializationError> {
        let mut deserializer = Deserializer::new(serialized);
        FontAtlasData::deserialize(&mut deserializer).map_err(|e| FontAtlasDeserializationError {
            message: format!("Failed to deserialize font atlas: {}", e.message),
        })
    }

    /// Serializes the font atlas to binary format.
    ///
    /// # Returns
    /// A byte vector containing the serialized font atlas data
    pub fn to_binary(&self) -> Vec<u8> {
        self.serialize()
    }

    /// Calculates how many terminal columns and rows fit in the given viewport dimensions.
    ///
    /// # Arguments
    /// * `viewport_width` - Width of the viewport in pixels
    /// * `viewport_height` - Height of the viewport in pixels
    ///
    /// # Returns
    /// A tuple of (columns, rows) that fit in the viewport
    pub fn terminal_size(&self, viewport_width: i32, viewport_height: i32) -> (i32, i32) {
        (
            viewport_width / self.cell_size.0,
            viewport_height / self.cell_size.1,
        )
    }

    /// Returns the padded terminal cell size.
    ///
    /// The cell size includes padding (1 pixel on each side, 2 pixels total per dimension)
    /// to prevent texture bleeding artifacts during GPU rendering.
    ///
    /// # Returns
    /// A tuple of (width, height) in pixels for each terminal cell
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
