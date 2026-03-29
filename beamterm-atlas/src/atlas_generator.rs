use std::{collections::HashSet, ops::RangeInclusive};

use beamterm_data::{DebugSpacePattern, FontAtlasData, FontStyle, Glyph, LineDecoration};
use beamterm_rasterizer::{NativeRasterizer, RasterizedGlyph};
use color_eyre::Report;
use tracing::{debug, info};
use unicode_width::UnicodeWidthStr;

use crate::{
    bitmap_font::BitmapFont,
    coordinate::{AtlasCoordinate, AtlasCoordinateProvider},
    glyph_bounds::{GlyphBounds, measure_glyph_bounds},
    grapheme::GraphemeSet,
    raster_config::RasterizationConfig,
};

/// A glyph that failed to render in a specific font style.
#[derive(Debug, Clone)]
pub struct MissingGlyph {
    /// The character or grapheme that could not be rendered.
    pub symbol: String,
    /// The font style that was attempted.
    pub style: FontStyle,
}

/// Report of missing glyphs from font coverage analysis.
#[derive(Debug)]
pub struct MissingGlyphReport {
    /// List of glyphs that could not be rendered.
    pub missing_glyphs: Vec<MissingGlyph>,
    /// Total number of glyphs checked (excluding emoji).
    pub total_checked: usize,
    /// Name of the font family that was analyzed.
    pub font_family_name: String,
}

/// A glyph that was rendered using a fallback font instead of the requested font.
#[derive(Debug, Clone)]
pub struct FallbackGlyph {
    /// The character or grapheme that used a fallback font.
    pub symbol: String,
    /// The font style that was requested.
    pub style: FontStyle,
    /// The name of the fallback font that was used.
    pub fallback_font_name: String,
}

/// Measured dimensions of a font's reference glyph (█).
#[derive(Debug, Clone, Copy)]
pub struct FontDimensions {
    /// Width of the reference glyph in pixels.
    pub width: i32,
    /// Height of the reference glyph in pixels.
    pub height: i32,
}

/// Statistics about glyphs that used fallback fonts during atlas generation.
#[derive(Debug, Default)]
pub struct FallbackGlyphStats {
    /// Glyphs that were rendered using a fallback font.
    pub fallback_glyphs: Vec<FallbackGlyph>,
    /// Total number of glyphs processed.
    pub total_glyphs: usize,
    /// Dimensions of the primary font's reference glyph.
    pub primary_font_dimensions: Option<FontDimensions>,
    /// Dimensions of each fallback font's reference glyph, keyed by font name.
    pub fallback_font_dimensions: Vec<(String, FontDimensions)>,
}

/// A rasterized glyph with pixel data and bounding box information.
pub struct GlyphBitmap {
    /// The rasterized glyph from beamterm-rasterizer.
    pub glyph: RasterizedGlyph,
    /// The bounding box of the rendered glyph (content area, excluding padding).
    pub bounds: GlyphBounds,
}

impl GlyphBitmap {
    /// Splits double-width glyph pixels into left half.
    fn split_left(source: &GlyphBitmap, cell_w: i32) -> Self {
        let padding = FontAtlasData::PADDING;
        let dst_w = (cell_w + 2 * padding) as u32;
        let dst_h = source.glyph.height;
        let src_w = source.glyph.width;
        let mut pixels = vec![0u8; (dst_w * dst_h * 4) as usize];

        for y in 0..dst_h {
            for x in 0..dst_w.min(src_w) {
                let src_idx = ((y * src_w + x) * 4) as usize;
                let dst_idx = ((y * dst_w + x) * 4) as usize;
                if src_idx + 4 <= source.glyph.pixels.len() && dst_idx + 4 <= pixels.len() {
                    pixels[dst_idx..dst_idx + 4]
                        .copy_from_slice(&source.glyph.pixels[src_idx..src_idx + 4]);
                }
            }
        }

        let bounds = GlyphBounds {
            min_x: 0,
            max_x: cell_w - 1,
            min_y: source.bounds.min_y,
            max_y: source.bounds.max_y,
        };

        Self {
            glyph: RasterizedGlyph {
                pixels,
                width: dst_w,
                height: dst_h,
                is_double_width: false,
                is_fallback: source.glyph.is_fallback,
                fallback_font_name: source.glyph.fallback_font_name.clone(),
            },
            bounds,
        }
    }

    /// Splits double-width glyph pixels into right half.
    fn split_right(source: &GlyphBitmap, cell_w: i32) -> Self {
        let padding = FontAtlasData::PADDING;
        let dst_w = (cell_w + 2 * padding) as u32;
        let dst_h = source.glyph.height;
        let src_w = source.glyph.width;
        let src_offset = cell_w as u32; // start of right half in source
        let mut pixels = vec![0u8; (dst_w * dst_h * 4) as usize];

        for y in 0..dst_h {
            for dst_x in 0..dst_w {
                let src_x = src_offset + dst_x;
                if src_x < src_w {
                    let src_idx = ((y * src_w + src_x) * 4) as usize;
                    let dst_idx = ((y * dst_w + dst_x) * 4) as usize;
                    if src_idx + 4 <= source.glyph.pixels.len() && dst_idx + 4 <= pixels.len() {
                        pixels[dst_idx..dst_idx + 4]
                            .copy_from_slice(&source.glyph.pixels[src_idx..src_idx + 4]);
                    }
                }
            }
        }

        let bounds = GlyphBounds {
            min_x: 0,
            max_x: cell_w - 1,
            min_y: source.bounds.min_y,
            max_y: source.bounds.max_y,
        };

        Self {
            glyph: RasterizedGlyph {
                pixels,
                width: dst_w,
                height: dst_h,
                is_double_width: false,
                is_fallback: source.glyph.is_fallback,
                fallback_font_name: source.glyph.fallback_font_name.clone(),
            },
            bounds,
        }
    }
}

/// Generator for creating GPU-optimized bitmap font atlases from TrueType/OpenType fonts.
///
/// This is the main entry point for font atlas generation. It manages font loading,
/// glyph rasterization, and texture packing for efficient GPU rendering.
pub struct AtlasFontGenerator {
    rasterizer: NativeRasterizer,
    font_size: f32,
    line_height: f32,
    underline: LineDecoration,
    strikethrough: LineDecoration,
    font_family_name: String,
    debug_space_pattern: Option<DebugSpacePattern>,
}

impl AtlasFontGenerator {
    /// Measures the dimensions of a font by rasterizing the full block character (█).
    /// Returns None if the glyph produces no visible pixels.
    fn measure_font_dimensions_for(rasterizer: &mut NativeRasterizer) -> Option<FontDimensions> {
        let glyph = rasterizer
            .rasterize("\u{2588}", FontStyle::Normal)
            .ok()?;

        let bounds = measure_glyph_bounds(&glyph);
        if bounds.width() <= 0 || bounds.height() <= 0 {
            return None;
        }

        Some(FontDimensions { width: bounds.width(), height: bounds.height() })
    }

    /// Creates a new atlas font generator with the specified font family and rendering parameters.
    ///
    /// Note: `line_height` is applied in `calculate_optimized_cell_dimensions()`
    /// after the optimal font size is determined.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified font family cannot be found or loaded.
    pub fn new_with_family(
        font_family_name: String,
        emoji_font_family_name: &str,
        font_size: f32,
        line_height: f32,
        underline: LineDecoration,
        strikethrough: LineDecoration,
        debug_space_pattern: Option<DebugSpacePattern>,
    ) -> Result<Self, Report> {
        info!(
            font_family = %font_family_name,
            font_size = font_size,
            line_height = line_height,
            "Creating bitmap font generator"
        );

        let rasterizer =
            NativeRasterizer::new(&[&font_family_name, emoji_font_family_name], font_size)?;

        Ok(Self {
            rasterizer,
            font_size,
            line_height,
            underline,
            strikethrough,
            font_family_name,
            debug_space_pattern,
        })
    }

    /// Generates a complete bitmap font atlas from Unicode ranges and emoji.
    ///
    /// # Errors
    ///
    /// Returns an error if grapheme categorization or glyph rasterization fails.
    pub fn generate(
        &mut self,
        unicode_ranges: &[RangeInclusive<char>],
        other_symbols: &str,
    ) -> Result<(BitmapFont, FallbackGlyphStats), Report> {
        info!(
            font_family = %self.font_family_name,
            "Starting font generation"
        );

        // calculate texture dimensions using optimized cell dimensions
        let bounds = self.calculate_optimized_cell_dimensions();

        // categorize and allocate IDs
        let grapheme_set = GraphemeSet::new(unicode_ranges, other_symbols)?;
        let halfwidth_glyphs_per_layer = grapheme_set.halfwidth_glyphs_count();
        let glyphs = grapheme_set.into_glyphs(bounds);

        debug!(glyph_count = glyphs.len(), "Generated glyph set");

        let config = RasterizationConfig::new(bounds, &glyphs);
        info!(
            bounds = ?bounds,
            texture_width = config.texture_width,
            texture_height = config.texture_height,
            texture_layers = config.layers,
            "Atlas configuration calculated"
        );

        // allocate 3d rgba texture data
        let mut texture_data = vec![0u32; config.texture_size()];

        // rasterize glyphs and copy into texture, collecting fallback stats
        let mut fallback_stats =
            FallbackGlyphStats { total_glyphs: glyphs.len(), ..Default::default() };

        for glyph in &glyphs {
            if let Some(fallback) =
                self.place_glyph_in_3d_texture(glyph, &config, &mut texture_data)
            {
                fallback_stats.fallback_glyphs.push(fallback);
            }
        }

        // Measure font dimensions for primary and fallback fonts
        if !fallback_stats.fallback_glyphs.is_empty() {
            fallback_stats.primary_font_dimensions =
                Self::measure_font_dimensions_for(&mut self.rasterizer);

            // Collect unique fallback font names
            let unique_fallback_fonts: HashSet<_> = fallback_stats
                .fallback_glyphs
                .iter()
                .map(|g| g.fallback_font_name.clone())
                .collect();

            for font_name in unique_fallback_fonts {
                // Create a temporary rasterizer for the fallback font to measure its dimensions
                if let Ok(mut fallback_rasterizer) =
                    NativeRasterizer::new(&[&font_name], self.font_size)
                {
                    if let Some(dimensions) =
                        Self::measure_font_dimensions_for(&mut fallback_rasterizer)
                    {
                        fallback_stats
                            .fallback_font_dimensions
                            .push((font_name, dimensions));
                    } else {
                        info!(
                            font = font_name,
                            "Skipping dimension measurement - font lacks reference glyph (█)"
                        );
                    }
                }
            }

            // Sort by font name for consistent output
            fallback_stats
                .fallback_font_dimensions
                .sort_by(|a, b| a.0.cmp(&b.0));
        }

        let texture_data = texture_data
            .iter()
            .flat_map(|&color| color.to_be_bytes())
            .collect::<Vec<u8>>();

        // Nudge strikethrough and underline positions to nearest 0.5 pixel for perfect centering
        let cell_height = bounds.height() as f32;
        let nudged_underline = Self::nudge_decoration_to_half_pixel(self.underline, cell_height);
        let nudged_strikethrough =
            Self::nudge_decoration_to_half_pixel(self.strikethrough, cell_height);

        // drop right half of double-width glyphs (emoji and fullwidth; not needed for atlas)
        let glyphs: Vec<_> = glyphs
            .into_iter()
            .filter(|g| {
                let is_fullwidth = g.symbol().width() == 2;
                let is_double_width = g.is_emoji() || is_fullwidth;
                !is_double_width || g.id() & 1 == 0 // keep left half only
            })
            .collect();

        println!("Position Summary:");
        println!("  Cell height: {cell_height}");
        println!(
            "  Underline - Provided: {:.4} ({:.1}px) -> Actual: {:.4} ({:.1}px)",
            self.underline.position(),
            cell_height * self.underline.position(),
            nudged_underline.position(),
            cell_height * nudged_underline.position()
        );
        println!(
            "  Strikethrough - Provided: {:.4} ({:.1}px) -> Actual: {:.4} ({:.1}px)",
            self.strikethrough.position(),
            cell_height * self.strikethrough.position(),
            nudged_strikethrough.position(),
            cell_height * nudged_strikethrough.position()
        );

        info!(
            font_family = %self.font_family_name,
            glyph_count = glyphs.len(),
            texture_size_bytes = texture_data.len(),
            cell_height = cell_height,
            underline_provided_pos = self.underline.position(),
            underline_provided_pixel = cell_height * self.underline.position(),
            underline_actual_pos = nudged_underline.position(),
            underline_actual_pixel = cell_height * nudged_underline.position(),
            strikethrough_provided_pos = self.strikethrough.position(),
            strikethrough_provided_pixel = cell_height * self.strikethrough.position(),
            strikethrough_actual_pos = nudged_strikethrough.position(),
            strikethrough_actual_pixel = cell_height * nudged_strikethrough.position(),
            "Font generation completed successfully"
        );

        Ok((
            BitmapFont {
                atlas_data: FontAtlasData::new(
                    self.font_family_name.clone().into(),
                    self.font_size,
                    halfwidth_glyphs_per_layer,
                    (config.texture_width, config.texture_height, config.layers),
                    config.padded_cell_size(),
                    nudged_underline,
                    nudged_strikethrough,
                    glyphs,
                    texture_data,
                ),
            },
            fallback_stats,
        ))
    }

    /// Rasterizes a glyph and writes its pixels into the 3D texture at the computed atlas position.
    /// For emoji and fullwidth glyphs, splits the double-width rendering into left and right halves
    /// placed in consecutive cells.
    ///
    /// Returns `Some(FallbackGlyph)` if the glyph was rendered using a fallback font.
    fn place_glyph_in_3d_texture(
        &mut self,
        glyph: &Glyph,
        config: &RasterizationConfig,
        texture: &mut [u32],
    ) -> Option<FallbackGlyph> {
        let is_fullwidth = glyph.symbol().width() == 2;

        debug!(
            symbol = %glyph.symbol(),
            style = ?glyph.style(),
            glyph_id = format_args!("0x{:04X}", glyph.id()),
            is_emoji = glyph.is_emoji(),
            is_fullwidth = is_fullwidth,
            "Rasterizing glyph"
        );

        let bitmap = if glyph.is_emoji() || is_fullwidth {
            // Render double-width glyph at 2× width and split into left/right halves
            let full_bitmap = self.rasterize_symbol(
                glyph.symbol(),
                glyph.style(),
                config.double_width_glyph_bounds(),
            );
            let cell_w = config.glyph_bounds().width();

            let half_bitmap = if glyph.id() & 1 == 0 {
                GlyphBitmap::split_left(&full_bitmap, cell_w)
            } else {
                GlyphBitmap::split_right(&full_bitmap, cell_w)
            };

            self.render_pixels_to_texture(&half_bitmap, glyph.atlas_coordinate(), config, texture);

            full_bitmap
        } else {
            // Normal glyph rendering
            let bitmap =
                self.rasterize_symbol(glyph.symbol(), glyph.style(), config.glyph_bounds());

            self.render_pixels_to_texture(&bitmap, glyph.atlas_coordinate(), config, texture);

            bitmap
        };

        // Check if this glyph used a fallback font (skip emoji - they use a separate font)
        if !glyph.is_emoji() && bitmap.glyph.is_fallback {
            Some(FallbackGlyph {
                symbol: glyph.symbol().to_string(),
                style: glyph.style(),
                fallback_font_name: bitmap
                    .glyph
                    .fallback_font_name
                    .unwrap_or_else(|| "Unknown".to_string()),
            })
        } else {
            None
        }
    }

    /// Adjusts decoration position to the nearest half-pixel boundary for crisp rendering.
    fn nudge_decoration_to_half_pixel(
        decoration: LineDecoration,
        cell_height: f32,
    ) -> LineDecoration {
        let pixel_pos = cell_height * decoration.position();
        let nudged_pixel = (pixel_pos - 0.5).round() + 0.5;
        let nudged_position = nudged_pixel / cell_height;
        LineDecoration::new(nudged_position, decoration.thickness())
    }

    /// Rasterizes a single symbol with the specified style into a bitmap.
    pub fn rasterize_symbol(
        &mut self,
        symbol: &str,
        style: FontStyle,
        bounds: GlyphBounds,
    ) -> GlyphBitmap {
        // Check for debug space pattern - must return early since space has no pixels
        if symbol == " "
            && let Some(pattern) = self.debug_space_pattern
        {
            return Self::generate_checkered_bitmap(bounds, pattern);
        }

        let rasterized = self
            .rasterizer
            .rasterize(symbol, style)
            .unwrap_or_else(|_| {
                // Return empty glyph on rasterization failure
                let padding = FontAtlasData::PADDING;
                let w = (bounds.width() + 2 * padding) as u32;
                let h = (bounds.height() + 2 * padding) as u32;
                RasterizedGlyph::new(vec![0u8; (w * h * 4) as usize], w, h)
            });

        GlyphBitmap { glyph: rasterized, bounds }
    }

    /// Generates a checkered bitmap to validate pixel-perfect rendering of cell dimensions.
    fn generate_checkered_bitmap(bounds: GlyphBounds, pattern: DebugSpacePattern) -> GlyphBitmap {
        let width = bounds.width();
        let height = bounds.height();
        let padding = FontAtlasData::PADDING;
        let padded_w = (width + 2 * padding) as u32;
        let padded_h = (height + 2 * padding) as u32;
        let mut pixels = vec![0u8; (padded_w * padded_h * 4) as usize];

        for y in 0..height {
            for x in 0..width {
                let is_white = match pattern {
                    DebugSpacePattern::OnePixel => (x + y) % 2 == 0,
                    DebugSpacePattern::TwoByTwo => ((x / 2) + (y / 2)) % 2 == 0,
                };
                if is_white {
                    let px = (x + padding) as u32;
                    let py = (y + padding) as u32;
                    let idx = ((py * padded_w + px) * 4) as usize;
                    if idx + 4 <= pixels.len() {
                        pixels[idx] = 0xff;
                        pixels[idx + 1] = 0xff;
                        pixels[idx + 2] = 0xff;
                        pixels[idx + 3] = 0xff;
                    }
                }
            }
        }

        GlyphBitmap {
            glyph: RasterizedGlyph::new(pixels, padded_w, padded_h),
            bounds,
        }
    }

    /// Writes pixel data from a GlyphBitmap into the 3D texture array.
    fn render_pixels_to_texture(
        &self,
        bitmap: &GlyphBitmap,
        coord: AtlasCoordinate,
        config: &RasterizationConfig,
        texture: &mut [u32],
    ) {
        let cell_offset = coord.cell_offset_in_px(config.glyph_bounds());
        let cell_size = config.padded_cell_size();
        let src_w = bitmap.glyph.width as i32;

        for (i, px) in bitmap.glyph.pixels.chunks(4).enumerate() {
            if px[3] == 0 {
                continue;
            }

            let src_x = (i as i32) % src_w;
            let src_y = (i as i32) / src_w;

            // Map pixel to texture coordinates
            let tx = src_x + cell_offset.0;
            let ty = src_y + cell_offset.1;

            // x is clamped to cell width (prevents bleeding into adjacent cells),
            // y is clamped to texture height (cells are stacked vertically in the texture)
            if tx >= 0 && tx < cell_size.width && ty >= 0 && ty < config.texture_height {
                let idx = self.texture_index(tx, ty, coord.layer as i32, config);

                if idx < texture.len() {
                    let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
                    texture[idx] = r << 24 | g << 16 | b << 8 | a;
                }
            }
        }
    }

    /// Calculates the linear texture index for a 3D coordinate (x, y, layer).
    #[allow(clippy::unused_self)] // method on AtlasFontGenerator for coherence
    fn texture_index(&self, x: i32, y: i32, slice: i32, config: &RasterizationConfig) -> usize {
        (slice * config.texture_width * config.texture_height + y * config.texture_width + x)
            as usize
    }

    /// Calculates cell dimensions by rendering █ at the current font size.
    ///
    /// Since █ is synthesized programmatically (not rendered from the font),
    /// its edges are always pixel-perfect — no font size optimization needed.
    ///
    /// # Panics
    ///
    /// Panics if the reference glyph (█) fails to rasterize.
    pub fn calculate_optimized_cell_dimensions(&mut self) -> GlyphBounds {
        let glyph = self
            .rasterizer
            .rasterize("\u{2588}", FontStyle::Normal)
            .expect("reference glyph to rasterize");

        let mut bounds = measure_glyph_bounds(&glyph);

        // Apply line height multiplier to cell height
        if self.line_height > 1.0 {
            let extra = ((bounds.height() as f32 * (self.line_height - 1.0)).round()) as i32;
            bounds.max_y += extra;
            info!(
                line_height = self.line_height,
                extra_pixels = extra,
                "Applied line height scaling"
            );
        }

        info!(
            font_size = self.font_size,
            ?bounds,
            "Cell dimensions calculated"
        );

        bounds
    }

    /// Checks which glyphs are missing from the font by attempting to rasterize them.
    ///
    /// # Errors
    ///
    /// Returns an error if grapheme categorization fails.
    pub fn check_missing_glyphs(
        &mut self,
        ranges: &[RangeInclusive<char>],
        additional_symbols: &str,
    ) -> Result<MissingGlyphReport, Report> {
        let bounds = self.calculate_optimized_cell_dimensions();

        let grapheme_set = GraphemeSet::new(ranges, additional_symbols)?;
        let glyphs = grapheme_set.into_glyphs(bounds);

        let mut missing_glyphs = Vec::new();
        let mut total_checked = 0;

        for glyph in &glyphs {
            total_checked += 1;

            if is_empty_character(glyph.symbol()) {
                continue;
            }

            let rasterized = self.rasterize_symbol(glyph.symbol(), glyph.style(), bounds);
            let has_pixels = rasterized
                .glyph
                .pixels
                .chunks(4)
                .any(|px| px[3] > 0);

            if !has_pixels {
                debug!(
                    symbol = %glyph.symbol(),
                    style = ?glyph.style(),
                    "Glyph not supported - rasterization produced no pixels"
                );
                missing_glyphs.push(MissingGlyph {
                    symbol: glyph.symbol().to_string(),
                    style: glyph.style(),
                });
            }
        }

        Ok(MissingGlyphReport {
            missing_glyphs,
            total_checked,
            font_family_name: self.font_family_name.clone(),
        })
    }
}

/// Returns true if the string is a single Unicode space or whitespace character.
#[rustfmt::skip]
fn is_empty_character(s: &str) -> bool {
    if let Some(ch) = s.chars().next() {
        s.chars().count() == 1
            && matches!(ch,
                '\u{0020}' |  // SPACE
                '\u{00A0}' |  // NO-BREAK SPACE
                '\u{00AD}' |  // SOFT HYPHEN
                '\u{1680}' |  // OGHAM SPACE MARK
                '\u{2000}' |  // EN QUAD
                '\u{2001}' |  // EM QUAD
                '\u{2002}' |  // EN SPACE
                '\u{2003}' |  // EM SPACE
                '\u{2004}' |  // THREE-PER-EM SPACE
                '\u{2005}' |  // FOUR-PER-EM SPACE
                '\u{2006}' |  // SIX-PER-EM SPACE
                '\u{2007}' |  // FIGURE SPACE
                '\u{2008}' |  // PUNCTUATION SPACE
                '\u{2009}' |  // THIN SPACE
                '\u{200A}' |  // HAIR SPACE
                '\u{200B}' |  // ZERO WIDTH SPACE
                '\u{202F}' |  // NARROW NO-BREAK SPACE
                '\u{205F}' |  // MEDIUM MATHEMATICAL SPACE
                '\u{2800}' |  // BRAILLE PATTERN BLANK
                '\u{3000}'    // IDEOGRAPHIC SPACE
            )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use beamterm_rasterizer::FontDiscovery;

    use super::*;

    #[test]
    fn test_space_character_detection() {
        assert!(is_empty_character(" "));
        assert!(is_empty_character("\u{00A0}"));
        assert!(is_empty_character("\u{2003}"));
        assert!(is_empty_character("\u{200B}"));
        assert!(is_empty_character("\u{3000}"));

        assert!(!is_empty_character("A"));
        assert!(!is_empty_character("0"));
        assert!(!is_empty_character("█"));
        assert!(!is_empty_character(""));
        assert!(!is_empty_character("AB"));
    }

    #[test]
    fn test_missing_glyph_detection() {
        let discovery = FontDiscovery::new();
        let available_fonts = discovery.discover_complete_monospace_families();

        if available_fonts.is_empty() {
            println!("No fonts available for testing");
            return;
        }

        let font_family = &available_fonts[0];

        let mut generator = AtlasFontGenerator::new_with_family(
            font_family.name.clone(),
            "Noto Color Emoji",
            15.0,
            1.0,
            LineDecoration::new(0.85, 0.05),
            LineDecoration::new(0.5, 0.05),
            None,
        )
        .expect("Failed to create generator");

        let test_ranges = vec!['\u{0100}'..='\u{0105}'];
        let report = generator
            .check_missing_glyphs(&test_ranges, "")
            .unwrap();

        assert_eq!(report.font_family_name, font_family.name);
        assert_eq!(report.total_checked, 404);

        let coverage_percent = ((report.total_checked - report.missing_glyphs.len()) as f64
            / report.total_checked as f64)
            * 100.0;

        assert!(
            coverage_percent > 95.0,
            "Font coverage should be above 95%, got {coverage_percent:.1}%",
        );
    }
}
