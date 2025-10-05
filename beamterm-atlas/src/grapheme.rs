use std::{
    collections::{BTreeSet, HashSet},
    ops::RangeInclusive,
};

use beamterm_data::{FontStyle, Glyph};
use compact_str::ToCompactString;
use unicode_segmentation::UnicodeSegmentation;

use crate::{coordinate::AtlasCoordinateProvider, glyph_bounds::GlyphBounds};

// printable ASCII range
const ASCII_RANGE: RangeInclusive<char> = '\u{0020}'..='\u{007E}';

pub struct GraphemeSet<'a> {
    unicode: Vec<char>,
    emoji: Vec<&'a str>,
}

impl<'a> GraphemeSet<'a> {
    pub fn new(unicode_ranges: &[RangeInclusive<char>], other_symbols: &'a str) -> Self {
        let (emoji, unicode) = partition_emoji_and_unicode(other_symbols);
        let unicode = flatten_sorted(unicode_ranges, &unicode);

        let non_emoji_glyphs = ASCII_RANGE.size_hint().0 + unicode.len();
        assert!(
            non_emoji_glyphs <= 1024,
            "Too many unique graphemes: {non_emoji_glyphs}"
        );

        let emoji_glyphs = emoji.len();
        assert!(
            emoji_glyphs <= 2048, // each emoji takes two glyph slots
            "Too many unique graphemes: {emoji_glyphs}"
        );

        Self { unicode, emoji }
    }

    pub(super) fn into_glyphs(self, cell_dimensions: GlyphBounds) -> Vec<Glyph> {
        let mut glyphs = Vec::new();

        // pre-assigned glyphs (in the range 0x000-0x07F)
        let mut used_ids = HashSet::new();
        for c in ASCII_RANGE {
            used_ids.insert(c as u32); // \o/ fixed it
            let s = c.to_compact_string();
            for style in FontStyle::ALL {
                glyphs.push(Glyph::new(&s, style, (0, 0)));
            }
        }

        glyphs.extend(assign_missing_glyph_ids(used_ids, &self.unicode));

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

fn partition_emoji_and_unicode(chars: &str) -> (Vec<&str>, Vec<char>) {
    let (mut emoji, other_symbols): (Vec<&str>, Vec<&str>) = chars
        .graphemes(true)
        .filter(|s| !is_ascii_control(s))
        .filter(|s| !s.is_ascii()) // always inserted
        .partition(|s| is_emoji(s));

    emoji.sort();
    emoji.dedup();

    let mut other_symbols: Vec<char> = other_symbols
        .into_iter()
        .map(|s: &str| s.chars().next().unwrap())
        .collect();
    other_symbols.sort();
    other_symbols.dedup();

    (emoji, other_symbols)
}

fn is_ascii_control(s: &str) -> bool {
    is_ascii_control_char(s.chars().next().unwrap())
}

fn is_ascii_control_char(ch: char) -> bool {
    let ch = ch as u32;
    ch < 0x20 || ch == 0x7F
}

fn flatten_sorted(ranges: &[RangeInclusive<char>], additional_chars: &[char]) -> Vec<char> {
    let mut chars: BTreeSet<char> = ranges
        .iter()
        .cloned()
        .flat_map(|r| r.into_iter())
        .filter(|&c| !is_ascii_control_char(c))
        .collect();

    chars.extend(additional_chars);

    chars.into_iter().collect()
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
fn is_emoji(s: &str) -> bool {
    emojis::get(s).is_some()
}
