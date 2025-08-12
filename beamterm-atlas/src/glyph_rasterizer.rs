use color_eyre::eyre::{OptionExt, Result};
use cosmic_text::{Attrs, Family, Style, Weight};
use beamterm_data::FontStyle;
use beamterm_data::FontStyle::Normal;

#[derive(Debug)]
pub struct GlyphRasterizer<'a> {
    symbol: &'a str,
    font_family_name: Option<&'a str>,
    font_style: FontStyle,
    monospace_width: Option<u32>,
}

pub fn create_rasterizer(
    symbol: &str,
) -> GlyphRasterizer {
    GlyphRasterizer::new(symbol)
}

impl<'a> GlyphRasterizer<'a> {
    fn new(symbol: &'a str) -> Self {
        Self {
            symbol,
            font_family_name: None,
            font_style: Normal,
            monospace_width: None,
        }
    }

    pub fn font_family_name(mut self, font_family_name: &'a str) -> Self {
        self.font_family_name = Some(font_family_name);
        self
    }

    pub fn font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = font_style;
        self
    }

    pub fn monospace_width(mut self, width: u32) -> Self {
        self.monospace_width = Some(width);
        self
    }

    pub fn rasterize(
        self,
        font_system: &mut cosmic_text::FontSystem,
        metrics: cosmic_text::Metrics,
    ) -> Result<cosmic_text::Buffer> {
        let font_family_name = self.font_family_name
            .ok_or_eyre("font family name must be set before rasterizing")?;

        let mut buffer = cosmic_text::Buffer::new(font_system, metrics);
        // buffer.set_size(font_system, Some(self.inner_cell_w), Some(self.inner_cell_h));
        buffer.set_size(font_system, Some(200.0), Some(200.0)); // use large size to avoid issues
        buffer.set_monospace_width(font_system, self.monospace_width.map(|w| w as f32));

        let attrs = create_text_attrs(font_family_name, self.font_style);
        buffer.set_text(font_system, self.symbol, &attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(font_system, true);

        Ok(buffer)
    }
}


pub(super) fn create_text_attrs(font_family: &str, style: FontStyle) -> Attrs {
    let attrs = Attrs::new()
        .family(Family::Name(font_family))
        .style(Style::Normal)
        .weight(Weight::NORMAL);

    use FontStyle::*;
    match style {
        Normal => attrs,
        Bold => attrs.weight(Weight::BOLD),
        Italic => attrs.style(Style::Italic),
        BoldItalic => attrs.style(Style::Italic).weight(Weight::BOLD),
    }
}

