use std::{collections::HashSet, ops::RangeInclusive};

use beamterm_data::{FontAtlasData, FontStyle, Glyph, LineDecoration};
use color_eyre::Report;
use compact_str::ToCompactString;
use cosmic_text::{Buffer, Color, FontSystem, Metrics, SwashCache};
use itertools::Itertools;
use tracing::{debug, info};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    bitmap_font::BitmapFont,
    coordinate::{AtlasCoordinate, AtlasCoordinateProvider},
    font_discovery::{FontDiscovery, FontFamily},
    glyph_bounds::{measure_glyph_bounds, GlyphBounds},
    glyph_rasterizer::{create_rasterizer, create_text_attrs},
    grapheme::GraphemeSet,
    raster_config::RasterizationConfig,
};

const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);

/// Classification of a glyph's width based on its rendered dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphWidthInfo {
    /// Glyph fits within a single terminal cell.
    SingleWidth,
    /// Glyph requires two terminal cells (typically emoji or wide characters).
    DoubleWidth,
    /// Glyph cannot be rendered (contains the glyph string).
    Missing(String),
}

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

/// A rasterized glyph with pixel data and bounding box information.
pub struct GlyphBitmap {
    /// Pixel data as (x, y, color) tuples.
    pub data: Vec<(i32, i32, Color)>,
    /// The bounding box of the rendered glyph.
    pub bounds: GlyphBounds,
}

impl GlyphBitmap {
    /// Adds a checkered pattern to empty pixels for debugging visibility.
    #[allow(dead_code)]
    pub fn debug_checkered(self) -> Self {
        let width = self.bounds.width();
        let height = self.bounds.height();

        let existing_pixels: HashSet<_> = self
            .data
            .iter()
            .map(|(x, y, _)| (*x, *y))
            .collect();

        let mut data = self.data;

        (0..width)
            .cartesian_product(0..height)
            .filter(|&(x, y)| !existing_pixels.contains(&(x, y)))
            .for_each(|(x, y)| match (x + y) % 8 {
                0 => data.push((x, y, Color::rgb(0x7F, 0x00, 0x7f))),
                2 => data.push((x, y, Color::rgb(0x00, 0x7f, 0x7f))),
                4 => data.push((x, y, Color::rgb(0x00, 0x7f, 0x00))),
                6 => data.push((x, y, Color::rgb(0x7f, 0x7f, 0x00))),
                _ => (),
            });

        Self { data, bounds: self.bounds }
    }

    /// Splits double-width emoji pixels into left half (x < split_point).
    fn split_left(source: &GlyphBitmap, cell_w: i32) -> Self {
        let data: Vec<_> = source.data
            .iter()
            .copied()
            .filter(|(x, _, _)| *x < cell_w)
            .collect();

        let bounds = Self::calculate_bounds(&data, cell_w);
        Self { data, bounds }
    }

    /// Splits double-width emoji pixels into right half (x >= split_point), normalized to 0-based.
    fn split_right(source: &GlyphBitmap, cell_w: i32) -> Self {
        let data: Vec<_> = source.data
            .iter()
            .copied()
            .filter(|(x, _, _)| *x >= cell_w)
            .map(|(x, y, c)| (x - cell_w, y, c)) // Normalize x to 0-based
            .collect();

        let bounds = Self::calculate_bounds(&data, cell_w);
        Self { data, bounds }
    }

    fn pixels(&self) -> Vec<(i32, i32, Color)> {
        self.data
            .iter()
            .copied()
            .map(|(x, y, color)| {
                (
                    x + FontAtlasData::PADDING,
                    y + FontAtlasData::PADDING,
                    color,
                )
            })
            .collect()
    }

    /// Calculates bounding box from pixel data.
    fn calculate_bounds(pixels: &[(i32, i32, Color)], cell_w: i32) -> GlyphBounds {
        let min_x = pixels
            .iter()
            .map(|(x, _, _)| *x)
            .min()
            .unwrap_or(0);
        let max_x = pixels
            .iter()
            .map(|(x, _, _)| *x)
            .max()
            .unwrap_or(0)
            .max(cell_w - 1);
        let min_y = pixels
            .iter()
            .map(|(_, y, _)| *y)
            .min()
            .unwrap_or(0);
        let max_y = pixels
            .iter()
            .map(|(_, y, _)| *y)
            .max()
            .unwrap_or(0);

        GlyphBounds { min_x, max_x, min_y, max_y }
    }
}

/// Generator for creating GPU-optimized bitmap font atlases from TrueType/OpenType fonts.
///
/// This is the main entry point for font atlas generation. It manages font loading,
/// glyph rasterization, and texture packing for efficient GPU rendering.
pub struct AtlasFontGenerator {
    font_system: FontSystem,
    cache: SwashCache,
    line_height: f32,
    metrics: Metrics,
    underline: LineDecoration,
    strikethrough: LineDecoration,
    font_family_name: String,
}

impl AtlasFontGenerator {
    /// Creates a new atlas font generator with the specified font family and rendering parameters.
    ///
    /// # Arguments
    ///
    /// * `font_family` - The font family to use for glyph rasterization
    /// * `font_size` - Base font size in points
    /// * `line_height` - Line height multiplier (e.g., 1.2 for 120% line height)
    /// * `underline` - Underline decoration position and thickness
    /// * `strikethrough` - Strikethrough decoration position and thickness
    ///
    /// # Returns
    ///
    /// Returns a configured generator ready to produce bitmap fonts, or an error if the font family
    /// cannot be loaded.
    pub fn new_with_family(
        font_family: FontFamily,
        font_size: f32,
        line_height: f32,
        underline: LineDecoration,
        strikethrough: LineDecoration,
    ) -> Result<Self, Report> {
        info!(
            font_family = %font_family.name,
            font_size = font_size,
            line_height = line_height,
            "Creating bitmap font generator"
        );

        let discovery = FontDiscovery::new();
        let mut font_system = discovery.into_font_system();

        // verify the font family is loaded
        debug!(font_family = %font_family.name, "Loading font family");
        FontDiscovery::load_font_family(&mut font_system, &font_family)?;

        let metrics = Metrics::new(font_size, font_size * line_height);
        let cache = SwashCache::new();

        Ok(Self {
            font_system,
            cache,
            metrics,
            line_height,
            underline,
            strikethrough,
            font_family_name: font_family.name.clone(),
        })
    }

    /// Generates a complete bitmap font atlas from Unicode ranges and emoji.
    ///
    /// This is the main entry point for font atlas generation. It:
    /// 1. Calculates optimal cell dimensions for all font styles
    /// 2. Categorizes glyphs and allocates IDs
    /// 3. Rasterizes all glyphs into a 3D texture array
    /// 4. Packages everything into a [`BitmapFont`] ready for GPU upload
    ///
    /// # Arguments
    ///
    /// * `unicode_ranges` - Unicode character ranges to include (e.g., Basic Latin, Symbols)
    /// * `other_symbols` - String containing emoji and other emoji characters to rasterize
    ///
    /// # Returns
    ///
    /// A [`BitmapFont`] containing the atlas texture data and glyph metadata.
    pub fn generate(
        &mut self,
        unicode_ranges: &[RangeInclusive<char>],
        other_symbols: &str,
    ) -> BitmapFont {
        let char_count = unicode_ranges
            .iter()
            .map(|r| r.clone().into_iter().count())
            .sum::<usize>();
        info!(
            font_family = %self.font_family_name,
            char_count = char_count,
            char_dbl_count = other_symbols.graphemes(true).count(),
            "Starting font generation"
        );

        // calculate texture dimensions using all font styles to ensure proper cell sizing
        let bounds = self.calculate_optimized_cell_dimensions();

        // categorize and allocate IDs
        let grapheme_set = GraphemeSet::new(unicode_ranges, other_symbols);
        let glyphs = grapheme_set.into_glyphs(bounds);

        debug!(glyph_count = glyphs.len(), "Generated glyph set");

        // let test_glyphs = create_test_glyphs_for_cell_calculation();
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

        // rasterize glyphs and copy into texture
        for glyph in &glyphs {
            self.place_glyph_in_3d_texture(glyph, &config, &mut texture_data);
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

        println!("Position Summary:");
        println!("  Cell height: {cell_height}");
        println!(
            "  Underline - Provided: {:.4} ({:.1}px) -> Actual: {:.4} ({:.1}px)",
            self.underline.position,
            cell_height * self.underline.position,
            nudged_underline.position,
            cell_height * nudged_underline.position
        );
        println!(
            "  Strikethrough - Provided: {:.4} ({:.1}px) -> Actual: {:.4} ({:.1}px)",
            self.strikethrough.position,
            cell_height * self.strikethrough.position,
            nudged_strikethrough.position,
            cell_height * nudged_strikethrough.position
        );

        info!(
            font_family = %self.font_family_name,
            glyph_count = glyphs.len(),
            texture_size_bytes = texture_data.len(),
            cell_height = cell_height,
            underline_provided_pos = self.underline.position,
            underline_provided_pixel = cell_height * self.underline.position,
            underline_actual_pos = nudged_underline.position,
            underline_actual_pixel = cell_height * nudged_underline.position,
            strikethrough_provided_pos = self.strikethrough.position,
            strikethrough_provided_pixel = cell_height * self.strikethrough.position,
            strikethrough_actual_pos = nudged_strikethrough.position,
            strikethrough_actual_pixel = cell_height * nudged_strikethrough.position,
            "Font generation completed successfully"
        );

        BitmapFont {
            atlas_data: FontAtlasData {
                font_name: self.font_family_name.clone().into(),
                font_size: self.metrics.font_size,
                texture_dimensions: (config.texture_width, config.texture_height, config.layers),
                cell_size: config.padded_cell_size(),
                underline: nudged_underline,
                strikethrough: nudged_strikethrough,
                glyphs,
                texture_data,
            },
        }
    }

    /// Rasterizes a glyph and writes its pixels into the 3D texture at the computed atlas position.
    /// For emoji, splits the double-width rendering into left and right halves placed in consecutive cells.
    fn place_glyph_in_3d_texture(
        &mut self,
        glyph: &Glyph,
        config: &RasterizationConfig,
        texture: &mut [u32],
    ) {
        debug!(
            symbol = %glyph.symbol,
            style = ?glyph.style,
            glyph_id = format_args!("0x{:04X}", glyph.id),
            is_emoji = glyph.is_emoji,
            "Rasterizing glyph"
        );

        if glyph.is_emoji {
            // Render emoji at 2× width and split into left/right halves
            let bounds = config.double_width_glyph_bounds();
            let bitmap = self.rasterize_symbol(&glyph.symbol, glyph.style, bounds);
            let pixel_count = bitmap.data.len();
            eprintln!("EMOJI: '{}' id={:#06x} layer={} pixels={}",
                glyph.symbol, glyph.id, glyph.id >> 5, pixel_count);
            let cell_w = config.glyph_bounds().width();

            let half_bitmap = if glyph.id & 1 == 0 {
                GlyphBitmap::split_left(&bitmap, cell_w)
            } else {
                GlyphBitmap::split_right(&bitmap, cell_w)
            };

            let half_pixels = half_bitmap.pixels();
            eprintln!("  -> After split: {} pixels, half={}",
                half_pixels.len(), if glyph.id & 1 == 0 { "LEFT" } else { "RIGHT" });

            self.render_pixels_to_texture(
                half_pixels,
                glyph.atlas_coordinate(),
                config,
                texture,
            );

        } else {
            // Normal glyph rendering
            let pixels = self
                .rasterize_symbol(&glyph.symbol, glyph.style, config.glyph_bounds())
                .pixels();

            self.render_pixels_to_texture(pixels, glyph.atlas_coordinate(), config, texture);
        }
    }

    /// Adjusts decoration position to the nearest half-pixel boundary for crisp rendering.
    fn nudge_decoration_to_half_pixel(
        decoration: LineDecoration,
        cell_height: f32,
    ) -> LineDecoration {
        let pixel_pos = cell_height * decoration.position;
        let nudged_pixel = (pixel_pos - 0.5).round() + 0.5;
        let nudged_position = nudged_pixel / cell_height;
        LineDecoration::new(nudged_position, decoration.thickness)
    }

    /// Rasterizes a single symbol with the specified style into a bitmap.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The character or grapheme to rasterize
    /// * `style` - Font style (Normal, Bold, Italic, BoldItalic)
    /// * `bounds` - Target glyph bounds for rendering
    ///
    /// # Returns
    ///
    /// A [`GlyphBitmap`] containing pixel data and actual bounds.
    ///
    /// # Note
    ///
    /// For emoji, this uses a different rendering path with dynamic scaling
    /// to ensure proper sizing and display.
    pub fn rasterize_symbol(
        &mut self,
        symbol: &str,
        style: FontStyle,
        bounds: GlyphBounds,
    ) -> GlyphBitmap {
        let glyph = Glyph::new(symbol, style, (0, 0));
        let mut buffer = self.render_to_buffer(&glyph, bounds.width(), bounds.height());
        let mut buffer = buffer.borrow_with(&mut self.font_system);

        let pixels = Self::collect_glyph_pixels(&mut buffer, &mut self.cache, bounds);

        GlyphBitmap { data: pixels, bounds }
    }

    /// Creates a cosmic-text buffer with the glyph rendered.
    ///
    /// The `cell_w` parameter determines the rendering width:
    /// - For normal glyphs: single cell width
    /// - For double-width emoji: 2× cell width (via `double_width_glyph_bounds()`)
    fn render_to_buffer(&mut self, glyph: &Glyph, cell_w: i32, cell_h: i32) -> Buffer {
        create_rasterizer(&glyph.symbol)
            .font_family_name(&self.font_family_name)
            .font_style(glyph.style)
            .monospace_width(cell_w as u32)
            .rasterize(&mut self.font_system, self.metrics)
            .expect("glyph to rasterize to Buffer")
    }

    /// Extracts pixel data from a cosmic-text buffer within the specified bounds.
    fn collect_glyph_pixels(
        buffer: &mut cosmic_text::BorrowedWithFontSystem<Buffer>,
        cache: &mut SwashCache,
        bounds: GlyphBounds,
    ) -> Vec<(i32, i32, Color)> {
        let mut pixels = Vec::new();

        buffer.draw(cache, WHITE, |x, y, _w, _h, color| {
            if color.a() > 0 && bounds.contains(x, y) {
                // let (x, y) = bounds.normalize_xy(x, y);
                pixels.push((x, y, color));
            }
        });

        pixels
    }

    /// Writes pixel data into the 3D texture array at the specified cell position and layer.
    fn render_pixels_to_texture(
        &self,
        pixels: Vec<(i32, i32, Color)>,
        coord: AtlasCoordinate,
        config: &RasterizationConfig,
        texture: &mut [u32],
    ) {
        let cell_offset = coord.cell_offset_in_px(config.glyph_bounds());
        let (cell_w, cell_h) = config.padded_cell_size();

        for (x, y, color) in pixels {
            if x < 0 || x >= cell_w || y < 0 || y >= cell_h {
                continue;
            }

            let px = x + cell_offset.0;
            let py = y + cell_offset.1;

            if px >= 0 && px < config.texture_width && py >= 0 && py < config.texture_height {
                let idx = self.texture_index(px, py, coord.layer as i32, config);

                if idx < texture.len() {
                    let [r, g, b, a] = color.as_rgba().map(|c| c as u32);
                    texture[idx] = r << 24 | g << 16 | b << 8 | a;
                }
            }
        }
    }

    /// Calculates the linear texture index for a 3D coordinate (x, y, layer).
    fn texture_index(&self, x: i32, y: i32, slice: i32, config: &RasterizationConfig) -> usize {
        (slice * config.texture_width * config.texture_height + y * config.texture_width + x)
            as usize
    }

    /// Classifies a glyph as single-width, double-width, or missing by measuring its rendered width.
    ///
    /// # Arguments
    ///
    /// * `glyph` - The character or grapheme to classify
    /// * `inner_cell_w` - Cell width (without padding) for width comparison
    /// * `inner_cell_h` - Cell height (without padding) for rendering
    ///
    /// # Returns
    ///
    /// - [`GlyphWidthInfo::SingleWidth`] - Glyph fits within one cell (width < 1.5× cell width)
    /// - [`GlyphWidthInfo::DoubleWidth`] - Glyph requires two cells (width ≥ 1.5× cell width)
    /// - [`GlyphWidthInfo::Missing`] - Glyph cannot be rendered (no pixels produced)
    ///
    /// # Algorithm
    ///
    /// Renders the glyph at 4× size with 8× buffer dimensions to accurately measure its
    /// actual width, then classifies based on the threshold of 1.5× cell width.
    /// Space characters are always classified as single-width.
    pub fn classify_glyph_width(
        &mut self,
        glyph: impl Into<String>,
        inner_cell_w: f32,
        inner_cell_h: f32,
    ) -> GlyphWidthInfo {
        // Measure at 4× size (same approach as rasterize_emoji)
        let measure_size = self.metrics.font_size * 4.0;
        let measure_metrics = Metrics::new(measure_size, measure_size * self.line_height);
        let scale_factor = 8.0;

        let glyph = glyph.into();
        let mut measure_buffer = create_rasterizer(&glyph)
            .font_family_name(&self.font_family_name)
            .font_style(FontStyle::Normal)
            .buffer_size(inner_cell_w * scale_factor, inner_cell_h * scale_factor)
            .rasterize(&mut self.font_system, measure_metrics)
            .expect("glyph to rasterize to Buffer");

        let mut measure_buffer = measure_buffer.borrow_with(&mut self.font_system);
        let bounds = measure_glyph_bounds(&mut measure_buffer, &mut self.cache);

        if !bounds.has_content() {
            // Check if this is an intentionally empty glyph (space character)
            // If it's a space, treat as single-width; otherwise it's missing
            if is_empty_character(&glyph) {
                debug!(
                    glyph = glyph,
                    "Classified as single-width (space character)"
                );
                return GlyphWidthInfo::SingleWidth;
            } else {
                debug!(glyph = glyph, "Classified as missing (no pixels)");
                return GlyphWidthInfo::Missing(glyph);
            }
        }

        let actual_width = bounds.width();

        // Buffer is 8× cell size, so scale back to cell space for comparison
        let actual_width_in_cell_space = actual_width as f32 / scale_factor;

        // Consider double-width if actual width is >= 1.5× cell width
        // This threshold accounts for glyphs that render at ~2× cell width
        let glyph_info = if actual_width_in_cell_space >= inner_cell_w * 1.5 {
            GlyphWidthInfo::DoubleWidth
        } else {
            GlyphWidthInfo::SingleWidth
        };

        debug!(
            glyph = glyph,
            actual_width_px = actual_width,
            actual_width_cells = actual_width_in_cell_space,
            cell_width = inner_cell_w,
            glyph_info = ?glyph_info,
            "Classified glyph width"
        );

        glyph_info
    }


    /// Calculates optimal cell dimensions by iteratively tuning font size for crisp edges.
    ///
    /// This function renders a full block character (U+2588) at various font sizes to find
    /// the configuration that produces the cleanest edge alignment. It minimizes antialiasing
    /// artifacts by finding dimensions where edge pixels have intensities close to 0.0 or 1.0.
    ///
    /// # Algorithm
    ///
    /// 1. Renders the block character at 512 different font sizes (±12.8% range)
    /// 2. Measures edge pixel intensities on right and bottom edges
    /// 3. Calculates fitness as deviation from integer intensity values (0.0 or 1.0)
    /// 4. Selects the font size with minimal edge deviation
    /// 5. Optimizes final bounds by trimming faint edges (intensity < 0.1)
    ///
    /// # Returns
    ///
    /// Optimized [`GlyphBounds`] with the font size adjusted for crisp rendering.
    ///
    /// # Side Effects
    ///
    /// Updates `self.metrics.font_size` to the optimized value.
    pub fn calculate_optimized_cell_dimensions(&mut self) -> GlyphBounds {
        let reference_glyph = Glyph {
            id: 0,
            symbol: "\u{2588}".to_compact_string(), // Full block character
            style: FontStyle::Normal,
            pixel_coords: (0, 0),
            is_emoji: false,
        };

        let mut dynamic_metrics = self.metrics;
        let mut best_fitness = f32::INFINITY;
        let mut best_bounds = GlyphBounds::empty();
        let mut best_metrics = self.metrics;

        let iterations = 512; // Number of steps to optimize
        for step in 0..iterations {
            // Adjust font size for next iteration
            let adjustment = 1.0 + ((step - iterations / 2) as f32 * 0.00025);

            dynamic_metrics.font_size = self.metrics.font_size * adjustment;
            dynamic_metrics.line_height = dynamic_metrics.font_size * self.line_height;

            let mut buffer = create_rasterizer(&reference_glyph.symbol) // Full block character
                .font_family_name(&self.font_family_name)
                .font_style(reference_glyph.style)
                .rasterize(&mut self.font_system, dynamic_metrics)
                .expect("glyph to rasterize to Buffer");

            let mut buffer = buffer.borrow_with(&mut self.font_system);
            let bounds = measure_glyph_bounds(&mut buffer, &mut self.cache);

            let pixels = Self::collect_glyph_pixels(&mut buffer, &mut self.cache, bounds);

            // Calculate edge intensity fitness with font size deviation penalty
            let right_intensity = Self::calculate_edge_intensity(&pixels, bounds, true);
            let bottom_intensity = Self::calculate_edge_intensity(&pixels, bounds, false);

            // Fitness is the worst deviation from integer values (0.0 or 1.0)
            let right_frac = right_intensity % 1.0;
            let bottom_frac = bottom_intensity % 1.0;
            let right_deviation = right_frac.min(1.0 - right_frac);
            let bottom_deviation = bottom_frac.min(1.0 - bottom_frac);
            let edge_fitness = right_deviation.max(bottom_deviation);

            debug!("Step {}: font_size={:.4}, right_intensity={:.4}, bottom_intensity={:.4}, edge_fitness={:.4}, bounds={:?}", 
                step, dynamic_metrics.font_size, right_intensity, bottom_intensity, edge_fitness, bounds);

            if edge_fitness < best_fitness {
                best_fitness = edge_fitness;
                best_bounds = bounds;
                best_metrics = dynamic_metrics;
                debug!(
                    "New best fitness, error score: {:.4} at font_size={:.4}",
                    best_fitness, dynamic_metrics.font_size
                );
            }
        }

        // Optimize final bounds based on edge intensities
        info!(
            "Optimizing bounds for overdraw, error score: {:.4}",
            best_fitness
        );
        info!(
            "font size update to {:.4} from {:.4}",
            best_metrics.font_size, self.metrics.font_size
        );
        self.metrics = best_metrics;

        Self::optimize_bounds_for_overdraw(
            best_bounds,
            &self.reference_glyph_pixels(&reference_glyph, best_bounds),
        )
    }

    /// Calculates average pixel intensity along the right or bottom edge of a glyph.
    fn calculate_edge_intensity(
        pixels: &[(i32, i32, Color)],
        bounds: GlyphBounds,
        is_right_edge: bool,
    ) -> f32 {
        let mut total_intensity = 0.0;
        let mut pixel_count = 0;

        for &(x, y, color) in pixels {
            let is_edge_pixel =
                if is_right_edge { x == bounds.width() - 1 } else { y == bounds.height() - 1 };

            if is_edge_pixel {
                // Convert color to intensity (0.0 - 1.0)
                let intensity = color.a() as f32 / 255.0;
                total_intensity += intensity;
                pixel_count += 1;
            }
        }

        if pixel_count > 0 {
            total_intensity / pixel_count as f32
        } else {
            0.0
        }
    }

    /// Shrinks bounds by removing edges with very faint pixel intensity (< 0.1).
    fn optimize_bounds_for_overdraw(
        mut bounds: GlyphBounds,
        pixels: &[(i32, i32, Color)],
    ) -> GlyphBounds {
        let right_intensity = Self::calculate_edge_intensity(pixels, bounds, true);
        let bottom_intensity = Self::calculate_edge_intensity(pixels, bounds, false);

        // If edge intensity is approaching 0.0 (very faint), shrink the dimension
        if right_intensity < 0.1 {
            bounds = bounds.shrink_width(1);
        }
        if bottom_intensity < 0.1 {
            bounds = bounds.shrink_height(1);
        }

        bounds
    }

    /// Renders a glyph and returns its pixel data for analysis or optimization.
    fn reference_glyph_pixels(
        &mut self,
        glyph: &Glyph,
        bounds: GlyphBounds,
    ) -> Vec<(i32, i32, Color)> {
        let mut buffer = create_rasterizer(&glyph.symbol)
            .font_family_name(&self.font_family_name)
            .font_style(glyph.style)
            .rasterize(&mut self.font_system, self.metrics)
            .expect("glyph to rasterize to Buffer");

        Self::collect_glyph_pixels(
            &mut buffer.borrow_with(&mut self.font_system),
            &mut self.cache,
            bounds,
        )
    }

    /// Calculates unified cell dimensions by measuring maximum bounds across all provided glyphs.
    fn calculate_cell_dimensions(&mut self, glyphs: &[Glyph]) -> GlyphBounds {
        let mut bounds = GlyphBounds::empty();

        debug!(
            "Measuring unified cell dimensions across {} glyphs",
            glyphs.len()
        );

        // Measure all style combinations to find maximum bounds
        for glyph in glyphs.iter() {
            let mut buffer = create_rasterizer(&glyph.symbol)
                .font_family_name(&self.font_family_name)
                .font_style(glyph.style)
                .rasterize(&mut self.font_system, self.metrics)
                .expect("glyph to rasterize to Buffer");

            // Measure actual glyph bounds with baseline awareness using new module
            let glyph_bounds = measure_glyph_bounds(
                &mut buffer.borrow_with(&mut self.font_system),
                &mut self.cache,
            );
            bounds = bounds.merge(glyph_bounds);

            debug!(
                symbol = %glyph.symbol,
                style = ?glyph.style,
                bounds = ?glyph_bounds,
                "Glyph metrics"
            );
        }

        bounds
    }

    /// Checks which glyphs are missing from the font by attempting to rasterize them.
    ///
    /// # Arguments
    ///
    /// * `chars` - String containing characters to check for font support
    ///
    /// # Returns
    ///
    /// A [`MissingGlyphReport`] containing:
    /// - List of glyphs that failed to render (produced no pixels)
    /// - Total number of glyphs checked (excluding emoji)
    /// - Font family name
    ///
    /// # Algorithm
    ///
    /// Attempts to rasterize each character in all font styles. If rasterization produces
    /// no visible pixels, the glyph is considered missing from the font. Emoji glyphs are
    /// skipped as they use a different rendering path.
    pub fn check_missing_glyphs(
        &mut self,
        ranges: &[RangeInclusive<char>],
        additional_symbols: &str,
    ) -> MissingGlyphReport {
        // Use the same glyph bounds as the main generation
        let bounds = self.calculate_optimized_cell_dimensions();

        let grapheme_set = GraphemeSet::new(ranges, additional_symbols);
        let glyphs = grapheme_set.into_glyphs(bounds);

        let mut missing_glyphs = Vec::new();
        let mut total_checked = 0;

        for glyph in &glyphs {
            total_checked += 1;

            // skip intentionally empty glyphs (space characters)
            if is_empty_character(&glyph.symbol) {
                continue;
            }

            // Try to rasterize the glyph - if it produces no visible pixels, it's missing
            let rasterized = self.rasterize_symbol(&glyph.symbol, glyph.style, bounds);
            let is_supported = !rasterized.data.is_empty();

            if !is_supported {
                debug!(
                    symbol = %glyph.symbol,
                    style = ?glyph.style,
                    "Glyph not supported - rasterization produced no pixels"
                );
                missing_glyphs.push(MissingGlyph {
                    symbol: glyph.symbol.to_string(),
                    style: glyph.style,
                });
            }
        }

        MissingGlyphReport {
            missing_glyphs,
            total_checked,
            font_family_name: self.font_family_name.clone(),
        }
    }
}

/// Creates test glyphs for cell dimension calculation using the full block character.
fn create_test_glyphs_for_cell_calculation() -> Vec<Glyph> {
    // todo: re-enable all glyphs when renderer supports overdrawing terminal cells.
    // certain glyphs extend into neighboring cells (typically with negative x/y offsets),
    // so we need to track inner and outer cell bounds.

    // Use multiple test characters that stress different dimensions and baseline positions:
    // - Block character for full block coverage
    // - Capital letters for ascender measurement
    // - Lowercase with descenders for full baseline range
    // - Mixed ascender/descender combinations for edge cases
    // - Characters that may have different metrics in different styles
    [
        // Block character for full coverage
        "\u{2588}", // █
                   // // Capital letters for ascender measurement
                   // "M", "W", "H", "I",
                   // // Lowercase with descenders
                   // "g", "y", "p", "q", "j",
                   // // Mixed ascender/descender combinations
                   // "b", "d", "f", "h", "k", "l", "t",
                   // // Characters that may have different metrics
                   // "Q", "@", "#", "&"
    ]
    .into_iter()
    .flat_map(|ch| {
        FontStyle::ALL
            .into_iter()
            .filter(|s| *s == FontStyle::Normal)
            .map(move |style| Glyph::new(ch, style, (0, 0)))
    })
    .collect()
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
    use super::*;
    use crate::font_discovery::FontDiscovery;

    #[test]
    fn test_space_character_detection() {
        // Test common space characters
        assert!(is_empty_character(" ")); // U+0020 SPACE
        assert!(is_empty_character("\u{00A0}")); // NO-BREAK SPACE
        assert!(is_empty_character("\u{2003}")); // EM SPACE
        assert!(is_empty_character("\u{200B}")); // ZERO WIDTH SPACE
        assert!(is_empty_character("\u{3000}")); // IDEOGRAPHIC SPACE

        // Test non-space characters
        assert!(!is_empty_character("A"));
        assert!(!is_empty_character("0"));
        assert!(!is_empty_character("█"));
        assert!(!is_empty_character("")); // Empty string
        assert!(!is_empty_character("AB")); // Multi-char
    }

    #[test]
    fn test_missing_glyph_detection() {
        // Create a font discovery instance and get available fonts
        let discovery = FontDiscovery::new();
        let available_fonts = discovery.discover_complete_monospace_families();

        if available_fonts.is_empty() {
            println!("No fonts available for testing");
            return;
        }

        // Use the first available font
        let font_family = available_fonts[0].clone();

        // Create a generator
        let mut generator = AtlasFontGenerator::new_with_family(
            font_family.clone(),
            15.0,
            1.0,
            LineDecoration::new(0.85, 0.05),
            LineDecoration::new(0.5, 0.05),
        )
        .expect("Failed to create generator");

        // Test with ranges that don't duplicate ASCII
        // Using Latin Extended-A range for non-overlapping chars
        let test_ranges = vec!['\u{0100}'..='\u{0105}']; // Ā-ą (6 chars)
        let report = generator.check_missing_glyphs(&test_ranges, "");

        // Verify basic properties of the report
        assert_eq!(report.font_family_name, font_family.name);
        // ASCII (95 chars) + Latin Extended-A (6 chars) = 101 chars * 4 styles = 404 glyphs
        assert_eq!(report.total_checked, 404);

        // The missing count should be reasonable
        let coverage_percent = ((report.total_checked - report.missing_glyphs.len()) as f64
            / report.total_checked as f64)
            * 100.0;

        // Coverage may be lower due to Latin Extended-A chars, so we accept > 80%
        assert!(
            coverage_percent > 95.0,
            "Font coverage should be above 95%, got {:.1}%",
            coverage_percent
        );
    }
}
