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
    const GLYPHS_PER_SLICE: i32 = 32; // 1x32 grid
    const GRID_WIDTH: i32 = 1;
    const GRID_HEIGHT: i32 = 32;

    pub(super) fn new(bounds: GlyphBounds, glyphs: &[Glyph]) -> Self {
        let (inner_cell_w, inner_cell_h) = (bounds.width(), bounds.height());

        let slice_width = Self::GRID_WIDTH * (inner_cell_w + 2 * FontAtlasData::PADDING);
        let slice_height = Self::GRID_HEIGHT * (inner_cell_h + 2 * FontAtlasData::PADDING);

        let max_id = glyphs.iter().map(|g| g.id).max().unwrap_or(0) as i32;
        let layers = max_id / Self::GLYPHS_PER_SLICE + 1;

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

    pub(super) fn double_width_glyph_bounds(&self) -> GlyphBounds {
        GlyphBounds {
            max_x: self.bounds.max_x + self.bounds.width(),
            ..self.bounds
        }
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

#[cfg(test)]
mod tests {
    use beamterm_data::FontStyle;

    use super::*;

    fn test_bounds() -> GlyphBounds {
        GlyphBounds { max_x: 9, max_y: 19, min_x: 0, min_y: 0 }
    }

    fn glyph(id: u16) -> Glyph {
        Glyph::new_with_id(id, "x", FontStyle::Normal, (0, 0))
    }

    fn layers_for(glyphs: &[Glyph]) -> i32 {
        RasterizationConfig::new(test_bounds(), glyphs).layers
    }

    #[test]
    fn layer_count_covers_max_glyph_id() {
        // layer = id >> 5, so layers needed = max_id / 32 + 1
        assert_eq!(layers_for(&[glyph(0)]), 1); // layer 0
        assert_eq!(layers_for(&[glyph(31)]), 1); // last slot in layer 0
        assert_eq!(layers_for(&[glyph(32)]), 2); // first slot in layer 1
        assert_eq!(layers_for(&[glyph(33)]), 2);
        assert_eq!(layers_for(&[glyph(64)]), 3); // first slot in layer 2

        // uses max glyph id across all glyphs
        assert_eq!(layers_for(&[glyph(5), glyph(100), glyph(50)]), 4);
    }
}
