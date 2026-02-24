use std::{
    collections::{BTreeSet, HashSet},
    ops::RangeInclusive,
};

use beamterm_data::{FontStyle, Glyph};
use color_eyre::{Report, eyre::bail};
use compact_str::{CompactString, ToCompactString};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

use crate::{coordinate::AtlasCoordinateProvider, glyph_bounds::GlyphBounds};

// printable ASCII range
const ASCII_RANGE: RangeInclusive<char> = '\u{0020}'..='\u{007E}';

pub struct GraphemeSet {
    unicode: Vec<char>,
    fullwidth_unicode: Vec<char>,
    emoji: Vec<CompactString>,
}

impl GraphemeSet {
    pub fn new(
        unicode_ranges: &[RangeInclusive<char>],
        other_symbols: &str,
    ) -> Result<Self, Report> {
        let gs = grapheme_set_from(unicode_ranges, other_symbols);

        let non_emoji_glyphs = ASCII_RANGE.size_hint().0 + gs.unicode.len();
        let fullwidth_glyphs = gs.fullwidth_unicode.len();
        if (non_emoji_glyphs + fullwidth_glyphs * 2) > 1024 {
            bail!(
                "Too many unique graphemes (max 1024): \
                 halfwidth={non_emoji_glyphs}, fullwidth={fullwidth_glyphs} \
                 (total slots = {total}). Reduce the number of --range entries \
                 or symbols in the symbols file.",
                total = non_emoji_glyphs + fullwidth_glyphs * 2,
            );
        }

        let emoji_glyphs = gs.emoji.len();
        if emoji_glyphs > 2048 {
            bail!(
                "Too many emoji glyphs (max 2048): {emoji_glyphs}. \
                 Reduce the number of emoji in the symbols file.",
            );
        }

        Ok(gs)
    }

    pub fn halfwidth_glyphs_count(&self) -> u16 {
        (ASCII_RANGE.size_hint().0 + self.unicode.len()) as _
    }

    pub(super) fn into_glyphs(self, cell_dimensions: GlyphBounds) -> Vec<Glyph> {
        let mut glyphs = Vec::new();

        // pre-assigned glyphs (in the range 0x000-0x07F)
        let mut used_ids = HashSet::new();
        for c in ASCII_RANGE {
            used_ids.insert(c as u32);
            let s = c.to_compact_string();
            for style in FontStyle::ALL {
                glyphs.push(Glyph::new(&s, style, (0, 0)));
            }
        }

        glyphs.extend(assign_missing_glyph_ids(used_ids, &self.unicode));
        let last_halfwidth_id = glyphs
            .iter()
            .map(|g| g.base_id())
            .max()
            .unwrap_or(0);

        // fullwidth glyphs are assigned after halfwidth, each occupying 2 consecutive IDs
        glyphs.extend(assign_fullwidth_glyph_ids(
            last_halfwidth_id,
            &self.fullwidth_unicode,
        ));

        // emoji glyphs are assigned IDs starting from 0x1000
        for (i, c) in self.emoji.iter().enumerate() {
            // double-width emoji occupy two cells, so spans two IDs
            let id = (i * 2) as u16 | Glyph::EMOJI_FLAG;
            glyphs.push(Glyph::new_emoji(id, c, (0, 0)));
            glyphs.push(Glyph::new_emoji(id + 1, c, (0, 0)));
        }

        glyphs.sort_by_key(|g| g.id);

        // update glyphs with actual texture coordinates
        for glyph in &mut glyphs {
            glyph.pixel_coords = glyph
                .atlas_coordinate()
                .to_pixel_xy(cell_dimensions);
        }

        glyphs
    }
}

fn grapheme_set_from(ranges: &[RangeInclusive<char>], chars: &str) -> GraphemeSet {
    // Range characters use strict is_emoji() — text-presentation-by-default
    // characters from Unicode ranges should be treated as text glyphs.
    let (emoji_ranged, unicode_ranged) = flatten_ranges_no_ascii(ranges);
    let emoji_ranged = emoji_ranged
        .into_iter()
        .map(|c| c.to_compact_string());

    // Symbols file characters use emojis::get() directly — these are
    // explicitly curated and should be treated as emoji when recognized.
    let (emoji, other_symbols): (Vec<&str>, Vec<&str>) = chars
        .graphemes(true)
        .filter(|s| !is_ascii_control(s))
        .filter(|s| !s.is_ascii()) // always inserted
        .partition(|s| emojis::get(s).is_some());

    let mut emoji: Vec<_> = emoji
        .into_iter()
        .map(|s| s.to_compact_string())
        .collect();
    emoji.extend(emoji_ranged);
    emoji.sort();
    emoji.dedup();

    // Build set of emoji first-chars so we can exclude range characters
    // that are already classified as emoji from the symbols file.
    let emoji_chars: HashSet<char> = emoji
        .iter()
        .filter_map(|s| {
            let mut chars = s.chars();
            let first = chars.next()?;
            // only single-char emoji (not multi-codepoint sequences)
            if chars.next().is_none() { Some(first) } else { None }
        })
        .collect();

    let mut other_symbols: Vec<char> = other_symbols
        .into_iter()
        .map(|s: &str| s.chars().next().unwrap())
        .collect();
    other_symbols.extend(unicode_ranged);
    other_symbols.sort();
    other_symbols.dedup();
    // Remove characters already classified as emoji (from the symbols file)
    other_symbols.retain(|c| !emoji_chars.contains(c));

    let (halfwidth, fullwidth): (Vec<char>, Vec<char>) = other_symbols
        .into_iter()
        .partition(|&ch| ch.width() == Some(1)); // control characters are already excluded

    GraphemeSet {
        emoji,
        unicode: halfwidth,
        fullwidth_unicode: fullwidth,
    }
}

fn is_ascii_control(s: &str) -> bool {
    is_ascii_control_char(s.chars().next().unwrap())
}

fn is_ascii_control_char(ch: char) -> bool {
    let ch = ch as u32;
    ch < 0x20 || ch == 0x7F
}

fn flatten_ranges_no_ascii(ranges: &[RangeInclusive<char>]) -> (Vec<char>, Vec<char>) {
    let chars: BTreeSet<char> = ranges
        .iter()
        .cloned()
        .flat_map(|r| r.into_iter())
        .filter(|&c| !is_ascii_control_char(c))
        .filter(|c| !c.is_ascii())
        .collect();

    chars
        .into_iter()
        .partition(|c| is_emoji(&c.to_compact_string()))
}

fn assign_missing_glyph_ids(used_ids: HashSet<u32>, symbols: &[char]) -> Vec<Glyph> {
    let mut next_id: i32 = -1; // initial value to -1
    let mut next_glyph_id = || {
        let mut id = next_id;
        while id == -1 || used_ids.contains(&(id as u32)) {
            id += 1;
        }

        next_id = id + 1;
        id as u16
    };

    symbols
        .iter()
        .flat_map(|c| {
            let base_id = next_glyph_id();
            let s = c.to_compact_string();
            [
                Glyph::new_with_id(base_id, &s, FontStyle::Normal, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::Bold, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::Italic, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::BoldItalic, (0, 0)),
            ]
        })
        .collect()
}

fn assign_fullwidth_glyph_ids(last_id: u16, symbols: &[char]) -> Vec<Glyph> {
    let mut current_id = last_id;
    if !current_id.is_multiple_of(2) {
        current_id += 1; // align to even cells; for a leaner font atlas
    }

    let mut next_glyph_id = || {
        current_id += 2;
        current_id
    };

    symbols
        .iter()
        .flat_map(|c| {
            let base_id = next_glyph_id();
            let s = c.to_compact_string();
            // each fullwidth glyph occupies 2 consecutive cells: left (base_id) and right (base_id + 1)
            [
                // left half (even ID)
                Glyph::new_with_id(base_id, &s, FontStyle::Normal, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::Bold, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::Italic, (0, 0)),
                Glyph::new_with_id(base_id, &s, FontStyle::BoldItalic, (0, 0)),
                // right half (odd ID)
                Glyph::new_with_id(base_id + 1, &s, FontStyle::Normal, (0, 0)),
                Glyph::new_with_id(base_id + 1, &s, FontStyle::Bold, (0, 0)),
                Glyph::new_with_id(base_id + 1, &s, FontStyle::Italic, (0, 0)),
                Glyph::new_with_id(base_id + 1, &s, FontStyle::BoldItalic, (0, 0)),
            ]
        })
        .collect()
}

pub(super) fn is_emoji(s: &str) -> bool {
    match emojis::get(s) {
        Some(emoji) => {
            // If the canonical form contains FE0F, the base character is
            // text-presentation-by-default and should only be emoji when
            // the caller explicitly includes the variant selector.
            if emoji.as_str().contains('\u{FE0F}') {
                s.contains('\u{FE0F}')
            } else {
                // Emoji-presentation-by-default (e.g., 🚀, ⏩)
                true
            }
        },
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji() {
        // Emoji-presentation-by-default: always emoji
        assert!(is_emoji("🚀"));
        assert!(is_emoji("😀"));
        assert!(is_emoji("⏩"));
        assert!(is_emoji("⏪"));
        assert!(is_emoji("⏫"));
        assert!(is_emoji("⏬"));

        // Text-presentation-by-default with FE0F: emoji
        assert!(is_emoji("▶\u{FE0F}"));

        // Text-presentation-by-default without FE0F: NOT emoji
        assert!(!is_emoji("▶"));
        assert!(!is_emoji("◀"));
        assert!(!is_emoji("⏭"));
        assert!(!is_emoji("⏹"));
        assert!(!is_emoji("⏮"));
        assert!(!is_emoji("▪"));
        assert!(!is_emoji("▫"));
        assert!(!is_emoji("◼"));

        // Not recognized by emojis crate at all
        assert!(!is_emoji("A"));
        assert!(!is_emoji("█"));
    }

    #[test]
    fn test_fullwidth_id_assignment() {
        let fullwidth_chars = vec!['一', '二', '三']; // CJK characters
        let glyphs = assign_fullwidth_glyph_ids(10, &fullwidth_chars);

        // Should start at even boundary (12, since 10+1 rounds up)
        assert_eq!(glyphs[0].base_id(), 12); // Left half
        assert_eq!(glyphs[1].base_id(), 12); // Different styles
        assert_eq!(glyphs[4].base_id(), 13); // Right half

        // Second character should increment by 2
        assert_eq!(glyphs[8].base_id(), 14); // Left half
        assert_eq!(glyphs[12].base_id(), 15); // Right half
    }

    #[test]
    fn test_fullwidth_detection() {
        let symbols = "一abc二de"; // Mix of fullwidth and halfwidth
        let gs = grapheme_set_from(&[], symbols);

        assert_eq!(gs.fullwidth_unicode.len(), 2); // '一', '二'
        assert_eq!(gs.unicode.len(), 0); // ascii always included, handled elsewhere
    }

    #[test]
    fn test_width_edge_cases() {
        // Zero-width characters should be handled gracefully
        let symbols = "\u{200B}"; // Zero-width space
        let gs = grapheme_set_from(&[], symbols);

        // Should not panic or misclassify
        assert!(gs.unicode.len() + gs.fullwidth_unicode.len() <= 1);
    }

    #[test]
    fn test_text_presentation_defaults_respected() {
        // Text-presentation-by-default glyphs: recognized by emojis crate
        // but their canonical forms contain FE0F, so they should be treated
        // as regular text glyphs unless explicitly followed by FE0F.
        let text_default = [
            ("▪", "BLACK SMALL SQUARE"),
            ("▫", "WHITE SMALL SQUARE"),
            ("◼", "BLACK MEDIUM SQUARE"),
            ("▶", "BLACK RIGHT-POINTING TRIANGLE"),
            ("◀", "BLACK LEFT-POINTING TRIANGLE"),
            ("⏭", "NEXT TRACK"),
            ("⏹", "STOP"),
            ("⏮", "PREVIOUS TRACK"),
        ];

        for (s, name) in &text_default {
            assert!(
                emojis::get(s).is_some(),
                "{name} should be recognized by emojis crate",
            );
            assert!(
                !is_emoji(s),
                "{name} ({s}) should NOT be classified as emoji without FE0F",
            );
        }

        // Emoji-presentation-by-default: always emoji regardless of FE0F
        let emoji_default =
            [("🚀", "ROCKET"), ("😀", "GRINNING FACE"), ("⏩", "FAST-FORWARD"), ("⏪", "REWIND")];

        for (s, name) in &emoji_default {
            assert!(is_emoji(s), "{name} ({s}) should be classified as emoji",);
        }
    }
}
