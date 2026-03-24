use beamterm_data::{FontStyle, LineDecoration};

use super::texture::RasterizedGlyph;
use crate::Error;

/// Trait abstracting platform-specific glyph rasterization for dynamic font atlases.
///
/// Implemented by native (swash+fontdb) and WASM (Canvas API) backends.
/// The [`DynamicFontAtlas`](super::DynamicFontAtlas) calls these methods to
/// rasterize glyphs on demand.
#[doc(hidden)]
pub trait GlyphRasterizer {
    /// Rasterizes a batch of glyphs, returning one [`RasterizedGlyph`] per input in order.
    fn rasterize_batch(
        &mut self,
        glyphs: &[(&str, FontStyle)],
    ) -> Result<Vec<RasterizedGlyph>, Error>;

    /// Maximum glyphs per [`rasterize_batch`](Self::rasterize_batch) call.
    fn max_batch_size(&self) -> usize;

    /// Cell dimensions in physical pixels (without padding).
    fn cell_size(&self) -> beamterm_data::CellSize;

    /// Whether a grapheme should be treated as double-width based on font metrics.
    ///
    /// Returns `true` if the font's advance width for this grapheme exceeds 1.5x cell
    /// width. Backends without font metric access (e.g. Canvas API) should return `false`.
    fn is_double_width(&mut self, grapheme: &str) -> bool;

    fn underline(&self) -> LineDecoration;
    fn strikethrough(&self) -> LineDecoration;

    /// Reinitialize at a new effective font size (called on DPR change).
    fn update_font_size(&mut self, font_size: f32) -> Result<(), Error>;
}
