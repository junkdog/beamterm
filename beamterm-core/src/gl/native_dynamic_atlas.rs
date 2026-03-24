use beamterm_data::{FontStyle, LineDecoration};
use beamterm_rasterizer::NativeRasterizer;

use super::{
    dynamic_atlas::DynamicFontAtlas, glyph_rasterizer::GlyphRasterizer, texture::RasterizedGlyph,
};
use crate::Error;

/// Native glyph rasterizer using swash + fontdb.
///
/// Wraps [`NativeRasterizer`] to implement [`GlyphRasterizer`] for use with
/// [`DynamicFontAtlas`].
pub struct NativeGlyphRasterizer {
    inner: NativeRasterizer,
}

impl NativeGlyphRasterizer {
    /// Creates a new native glyph rasterizer.
    ///
    /// # Arguments
    /// * `font_families` - font family names in priority order
    /// * `font_size` - effective font size in physical pixels (base_size * pixel_ratio)
    pub fn new(font_families: &[&str], font_size: f32) -> Result<Self, Error> {
        let inner = NativeRasterizer::new(font_families, font_size)
            .map_err(|e| Error::Resource(e.to_string()))?;
        Ok(Self { inner })
    }
}

impl GlyphRasterizer for NativeGlyphRasterizer {
    fn rasterize_batch(
        &mut self,
        glyphs: &[(&str, FontStyle)],
    ) -> Result<Vec<RasterizedGlyph>, Error> {
        let mut results = Vec::with_capacity(glyphs.len());
        for &(grapheme, style) in glyphs {
            let rasterized = self
                .inner
                .rasterize(grapheme, style)
                .map_err(|e| Error::Resource(e.to_string()))?;
            results.push(RasterizedGlyph::new(
                rasterized.pixels,
                rasterized.width,
                rasterized.height,
            ));
        }
        Ok(results)
    }

    fn max_batch_size(&self) -> usize {
        usize::MAX
    }

    fn cell_size(&self) -> beamterm_data::CellSize {
        self.inner.cell_size()
    }

    fn is_double_width(&mut self, grapheme: &str) -> bool {
        self.inner.is_double_width(grapheme)
    }

    fn underline(&self) -> LineDecoration {
        self.inner.underline()
    }

    fn strikethrough(&self) -> LineDecoration {
        self.inner.strikethrough()
    }

    fn update_font_size(&mut self, font_size: f32) -> Result<(), Error> {
        self.inner
            .update_font_size(font_size)
            .map_err(|e| Error::Resource(e.to_string()))
    }
}

/// Type alias for the native dynamic font atlas.
pub type NativeDynamicAtlas = DynamicFontAtlas<NativeGlyphRasterizer>;
