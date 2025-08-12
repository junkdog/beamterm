use beamterm_data::FontStyle;
use cosmic_text::Color;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Color as RatatuiColor,
    widgets::{Block, Borders, Widget},
};

use crate::font_preview::theme::Theme;

pub struct GlyphImage {
    pub bitmap_data: Vec<(i32, i32, Color)>,
    pub width: u32,
    pub height: u32,
}

impl Default for GlyphImage {
    fn default() -> Self {
        Self { bitmap_data: Vec::new(), width: 1, height: 1 }
    }
}

#[derive(Default)]
pub struct FontDisplayState {
    pub symbol: String,
    pub rendered_variants: Vec<(FontStyle, GlyphImage)>,
}

pub struct FontDisplay<'a> {
    theme: &'a Theme,
    block: Option<Block<'a>>,
    symbol: &'a str,
    rendered_variants: &'a [(FontStyle, GlyphImage)],
}

impl<'a> FontDisplay<'a> {
    pub fn new(
        theme: &'a Theme,
        symbol: &'a str,
        rendered_variants: &'a [(FontStyle, GlyphImage)],
    ) -> Self {
        Self { theme, block: None, symbol, rendered_variants }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl<'a> Widget for FontDisplay<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Apply block if provided
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Split into 2x2 grid for the four font variants
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner_area);

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vertical_chunks[0]);

        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vertical_chunks[1]);

        let areas = [top_chunks[0], top_chunks[1], bottom_chunks[0], bottom_chunks[1]];
        let labels = ["Normal", "Bold", "Italic", "Bold+Italic"];
        let styles = [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic];

        // Render each variant
        let FontDisplay { theme, rendered_variants, symbol, .. } = self;

        for ((area, label), style) in areas.iter().zip(labels.iter()).zip(styles.iter()) {
            // Create a border for this variant
            let block = Block::default()
                .borders(Borders::ALL)
                .title(*label)
                .style(theme.preview_canvas)
                .title_style(theme.variant_label)
                .border_style(theme.border_unfocused);

            let inner = block.inner(*area);
            block.render(*area, buf);

            // Find the rendered glyph for this style
            if let Some((_, glyph_image)) = rendered_variants
                .iter()
                .find(|(s, _)| *s == *style)
            {
                render_glyph_half_blocks(glyph_image, inner, buf);
            } else {
                // Render placeholder if no data found - simple fallback
                // Just put the symbol character in the center
                if let Some(ch) = symbol.chars().next() {
                    let center_x = inner.x + inner.width / 2;
                    let center_y = inner.y + inner.height / 2;
                    if let Some(cell) = buf.cell_mut((center_x, center_y)) {
                        cell.set_char(ch).set_style(theme.block_glyph);
                    }
                }
            }
        }
    }
}

/// Render glyph using half-block characters where each terminal cell represents 2 vertical pixels
fn render_glyph_half_blocks(glyph_image: &GlyphImage, area: Rect, buf: &mut Buffer) {
    let display_width = (glyph_image.width as u16).min(area.width);
    let display_height = glyph_image
        .height
        .div_ceil(2)
        .min(area.height as u32) as u16;

    // Create a lookup map for quick pixel access
    let mut pixel_map = std::collections::HashMap::new();
    for &(x, y, color) in &glyph_image.bitmap_data {
        if color.a() > 0 {
            pixel_map.insert((x, y), color);
        }
    }

    // Center the glyph in the available area
    let offset_x = area.x + (area.width.saturating_sub(display_width)) / 2;
    let offset_y = area.y + (area.height.saturating_sub(display_height)) / 2;

    let black = Color::rgba(0, 0, 0, 255);
    let dark_gray = Color::rgba(64, 64, 64, 255);

    // Process pixels in pairs (top/bottom) for half-block rendering
    for term_y in 0..display_height {
        for term_x in 0..display_width {
            let cell_x = offset_x + term_x;
            let cell_y = offset_y + term_y;

            if let Some(cell) = buf.cell_mut((cell_x, cell_y)) {
                // ▀: first pixel is the top half, second pixel is the bottom half
                cell.set_char('▀');

                let bitmap_x = term_x as i32;
                let bitmap_y_top = (term_y * 2) as i32;
                let bitmap_y_bottom = bitmap_y_top + 1;

                let top_color = pixel_map
                    .get(&(bitmap_x, bitmap_y_top))
                    .unwrap_or(&black);

                let bottom_color = if bitmap_y_bottom < glyph_image.height as i32 {
                    pixel_map
                        .get(&(bitmap_x, bitmap_y_bottom))
                        .unwrap_or(&black)
                } else {
                    pixel_map
                        .get(&(bitmap_x, bitmap_y_bottom))
                        .unwrap_or(&dark_gray)
                };

                cell.set_fg(cosmic_to_ratatui_color(*top_color));
                cell.set_bg(cosmic_to_ratatui_color(*bottom_color));
            }
        }
    }
}

/// Convert cosmic_text::Color to ratatui::Color with alpha premultiplication
fn cosmic_to_ratatui_color(color: Color) -> RatatuiColor {
    let [r, g, b, a] = color.as_rgba();
    let alpha = a as u16;

    // premultiply alpha for proper blending
    let r = ((r as u16 * alpha) / 255) as u8;
    let g = ((g as u16 * alpha) / 255) as u8;
    let b = ((b as u16 * alpha) / 255) as u8;

    RatatuiColor::Rgb(r, g, b)
}

impl FontDisplayState {
    /// Create glyph image from bitmap data  
    pub fn create_glyph_image(
        bitmap_data: &[(i32, i32, Color)],
        width: u32,
        height: u32,
    ) -> GlyphImage {
        // Store bitmap data directly - no scaling needed
        let bitmap_data = bitmap_data.to_vec();

        tracing::debug!(
            "Created glyph image {}x{}, {} pixels",
            width,
            height,
            bitmap_data.len()
        );

        GlyphImage { bitmap_data, width, height }
    }
}
