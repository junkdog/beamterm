use beamterm_data::{FontAtlasData, Glyph};

use crate::glyph_bounds::GlyphBounds;

#[derive(Debug, Clone, Copy)]
pub(super) struct AtlasCoordinate {
    pub(super) layer: u16,      // Depth in the 2D Texture Array
    pub(super) glyph_index: u8, // 0..=31; each layer contains 32 glyphs
}

impl AtlasCoordinate {
    pub(super) fn to_pixel_xy(self, config: GlyphBounds) -> (i32, i32) {
        let x = FontAtlasData::PADDING;
        let y = self.cell_offset_in_px(config).1 + FontAtlasData::PADDING;

        (x, y)
    }

    pub(super) fn cell_offset_in_px(&self, config: GlyphBounds) -> (i32, i32) {
        let cell_height = config.height_with_padding();
        (0, self.glyph_index as i32 * cell_height)
    }
}

impl From<u16> for AtlasCoordinate {
    fn from(id: u16) -> Self {
        // 32 glyphs per layer, indexed from 0 to 31
        Self { layer: id >> 5, glyph_index: (id & 0x1F) as u8 }
    }
}

pub(super) trait AtlasCoordinateProvider {
    fn atlas_coordinate(&self) -> AtlasCoordinate;
}

impl AtlasCoordinateProvider for Glyph {
    fn atlas_coordinate(&self) -> AtlasCoordinate {
        self.id.into()
    }
}
