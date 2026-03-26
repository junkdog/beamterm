use beamterm_data::FontAtlasData;
use beamterm_rasterizer::RasterizedGlyph;

/// Glyph bounds information
#[derive(Debug, Clone, Copy)]
pub struct GlyphBounds {
    pub(crate) max_x: i32,
    pub(crate) max_y: i32,
    pub(crate) min_x: i32,
    pub(crate) min_y: i32,
}

impl GlyphBounds {
    /// Creates empty bounds for initialization
    pub(crate) fn empty() -> Self {
        Self { max_x: 0, max_y: 0, min_x: 0, min_y: 0 }
    }

    pub fn width(&self) -> i32 {
        1 + self.max_x - self.min_x
    }

    pub fn width_with_padding(&self) -> i32 {
        self.width() + 2 * FontAtlasData::PADDING
    }

    pub fn height_with_padding(&self) -> i32 {
        self.height() + 2 * FontAtlasData::PADDING
    }

    pub fn height(&self) -> i32 {
        1 + self.max_y - self.min_y
    }
}

/// Measures precise glyph bounds from a rasterized RGBA pixel buffer.
///
/// The glyph is expected to include padding; this function measures
/// the content area within the padding (offsets by `PADDING`).
pub(crate) fn measure_glyph_bounds(glyph: &RasterizedGlyph) -> GlyphBounds {
    let mut bounds = GlyphBounds::empty();
    let w = glyph.width as i32;
    let padding = FontAtlasData::PADDING;

    for (i, px) in glyph.pixels.chunks(4).enumerate() {
        if px[3] > 0 {
            let raw_x = (i as i32) % w;
            let raw_y = (i as i32) / w;

            // Convert from padded coordinates to content coordinates
            let x = raw_x - padding;
            let y = raw_y - padding;

            if x >= 0 && y >= 0 {
                bounds.max_x = bounds.max_x.max(x);
                bounds.min_x = 0;
                bounds.max_y = bounds.max_y.max(y);
                bounds.min_y = 0;
            }
        }
    }

    bounds
}
