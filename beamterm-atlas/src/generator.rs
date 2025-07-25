use beamterm_data::{FontAtlasData, FontStyle, Glyph, LineDecoration};
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Style, SwashCache, Weight};

use crate::{
    coordinate::GlyphCoordinate,
    font_discovery::{FontDiscovery, FontFamily},
    grapheme::GraphemeSet,
    raster_config::RasterizationConfig,
    BitmapFont,
};

const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);

pub(super) struct BitmapFontGenerator {
    font_system: FontSystem,
    cache: SwashCache,
    font_size: f32,
    line_height: f32,
    metrics: Metrics,
    underline: LineDecoration,
    strikethrough: LineDecoration,
    font_family_name: String,
}

impl BitmapFontGenerator {
    /// Creates a new generator with the specified font family
    pub fn new_with_family(
        font_family: FontFamily,
        font_size: f32,
        line_height: f32,
        underline: LineDecoration,
        strikethrough: LineDecoration,
    ) -> Result<Self, String> {
        let discovery = FontDiscovery::new();
        let mut font_system = discovery.into_font_system();

        // verify the font family is loaded
        FontDiscovery::load_font_family(&mut font_system, &font_family)?;

        let metrics = Metrics::new(font_size, font_size * line_height);
        let cache = SwashCache::new();

        Ok(Self {
            font_system,
            cache,
            metrics,
            font_size,
            line_height,
            underline,
            strikethrough,
            font_family_name: font_family.name,
        })
    }

    pub fn generate(&mut self, chars: &str) -> BitmapFont {
        // categorize and allocate IDs
        let grapheme_set = GraphemeSet::new(chars);
        let glyphs = grapheme_set.into_glyphs();

        // calculate texture dimensions using all font styles to ensure proper cell sizing
        let test_glyphs = create_test_glyphs_for_cell_calculation();
        let (cell_w, cell_h) = self.calculate_cell_dimensions(&test_glyphs);

        let config = RasterizationConfig::new(cell_w, cell_h, &glyphs);
        println!(
            "Font: {} @ {}pt (line height: {})",
            self.font_family_name, self.font_size, self.line_height
        );
        println!("{:?}", &config);

        // allocate 3d rgba texture data
        let mut texture_data = vec![0u32; config.texture_size()];

        // rasterize glyphs into 3d texture
        let mut rasterized_glyphs = Vec::with_capacity(glyphs.len());
        for glyph in glyphs.into_iter() {
            let coord = GlyphCoordinate::from_glyph_id(glyph.id);

            self.place_glyph_in_3d_texture(&glyph, &config, &mut texture_data, coord);

            // update glyph with actual texture coordinates
            let mut updated_glyph = glyph;
            updated_glyph.pixel_coords = coord.xy(&config);
            rasterized_glyphs.push(updated_glyph);
        }

        let texture_data = texture_data
            .iter()
            .flat_map(|&color| {
                let r = (color >> 24) as u8;
                let g = (color >> 16) as u8;
                let b = (color >> 8) as u8;
                let a = (color & 0xFF) as u8;
                [r, g, b, a]
            })
            .collect::<Vec<u8>>();

        BitmapFont {
            atlas_data: FontAtlasData {
                font_name: self.font_family_name.clone().into(),
                font_size: self.font_size,
                texture_dimensions: (config.texture_width, config.texture_height, config.layers),
                cell_size: (config.cell_width, config.cell_height),
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
        coord: GlyphCoordinate,
    ) {
        let inner_cell_w = config.cell_width - FontAtlasData::PADDING * 2;
        let inner_cell_h = config.cell_height - FontAtlasData::PADDING * 2;

        // rasterize the glyph
        let mut buffer = self.rasterize_glyph_for_atlas(glyph, inner_cell_w, inner_cell_h);
        let buffer_size = self.get_buffer_size(glyph.is_emoji, inner_cell_w, inner_cell_h);

        buffer.set_size(
            &mut self.font_system,
            Some(buffer_size.0),
            Some(buffer_size.1),
        );

        let mut buffer = buffer.borrow_with(&mut self.font_system);
        let cell_offset = coord.cell_offset_in_px(config);

        // collect pixels and optionally calculate centering
        let pixels = Self::collect_glyph_pixels(
            &mut buffer,
            &mut self.cache,
            glyph.is_emoji,
            inner_cell_w,
            inner_cell_h,
        );

        // render pixels to texture
        let cell_offset = (cell_offset.0, cell_offset.1);
        debug_assert_eq!(
            cell_offset.1, 0,
            "Y offset should be zero for single row texture"
        );
        self.render_pixels_to_texture(pixels, cell_offset, coord.layer as i32, config, texture);
    }

    fn rasterize_glyph_for_atlas(&mut self, glyph: &Glyph, inner_w: i32, inner_h: i32) -> Buffer {
        if glyph.is_emoji {
            self.rasterize_emoji(&glyph.symbol, inner_w as f32, inner_h as f32)
        } else {
            self.rasterize_glyph(&glyph.symbol, glyph.style, inner_w, inner_h)
        }
    }

    fn get_buffer_size(&self, is_emoji: bool, inner_w: i32, inner_h: i32) -> (f32, f32) {
        if is_emoji {
            (inner_w as f32 * 2.0, inner_h as f32 * 2.0)
        } else {
            (inner_w as f32, inner_h as f32)
        }
    }

    fn collect_glyph_pixels(
        buffer: &mut cosmic_text::BorrowedWithFontSystem<Buffer>,
        cache: &mut SwashCache,
        center_emoji: bool,
        inner_w: i32,
        inner_h: i32,
    ) -> Vec<(i32, i32, Color)> {
        let mut pixels = Vec::new();
        let mut bounds = GlyphBounds::new();

        buffer.draw(cache, WHITE, |x, y, w, h, color| {
            if color.a() > 0 && w == 1 && h == 1 {
                bounds.update(x, y);
                pixels.push((x, y, color));
            }
        });

        if center_emoji && !pixels.is_empty() {
            let (offset_x, offset_y) = bounds.centering_offset(inner_w, inner_h);
            pixels.iter_mut().for_each(|(x, y, _)| {
                *x += offset_x;
                *y += offset_y;
            });
        }

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
        let inner_w = config.cell_width - FontAtlasData::PADDING * 2;
        let inner_h = config.cell_height - FontAtlasData::PADDING * 2;

        for (x, y, color) in pixels {
            if x < 0 || x >= inner_w || y < 0 || y >= inner_h {
                continue;
            }

            let px = x + cell_offset.0 + FontAtlasData::PADDING;
            let py = y + cell_offset.1 + FontAtlasData::PADDING;

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

    fn rasterize_glyph(
        &mut self,
        c: &str,
        style: FontStyle,
        inner_cell_w: i32,
        inner_cell_h: i32,
    ) -> Buffer {
        let f = &mut self.font_system;

        let mut buffer = Buffer::new(f, self.metrics);
        buffer.set_size(f, Some(inner_cell_w as f32), Some(inner_cell_h as f32));

        buffer.set_monospace_width(f, Some(inner_cell_w as f32));

        let attrs = attrs(&self.font_family_name, style);
        buffer.set_text(f, c, &attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(f, true);

        buffer
    }

    fn rasterize_emoji(&mut self, emoji: &str, inner_cell_w: f32, inner_cell_h: f32) -> Buffer {
        let f = &mut self.font_system;

        // First pass: measure at default size
        let measure_size = self.font_size * 4.0; // Start larger
        let measure_metrics = Metrics::new(measure_size, measure_size * self.line_height);

        let mut measure_buffer = Buffer::new(f, measure_metrics);
        measure_buffer.set_size(f, Some(inner_cell_w * 8.0), Some(inner_cell_h * 8.0));

        let attrs = &attrs(&self.font_family_name, FontStyle::Normal);
        measure_buffer.set_text(f, emoji, attrs, cosmic_text::Shaping::Advanced);
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
            return self.rasterize_glyph(
                emoji,
                FontStyle::Normal,
                inner_cell_w as i32,
                inner_cell_h as i32,
            );
        }

        // calculate actual dimensions
        let actual_width = (max_x - min_x + 1) as f32;
        let actual_height = (max_y - min_y + 1) as f32;

        // calculate scale factor; overscale slightly to ensure it fits better
        let scale_x = 1.06 * (inner_cell_w) / actual_width;
        let scale_y = 1.06 * (inner_cell_h) / actual_height;

        let scale = scale_x.min(scale_y).min(1.0); // Don't scale up

        // render at scaled size
        let scaled_size = measure_size * scale;
        let scaled_metrics = Metrics::new(scaled_size, scaled_size * self.line_height);

        let mut buffer = Buffer::new(f, scaled_metrics);
        buffer.set_size(f, Some(inner_cell_w), Some(inner_cell_w));
        buffer.set_text(f, emoji, attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(f, true);

        buffer
    }

    /// Calculates the required cell dimensions for a monospaced bitmap font
    /// by finding the maximum width and height of all glyphs in the character set.
    fn calculate_cell_dimensions(&mut self, glyphs: &[Glyph]) -> (i32, i32) {
        let mut max_width = 0;
        let mut max_height = 0;

        // create a temporary buffer for measuring
        let width = 100.0;
        let height = 100.0;

        let font_system = &mut self.font_system;
        let swash_cache = &mut self.cache;
        let metrics = self.metrics;
        let font_family_name = &self.font_family_name;

        // iterate through all glyphs, accounting for their specific styles
        for glyph in glyphs.iter() {
            let attrs = attrs(font_family_name, glyph.style);

            let mut buffer = Buffer::new(font_system, metrics);
            let mut buffer = buffer.borrow_with(font_system);
            buffer.set_size(Some(width), Some(height));

            // add the character to the buffer with its specific style
            buffer.set_text(&glyph.symbol, &attrs, cosmic_text::Shaping::Advanced);

            let mut glyph_max_x = 0;
            let mut glyph_max_y = 0;

            buffer.draw(swash_cache, WHITE, |x, y, _w, _h, color| {
                let a = color.a();
                if a == 0 || x > width as i32 || y > height as i32 {
                    return;
                }

                glyph_max_x = x.max(glyph_max_x);
                glyph_max_y = y.max(glyph_max_y);
            });

            max_width = glyph_max_x.max(max_width);
            max_height = glyph_max_y.max(max_height);
        }

        // add padding
        let cell_width = max_width + FontAtlasData::PADDING * 2;
        let cell_height = max_height + FontAtlasData::PADDING * 2;

        (cell_width, cell_height)
    }
}

fn attrs(font_family: &str, style: FontStyle) -> Attrs {
    let default_weight = Weight((Weight::NORMAL.0 + Weight::MEDIUM.0) / 2);
    let attrs = Attrs::new()
        .family(Family::Name(font_family))
        .style(Style::Normal)
        .family(Family::Monospace)
        .weight(default_weight);

    use FontStyle::*;
    match style {
        Normal => attrs,
        Bold => attrs.weight(Weight::BOLD),
        Italic => attrs.style(Style::Italic),
        BoldItalic => attrs.style(Style::Italic).weight(Weight::BOLD),
    }
}

struct GlyphBounds {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

impl GlyphBounds {
    fn new() -> Self {
        Self {
            min_x: i32::MAX,
            max_x: i32::MIN,
            min_y: i32::MAX,
            max_y: i32::MIN,
        }
    }

    fn update(&mut self, x: i32, y: i32) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    fn centering_offset(&self, cell_w: i32, cell_h: i32) -> (i32, i32) {
        let content_w = self.max_x - self.min_x + 1;
        let content_h = self.max_y - self.min_y + 1;

        let offset_x = (cell_w - content_w) / 2 - self.min_x;
        let offset_y = (cell_h - content_h) / 2 - self.min_y;

        (offset_x, offset_y)
    }
}

/// Creates test glyphs for accurate cell dimension calculation across all font styles
fn create_test_glyphs_for_cell_calculation() -> Vec<Glyph> {
    // Use multiple test characters that stress different dimensions:
    // - "█" for full block coverage
    // - "M" for typical capital letter width
    // - "g" and "y" for descenders
    // - "f" and "j" for potential ascender/descender combinations
    [
        "█",
        // "M",
        // "W",
        // "g",
        // "y",
        // "f",
        // "j",
        // "Q",
        // "b"
    ]
    .into_iter()
    .flat_map(|ch| {
        FontStyle::ALL
            .iter()
            .map(move |style| Glyph::new(ch, *style, (0, 0)))
    })
    .collect()
}
