use std::collections::HashMap;

use beamterm_data::{FontAtlasData, FontStyle, LineDecoration};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::scale::image::Content;
use unicode_width::UnicodeWidthStr;

use crate::error::Error;
use crate::font_fallback::FontResolver;
use crate::metrics::{CellMetrics, compute_fallback_font_size, measure_cell_metrics};

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
}

impl RasterizedGlyph {
    pub fn new(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self { pixels, width, height, is_double_width: false }
    }

    pub fn new_wide(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self { pixels, width, height, is_double_width: true }
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
        let cell_metrics = measure_cell_metrics(
            font_resolver.primary_font(),
            font_size,
            &mut scale_context,
        )?;

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
        let padding = FontAtlasData::PADDING;
        let cell_w = self.cell_metrics.width;

        // get the first codepoint for font resolution
        let ch = match grapheme.chars().next() {
            Some(c) => c,
            None => {
                let pw = (cell_w + padding * 2) as u32;
                let ph = (self.cell_metrics.height + padding * 2) as u32;
                return Ok(RasterizedGlyph::new(vec![0u8; (pw * ph * 4) as usize], pw, ph));
            }
        };

        // resolve font and glyph ID
        let is_emoji = is_emoji_grapheme(grapheme);
        let primary_count = self.font_resolver.primary_count();
        let (font_ref, font_idx) = if is_emoji {
            match self.font_resolver.resolve_char(ch) {
                Some(r) => r,
                None => {
                    let pw = (cell_w + padding * 2) as u32;
                    let ph = (self.cell_metrics.height + padding * 2) as u32;
                    return Ok(RasterizedGlyph::new(vec![0u8; (pw * ph * 4) as usize], pw, ph));
                }
            }
        } else {
            match self.font_resolver.resolve_styled(ch, style) {
                Some(r) => r,
                None => {
                    let pw = (cell_w + padding * 2) as u32;
                    let ph = (self.cell_metrics.height + padding * 2) as u32;
                    return Ok(RasterizedGlyph::new(vec![0u8; (pw * ph * 4) as usize], pw, ph));
                }
            }
        };

        let glyph_id = font_ref.charmap().map(ch);
        if glyph_id == 0 {
            let pw = (cell_w + padding * 2) as u32;
            let ph = (self.cell_metrics.height + padding * 2) as u32;
            return Ok(RasterizedGlyph::new(vec![0u8; (pw * ph * 4) as usize], pw, ph));
        }

        // determine double-width from unicode properties OR from the font's
        // own advance width. PUA glyphs (e.g. Nerd Font icons) often have
        // advance widths > 1 cell but unicode-width returns 1.
        let is_double_width = if is_wide(grapheme) {
            true
        } else {
            let glyph_metrics = font_ref.glyph_metrics(&[]).scale(self.font_size);
            let advance = glyph_metrics.advance_width(glyph_id);
            advance > cell_w as f32 * 1.5
        };

        let content_w = if is_double_width { cell_w * 2 } else { cell_w };
        let content_h = self.cell_metrics.height;
        let padded_w = (content_w + padding * 2) as u32;
        let padded_h = (content_h + padding * 2) as u32;

        let mut pixels = vec![0u8; (padded_w * padded_h * 4) as usize];

        // scale fallback fonts to fit within the primary font's cell
        let is_primary_font = font_idx < primary_count;
        let effective_size = if is_primary_font {
            self.font_size
        } else {
            // get or compute the per-font base scale (from █ refinement)
            let base_size = if let Some(&cached) = self.fallback_sizes.get(&font_idx) {
                cached
            } else {
                let size = Self::refine_fallback_size(
                    &self.cell_metrics,
                    font_ref,
                    self.font_size,
                    &mut self.scale_context,
                );
                self.fallback_sizes.insert(font_idx, size);
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
                if w_scale > 1.05 {
                    base_size * w_scale
                } else {
                    base_size
                }
            } else {
                base_size
            }
        };

        // rasterize
        let mut scaler = self.scale_context
            .builder(font_ref)
            .size(effective_size)
            .hint(true)
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

        // always use the primary font's ascent for baseline placement,
        // so all glyphs align to the same baseline regardless of font
        let ascent = self.cell_metrics.ascent.round() as i32;

        // Horizontal placement: center the glyph image within the cell's
        // content area. This handles:
        // - Negative bearings (powerline glyphs): prevents left-bleed into
        //   padding that the shader would clip, creating a gap
        // - Narrow fallback glyphs: centers them rather than left-aligning
        //   with a gap on the right
        // - Normal glyphs: centered within the advance width (typical for
        //   monospace fonts where bearing ≈ 0)
        let image_w = image.placement.width as i32;
        let dst_x = padding + (content_w - image_w) / 2;
        let dst_y = padding + (ascent - image.placement.top);

        let src_w = image.placement.width as i32;
        let src_h = image.placement.height as i32;

        let is_color = image.content == Content::Color;
        let bytes_per_src_pixel = if is_color { 4 } else { 1 };
        let src_stride = src_w * bytes_per_src_pixel;
        let dst_stride = padded_w as i32 * 4;

        for row in 0..src_h {
            let sy = row;
            let dy = dst_y + row;
            if dy < 0 || dy >= padded_h as i32 {
                continue;
            }

            for col in 0..src_w {
                let dx = dst_x + col;
                if dx < 0 || dx >= padded_w as i32 {
                    continue;
                }

                let src_idx = (sy * src_stride + col * bytes_per_src_pixel) as usize;
                let dst_idx = (dy * dst_stride + dx * 4) as usize;

                if dst_idx + 3 >= pixels.len() {
                    continue;
                }

                if is_color {
                    // RGBA source
                    if src_idx + 3 < image.data.len() {
                        pixels[dst_idx] = image.data[src_idx];
                        pixels[dst_idx + 1] = image.data[src_idx + 1];
                        pixels[dst_idx + 2] = image.data[src_idx + 2];
                        pixels[dst_idx + 3] = image.data[src_idx + 3];
                    }
                } else {
                    // alpha-only source: white text with alpha
                    if src_idx < image.data.len() {
                        let alpha = image.data[src_idx];
                        if alpha > 0 {
                            pixels[dst_idx] = 0xff;
                            pixels[dst_idx + 1] = 0xff;
                            pixels[dst_idx + 2] = 0xff;
                            pixels[dst_idx + 3] = alpha;
                        }
                    }
                }
            }
        }

        Ok(RasterizedGlyph {
            pixels,
            width: padded_w,
            height: padded_h,
            is_double_width,
        })
    }

    /// Returns the cell size in pixels (without padding).
    pub fn cell_size(&self) -> (i32, i32) {
        (self.cell_metrics.width, self.cell_metrics.height)
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
        if is_wide(grapheme) {
            return true;
        }

        let ch = match grapheme.chars().next() {
            Some(c) => c,
            None => return false,
        };

        // resolve the font for this character
        let (font_ref, _) = match self.font_resolver.resolve_char(ch) {
            Some(r) => r,
            None => return false,
        };

        let glyph_id = font_ref.charmap().map(ch);
        if glyph_id == 0 {
            return false;
        }

        let glyph_metrics = font_ref.glyph_metrics(&[]).scale(self.font_size);
        let advance = glyph_metrics.advance_width(glyph_id);
        advance > self.cell_metrics.width as f32 * 1.5
    }

    /// Updates the font size and re-measures cell metrics.
    pub fn update_font_size(&mut self, font_size: f32) -> Result<(), Error> {
        self.font_size = font_size;
        self.cell_metrics = measure_cell_metrics(
            self.font_resolver.primary_font(),
            font_size,
            &mut self.scale_context,
        )?;
        self.fallback_sizes.clear();
        Ok(())
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

            let image = Render::new(&[Source::Outline])
                .render(&mut scaler, block_id);

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
}

fn is_wide(grapheme: &str) -> bool {
    is_emoji_grapheme(grapheme) || grapheme.width() == 2
}

fn is_emoji_grapheme(s: &str) -> bool {
    match emojis::get(s) {
        Some(emoji) => {
            if emoji.as_str().contains('\u{FE0F}') {
                s.contains('\u{FE0F}')
            } else {
                true
            }
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beamterm_data::FontAtlasData;

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

        let (w, h) = rasterizer.cell_size();
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
        let (cell_w, cell_h) = rasterizer.cell_size();
        let padded_w = (cell_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;

        let glyph = rasterizer.rasterize("A", FontStyle::Normal).unwrap();
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

        let glyph = rasterizer.rasterize(" ", FontStyle::Normal).unwrap();
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
        let (cell_w, _cell_h) = rasterizer.cell_size();
        let single_padded_w = (cell_w + padding * 2) as u32;

        // CJK character (double-width)
        let glyph = rasterizer.rasterize("\u{4E2D}", FontStyle::Normal).unwrap();
        let double_padded_w = (cell_w * 2 + padding * 2) as u32;
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
            (0.0..=1.0).contains(&underline.position),
            "underline position should be 0.0-1.0, got {}",
            underline.position
        );
        assert!(
            underline.thickness > 0.0,
            "underline thickness should be positive"
        );

        assert!(
            (0.0..=1.0).contains(&strikethrough.position),
            "strikethrough position should be 0.0-1.0, got {}",
            strikethrough.position
        );
        assert!(
            strikethrough.thickness > 0.0,
            "strikethrough thickness should be positive"
        );

        // strikethrough should be above underline (lower position value)
        assert!(
            strikethrough.position < underline.position,
            "strikethrough ({}) should be above underline ({})",
            strikethrough.position,
            underline.position
        );
    }

    #[test]
    fn update_font_size_changes_cell_size() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let (w1, h1) = rasterizer.cell_size();

        rasterizer.update_font_size(32.0).unwrap();
        let (w2, h2) = rasterizer.cell_size();

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
        let (cell_w, cell_h) = rasterizer.cell_size();

        let glyph = rasterizer.rasterize(symbol, FontStyle::Normal).unwrap();

        // output size must match the primary cell dimensions (1x or 2x wide)
        let expected_content_w = if glyph.is_double_width { cell_w * 2 } else { cell_w };
        let padded_w = (expected_content_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;
        assert_eq!(glyph.width, padded_w, "fallback glyph width mismatch (double_width={})", glyph.is_double_width);
        assert_eq!(glyph.height, padded_h, "fallback glyph height mismatch");

        // fallback must have produced visible pixels
        let has_pixels = glyph.pixels.chunks(4).any(|px| px[3] > 0);
        assert!(
            has_pixels,
            "fallback glyph '{symbol}' should have visible pixels"
        );

        // verify all visible pixels stay within the padded cell
        let bbox = pixel_bbox(&glyph);
        assert!(bbox.max_x < padded_w, "pixels exceed cell width: {} >= {padded_w}", bbox.max_x);
        assert!(bbox.max_y < padded_h, "pixels exceed cell height: {} >= {padded_h}", bbox.max_y);
    }

    #[test]
    fn full_block_fits_within_cell() {
        let Some(mut rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        let padding = FontAtlasData::PADDING;
        let (cell_w, cell_h) = rasterizer.cell_size();
        let padded_w = (cell_w + padding * 2) as u32;
        let padded_h = (cell_h + padding * 2) as u32;

        let glyph = rasterizer.rasterize("\u{2588}", FontStyle::Normal).unwrap();

        assert_eq!(glyph.width, padded_w);
        assert_eq!(glyph.height, padded_h);

        let bbox = pixel_bbox(&glyph);

        eprintln!("█ cell_size=({cell_w},{cell_h}) padded=({padded_w},{padded_h})");
        eprintln!("  visible bbox: ({},{})-({},{}) = {}x{}",
            bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y, bbox.width(), bbox.height());
        eprintln!("  expected content area: padding={padding}..padding+cell = ({padding},{padding})-({},{})",
            padding + cell_w - 1, padding + cell_h - 1);

        // pixels must stay within the padded cell
        assert!(bbox.max_x < padded_w, "█ exceeds cell width: max_x={} >= {padded_w}", bbox.max_x);
        assert!(bbox.max_y < padded_h, "█ exceeds cell height: max_y={} >= {padded_h}", bbox.max_y);

        // visible area should not exceed cell dimensions (ignoring padding)
        assert!(
            bbox.width() <= cell_w as u32,
            "█ visible width {} exceeds cell width {cell_w}", bbox.width()
        );
        assert!(
            bbox.height() <= cell_h as u32,
            "█ visible height {} exceeds cell height {cell_h}", bbox.height()
        );

        // check various powerline/nerd-font glyphs
        let nerd_glyphs: &[(&str, &str)] = &[
            ("\u{E0B0}", "right triangle"),
            ("\u{E0B2}", "left triangle"),
            ("\u{E0B4}", "right semicircle"),
            ("\u{E0B6}", "left semicircle"),
        ];

        for &(symbol, name) in nerd_glyphs {
            let gl = rasterizer.rasterize(symbol, FontStyle::Normal).unwrap();
            let has_pixels = gl.pixels.chunks(4).any(|px| px[3] > 0);
            if !has_pixels {
                eprintln!("{symbol} ({name}) not available");
                continue;
            }

            let gl_bbox = pixel_bbox(&gl);
            let in_primary = rasterizer.font_resolver.primary_has_char(symbol.chars().next().unwrap());
            let is_wide = super::is_wide(symbol);
            eprintln!("{symbol} ({name}) primary={in_primary} is_wide={is_wide} glyph_size={}x{}",
                gl.width, gl.height);
            eprintln!("  visible bbox: ({},{})-({},{}) = {}x{}",
                gl_bbox.min_x, gl_bbox.min_y, gl_bbox.max_x, gl_bbox.max_y,
                gl_bbox.width(), gl_bbox.height());
            eprintln!("  content area: {cell_w}x{cell_h}, padded: {padded_w}x{padded_h}");

            if gl_bbox.width() > cell_w as u32 {
                eprintln!("  WARNING: glyph {}px wider than cell {}px",
                    gl_bbox.width() - cell_w as u32, cell_w);
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

    #[test]
    fn fallback_font_size_scaling() {
        use crate::metrics::compute_fallback_font_size;

        let Some(rasterizer) = test_rasterizer() else {
            eprintln!("skipping: no monospace font found");
            return;
        };

        // the primary font at its own size should produce scale ~1.0
        let primary_ref = rasterizer.font_resolver.primary_font();
        let scaled = compute_fallback_font_size(
            &rasterizer.cell_metrics,
            primary_ref,
            rasterizer.font_size,
        );

        let ratio = scaled / rasterizer.font_size;
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "primary font should scale to ~1.0, got {ratio:.4}"
        );
    }
}
