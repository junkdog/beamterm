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
    fullwidth_unicde: Vec<char>,
    emoji: Vec<CompactString>,
}

impl GraphemeSet {
    pub fn new(unicode_ranges: &[RangeInclusive<char>], other_symbols: &str) -> Self {
        let gs = grapheme_set_from(unicode_ranges, other_symbols);

        let non_emoji_glyphs = ASCII_RANGE.size_hint().0 + gs.unicode.len();
        let fullwidth_glyphs = gs.fullwidth_unicde.len();
        assert!(
            (non_emoji_glyphs + fullwidth_glyphs * 2) <= 1024,
            "Too many unique graphemes: halfwidth={non_emoji_glyphs}, fullwidth={fullwidth_glyphs}"
        );

        let emoji_glyphs = gs.emoji.len();
        assert!(
            emoji_glyphs <= 2048, // each emoji takes two glyph slots
            "Too many unique graphemes: {emoji_glyphs}"
        );

        gs
    }
    
    pub fn halfwidth_glyphs_count(&self) -> u32 {
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
        glyphs.extend(assign_fullwidth_glyph_ids(last_halfwidth_id, &self.fullwidth_unicde));

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

fn grapheme_set_from(
    ranges: &[RangeInclusive<char>],
    chars: &str,
) -> GraphemeSet {
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
        fullwidth_unicde: fullwidth,
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
    if current_id % 2 != 0 {
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

    #[test]
    fn test_is_emoji() {
        assert!(super::is_emoji("⏭"));
        assert!(super::is_emoji("⏹"));
        assert!(super::is_emoji("▶️"));
        assert!(super::is_emoji("⏹"));
        assert!(super::is_emoji("⏮"));
    }
}
