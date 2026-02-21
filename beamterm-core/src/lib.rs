pub mod error;
pub mod gl;
mod mat4;
mod position;
mod url;

pub use ::beamterm_data::{DebugSpacePattern, FontAtlasData, GlyphEffect};
pub use beamterm_data::FontStyle;
pub use error::Error;
pub use gl::{
    Atlas, CellData, CellDynamic, CellIterator, CellQuery, Drawable, FontAtlas, GlState, GlyphSlot,
    GlyphTracker, RenderContext, SelectionMode, SelectionTracker, StaticFontAtlas, TerminalGrid,
    select,
};
pub use position::CursorPosition;
use unicode_width::UnicodeWidthStr;
pub use url::{UrlMatch, find_url_at_cursor};

/// GL shader language target for version injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlslVersion {
    /// WebGL2 / OpenGL ES 3.0: `#version 300 es`
    Es300,
    /// OpenGL 3.3 Core: `#version 330 core`
    Gl330,
}

impl GlslVersion {
    pub fn vertex_preamble(&self) -> &'static str {
        match self {
            Self::Es300 => "#version 300 es\nprecision highp float;\n",
            Self::Gl330 => "#version 330 core\n",
        }
    }

    pub fn fragment_preamble(&self) -> &'static str {
        match self {
            Self::Es300 => "#version 300 es\nprecision mediump float;\n",
            Self::Gl330 => "#version 330 core\n",
        }
    }
}

/// Checks if a grapheme is an emoji-presentation-by-default character.
///
/// Text-presentation-by-default characters (e.g., `\u{25B6}`, `\u{23ED}`, `\u{23F9}`, `\u{25AA}`) are
/// recognized by the `emojis` crate but should only be treated as emoji when
/// explicitly followed by the variation selector `\u{FE0F}`. Without it, they
/// are regular text glyphs.
pub fn is_emoji(s: &str) -> bool {
    match emojis::get(s) {
        Some(emoji) => {
            // If the canonical form contains FE0F, the base character is
            // text-presentation-by-default and should only be emoji when
            // the caller explicitly includes the variant selector.
            if emoji.as_str().contains('\u{FE0F}') { s.contains('\u{FE0F}') } else { true }
        },
        None => false,
    }
}

/// Checks if a grapheme is double-width (emoji or fullwidth character).
pub fn is_double_width(grapheme: &str) -> bool {
    grapheme.len() > 1 && (is_emoji(grapheme) || grapheme.width() == 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji() {
        // Emoji-presentation-by-default: always emoji
        assert!(is_emoji("\u{1F680}"));
        assert!(is_emoji("\u{1F600}"));
        assert!(is_emoji("\u{23E9}"));
        assert!(is_emoji("\u{23EA}"));

        // Text-presentation-by-default with FE0F: emoji
        assert!(is_emoji("\u{25B6}\u{FE0F}"));

        // Text-presentation-by-default without FE0F: NOT emoji
        assert!(!is_emoji("\u{25B6}"));
        assert!(!is_emoji("\u{25C0}"));
        assert!(!is_emoji("\u{23ED}"));
        assert!(!is_emoji("\u{23F9}"));
        assert!(!is_emoji("\u{23EE}"));
        assert!(!is_emoji("\u{25AA}"));
        assert!(!is_emoji("\u{25AB}"));
        assert!(!is_emoji("\u{25FC}"));

        // Not recognized by emojis crate at all
        assert!(!is_emoji("A"));
        assert!(!is_emoji("\u{2588}"));
    }

    #[test]
    fn test_is_double_width() {
        // emoji-presentation-by-default
        assert!(is_double_width("\u{1F600}"));
        assert!(is_double_width(
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
        )); // ZWJ sequence

        [
            "\u{231A}", "\u{231B}", "\u{23E9}", "\u{23F3}", "\u{2614}", "\u{2615}", "\u{2648}",
            "\u{2653}", "\u{267F}", "\u{2693}", "\u{26A1}", "\u{26AA}", "\u{26AB}", "\u{26BD}",
            "\u{26BE}", "\u{26C4}", "\u{26C5}", "\u{26CE}", "\u{26D4}", "\u{26EA}", "\u{26F2}",
            "\u{26F3}", "\u{26F5}", "\u{26FA}", "\u{26FD}", "\u{25FE}", "\u{2B1B}", "\u{2B1C}",
            "\u{2B50}", "\u{2B55}", "\u{3030}", "\u{303D}", "\u{3297}", "\u{3299}",
        ]
        .iter()
        .for_each(|s| {
            assert!(is_double_width(s), "Failed for emoji: {}", s);
        });

        // text-presentation-by-default with FE0F: double-width
        assert!(is_double_width("\u{25B6}\u{FE0F}"));
        assert!(is_double_width("\u{25C0}\u{FE0F}"));

        // text-presentation-by-default without FE0F: single-width
        assert!(!is_double_width("\u{23F8}"));
        assert!(!is_double_width("\u{23FA}"));
        assert!(!is_double_width("\u{25AA}"));
        assert!(!is_double_width("\u{25AB}"));
        assert!(!is_double_width("\u{25B6}"));
        assert!(!is_double_width("\u{25C0}"));
        assert!(!is_double_width("\u{25FB}"));
        assert!(!is_double_width("\u{2934}"));
        assert!(!is_double_width("\u{2935}"));
        assert!(!is_double_width("\u{2B05}"));
        assert!(!is_double_width("\u{2B07}"));
        assert!(!is_double_width("\u{26C8}"));

        // CJK
        assert!(is_double_width("\u{4E2D}"));
        assert!(is_double_width("\u{65E5}"));

        // single-width
        assert!(!is_double_width("A"));
        assert!(!is_double_width("\u{2192}"));
    }

    #[test]
    fn test_font_atlas_config_deserialization() {
        let _ = FontAtlasData::default();
    }
}
