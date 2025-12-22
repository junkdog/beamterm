use std::{
    collections::{BTreeSet, HashSet},
    ops::RangeInclusive,
};

use beamterm_data::{FontStyle, Glyph};
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
    pub fn new(unicode_ranges: &[RangeInclusive<char>], other_symbols: &str) -> Self {
        let gs = grapheme_set_from(unicode_ranges, other_symbols);

        // Removed assertions - now supports up to 1M glyphs with u32 IDs
        // Previous limits: 1024 halfwidth + 2048 emoji
        // New limits: 1,048,576 total glyphs (20-bit base ID)

        gs
    }

    pub fn halfwidth_glyphs_count(&self) -> u32 {
        (ASCII_RANGE.size_hint().0 + self.unicode.len()) as u32
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

        // emoji glyphs are assigned IDs starting from 0x400000 (bit 22 set)
        for (i, c) in self.emoji.iter().enumerate() {
            // double-width emoji occupy two cells, so spans two IDs
            let id = (i * 2) as u32 | Glyph::EMOJI_FLAG;
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
    let (emoji_ranged, unicode_ranged) = flatten_ranges_no_ascii(ranges);
    let emoji_ranged = emoji_ranged
        .into_iter()
        .map(|c| c.to_compact_string());

    let (emoji, other_symbols): (Vec<&str>, Vec<&str>) = chars
        .graphemes(true)
        .filter(|s| !is_ascii_control(s))
        .filter(|s| !s.is_ascii()) // always inserted
        .partition(|s| is_emoji(s));

    let mut emoji: Vec<_> = emoji
        .into_iter()
        .map(|s| s.to_compact_string())
        .collect();
    emoji.extend(emoji_ranged);
    emoji.sort();
    emoji.dedup();

    let mut other_symbols: Vec<char> = other_symbols
        .into_iter()
        .map(|s: &str| s.chars().next().unwrap())
        .collect();
    other_symbols.extend(unicode_ranged);
    other_symbols.sort();
    other_symbols.dedup();

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
        id as u32
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

fn assign_fullwidth_glyph_ids(last_id: u32, symbols: &[char]) -> Vec<Glyph> {
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
    emojis::get(s).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji() {
        assert!(super::is_emoji("⏭"));
        assert!(super::is_emoji("⏹"));
        assert!(super::is_emoji("▶️"));
        assert!(super::is_emoji("⏹"));
        assert!(super::is_emoji("⏮"));
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
}
