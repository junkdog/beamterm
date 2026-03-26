use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source},
};

use crate::error::Error;

/// Cell dimensions and decoration metrics for a font at a given size.
#[derive(Debug, Clone, Copy)]
pub struct CellMetrics {
    /// Cell width in pixels.
    pub width: i32,
    /// Cell height in pixels.
    pub height: i32,
    /// Ascent in pixels (distance from top of cell to baseline), from font metrics.
    pub ascent: f32,
    /// Descent in pixels (distance from baseline to bottom of cell), from font metrics.
    pub descent: f32,
    /// Pixel-exact baseline offset from the rendered reference glyph (█).
    /// This is `placement.top` from the unhinted render and should be used
    /// for vertical glyph placement instead of `ascent.round()`, which can
    /// differ by ±1px and cause gaps at cell edges.
    pub baseline_y: i32,
    /// Underline position as a fraction of cell height (0.0 = top, 1.0 = bottom).
    pub underline_position: f32,
    /// Underline thickness as a fraction of cell height.
    pub underline_thickness: f32,
    /// Strikethrough position as a fraction of cell height.
    pub strikethrough_position: f32,
    /// Strikethrough thickness as a fraction of cell height.
    pub strikethrough_thickness: f32,
}

/// Measures cell metrics for the terminal grid.
///
/// Cell width comes from the advance width (ceiled to ensure no sub-pixel gaps).
/// Cell height and baseline come from a hinted render of `█` (U+2588).
/// Block elements (U+2580-U+259F) are synthesized programmatically in the
/// rasterizer, so cell dimensions don't need to match any rendered block glyph.
pub fn measure_cell_metrics(
    font_ref: FontRef<'_>,
    font_size: f32,
    scale_ctx: &mut ScaleContext,
) -> Result<CellMetrics, Error> {
    let font_metrics = font_ref.metrics(&[]).scale(font_size);
    let glyph_metrics = font_ref.glyph_metrics(&[]).scale(font_size);

    let block_id = font_ref.charmap().map('\u{2588}');

    // cell width from the advance width: this is the canonical monospace cell
    // width. Block elements are synthesized programmatically (not rendered from
    // the font), so the cell width doesn't need to match any rendered glyph.
    let advance_w = if block_id != 0 {
        glyph_metrics.advance_width(block_id)
    } else {
        font_metrics.average_width
    };
    let cell_width = advance_w.ceil() as i32;

    // cell height and baseline from hinted render of █: these define the
    // vertical grid and baseline placement for all glyphs.
    let (cell_height, baseline_y) = if block_id != 0 {
        let mut scaler = scale_ctx
            .builder(font_ref)
            .size(font_size)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline]).render(&mut scaler, block_id);

        match image {
            Some(img) if img.placement.height > 0 => {
                (img.placement.height as i32, img.placement.top)
            },
            _ => {
                let h = (font_metrics.ascent + font_metrics.descent.abs()).ceil() as i32;
                (h, font_metrics.ascent.round() as i32)
            },
        }
    } else {
        let h = (font_metrics.ascent + font_metrics.descent.abs()).ceil() as i32;
        (h, font_metrics.ascent.round() as i32)
    };

    if cell_width <= 0 || cell_height <= 0 {
        return Err(Error::RasterizationFailed(
            "reference glyph produced zero-size cell".into(),
        ));
    }

    let cell_h = cell_height as f32;
    let ascent = font_metrics.ascent;
    let descent = font_metrics.descent.abs();

    // derive decoration positions from font metrics
    // underline_offset is distance below baseline (positive = below)
    let underline_pos = (ascent - font_metrics.underline_offset) / cell_h;
    let strikethrough_pos = (ascent - font_metrics.strikeout_offset) / cell_h;

    let min_thickness = 0.05;
    let stroke_thickness = (font_metrics.stroke_size / cell_h).max(min_thickness);

    Ok(CellMetrics {
        width: cell_width,
        height: cell_height,
        ascent,
        descent,
        baseline_y,
        underline_position: underline_pos.clamp(0.0, 1.0),
        underline_thickness: stroke_thickness,
        strikethrough_position: strikethrough_pos.clamp(0.0, 1.0),
        strikethrough_thickness: stroke_thickness,
    })
}

/// Computes the font size needed so that a fallback font's glyphs fit
/// within the primary font's cell.
///
/// Scales the fallback font so its advance width matches the primary's
/// cell width. This ensures edge-to-edge glyphs (like powerline characters)
/// fill the full cell, and regular text glyphs occupy the correct width.
///
/// If the advance-width-scaled glyphs would exceed the cell height, the
/// scale is reduced to fit vertically as well.
pub fn compute_fallback_font_size(
    primary: &CellMetrics,
    fallback_ref: FontRef<'_>,
    base_font_size: f32,
) -> f32 {
    let fallback_metrics = fallback_ref.metrics(&[]).scale(base_font_size);
    let fallback_glyph_metrics = fallback_ref
        .glyph_metrics(&[])
        .scale(base_font_size);

    // scale by advance width: fallback advance should match primary cell width
    let fallback_advance = {
        let block_id = fallback_ref.charmap().map('\u{2588}');
        if block_id != 0 {
            fallback_glyph_metrics.advance_width(block_id)
        } else {
            fallback_metrics.average_width
        }
    };

    let width_scale = if fallback_advance > 0.0 {
        primary.width as f32 / fallback_advance
    } else {
        1.0
    };

    // also compute height scale to prevent vertical overflow
    let fallback_height = fallback_metrics.ascent + fallback_metrics.descent.abs();
    let height_scale = if fallback_height > 0.0 {
        let primary_height = primary.ascent + primary.descent;
        primary_height / fallback_height
    } else {
        1.0
    };

    // use the smaller scale so the glyph fits in both dimensions
    let scale = width_scale.min(height_scale);

    base_font_size * scale
}
