use std::collections::HashMap;

use beamterm_data::{FontAtlasData, FontStyle, LineDecoration};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, image::Content},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    error::Error,
    font_fallback::FontResolver,
    metrics::{CellMetrics, compute_fallback_font_size, measure_cell_metrics},
};

/// A rasterized glyph with RGBA pixel data sized to fit a cell.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// RGBA pixel data.
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// True if the glyph occupies two cells (detected from the font's
    /// advance width, not just unicode-width).
    pub is_double_width: bool,
    /// True if the glyph was rendered using a fallback font instead
    /// of the primary font family.
    pub is_fallback: bool,
    /// Name of the font family that rendered this glyph, if it was
    /// a fallback font (i.e., not the primary font).
    pub fallback_font_name: Option<String>,
}

impl RasterizedGlyph {
    pub fn new(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
            is_double_width: false,
            is_fallback: false,
            fallback_font_name: None,
        }
    }

    pub fn new_wide(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
            is_double_width: true,
            is_fallback: false,
            fallback_font_name: None,
        }
    }
}

/// Native font rasterizer using swash + fontdb.
///
/// Rasterizes individual graphemes into cell-sized RGBA bitmaps suitable
/// for upload to a GL texture atlas.
pub struct NativeRasterizer {
    font_resolver: FontResolver,
    scale_context: ScaleContext,
    font_size: f32,
    cell_metrics: CellMetrics,
    /// Cached effective font sizes for fallback fonts, keyed by font index.
    /// Computed on first use via rasterization-based refinement.
    fallback_sizes: HashMap<usize, f32>,
}

impl NativeRasterizer {
    /// Creates a new rasterizer for the given font families and size.
    ///
    /// Loads system fonts and resolves the requested families. At least one
    /// family must be found. The first family is the primary font.
    pub fn new(font_families: &[&str], font_size: f32) -> Result<Self, Error> {
        let font_resolver = FontResolver::new(font_families)?;
        let mut scale_context = ScaleContext::new();
        let cell_metrics = font_resolver
            .with_primary_font(|font_ref| {
                measure_cell_metrics(font_ref, font_size, &mut scale_context)
            })
            .ok_or_else(|| Error::RasterizationFailed("primary font unavailable".into()))??;

        Ok(Self {
            font_resolver,
            scale_context,
            font_size,
            cell_metrics,
            fallback_sizes: HashMap::new(),
        })
    }

    /// Rasterizes a single grapheme into a cell-sized RGBA bitmap.
    ///
    /// The output is padded by `FontAtlasData::PADDING` on each side.
    /// Double-width graphemes (emoji, CJK, or glyphs whose advance width
    /// exceeds 1.5x the cell width) produce a bitmap 2 cells wide.
    pub fn rasterize(
        &mut self,
        grapheme: &str,
        style: FontStyle,
    ) -> Result<RasterizedGlyph, Error> {
        // get the first codepoint for font resolution
        let ch = match grapheme.chars().next() {
            Some(c) => c,
            None => return Ok(empty_glyph_from_metrics(&self.cell_metrics)),
        };

        // resolve font index
        let is_emoji = is_emoji_grapheme(grapheme);
        let font_idx = if is_emoji {
            match self.font_resolver.resolve_color_char(ch) {
                Some(idx) => idx,
                None => return Ok(empty_glyph_from_metrics(&self.cell_metrics)),
            }
        } else {
            match self.font_resolver.resolve_styled(ch, style) {
                Some(idx) => idx,
                None => return Ok(empty_glyph_from_metrics(&self.cell_metrics)),
            }
        };

        // split borrows: font_resolver (immutable) vs other fields (mutable)
        let resolver = &self.font_resolver;
        let mut ctx = RasterizeContext {
            font_idx,
            grapheme,
            ch,
            primary_count: resolver.primary_count(),
            cell_metrics: &self.cell_metrics,
            font_size: self.font_size,
            scale_ctx: &mut self.scale_context,
            fallback_sizes: &mut self.fallback_sizes,
        };

        let mut result = resolver
            .with_font(font_idx, |font_ref| rasterize_with_font(font_ref, &mut ctx))
            .unwrap_or_else(|| Ok(empty_glyph_from_metrics(&self.cell_metrics)))?;

        // tag fallback info
        let is_fallback = font_idx >= self.font_resolver.primary_count();
        result.is_fallback = is_fallback;
        if is_fallback {
            result.fallback_font_name = self.font_resolver.font_family_name(font_idx);
        }

        Ok(result)
    }

    /// Returns the cell metrics for the primary font.
    pub fn cell_metrics(&self) -> &CellMetrics {
        &self.cell_metrics
    }

    /// Returns the cell size in pixels (without padding).
    pub fn cell_size(&self) -> beamterm_data::CellSize {
        beamterm_data::CellSize::new(self.cell_metrics.width, self.cell_metrics.height)
    }

    /// Returns the underline decoration metrics.
    pub fn underline(&self) -> LineDecoration {
        LineDecoration::new(
            self.cell_metrics.underline_position,
            self.cell_metrics.underline_thickness,
        )
    }

    /// Returns the strikethrough decoration metrics.
    pub fn strikethrough(&self) -> LineDecoration {
        LineDecoration::new(
            self.cell_metrics.strikethrough_position,
            self.cell_metrics.strikethrough_thickness,
        )
    }

    /// Checks if a grapheme should be treated as double-width by examining
    /// the font's advance width. Falls back to unicode-width / emoji detection
    /// when the font doesn't contain the glyph.
    ///
    /// This detects PUA glyphs (e.g. Nerd Font icons) that have advance widths
    /// wider than one cell, even though `unicode-width` returns 1 for them.
    pub fn is_double_width(&mut self, grapheme: &str) -> bool {
        if grapheme.len() == 1 {
            return false;
        }

        if is_wide(grapheme) {
            return true;
        }

        let ch = match grapheme.chars().next() {
            Some(c) => c,
            None => return false,
        };

        // resolve the font for this character
        let font_idx = match self.font_resolver.resolve_char(ch) {
            Some(idx) => idx,
            None => return false,
        };

        let font_size = self.font_size;
        let cell_w = self.cell_metrics.width;

        self.font_resolver
            .with_font(font_idx, |font_ref| {
                let glyph_id = font_ref.charmap().map(ch);
                if glyph_id == 0 {
                    return false;
                }

                let glyph_metrics = font_ref.glyph_metrics(&[]).scale(font_size);
                let advance = glyph_metrics.advance_width(glyph_id);
                advance > cell_w as f32 * 1.5
            })
            .unwrap_or(false)
    }

    /// Updates the font size and re-measures cell metrics.
    pub fn update_font_size(&mut self, font_size: f32) -> Result<(), Error> {
        self.font_size = font_size;

        let resolver = &self.font_resolver;
        let scale_ctx = &mut self.scale_context;

        self.cell_metrics = resolver
            .with_primary_font(|font_ref| measure_cell_metrics(font_ref, font_size, scale_ctx))
            .ok_or_else(|| Error::RasterizationFailed("primary font unavailable".into()))??;

        self.fallback_sizes.clear();
        Ok(())
    }
}

/// Rasterizes a glyph using the given font reference.
///
/// Extracted as a free function to work within the `with_font` callback,
/// where the `FontRef` borrows from the database's mmap'd font data.
/// Mutable state passed into the rasterization callback.
struct RasterizeContext<'a> {
    font_idx: usize,
    grapheme: &'a str,
    ch: char,
    primary_count: usize,
    cell_metrics: &'a CellMetrics,
    font_size: f32,
    scale_ctx: &'a mut ScaleContext,
    fallback_sizes: &'a mut HashMap<usize, f32>,
}

fn rasterize_with_font(
    font_ref: FontRef<'_>,
    ctx: &mut RasterizeContext<'_>,
) -> Result<RasterizedGlyph, Error> {
    let padding = FontAtlasData::PADDING;
    let cell_w = ctx.cell_metrics.width;

    // █ is synthesized: it has no interior edges, only cell boundaries
    // that must be fully opaque. Font rendering produces AA edges (~4 alpha).
    if let Some(glyph) = synthesize_full_block(ctx.ch, ctx.cell_metrics) {
        return Ok(glyph);
    }

    let glyph_id = font_ref.charmap().map(ctx.ch);
    if glyph_id == 0 {
        return Ok(empty_glyph_from_metrics(ctx.cell_metrics));
    }

    // determine double-width from unicode properties OR from the font's
    // own advance width. PUA glyphs (e.g. Nerd Font icons) often have
    // advance widths > 1 cell but unicode-width returns 1.
    let is_double_width = if is_wide(ctx.grapheme) {
        true
    } else {
        let glyph_metrics = font_ref.glyph_metrics(&[]).scale(ctx.font_size);
        let advance = glyph_metrics.advance_width(glyph_id);
        advance > cell_w as f32 * 1.5
    };

    let content_w = if is_double_width { cell_w * 2 } else { cell_w };
    let content_h = ctx.cell_metrics.height;
    let padded_w = (content_w + padding * 2) as u32;
    let padded_h = (content_h + padding * 2) as u32;

    let mut pixels = vec![0u8; (padded_w * padded_h * 4) as usize];

    // scale fallback fonts to fit within the primary font's cell
    let is_primary_font = ctx.font_idx < ctx.primary_count;
    let effective_size = if is_primary_font {
        ctx.font_size
    } else {
        // get or compute the per-font base scale (from █ refinement)
        let base_size = if let Some(&cached) = ctx.fallback_sizes.get(&ctx.font_idx) {
            cached
        } else {
            let size =
                refine_fallback_size(ctx.cell_metrics, font_ref, ctx.font_size, ctx.scale_ctx);
            ctx.fallback_sizes.insert(ctx.font_idx, size);
            size
        };

        // further adjust per-glyph: scale so this glyph's advance width
        // matches the target cell width (1x or 2x). Handles proportional
        // fallback fonts where each glyph has a different advance.
        let glyph_metrics = font_ref.glyph_metrics(&[]).scale(base_size);
        let glyph_advance = glyph_metrics.advance_width(glyph_id);
        if glyph_advance > 0.0 {
            let target_w = content_w as f32;
            let w_scale = target_w / glyph_advance;
            if w_scale > 1.05 { base_size * w_scale } else { base_size }
        } else {
            base_size
        }
    };

    // rasterize without hinting: hinting applies per-glyph grid fitting
    // that shifts strokes to inconsistent pixel positions between related
    // glyphs (e.g. ═ and ╝ horizontal strokes landing on different rows).
    // unhinted rendering preserves the font's designed stroke positions
    // for consistent alignment across all glyphs.
    // (block elements are synthesized above and never reach this path.)
    let mut scaler = ctx
        .scale_ctx
        .builder(font_ref)
        .size(effective_size)
        .hint(false)
        .build();

    let image = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(swash::scale::StrikeWith::BestFit),
        Source::Outline,
    ])
    .default_color([0xff, 0xff, 0xff, 0xff])
    .render(&mut scaler, glyph_id);

    let image = match image {
        Some(img) => img,
        None => return Ok(RasterizedGlyph::new(pixels, padded_w, padded_h)),
    };

    // use the pixel-exact baseline offset from the rendered reference glyph (█),
    // so all glyphs align to the same baseline and █ fills the cell exactly
    let ascent = ctx.cell_metrics.baseline_y;

    // Horizontal placement: always use the font's left bearing to
    // preserve alignment between related glyphs (e.g. box-drawing
    // characters ║ and ╢ share the same vertical stroke x-positions).
    // Pixels that land outside the padded cell are clipped by the
    // copy loop below.
    let dst_x = padding + image.placement.left;
    let dst_y = padding + (ascent - image.placement.top);

    let src_w = image.placement.width as i32;
    let src_h = image.placement.height as i32;

    let is_color = image.content == Content::Color;
    let bytes_per_src_pixel = if is_color { 4 } else { 1 };
    let src_stride = src_w * bytes_per_src_pixel;
    let dst_stride = padded_w as i32 * 4;

    // precompute the valid row/col ranges, eliminating per-pixel bounds checks
    let row_start = 0.max(-dst_y) as usize;
    let row_end = src_h.min(padded_h as i32 - dst_y) as usize;
    let col_start = 0.max(-dst_x) as usize;
    let col_end = src_w.min(padded_w as i32 - dst_x) as usize;

    for row in row_start..row_end {
        let src_row_offset = (row as i32 * src_stride) as usize;
        let dst_row_offset = ((dst_y + row as i32) * dst_stride) as usize;
        let dst_col_base = (dst_x + col_start as i32) as usize * 4;

        if is_color {
            for col in col_start..col_end {
                let src_idx = src_row_offset + col * 4;
                let dst_idx = dst_row_offset + dst_col_base + (col - col_start) * 4;
                pixels[dst_idx..dst_idx + 4].copy_from_slice(&image.data[src_idx..src_idx + 4]);
            }
        } else {
            for col in col_start..col_end {
                let alpha = image.data[src_row_offset + col];

                // avoiding an if alpha > 0 to not mess with the branch predictor
                let v = 0xff * alpha.min(1);
                let dst_idx = dst_row_offset + dst_col_base + (col - col_start) * 4;
                pixels[dst_idx] = v;
                pixels[dst_idx + 1] = v;
                pixels[dst_idx + 2] = v;
                pixels[dst_idx + 3] = alpha;
            }
        }
    }

    // for box-drawing and block elements, extend cell-edge pixels by copying
    // from the adjacent interior pixel, so lines connect between cells
    if needs_edge_boost(ctx.ch) {
        extend_cell_edges(&mut pixels, padded_w, padded_h, content_w, content_h);
    }

    Ok(RasterizedGlyph {
        pixels,
        width: padded_w,
        height: padded_h,
        is_double_width,
        is_fallback: false,
        fallback_font_name: None,
    })
}

/// Returns a fully transparent glyph sized to a single cell (with padding).
fn empty_glyph_from_metrics(cell_metrics: &CellMetrics) -> RasterizedGlyph {
    let padding = FontAtlasData::PADDING;
    let pw = (cell_metrics.width + padding * 2) as u32;
    let ph = (cell_metrics.height + padding * 2) as u32;
    RasterizedGlyph::new(vec![0u8; (pw * ph * 4) as usize], pw, ph)
}

/// Computes the optimal font size for a fallback font by:
/// 1. Starting with a metrics-based estimate
/// 2. Rendering █ at that size to measure actual pixel dimensions
/// 3. Adjusting the size so the rendered glyph fits the primary cell
///
/// This is called once per fallback font and cached.
fn refine_fallback_size(
    primary: &CellMetrics,
    fallback_ref: FontRef<'_>,
    base_font_size: f32,
    scale_ctx: &mut ScaleContext,
) -> f32 {
    // start with metrics-based estimate
    let mut size = compute_fallback_font_size(primary, fallback_ref, base_font_size);

    let block_id = fallback_ref.charmap().map('\u{2588}');
    if block_id == 0 {
        return size;
    }

    // refine: render █ and adjust to match primary cell dimensions
    for _ in 0..3 {
        let mut scaler = scale_ctx
            .builder(fallback_ref)
            .size(size)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline]).render(&mut scaler, block_id);

        let Some(img) = image else { break };
        let rendered_w = img.placement.width as f32;
        let rendered_h = img.placement.height as f32;

        if rendered_w <= 0.0 || rendered_h <= 0.0 {
            break;
        }

        let w_ratio = primary.width as f32 / rendered_w;
        let h_ratio = primary.height as f32 / rendered_h;
        let ratio = w_ratio.min(h_ratio);

        // converged: rendered size matches target within 1px
        if (ratio - 1.0).abs() < 0.05 {
            break;
        }

        size *= ratio;
    }

    size
}

/// Synthesizes █ (U+2588) programmatically. This is the only block element
/// that needs full synthesis — it has no interior edges, just cell boundaries
/// that must be fully opaque. All other block/box-drawing characters are
/// rendered from the font with cell-edge alpha boosting.
fn synthesize_full_block(ch: char, metrics: &CellMetrics) -> Option<RasterizedGlyph> {
    if ch != '\u{2588}' {
        return None;
    }

    let padding = FontAtlasData::PADDING;
    let cell_w = metrics.width;
    let cell_h = metrics.height;
    let padded_w = (cell_w + padding * 2) as u32;
    let padded_h = (cell_h + padding * 2) as u32;
    let stride = padded_w as usize * 4;

    let mut pixels = vec![0u8; stride * padded_h as usize];
    for row in 0..cell_h {
        for col in 0..cell_w {
            let idx = ((padding + row) as usize * stride) + (padding + col) as usize * 4;
            pixels[idx] = 0xff;
            pixels[idx + 1] = 0xff;
            pixels[idx + 2] = 0xff;
            pixels[idx + 3] = 0xff;
        }
    }

    Some(RasterizedGlyph::new(pixels, padded_w, padded_h))
}

/// Returns true for characters where lines/fills must connect seamlessly
/// at cell boundaries: Box Drawing (U+2500-U+257F) and Block Elements
/// (U+2580-U+259F). For these, cell-edge pixels with alpha > 0 are
/// boosted to 255 after rendering to eliminate anti-aliased gaps.
fn needs_edge_boost(ch: char) -> bool {
    ('\u{2500}'..='\u{259F}').contains(&ch)
}

/// Extends cell-edge pixels by copying RGBA from the nearest interior neighbor.
/// For edge pixels with alpha > 0 but low coverage (from anti-aliasing), this
/// replaces the faint fringe with a proper continuation of the adjacent interior
/// pixel, ensuring lines connect seamlessly between cells.
fn extend_cell_edges(pixels: &mut [u8], padded_w: u32, padded_h: u32, cell_w: i32, cell_h: i32) {
    let padding = FontAtlasData::PADDING;
    let stride = padded_w as usize * 4;

    let left_col = padding as usize;
    let right_col = (padding + cell_w - 1) as usize;
    let top_row = padding as usize;
    let bottom_row = (padding + cell_h - 1) as usize;

    // extend left/right edge columns from their interior neighbor
    for row in top_row..=(bottom_row.min(padded_h as usize - 1)) {
        // left edge ← copy from column left_col + 1
        let edge = row * stride + left_col * 4;
        let neighbor = row * stride + (left_col + 1) * 4;
        if edge + 3 < pixels.len()
            && neighbor + 3 < pixels.len()
            && pixels[edge + 3] > 0
            && pixels[neighbor + 3] > pixels[edge + 3]
        {
            pixels.copy_within(neighbor..neighbor + 4, edge);
        }

        // right edge ← copy from column right_col - 1
        let edge = row * stride + right_col * 4;
        let neighbor = row * stride + (right_col - 1) * 4;
        if edge + 3 < pixels.len()
            && neighbor + 3 < pixels.len()
            && pixels[edge + 3] > 0
            && pixels[neighbor + 3] > pixels[edge + 3]
        {
            pixels.copy_within(neighbor..neighbor + 4, edge);
        }
    }

    // extend top/bottom edge rows from their interior neighbor
    for col in left_col..=(right_col.min(padded_w as usize - 1)) {
        // top edge ← copy from row top_row + 1
        let edge = top_row * stride + col * 4;
        let neighbor = (top_row + 1) * stride + col * 4;
        if edge + 3 < pixels.len()
            && neighbor + 3 < pixels.len()
            && pixels[edge + 3] > 0
            && pixels[neighbor + 3] > pixels[edge + 3]
        {
            pixels.copy_within(neighbor..neighbor + 4, edge);
        }

        // bottom edge ← copy from row bottom_row - 1
        let edge = bottom_row * stride + col * 4;
        let neighbor = (bottom_row - 1) * stride + col * 4;
        if edge + 3 < pixels.len()
            && neighbor + 3 < pixels.len()
            && pixels[edge + 3] > 0
            && pixels[neighbor + 3] > pixels[edge + 3]
        {
            pixels.copy_within(neighbor..neighbor + 4, edge);
        }
    }
}

fn is_wide(grapheme: &str) -> bool {
    grapheme.width() >= 2
}

/// Checks if a grapheme is an emoji that should use color font rendering.
fn is_emoji_grapheme(s: &str) -> bool {
    let bytes = s.as_bytes();
    let first_byte = match bytes.first() {
        Some(&b) => b,
        None => return false,
    };

    if first_byte < 0x80 {
        return s.len() > 1 && s.width() >= 2;
    }

    if first_byte < 0xE0 {
        return s.len() > 2 && s.width() >= 2;
    }

    // SAFETY: verified non-empty with 3+ byte lead
    let first = unsafe { s.chars().next().unwrap_unchecked() };
    let first_len = first.len_utf8();

    if s.len() == first_len {
        return if first_len == 3 {
            is_emoji_presentation(first)
        } else {
            s.width() >= 2 && is_emoji_presentation(first)
        };
    }

    s.width() >= 2
}

/// Returns `true` for characters with emoji-presentation-by-default.
fn is_emoji_presentation(c: char) -> bool {
    let cp = c as u32;

    match cp {
        0x231A..=0x2B55 => matches!(
            cp,
            0x231A..=0x231B
                | 0x23E9..=0x23EC
                | 0x23F0
                | 0x23F3
                | 0x25FD..=0x25FE
                | 0x2614..=0x2615
                | 0x2648..=0x2653
                | 0x267F
                | 0x2693
                | 0x26A1
                | 0x26AA..=0x26AB
                | 0x26BD..=0x26BE
                | 0x26C4..=0x26C5
                | 0x26CE
                | 0x26D4
                | 0x26EA
                | 0x26F2..=0x26F3
                | 0x26F5
                | 0x26FA
                | 0x26FD
                | 0x2705
                | 0x270A..=0x270B
                | 0x2728
                | 0x274C
                | 0x274E
                | 0x2753..=0x2755
                | 0x2757
                | 0x2795..=0x2797
                | 0x27B0
                | 0x27BF
                | 0x2B1B..=0x2B1C
                | 0x2B50
                | 0x2B55
        ),
        0x1F000..=0x1FFFF => !matches!(
            cp,
            0x1F200
                | 0x1F202..=0x1F219
                | 0x1F21B..=0x1F22E
                | 0x1F230..=0x1F231
                | 0x1F237
                | 0x1F23B..=0x1F24F
                | 0x1F260..=0x1F265
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use beamterm_data::FontAtlasData;

    use super::*;

    /// Helper: create a rasterizer with a common monospace font.
    /// Skips the test if no suitable font is found.
    fn test_rasterizer() -> Option<NativeRasterizer> {
        // try common monospace fonts in order of likelihood
        for families in [
            &["Hack"][..],
            &["DejaVu Sans Mono"],
            &["Liberation Mono"],
            &["Noto Sans Mono"],
            &["Courier New"],
            &["monospace"],
        ] {
            if let Ok(r) = NativeRasterizer::new(families, 16.0) {
                return Some(r);
            }
        }
        None
    }

    #[test]
    fn cell_size_is_positive() {
        let Some(rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let cs = rasterizer.cell_size();
        let (w, h) = (cs.width, cs.height);
        assert!(w > 0, "cell width must be positive, got {w}");
        assert!(h > 0, "cell height must be positive, got {h}");
        // typical monospace cells at 16px: roughly 8-12 wide, 16-24 tall
        assert!(w < 100, "cell width unreasonably large: {w}");
        assert!(h < 100, "cell height unreasonably large: {h}");
    }

    #[test]
    fn ascii_rasterization_produces_pixels() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING;
        let cs = rasterizer.cell_size();
        let (cell_w, cell_h) = (cs.width, cs.height);
        let padded_w = (cell_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;

        let glyph = rasterizer
            .rasterize("A", FontStyle::Normal)
            .unwrap();
        assert_eq!(glyph.width, padded_w);
        assert_eq!(glyph.height, padded_h);

        // "A" should have visible pixels (non-zero alpha)
        let has_visible = glyph.pixels.chunks(4).any(|px| px[3] > 0);
        assert!(has_visible, "'A' glyph should have visible pixels");
    }

    #[test]
    fn space_rasterization_is_blank() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let glyph = rasterizer
            .rasterize(" ", FontStyle::Normal)
            .unwrap();
        let all_transparent = glyph.pixels.chunks(4).all(|px| px[3] == 0);
        assert!(all_transparent, "space glyph should be fully transparent");
    }

    #[test]
    fn double_width_glyph_is_wider() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING;
        let cell_w = rasterizer.cell_size().width;
        let single_padded_w = (cell_w + padding * 2) as u32;
        let double_padded_w = (cell_w * 2 + padding * 2) as u32;

        // CJK character (double-width)
        let glyph = rasterizer
            .rasterize("\u{4E2D}", FontStyle::Normal)
            .unwrap();

        // skip if the font doesn't have the CJK glyph (returns single-width blank)
        if glyph.width == single_padded_w {
            eprintln!("skipping: font lacks CJK glyph U+4E2D");
            return;
        }

        assert_eq!(
            glyph.width, double_padded_w,
            "CJK glyph width should be 2*cell_w + 2*padding"
        );
        assert!(
            glyph.width > single_padded_w,
            "double-width glyph should be wider than single"
        );
    }

    #[test]
    fn decoration_metrics_in_valid_range() {
        let Some(rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let underline = rasterizer.underline();
        let strikethrough = rasterizer.strikethrough();

        assert!(
            (0.0..=1.0).contains(&underline.position()),
            "underline position should be 0.0-1.0, got {}",
            underline.position()
        );
        assert!(
            underline.thickness() > 0.0,
            "underline thickness should be positive"
        );

        assert!(
            (0.0..=1.0).contains(&strikethrough.position()),
            "strikethrough position should be 0.0-1.0, got {}",
            strikethrough.position()
        );
        assert!(
            strikethrough.thickness() > 0.0,
            "strikethrough thickness should be positive"
        );

        // strikethrough should be above underline (lower position value)
        assert!(
            strikethrough.position() < underline.position(),
            "strikethrough ({}) should be above underline ({})",
            strikethrough.position(),
            underline.position()
        );
    }

    #[test]
    fn update_font_size_changes_cell_size() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let cs1 = rasterizer.cell_size();
        let (w1, h1) = (cs1.width, cs1.height);

        rasterizer.update_font_size(32.0).unwrap();
        let cs2 = rasterizer.cell_size();
        let (w2, h2) = (cs2.width, cs2.height);

        assert!(
            w2 > w1 && h2 > h1,
            "doubling font size should increase cell size: \
             ({w1},{h1}) at 16px vs ({w2},{h2}) at 32px"
        );
    }

    #[test]
    fn font_not_found_returns_error() {
        let result = NativeRasterizer::new(&["ThisFontDoesNotExist99"], 16.0);
        assert!(result.is_err(), "nonexistent font should return error");
    }

    #[test]
    fn emoji_detection() {
        assert!(is_emoji_grapheme("\u{1F680}")); // rocket
        assert!(is_emoji_grapheme("\u{1F600}")); // grinning face
        assert!(!is_emoji_grapheme("A"));
        assert!(!is_emoji_grapheme(" "));

        // text-presentation-by-default without FE0F: not emoji
        assert!(!is_emoji_grapheme("\u{25B6}"));
        // with FE0F: emoji
        assert!(is_emoji_grapheme("\u{25B6}\u{FE0F}"));
    }

    #[test]
    fn double_width_detection() {
        assert!(is_wide("\u{1F680}")); // emoji
        assert!(is_wide("\u{4E2D}")); // CJK
        assert!(!is_wide("A"));
        assert!(!is_wide(" "));
    }

    /// Characters likely missing from monospace fonts, forcing a fallback
    /// to a system font.
    const FALLBACK_CANDIDATES: &[char] = &[
        '\u{2603}', // snowman
        '\u{2654}', // white chess king
        '\u{2318}', // place of interest sign
        '\u{263A}', // white smiling face
        '\u{2602}', // umbrella
        '\u{2708}', // airplane
        '\u{2744}', // snowflake
        '\u{2764}', // heavy black heart
        '\u{2660}', // black spade suit
    ];

    /// Finds a character that the primary font does NOT have but a system
    /// fallback does. Returns `None` if no such character is found.
    fn find_fallback_char(rasterizer: &mut NativeRasterizer) -> Option<char> {
        FALLBACK_CANDIDATES.iter().copied().find(|&ch| {
            // primary must NOT have it
            !rasterizer.font_resolver.primary_has_char(ch)
            // but some system font must
                && rasterizer.font_resolver.resolve_char(ch).is_some()
        })
    }

    #[test]
    fn fallback_glyph_fits_within_primary_cell() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let Some(ch) = find_fallback_char(&mut rasterizer) else {
            eprintln!("skipping: no fallback character found (primary font covers all candidates)");
            return;
        };

        let symbol = &String::from(ch);

        // confirm the primary font genuinely lacks this character
        assert!(
            !rasterizer.font_resolver.primary_has_char(ch),
            "'{symbol}' should NOT be in the primary font"
        );

        let padding = FontAtlasData::PADDING;
        let cs = rasterizer.cell_size();
        let (cell_w, cell_h) = (cs.width, cs.height);

        let glyph = rasterizer
            .rasterize(symbol, FontStyle::Normal)
            .unwrap();

        // output size must match the primary cell dimensions (1x or 2x wide)
        let expected_content_w = if glyph.is_double_width { cell_w * 2 } else { cell_w };
        let padded_w = (expected_content_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;
        assert_eq!(
            glyph.width, padded_w,
            "fallback glyph width mismatch (double_width={})",
            glyph.is_double_width
        );
        assert_eq!(glyph.height, padded_h, "fallback glyph height mismatch");

        // fallback must have produced visible pixels
        let has_pixels = glyph.pixels.chunks(4).any(|px| px[3] > 0);
        assert!(
            has_pixels,
            "fallback glyph '{symbol}' should have visible pixels"
        );

        // verify all visible pixels stay within the padded cell
        let bbox = pixel_bbox(&glyph);
        assert!(
            bbox.max_x < padded_w,
            "pixels exceed cell width: {} >= {padded_w}",
            bbox.max_x
        );
        assert!(
            bbox.max_y < padded_h,
            "pixels exceed cell height: {} >= {padded_h}",
            bbox.max_y
        );
    }

    #[test]
    fn full_block_fits_within_cell() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING;
        let cs = rasterizer.cell_size();
        let (cell_w, cell_h) = (cs.width, cs.height);
        let padded_w = (cell_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;

        let glyph = rasterizer
            .rasterize("\u{2588}", FontStyle::Normal)
            .unwrap();

        assert_eq!(glyph.width, padded_w);
        assert_eq!(glyph.height, padded_h);

        let bbox = pixel_bbox(&glyph);

        eprintln!("█ cell_size=({cell_w},{cell_h}) padded=({padded_w},{padded_h})");
        eprintln!(
            "  visible bbox: ({},{})-({},{}) = {}x{}",
            bbox.min_x,
            bbox.min_y,
            bbox.max_x,
            bbox.max_y,
            bbox.width(),
            bbox.height()
        );
        eprintln!(
            "  expected content area: padding={padding}..padding+cell = ({padding},{padding})-({},{})",
            padding + cell_w - 1,
            padding + cell_h - 1
        );

        // pixels must stay within the padded cell
        assert!(
            bbox.max_x < padded_w,
            "█ exceeds cell width: max_x={} >= {padded_w}",
            bbox.max_x
        );
        assert!(
            bbox.max_y < padded_h,
            "█ exceeds cell height: max_y={} >= {padded_h}",
            bbox.max_y
        );

        // visible area should not exceed cell dimensions (ignoring padding);
        // allow 1px tolerance for font hinting differences across environments
        let w_excess = bbox.width().saturating_sub(cell_w as u32);
        let h_excess = bbox.height().saturating_sub(cell_h as u32);
        if w_excess > 0 || h_excess > 0 {
            if w_excess <= 1 && h_excess <= 1 {
                eprintln!(
                    "skipping strict size check: font hinting causes █ to exceed cell by \
                     {w_excess}x{h_excess}px (visible={}x{}, cell={cell_w}x{cell_h})",
                    bbox.width(),
                    bbox.height()
                );
                return;
            }
            panic!(
                "█ visible size {}x{} exceeds cell {cell_w}x{cell_h} by more than 1px",
                bbox.width(),
                bbox.height()
            );
        }

        // check various powerline/nerd-font glyphs
        let nerd_glyphs: &[(&str, &str)] = &[
            ("\u{E0B0}", "right triangle"),
            ("\u{E0B2}", "left triangle"),
            ("\u{E0B4}", "right semicircle"),
            ("\u{E0B6}", "left semicircle"),
        ];

        for &(symbol, name) in nerd_glyphs {
            let gl = rasterizer
                .rasterize(symbol, FontStyle::Normal)
                .unwrap();
            let has_pixels = gl.pixels.chunks(4).any(|px| px[3] > 0);
            if !has_pixels {
                eprintln!("{symbol} ({name}) not available");
                continue;
            }

            let gl_bbox = pixel_bbox(&gl);
            let in_primary = rasterizer
                .font_resolver
                .primary_has_char(symbol.chars().next().unwrap());
            let is_wide = super::is_wide(symbol);
            eprintln!(
                "{symbol} ({name}) primary={in_primary} is_wide={is_wide} glyph_size={}x{}",
                gl.width, gl.height
            );
            eprintln!(
                "  visible bbox: ({},{})-({},{}) = {}x{}",
                gl_bbox.min_x,
                gl_bbox.min_y,
                gl_bbox.max_x,
                gl_bbox.max_y,
                gl_bbox.width(),
                gl_bbox.height()
            );
            eprintln!("  content area: {cell_w}x{cell_h}, padded: {padded_w}x{padded_h}");

            if gl_bbox.width() > cell_w as u32 {
                eprintln!(
                    "  WARNING: glyph {}px wider than cell {}px",
                    gl_bbox.width() - cell_w as u32,
                    cell_w
                );
            }
        }
    }

    /// Bounding box of visible (alpha > 0) pixels in a glyph.
    struct PixelBbox {
        min_x: u32,
        min_y: u32,
        max_x: u32,
        max_y: u32,
    }

    impl PixelBbox {
        fn width(&self) -> u32 {
            self.max_x - self.min_x + 1
        }
        fn height(&self) -> u32 {
            self.max_y - self.min_y + 1
        }
    }

    fn pixel_bbox(glyph: &RasterizedGlyph) -> PixelBbox {
        let mut bbox = PixelBbox {
            min_x: glyph.width,
            min_y: glyph.height,
            max_x: 0,
            max_y: 0,
        };
        for (i, px) in glyph.pixels.chunks(4).enumerate() {
            if px[3] > 0 {
                let x = (i as u32) % glyph.width;
                let y = (i as u32) / glyph.width;
                bbox.min_x = bbox.min_x.min(x);
                bbox.min_y = bbox.min_y.min(y);
                bbox.max_x = bbox.max_x.max(x);
                bbox.max_y = bbox.max_y.max(y);
            }
        }
        bbox
    }

    /// Box-drawing characters with negative bearings (e.g. ╢, ╝) must
    /// use bearing-based placement so their vertical strokes align with
    /// characters like ║ that have positive bearings.
    #[test]
    fn box_drawing_vertical_stroke_alignment() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        // helper: find which absolute columns have visible pixels
        let visible_cols = |glyph: &RasterizedGlyph| -> Vec<usize> {
            let w = glyph.width as usize;
            let mut cols = vec![false; w];
            for (i, px) in glyph.pixels.chunks(4).enumerate() {
                if px[3] > 0 {
                    cols[i % w] = true;
                }
            }
            cols.iter()
                .enumerate()
                .filter(|(_, has)| **has)
                .map(|(x, _)| x)
                .collect()
        };

        let vert_double = visible_cols(
            &rasterizer
                .rasterize("║", FontStyle::Normal)
                .unwrap(),
        );
        let vert_left = visible_cols(
            &rasterizer
                .rasterize("╢", FontStyle::Normal)
                .unwrap(),
        );
        let corner_br = visible_cols(
            &rasterizer
                .rasterize("╝", FontStyle::Normal)
                .unwrap(),
        );

        // ╢ and ╝ extend further left (horizontal lines), but their
        // rightmost columns (the vertical strokes) must overlap with ║
        for col in &vert_double {
            assert!(
                vert_left.contains(col),
                "╢ must include ║'s column {col}; ║={vert_double:?}, ╢={vert_left:?}"
            );
            assert!(
                corner_br.contains(col),
                "╝ must include ║'s column {col}; ║={vert_double:?}, ╝={corner_br:?}"
            );
        }
    }

    /// Helper: find rows with visible pixels at a specific column range.
    fn stroke_rows_in_col_range(glyph: &RasterizedGlyph, col_start: u32, col_end: u32) -> Vec<u32> {
        let w = glyph.width;
        let h = glyph.height;
        let mut rows = Vec::new();
        for y in 0..h {
            let has_pixel_in_range = (col_start..col_end).any(|x| {
                let idx = ((y * w + x) * 4 + 3) as usize;
                idx < glyph.pixels.len() && glyph.pixels[idx] > 0
            });
            if has_pixel_in_range {
                rows.push(y);
            }
        }
        rows
    }

    /// Box-drawing characters that share horizontal strokes (e.g. ═ and ╝)
    /// must have those strokes at the same rows within the padded cell.
    #[test]
    fn box_drawing_horizontal_stroke_alignment() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        for size in [10.0_f32, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 20.0, 24.0] {
            rasterizer.update_font_size(size).unwrap();

            let eq_glyph = rasterizer
                .rasterize("═", FontStyle::Normal)
                .unwrap();
            let corner_glyph = rasterizer
                .rasterize("╝", FontStyle::Normal)
                .unwrap();

            // find rows with visible pixels at column 0 (where both
            // characters should have horizontal strokes)
            let eq_rows = stroke_rows_in_col_range(&eq_glyph, 0, 1);
            let corner_rows = stroke_rows_in_col_range(&corner_glyph, 0, 1);

            // every row that has a horizontal stroke in ═ must also
            // have a horizontal stroke in ╝ at the connecting edge
            for row in &eq_rows {
                assert!(
                    corner_rows.contains(row),
                    "size={size}: ═ has stroke at row {row} but ╝ does not; \
                     ═ rows={eq_rows:?}, ╝ rows={corner_rows:?}"
                );
            }
        }
    }

    /// Adjacent full-block characters must connect without gaps. The rendered
    /// █ must have strong alpha (≥128) at all four edges of the content area,
    /// not just non-zero. Anti-aliased fringes with low alpha still look like
    /// gaps when adjacent cells both have weak edges.
    #[test]
    fn full_block_fills_cell_edges() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING as u32;
        let min_alpha: u8 = 128;

        for size in [10.0_f32, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 18.0, 20.0, 24.0] {
            rasterizer.update_font_size(size).unwrap();

            let cs = rasterizer.cell_size();
            let (cell_w, cell_h) = (cs.width as u32, cs.height as u32);

            let glyph = rasterizer
                .rasterize("\u{2588}", FontStyle::Normal)
                .unwrap();

            let w = glyph.width;

            // max alpha in a column within content rows
            let col_max_alpha = |col: u32| -> u8 {
                (padding..padding + cell_h)
                    .map(|row| {
                        let idx = ((row * w + col) * 4 + 3) as usize;
                        if idx < glyph.pixels.len() { glyph.pixels[idx] } else { 0 }
                    })
                    .max()
                    .unwrap_or(0)
            };

            // max alpha in a row within content columns
            let row_max_alpha = |row: u32| -> u8 {
                (padding..padding + cell_w)
                    .map(|col| {
                        let idx = ((row * w + col) * 4 + 3) as usize;
                        if idx < glyph.pixels.len() { glyph.pixels[idx] } else { 0 }
                    })
                    .max()
                    .unwrap_or(0)
            };

            let left_col = padding;
            let right_col = padding + cell_w - 1;
            let top_row = padding;
            let bottom_row = padding + cell_h - 1;

            let left_a = col_max_alpha(left_col);
            let right_a = col_max_alpha(right_col);
            let top_a = row_max_alpha(top_row);
            let bottom_a = row_max_alpha(bottom_row);

            eprintln!(
                "size={size}: cell={cell_w}x{cell_h}, edge alpha: L={left_a} R={right_a} T={top_a} B={bottom_a}"
            );

            assert!(
                left_a >= min_alpha,
                "size={size}: █ left edge alpha={left_a} < {min_alpha}, \
                 adjacent cells would have a visible left gap"
            );
            assert!(
                right_a >= min_alpha,
                "size={size}: █ right edge alpha={right_a} < {min_alpha}, \
                 adjacent cells would have a visible right gap"
            );
            assert!(
                top_a >= min_alpha,
                "size={size}: █ top edge alpha={top_a} < {min_alpha}, \
                 adjacent cells would have a visible top gap"
            );
            assert!(
                bottom_a >= min_alpha,
                "size={size}: █ bottom edge alpha={bottom_a} < {min_alpha}, \
                 adjacent cells would have a visible bottom gap"
            );
        }
    }

    /// Box-drawing horizontal lines (═, ─, etc.) must connect at cell
    /// boundaries. The edge-boost ensures left/right edge pixels have
    /// alpha=255 after rendering.
    #[test]
    fn box_drawing_horizontal_edges_connect() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING as u32;
        let min_alpha: u8 = 128;

        let glyphs =
            [("─", "light horizontal"), ("═", "double horizontal"), ("━", "heavy horizontal")];

        for size in [10.0_f32, 14.0, 16.0, 20.0, 24.0] {
            rasterizer.update_font_size(size).unwrap();
            let cs = rasterizer.cell_size();
            let (cell_w, cell_h) = (cs.width as u32, cs.height as u32);

            for &(ch, name) in &glyphs {
                let glyph = rasterizer
                    .rasterize(ch, FontStyle::Normal)
                    .unwrap();
                let w = glyph.width;

                let left_col = padding;
                let right_col = padding + cell_w - 1;

                let left_max = (padding..padding + cell_h)
                    .map(|row| {
                        let idx = ((row * w + left_col) * 4 + 3) as usize;
                        if idx < glyph.pixels.len() { glyph.pixels[idx] } else { 0 }
                    })
                    .max()
                    .unwrap_or(0);

                let right_max = (padding..padding + cell_h)
                    .map(|row| {
                        let idx = ((row * w + right_col) * 4 + 3) as usize;
                        if idx < glyph.pixels.len() { glyph.pixels[idx] } else { 0 }
                    })
                    .max()
                    .unwrap_or(0);

                assert!(
                    left_max >= min_alpha,
                    "size={size}: {name} ({ch}) left edge alpha={left_max} < {min_alpha}"
                );
                assert!(
                    right_max >= min_alpha,
                    "size={size}: {name} ({ch}) right edge alpha={right_max} < {min_alpha}"
                );
            }
        }
    }

    #[test]
    fn fallback_font_size_scaling() {
        use crate::metrics::compute_fallback_font_size;

        let Some(rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        // the primary font at its own size should produce scale ~1.0
        let scaled = rasterizer
            .font_resolver
            .with_primary_font(|primary_ref| {
                compute_fallback_font_size(
                    &rasterizer.cell_metrics,
                    primary_ref,
                    rasterizer.font_size,
                )
            })
            .expect("primary font should be available");

        let ratio = scaled / rasterizer.font_size;
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "primary font should scale to ~1.0, got {ratio:.4}"
        );
    }
}
