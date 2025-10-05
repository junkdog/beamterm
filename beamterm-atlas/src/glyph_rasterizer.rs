use beamterm_data::{FontStyle, FontStyle::Normal};
use color_eyre::eyre::{OptionExt, Result};
use cosmic_text::{Attrs, Family, Style, Weight};

#[derive(Debug)]
pub struct GlyphRasterizer<'a> {
    symbol: &'a str,
    font_family_name: Option<&'a str>,
    font_style: FontStyle,
    monospace_width: Option<u32>,
    buffer_size: Option<(f32, f32)>,
}

pub fn create_rasterizer(symbol: &str) -> GlyphRasterizer<'_> {
    GlyphRasterizer::new(symbol)
}

impl<'a> GlyphRasterizer<'a> {
    fn new(symbol: &'a str) -> Self {
        Self {
            symbol,
            font_family_name: None,
            font_style: Normal,
            monospace_width: None,
            buffer_size: None,
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

    #[allow(dead_code)]
    pub fn buffer_size(mut self, width: f32, height: f32) -> Self {
        self.buffer_size = Some((width, height));
        self
    }

    pub fn rasterize(
        self,
        font_system: &mut cosmic_text::FontSystem,
        metrics: cosmic_text::Metrics,
    ) -> Result<cosmic_text::Buffer> {
        let font_family_name = self
            .font_family_name
            .ok_or_eyre("font family name must be set before rasterizing")?;

        let mut buffer = cosmic_text::Buffer::new(font_system, metrics);
        let (width, height) = self.buffer_size.unwrap_or((200.0, 200.0));
        buffer.set_size(font_system, Some(width), Some(height));
        buffer.set_monospace_width(font_system, self.monospace_width.map(|w| w as f32));

        let attrs = create_text_attrs(font_family_name, self.font_style);
        buffer.set_text(
            font_system,
            self.symbol,
            &attrs,
            cosmic_text::Shaping::Advanced,
        );
        buffer.shape_until_scroll(font_system, true);

        Ok(buffer)
    }
}

pub(super) fn create_text_attrs(font_family: &str, style: FontStyle) -> Attrs<'_> {
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
