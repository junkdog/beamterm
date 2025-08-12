use itertools::Itertools;
use std::collections::HashSet;
use compact_str::ToCompactString;
use beamterm_data::{FontAtlasData, FontStyle, Glyph, LineDecoration};
use cosmic_text::{Buffer, Color, FontSystem, Metrics, SwashCache};
use tracing::{debug, info};
use crate::{coordinate::AtlasCoordinate, font_discovery::{FontDiscovery, FontFamily}, grapheme::GraphemeSet, raster_config::RasterizationConfig};
use crate::bitmap_font::BitmapFont;
use crate::glyph_bounds::{GlyphBounds, measure_glyph_bounds};
use crate::glyph_rasterizer::{create_text_attrs, create_rasterizer};

const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);

pub(crate) struct GlyphBitmap {
    pub data: Vec<(i32, i32, Color)>, // (x, y, color)
    pub bounds: GlyphBounds,
}

impl GlyphBitmap {
    #[allow(dead_code)]
    pub fn debug_checkered(self) -> Self {
        let width = self.bounds.width();
        let height = self.bounds.height();

        let existing_pixels: HashSet<_> = self.data
            .iter()
            .map(|(x, y, _)| (*x, *y))
            .collect();

        let mut data = self.data;

        (0..width)
            .cartesian_product(0..height)
            .filter(|&(x, y)| !existing_pixels.contains(&(x, y)))
            .for_each(|(x, y)| {
                match (x + y) % 8 {
                    0 => data.push((x, y, Color::rgb(0x7F, 0x00, 0x7f))),
                    2 => data.push((x, y, Color::rgb(0x00, 0x7f, 0x7f))),
                    4 => data.push((x, y, Color::rgb(0x00, 0x7f, 0x00))),
                    6 => data.push((x, y, Color::rgb(0x7f, 0x7f, 0x00))),
                    _ => (),
                }
            });

        Self { data, bounds: self.bounds }
    }
}

pub(super) struct AtlasFontGenerator {
    font_system: FontSystem,
    cache: SwashCache,
    line_height: f32,
    metrics: Metrics,
    underline: LineDecoration,
    strikethrough: LineDecoration,
    font_family_name: String,
}

impl AtlasFontGenerator {
    /// Creates a new generator with the specified font family
    pub fn new_with_family(
        font_family: FontFamily,
        font_size: f32,
        line_height: f32,
        underline: LineDecoration,
        strikethrough: LineDecoration,
    ) -> Result<Self, String> {
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
            font_family_name: font_family.name,
        })
    }

    pub fn generate(&mut self, chars: &str) -> BitmapFont {
        info!(
            font_family = %self.font_family_name,
            char_count = chars.chars().count(),
            chars = %chars,
            "Starting font generation"
        );
        
        // categorize and allocate IDs
        let grapheme_set = GraphemeSet::new(chars);
        let glyphs = grapheme_set.into_glyphs();
        
        debug!(
            glyph_count = glyphs.len(),
            "Generated glyph set"
        );

        // calculate texture dimensions using all font styles to ensure proper cell sizing
        // let test_glyphs = create_test_glyphs_for_cell_calculation();
        // let bounds = self.calculate_cell_dimensions(&test_glyphs);
        let bounds = self.calculate_optimized_cell_dimensions();
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

        // rasterize glyphs into 3d texture
        let mut rasterized_glyphs = Vec::with_capacity(glyphs.len());
        for glyph in glyphs.into_iter() {
            let coord = AtlasCoordinate::from_glyph_id(glyph.id);

            self.place_glyph_in_3d_texture(&glyph, &config, &mut texture_data, coord);

            // update glyph with actual texture coordinates
            let mut updated_glyph = glyph;
            updated_glyph.pixel_coords = coord.xy(&config);
            rasterized_glyphs.push(updated_glyph);
        }

        let texture_data = texture_data
            .iter()
            .flat_map(|&color| {
                let [a, b, g, r] = color.to_le_bytes();
                [r, g, b, a]
            })
            .collect::<Vec<u8>>();

        info!(
            font_family = %self.font_family_name,
            glyph_count = rasterized_glyphs.len(),
            texture_size_bytes = texture_data.len(),
            "Font generation completed successfully"
        );

        BitmapFont {
            atlas_data: FontAtlasData {
                font_name: self.font_family_name.clone().into(),
                font_size: self.metrics.font_size,
                texture_dimensions: (config.texture_width, config.texture_height, config.layers),
                cell_size: config.padded_cell_size(),
                underline: self.underline,
                strikethrough: self.strikethrough,
                glyphs: rasterized_glyphs,
                texture_data,
            },
        }
    }

    /// Places a single glyph into the texture at the specified position
    fn place_glyph_in_3d_texture(
        &mut self,
        glyph: &Glyph,
        config: &RasterizationConfig,
        texture: &mut [u32],
        coord: AtlasCoordinate,
    ) {
        debug!(
            symbol = %glyph.symbol,
            style = ?glyph.style,
            glyph_id = format_args!("0x{:04X}", glyph.id),
            is_emoji = glyph.is_emoji,
            "Rasterizing glyph"
        );

        let pixels = self.rasterize_symbol(&glyph.symbol, glyph.style, config.glyph_bounds())
            // .checkered()
            .data
            .into_iter()
            .map(|(x, y, color)| (x + FontAtlasData::PADDING, y + FontAtlasData::PADDING, color))
            .collect::<Vec<_>>();

        // render pixels to texture
        let cell_offset = coord.cell_offset_in_px(config);
        self.render_pixels_to_texture(pixels, cell_offset, coord.layer as i32, config, texture);
    }

    #[allow(dead_code)] // consider removal
    pub(crate) fn calculate_glyph_bounds(&mut self) -> GlyphBounds {
        let test_glyphs = create_test_glyphs_for_cell_calculation();
        self.calculate_cell_dimensions(&test_glyphs)
    }

    pub(super) fn rasterize_symbol(
        &mut self,
        symbol: &str,
        style: FontStyle,
        bounds: GlyphBounds,
    ) -> GlyphBitmap {
        let glyph = Glyph::new(symbol, style, (0, 0));
        let mut buffer = self.render_to_buffer(&glyph, bounds.width(), bounds.height());
        let mut buffer = buffer.borrow_with(&mut self.font_system);

        let pixels = Self::collect_glyph_pixels(
            &mut buffer,
            &mut self.cache,
            bounds,
        );

        GlyphBitmap {
            data: pixels,
            bounds
        }
    }

    fn render_to_buffer(&mut self, glyph: &Glyph, cell_w: i32, cell_h: i32) -> Buffer {
        if glyph.is_emoji {
            self.rasterize_emoji(&glyph.symbol, cell_w as f32, cell_h as f32)
        } else {
            create_rasterizer(&glyph.symbol)
                .font_family_name(&self.font_family_name)
                .font_style(glyph.style)
                .monospace_width(cell_w as u32)
                .rasterize(&mut self.font_system, self.metrics)
                .expect("glyph to rasterize to Buffer")
        }
    }

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

        // todo: fix emoji glyphs"

        pixels
    }

    fn render_pixels_to_texture(
        &self,
        pixels: Vec<(i32, i32, Color)>,
        cell_offset: (i32, i32),
        layer: i32,
        config: &RasterizationConfig,
        texture: &mut [u32],
    ) {
        // rasteriz full cell size, pixels already include padding area
        let (cell_w, cell_h) = config.padded_cell_size();

        for (x, y, color) in pixels {
            if x < 0 || x >= cell_w || y < 0 || y >= cell_h {
                continue;
            }

            let px = x + cell_offset.0;
            let py = y + cell_offset.1;

            if px >= 0 && px < config.texture_width && py >= 0 && py < config.texture_height {
                let idx = self.texture_index(px, py, layer, config);

                if idx < texture.len() {
                    let [r, g, b, a] = color.as_rgba().map(|c| c as u32);
                    texture[idx] = r << 24 | g << 16 | b << 8 | a;
                }
            }
        }
    }

    fn texture_index(&self, x: i32, y: i32, slice: i32, config: &RasterizationConfig) -> usize {
        (slice * config.texture_width * config.texture_height + y * config.texture_width + x)
            as usize
    }

    fn rasterize_emoji(&mut self, emoji: &str, inner_cell_w: f32, inner_cell_h: f32) -> Buffer {
        let f = &mut self.font_system;

        // First pass: measure at default size
        let measure_size = self.metrics.font_size * 4.0; // Start larger
        let measure_metrics = Metrics::new(measure_size, measure_size * self.line_height);

        let mut measure_buffer = Buffer::new(f, measure_metrics);
        measure_buffer.set_size(f, Some(inner_cell_w * 8.0), Some(inner_cell_h * 8.0));

        let attrs = create_text_attrs(&self.font_family_name, FontStyle::Normal);
        measure_buffer.set_text(f, emoji, &attrs, cosmic_text::Shaping::Advanced);
        measure_buffer.shape_until_scroll(f, true);

        // Measure actual bounds
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        let mut has_content = false;

        let mut measure_buffer = measure_buffer.borrow_with(f);
        measure_buffer.draw(&mut self.cache, WHITE, |x, y, _w, _h, color| {
            if color.a() > 0 {
                has_content = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        });

        if !has_content {
            // Fallback for emojis that don't render
            return create_rasterizer(emoji)
                .font_family_name(&self.font_family_name)
                .rasterize(&mut self.font_system, self.metrics)
                .expect("glyph to rasterize to Buffer");
        }

        // calculate actual dimensions
        let actual_width = (max_x - min_x + 1) as f32;
        let actual_height = (max_y - min_y + 1) as f32;

        // calculate scale factor; overscale slightly to ensure it fits better
        let scale_x = inner_cell_w / actual_width;
        let scale_y = inner_cell_h / actual_height;

        let scale = scale_x.min(scale_y).min(1.0); // Don't scale up

        // render at scaled size
        let scaled_size = measure_size * scale;
        let scaled_metrics = Metrics::new(scaled_size, scaled_size * self.line_height);

        let mut buffer = Buffer::new(f, scaled_metrics);
        buffer.set_size(f, Some(inner_cell_w), Some(inner_cell_w));
        buffer.set_text(f, emoji, &attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(f, true);

        buffer
    }

    pub(crate) fn calculate_optimized_cell_dimensions(&mut self) -> GlyphBounds {
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
            let bounds = measure_glyph_bounds(
                &mut buffer,
                &mut self.cache
            );

            let pixels = Self::collect_glyph_pixels(
                &mut buffer,
                &mut self.cache,
                bounds
            );

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
                debug!("New best fitness, error score: {:.4} at font_size={:.4}", best_fitness, dynamic_metrics.font_size);
            }

        }

        // Optimize final bounds based on edge intensities
        info!("Optimizing bounds for overdraw, error score: {:.4}", best_fitness);
        info!("font size update to {:.4} from {:.4}",
            best_metrics.font_size, self.metrics.font_size);
        self.metrics = best_metrics;

        Self::optimize_bounds_for_overdraw(
            best_bounds,
            &self.reference_glyph_pixels(&reference_glyph, best_bounds)
        )
    }

    fn calculate_edge_intensity(pixels: &[(i32, i32, Color)], bounds: GlyphBounds, is_right_edge: bool) -> f32 {
        let mut total_intensity = 0.0;
        let mut pixel_count = 0;

        for &(x, y, color) in pixels {
            let is_edge_pixel = if is_right_edge {
                x == bounds.width() - 1
            } else {
                y == bounds.height() - 1
            };

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

    fn optimize_bounds_for_overdraw(mut bounds: GlyphBounds, pixels: &[(i32, i32, Color)]) -> GlyphBounds {
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

    fn reference_glyph_pixels(&mut self, glyph: &Glyph, bounds: GlyphBounds) -> Vec<(i32, i32, Color)> {
        let mut buffer = create_rasterizer(&glyph.symbol)
            .font_family_name(&self.font_family_name)
            .font_style(glyph.style)
            .rasterize(&mut self.font_system, self.metrics)
            .expect("glyph to rasterize to Buffer");

        Self::collect_glyph_pixels(
            &mut buffer.borrow_with(&mut self.font_system),
            &mut self.cache,
            bounds
        )
    }

    /// Calculates unified cell dimensions with baseline awareness
    /// by finding maximum bounds across all font styles in the test set.
    fn calculate_cell_dimensions(&mut self, glyphs: &[Glyph]) -> GlyphBounds {

        let mut bounds = GlyphBounds::empty();

        debug!("Measuring unified cell dimensions across {} glyphs", glyphs.len());

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
                &mut self.cache
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
}

/// Creates comprehensive test glyphs for accurate cell dimension calculation across all font styles
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
        "\u{2588}",  // █
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
