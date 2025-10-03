use beamterm_data::{FontAtlasData, Glyph};

use crate::glyph_bounds::GlyphBounds;

#[derive(Debug)]
pub(super) struct RasterizationConfig {
    pub(super) texture_width: i32,
    pub(super) texture_height: i32,
    pub(super) layers: i32,
    bounds: GlyphBounds,
}

impl RasterizationConfig {
    const GLYPHS_PER_SLICE: i32 = 32; // 32x1 grid
    const GRID_WIDTH: i32 = 32;
    const GRID_HEIGHT: i32 = 1;

    pub(super) fn new(bounds: GlyphBounds, glyphs: &[Glyph]) -> Self {
        let (inner_cell_w, inner_cell_h) = (bounds.width(), bounds.height());

        let slice_width = Self::GRID_WIDTH * (inner_cell_w + 2 * FontAtlasData::PADDING);
        let slice_height = Self::GRID_HEIGHT * (inner_cell_h + 2 * FontAtlasData::PADDING);

        let max_id = glyphs.iter().map(|g| g.id).max().unwrap_or(0) as i32;
        let layers = (max_id + Self::GLYPHS_PER_SLICE - 1) / Self::GLYPHS_PER_SLICE;

        Self {
            texture_width: slice_width,
            texture_height: slice_height,
            layers,
            bounds,
        }
    }

    pub(super) fn glyph_bounds(&self) -> GlyphBounds {
        self.bounds
    }

    pub(super) fn texture_size(&self) -> usize {
        (self.texture_width * self.texture_height * self.layers) as usize
    }

    pub(super) fn padded_cell_size(&self) -> (i32, i32) {
        (
            self.bounds.width_with_padding(),
            self.bounds.height_with_padding(),
        )
    }
}
