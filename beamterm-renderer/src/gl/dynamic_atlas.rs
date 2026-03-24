use beamterm_core::gl::{GlyphRasterizer, RasterizedGlyph};
use beamterm_data::{CellSize, FontAtlasData, FontStyle, LineDecoration};

use super::canvas_rasterizer::CanvasRasterizer;
use crate::error::Error;

/// Canvas-based glyph rasterizer for WASM/browser environments.
///
/// Wraps [`CanvasRasterizer`] to implement [`GlyphRasterizer`] for use with
/// [`DynamicFontAtlas`](beamterm_core::gl::DynamicFontAtlas).
pub(crate) struct CanvasGlyphRasterizer {
    inner: CanvasRasterizer,
    cell_size: CellSize,
}

impl CanvasGlyphRasterizer {
    pub(crate) fn new(font_family: &str, font_size: f32) -> Result<Self, Error> {
        let inner = CanvasRasterizer::new(font_family, font_size)?;
        let cell_size = Self::measure_cell_size(&inner)?;
        Ok(Self { inner, cell_size })
    }

    fn measure_cell_size(rasterizer: &CanvasRasterizer) -> Result<CellSize, Error> {
        let reference_glyphs = rasterizer.rasterize(&[("\u{2588}", FontStyle::Normal)])?;

        if let Some(g) = reference_glyphs.first() {
            Ok(CellSize::new(
                g.width as i32 - FontAtlasData::PADDING * 2,
                g.height as i32 - FontAtlasData::PADDING * 2,
            ))
        } else {
            Err(Error::rasterizer_empty_reference_glyph())
        }
    }
}

impl GlyphRasterizer for CanvasGlyphRasterizer {
    fn rasterize_batch(
        &mut self,
        glyphs: &[(&str, FontStyle)],
    ) -> Result<Vec<RasterizedGlyph>, beamterm_core::Error> {
        self.inner
            .rasterize(glyphs)
            .map_err(|e| beamterm_core::Error::Resource(e.to_string()))
    }

    fn max_batch_size(&self) -> usize {
        self.inner.max_batch_size()
    }

    fn cell_size(&self) -> CellSize {
        self.cell_size
    }

    fn is_double_width(&mut self, _grapheme: &str) -> bool {
        false // Canvas API doesn't expose font advance metrics
    }

    fn underline(&self) -> LineDecoration {
        LineDecoration::new(0.9, 0.05) // near bottom, thin
    }

    fn strikethrough(&self) -> LineDecoration {
        LineDecoration::new(0.5, 0.05) // middle, thin
    }

    fn update_font_size(&mut self, font_size: f32) -> Result<(), beamterm_core::Error> {
        self.inner = CanvasRasterizer::new(self.inner.font_family(), font_size)
            .map_err(|e| beamterm_core::Error::Resource(e.to_string()))?;
        self.cell_size = Self::measure_cell_size(&self.inner)
            .map_err(|e| beamterm_core::Error::Resource(e.to_string()))?;
        Ok(())
    }
}

/// Type alias for the WASM dynamic font atlas.
pub(crate) type DynamicFontAtlas = beamterm_core::gl::DynamicFontAtlas<CanvasGlyphRasterizer>;
