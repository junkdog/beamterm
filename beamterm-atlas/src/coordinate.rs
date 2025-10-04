use beamterm_data::FontAtlasData;

use crate::{glyph_bounds::GlyphBounds, raster_config::RasterizationConfig};

#[derive(Debug, Clone, Copy)]
pub(super) struct AtlasCoordinate {
    pub(super) layer: u16,      // Depth in the 2D Texture Array
    pub(super) glyph_index: u8, // 0..=31; each layer contains 32 glyphs
}

impl AtlasCoordinate {
    pub(super) fn from_glyph_id(id: u16) -> Self {
        // 32 glyphs per layer, indexed from 0 to 31
        Self { layer: id >> 5, glyph_index: (id & 0x1F) as u8 }
    }

    pub(super) fn xy(&self, config: GlyphBounds) -> (i32, i32) {
        let x = self.cell_offset_in_px(config).0 + FontAtlasData::PADDING;
        let y = FontAtlasData::PADDING;

        (x, y)
    }

    pub(super) fn cell_offset_in_px(&self, config: GlyphBounds) -> (i32, i32) {
        let cell_width = config.width_with_padding();
        (self.glyph_index as i32 * cell_width, 0)
    }
}
