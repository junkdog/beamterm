use std::fmt::Debug;

use compact_str::CompactString;

use crate::{CellSize, Deserializer, Glyph, Serializable, SerializationError};

/// Font atlas data for GPU-accelerated terminal rendering.
///
/// Contains a pre-rasterized font atlas stored as a 2D texture array, where each layer
/// holds 32 glyphs in a 1×32 grid. The atlas includes multiple font styles (normal, bold,
/// italic, bold+italic) and full Unicode support including emoji.
#[derive(Clone, PartialEq)]
pub struct FontAtlasData {
    /// The name of the font
    pub(crate) font_name: CompactString,
    /// The font size in points
    pub(crate) font_size: f32,
    /// The number of single-cell (halfwidth) glyphs per layer, before fullwidth glyphs begin.
    ///
    /// Fullwidth glyphs (e.g., CJK characters) are assigned IDs starting from this value,
    /// aligned to even boundaries. This allows the renderer to distinguish halfwidth from
    /// fullwidth glyphs by comparing against this threshold.
    pub(crate) max_halfwidth_base_glyph_id: u16,
    /// Width, height and depth of the texture in pixels
    pub(crate) texture_dimensions: (i32, i32, i32),
    /// Width and height of each character cell
    pub(crate) cell_size: CellSize,
    /// Underline configuration
    pub(crate) underline: LineDecoration,
    /// Strikethrough configuration
    pub(crate) strikethrough: LineDecoration,
    /// The glyphs in the font
    pub(crate) glyphs: Vec<Glyph>,
    /// The 3d texture data containing the font glyphs
    pub(crate) texture_data: Vec<u8>,
}

impl Debug for FontAtlasData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontAtlasData")
            .field("font_name", &self.font_name)
            .field("font_size", &self.font_size)
            .field("texture_dimensions", &self.texture_dimensions)
            .field("cell_size", &self.cell_size)
            .field("glyphs_count", &self.glyphs.len())
            .field("texture_data_kb", &(self.texture_data.len() / 1024))
            .finish()
    }
}

impl FontAtlasData {
    /// Padding in pixels around each glyph cell to prevent texture bleeding.
    pub const PADDING: i32 = 1;
    /// Number of glyph cells per texture layer.
    pub const CELLS_PER_SLICE: i32 = 32;

    /// Creates a new font atlas with the given parameters.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        font_name: CompactString,
        font_size: f32,
        max_halfwidth_base_glyph_id: u16,
        texture_dimensions: (i32, i32, i32),
        cell_size: CellSize,
        underline: LineDecoration,
        strikethrough: LineDecoration,
        glyphs: Vec<Glyph>,
        texture_data: Vec<u8>,
    ) -> Self {
        Self {
            font_name,
            font_size,
            max_halfwidth_base_glyph_id,
            texture_dimensions,
            cell_size,
            underline,
            strikethrough,
            glyphs,
            texture_data,
        }
    }

    /// Returns the font name.
    #[inline]
    #[must_use]
    pub fn font_name(&self) -> &str {
        &self.font_name
    }

    /// Returns the font size in points.
    #[inline]
    #[must_use]
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Returns the maximum halfwidth base glyph ID.
    ///
    /// Fullwidth glyphs are assigned IDs starting from this value.
    #[inline]
    #[must_use]
    pub fn max_halfwidth_base_glyph_id(&self) -> u16 {
        self.max_halfwidth_base_glyph_id
    }

    /// Returns the texture dimensions as (width, height, layers).
    #[inline]
    #[must_use]
    pub fn texture_dimensions(&self) -> (i32, i32, i32) {
        self.texture_dimensions
    }

    /// Returns the underline decoration configuration.
    #[inline]
    #[must_use]
    pub fn underline(&self) -> LineDecoration {
        self.underline
    }

    /// Returns the strikethrough decoration configuration.
    #[inline]
    #[must_use]
    pub fn strikethrough(&self) -> LineDecoration {
        self.strikethrough
    }

    /// Returns a slice of all glyphs in the atlas.
    #[inline]
    #[must_use]
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }

    /// Returns the raw texture data.
    #[inline]
    #[must_use]
    pub fn texture_data(&self) -> &[u8] {
        &self.texture_data
    }

    /// Consumes the atlas and returns its glyphs.
    #[must_use]
    pub fn into_glyphs(self) -> Vec<Glyph> {
        self.glyphs
    }

    /// Deserializes a font atlas from binary format.
    ///
    /// # Arguments
    /// * `serialized` - Binary data containing the serialized font atlas
    ///
    /// # Errors
    /// Returns [`SerializationError`] if the binary data is malformed or cannot be deserialized.
    ///
    /// # Returns
    /// The deserialized font atlas or an error if deserialization fails
    pub fn from_binary(serialized: &[u8]) -> Result<Self, SerializationError> {
        let mut deserializer = Deserializer::new(serialized);
        FontAtlasData::deserialize(&mut deserializer)
    }

    /// Serializes the font atlas to binary format.
    ///
    /// # Errors
    /// Returns [`SerializationError`] if serialization fails (e.g., a string field exceeds 255 bytes).
    ///
    /// # Returns
    /// A byte vector containing the serialized font atlas data, or an error
    /// if serialization fails (e.g., a string field exceeds 255 bytes)
    pub fn to_binary(&self) -> Result<Vec<u8>, SerializationError> {
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
    #[must_use]
    pub fn terminal_size(&self, viewport_width: i32, viewport_height: i32) -> (i32, i32) {
        (
            viewport_width / self.cell_size.width,
            viewport_height / self.cell_size.height,
        )
    }

    /// Returns the padded terminal cell size.
    ///
    /// The cell size includes padding (1 pixel on each side, 2 pixels total per dimension)
    /// to prevent texture bleeding artifacts during GPU rendering.
    ///
    /// # Returns
    /// The cell dimensions in pixels
    #[must_use]
    pub fn cell_size(&self) -> CellSize {
        self.cell_size
    }
}

impl Default for FontAtlasData {
    fn default() -> Self {
        Self::from_binary(include_bytes!("../atlas/bitmap_font.atlas")).unwrap()
    }
}

/// Configuration for underline or strikethrough line decorations.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LineDecoration {
    /// 0.0 to 1.0, where 0.0 is the top of the text line and 1.0 is the bottom.
    pub(crate) position: f32,
    /// Thickness of the line as a fraction of the cell height (0.0 to 1.0)
    pub(crate) thickness: f32,
}

impl LineDecoration {
    /// Creates a new line decoration with the given position and thickness.
    #[must_use]
    pub fn new(position: f32, thickness: f32) -> Self {
        Self {
            position: position.clamp(0.0, 1.0),
            thickness: thickness.clamp(0.0, 1.0),
        }
    }

    /// Returns the vertical position as a fraction of cell height (0.0 to 1.0).
    #[inline]
    #[must_use]
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Returns the thickness as a fraction of cell height (0.0 to 1.0).
    #[inline]
    #[must_use]
    pub fn thickness(&self) -> f32 {
        self.thickness
    }
}

/// Debug pattern for validating pixel-perfect rendering of cell dimensions.
///
/// When enabled, replaces the space glyph with a checkered pattern to help
/// verify that cell boundaries align correctly with pixel boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugSpacePattern {
    /// 1px alternating checkerboard pattern
    OnePixel,
    /// 2x2 pixel checkerboard pattern
    TwoByTwo,
}
