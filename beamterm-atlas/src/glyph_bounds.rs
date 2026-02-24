use beamterm_data::FontAtlasData;
use cosmic_text::{BorrowedWithFontSystem, Buffer, Color, SwashCache};

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

    pub fn shrink_width(mut self, amount: i32) -> Self {
        self.max_x -= amount;
        self
    }

    pub fn shrink_height(mut self, amount: i32) -> Self {
        self.max_y -= amount;
        self
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            max_x: self.max_x.max(other.max_x),

            min_y: self.min_y.min(other.min_y),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && x <= self.max_x && y >= 0 && y <= self.max_y
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

    pub fn has_content(&self) -> bool {
        self.width() > 0 && self.height() > 0
    }
}

/// Measures precise glyph bounds with pixel-level accuracy using cosmic-text drawing
pub(crate) fn measure_glyph_bounds(
    buffer: &mut BorrowedWithFontSystem<Buffer>,
    cache: &mut SwashCache,
) -> GlyphBounds {
    let mut bounds = GlyphBounds::empty();

    // Draw the buffer and collect pixel bounds
    buffer.draw(cache, Color::rgb(255, 255, 255), |x, y, _w, _h, color| {
        if color.a() > 0 {
            // todo: handle negative coordinates properly when cell overdraw is implemented

            bounds.max_x = bounds.max_x.max(x);
            // bounds.min_x = bounds.min_x.min(x);
            bounds.min_x = 0;

            bounds.max_y = bounds.max_y.max(y);
            // bounds.min_y = bounds.min_y.min(y);
            bounds.min_y = 0;
        }
    });

    bounds
}
