//! Canvas-based glyph rasterizer for dynamic font atlas generation.
//!
//! Uses the browser's native text rendering via OffscreenCanvas to rasterize
//! glyphs on demand. This approach handles:
//! - Color emoji (COLR/CBDT/SVG fonts)
//! - Complex emoji sequences (ZWJ, skin tones)
//! - CJK and other fullwidth characters
//! - Ligatures (when supported by the font)
//! - Font fallback chains (handled by browser)
//! - Per-glyph font styles (normal, bold, italic, bold-italic)
//!
//! # Example
//!
//! ```ignore
//! use beamterm_data::FontStyle;
//!
//! let rasterizer = CanvasRasterizer::new()?;
//!
//! // Batch rasterize glyphs with per-glyph styles
//! let glyphs = rasterizer
//!     .glyphs()
//!     .font_family("'JetBrains Mono', monospace")
//!     .font_size(16.0)
//!     .rasterize(&[
//!         ("A", FontStyle::Normal),
//!         ("B", FontStyle::Bold),
//!         ("C", FontStyle::Italic),
//!         ("🚀", FontStyle::Normal),  // emoji always uses Normal
//!     ])?;
//!
//! // Double-width glyphs (emoji, CJK) have width = cell_width * 2
//! for glyph in &glyphs {
//!     println!("{}x{}", glyph.width, glyph.height);
//! }
//! ```

use beamterm_data::{FontAtlasData, FontStyle};
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvas, OffscreenCanvasRenderingContext2d};

use crate::error::Error;

// padding around glyphs matches StaticFontAtlas to unify texture packing.
const PADDING: u32 = FontAtlasData::PADDING as u32;

const OFFSCREEN_CANVAS_WIDTH: u32 = 256;
const OFFSCREEN_CANVAS_HEIGHT: u32 = 1024;

/// Cell metrics for positioning glyphs correctly.
#[derive(Debug, Clone, Copy)]
pub(super) struct CellMetrics {
    padded_width: u32,
    padded_height: u32,
    /// How far above the baseline the glyph extends (for positioning with baseline "top")
    ascent: f64,
}

/// Pixel data from a rasterized glyph.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// RGBA pixel data (4 bytes per pixel, row-major order)
    pub pixels: Vec<u8>,
    /// Width of the rasterized glyph in pixels
    pub width: u32,
    /// Height of the rasterized glyph in pixels
    pub height: u32,
}

impl RasterizedGlyph {
    /// Returns true if the glyph produced no visible pixels.
    pub fn is_empty(&self) -> bool {
        self.pixels
            .iter()
            .skip(3)
            .step_by(4)
            .all(|&a| a == 0)
    }

    pub fn new(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self { pixels, width, height }
    }
}

/// Canvas-based glyph rasterizer using OffscreenCanvas.
///
/// This rasterizer leverages the browser's native text rendering capabilities
/// to handle complex Unicode rendering including emoji and fullwidth characters.
///
/// Create once and reuse for multiple glyphs via the builder pattern.
pub struct CanvasRasterizer {
    canvas: OffscreenCanvas,
    render_ctx: OffscreenCanvasRenderingContext2d,
}

impl CanvasRasterizer {
    /// Creates a new canvas rasterizer with the specified cell dimensions.
    ///
    /// # Returns
    ///
    /// A configured rasterizer context, or an error if canvas creation fails.
    pub fn new() -> Result<Self, Error> {
        let canvas = OffscreenCanvas::new(OFFSCREEN_CANVAS_WIDTH, OFFSCREEN_CANVAS_HEIGHT)
            .map_err(|e| Error::rasterizer_canvas_creation_failed(js_error_string(&e)))?;

        let ctx = canvas
            .get_context("2d")
            .map_err(|e| Error::rasterizer_canvas_creation_failed(js_error_string(&e)))?
            .ok_or_else(Error::rasterizer_context_failed)?
            .dyn_into::<OffscreenCanvasRenderingContext2d>()
            .map_err(|_| Error::rasterizer_context_failed())?;

        ctx.set_text_baseline("top");
        ctx.set_text_align("left");

        Ok(Self { canvas, render_ctx: ctx })
    }

    /// Creates a builder for batch rasterizing multiple graphemes.
    ///
    /// All glyphs in the batch share the same font settings (family, size, bold, italic).
    pub fn begin_batch(&self) -> RasterizeGlyphs<'_> {
        RasterizeGlyphs::new(self)
    }

    /// Measures cell size by rendering "█" and scanning actual pixel bounds.
    /// This is more accurate than text metrics which can have rounding issues.
    pub(super) fn measure_cell_metrics(&self) -> Result<CellMetrics, Error> {
        let buffer_size = 128u32;
        let draw_offset = 16.0; // Draw with offset to capture any negative positioning

        self.render_ctx
            .clear_rect(0.0, 0.0, buffer_size as f64, buffer_size as f64);
        self.render_ctx.set_fill_style_str("white");
        self.render_ctx
            .fill_text("█", draw_offset, draw_offset)
            .map_err(|e| Error::rasterizer_measure_failed(js_error_string(&e)))?;

        let image_data = self
            .render_ctx
            .get_image_data(0.0, 0.0, buffer_size as f64, buffer_size as f64)
            .map_err(|e| Error::rasterizer_measure_failed(js_error_string(&e)))?;

        let pixels = image_data.data();

        // infer good-enough pixel bounds (where alpha > threshold)
        const ALPHA_THRESHOLD: u8 = 128;
        let mut min_x = buffer_size;
        let mut max_x = 0u32;
        let mut min_y = buffer_size;
        let mut max_y = 0u32;

        for y in 0..buffer_size {
            for x in 0..buffer_size {
                let idx = ((y * buffer_size + x) * 4 + 3) as usize; // alpha channel
                if pixels[idx] >= ALPHA_THRESHOLD {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        // calculate dimensions from pixel bounds
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;

        // ascent is how far above the draw position the glyph started
        // (draw_offset - min_y) gives pixels above the draw point
        let ascent = draw_offset - min_y as f64;

        Ok(CellMetrics {
            padded_width: width + 2 * PADDING,
            padded_height: height + 2 * PADDING,
            ascent,
        })
    }
}

/// Builder for batch rasterizing multiple glyphs with shared font settings.
///
/// Created via [`CanvasRasterizer::begin_batch()`].
pub struct RasterizeGlyphs<'a> {
    rasterizer: &'a CanvasRasterizer,
    font_family: Option<&'a str>,
    font_size: Option<f32>,
    cell_metrics: Option<CellMetrics>,
}

impl<'a> RasterizeGlyphs<'a> {
    fn new(rasterizer: &'a CanvasRasterizer) -> Self {
        Self {
            rasterizer,
            font_family: None,
            font_size: None,
            cell_metrics: None,
        }
    }

    /// Sets the CSS font-family string for all glyphs.
    pub fn font_family(mut self, font_family: &'a str) -> Self {
        self.font_family = Some(font_family);
        self.cell_metrics = None;
        self
    }

    /// Sets the font size in pixels for all glyphs.
    ///
    /// If not set, defaults to `cell_height * 0.85`.
    pub fn font_size(mut self, size_px: f32) -> Self {
        self.font_size = Some(size_px);
        self.cell_metrics = None;
        self
    }

    /// Rasterizes all glyphs and returns them as a vector.
    ///
    /// Each glyph is paired with its font style. Emoji glyphs always use
    /// `FontStyle::Normal` regardless of the requested style.
    ///
    /// Glyphs are drawn vertically on the canvas (one per row) and extracted
    /// with a single `getImageData()` call for efficiency.
    ///
    /// Double-width glyphs (emoji, CJK) will have `width = cell_width * 2`.
    ///
    /// # Errors
    ///
    /// Returns an error if font_family was not set, or if canvas operations fail.
    pub fn rasterize(
        mut self,
        symbols: &[(&'a str, FontStyle)],
    ) -> Result<Vec<RasterizedGlyph>, Error> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }

        let font_family = self
            .font_family
            .ok_or_else(Error::rasterizer_missing_font_family)?;

        let font_size = self
            .font_size
            .ok_or_else(Error::rasterizer_missing_font_size)?;

        self.rasterizer
            .render_ctx
            .set_fill_style_str("white");

        let base_font = build_font_string(font_family, font_size, FontStyle::Normal);
        self.rasterizer.render_ctx.set_font(&base_font);

        let metrics = self.resolve_cell_metrics()?;
        let cell_w = metrics.padded_width;
        let cell_h = metrics.padded_height;

        let num_glyphs = symbols.len() as u32;

        // canvas needs to be double-width (for emoji) and tall enough for all glyphs
        let canvas_width = cell_w * 2;
        let canvas_height = cell_h * num_glyphs;

        self.rasterizer.render_ctx.clear_rect(
            0.0,
            0.0,
            self.rasterizer.canvas.width() as f64,
            self.rasterizer.canvas.height() as f64,
        );

        let mut current_style: Option<FontStyle> = Some(FontStyle::Normal);
        let y_offset = PADDING as f64 + metrics.ascent;

        // draw each glyph on its own row
        for (i, &(grapheme, style)) in symbols.iter().enumerate() {
            // emoji always uses normal style (no bold/italic variants)
            let effective_style =
                if emojis::get(grapheme).is_some() { FontStyle::Normal } else { style };

            // update font if style changed
            if current_style != Some(effective_style) {
                let font = build_font_string(font_family, font_size, effective_style);
                self.rasterizer.render_ctx.set_font(&font);
                current_style = Some(effective_style);
            }

            let y = (i as u32 * cell_h) as f64;
            self.rasterizer
                .render_ctx
                .fill_text(grapheme, PADDING as f64, y + y_offset)
                .map_err(|e| Error::rasterizer_fill_text_failed(grapheme, js_error_string(&e)))?;
        }

        // extract all pixels at once
        let image_data = self
            .rasterizer
            .render_ctx
            .get_image_data(0.0, 0.0, canvas_width as f64, canvas_height as f64)
            .map_err(|e| Error::rasterizer_get_image_data_failed(js_error_string(&e)))?;
        let all_pixels = image_data.data().to_vec();

        // split into individual glyphs
        let bytes_per_pixel = 4usize;
        let row_stride = canvas_width as usize * bytes_per_pixel;
        let glyph_stride = cell_h as usize * row_stride;

        let mut results = Vec::with_capacity(symbols.len());

        for (i, &(grapheme, _)) in symbols.iter().enumerate() {
            let padded_width = if is_double_width(grapheme) { cell_w * 2 } else { cell_w };

            let glyph_start = i * glyph_stride;
            let mut pixels = Vec::with_capacity((padded_width * cell_h) as usize * bytes_per_pixel);

            // extract rows, include padding
            for row in 0..cell_h as usize {
                let row_start = glyph_start + row * row_stride;
                let row_end = row_start + (padded_width as usize * bytes_per_pixel);
                pixels.extend_from_slice(&all_pixels[row_start..row_end]);
            }

            results.push(RasterizedGlyph::new(pixels, padded_width, cell_h));
        }

        Ok(results)
    }

    fn resolve_cell_metrics(&mut self) -> Result<CellMetrics, Error> {
        if let Some(metrics) = self.cell_metrics {
            return Ok(metrics);
        }

        let metrics = self.rasterizer.measure_cell_metrics()?;
        self.cell_metrics = Some(metrics);

        Ok(metrics)
    }
}

/// Converts a JsValue error to a displayable string for error messages.
fn js_error_string(err: &JsValue) -> String {
    err.as_string()
        .unwrap_or_else(|| format!("{err:?}"))
}

/// Checks if a grapheme is double-width (emoji or fullwidth character).
fn is_double_width(grapheme: &str) -> bool {
    use unicode_width::UnicodeWidthChar;

    emojis::get(grapheme).is_some()
        || grapheme
            .chars()
            .next()
            .and_then(UnicodeWidthChar::width)
            .is_some_and(|w| w == 2)
}

/// Builds a CSS font string with style modifiers.
fn build_font_string(font_family: &str, font_size: f32, style: FontStyle) -> String {
    let (bold, italic) = match style {
        FontStyle::Normal => (false, false),
        FontStyle::Bold => (true, false),
        FontStyle::Italic => (false, true),
        FontStyle::BoldItalic => (true, true),
    };

    let style_str = if italic { "italic " } else { "" };
    let weight = if bold { "bold " } else { "" };

    format!("{style_str}{weight}{font_size}px {font_family}, monospace")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_double_width() {
        // emoji
        assert!(is_double_width("😀"));
        assert!(is_double_width("👨‍👩‍👧")); // ZWJ sequence

        // CJK
        assert!(is_double_width("中"));
        assert!(is_double_width("日"));

        // single-width
        assert!(!is_double_width("A"));
        assert!(!is_double_width("→"));
    }

    #[test]
    fn test_build_font_string() {
        assert_eq!(
            build_font_string("'Hack'", 16.0, FontStyle::Normal),
            "16px 'Hack', monospace"
        );
        assert_eq!(
            build_font_string("'Hack'", 16.0, FontStyle::Bold),
            "bold 16px 'Hack', monospace"
        );
        assert_eq!(
            build_font_string("'Hack'", 16.0, FontStyle::Italic),
            "italic 16px 'Hack', monospace"
        );
        assert_eq!(
            build_font_string("'Hack'", 16.0, FontStyle::BoldItalic),
            "italic bold 16px 'Hack', monospace"
        );
    }
}
