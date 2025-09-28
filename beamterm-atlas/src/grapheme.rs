use std::collections::{BTreeSet, HashSet};
use std::ops::RangeInclusive;
use compact_str::ToCompactString;
use beamterm_data::{FontStyle, Glyph};
use unicode_segmentation::UnicodeSegmentation;

pub struct GraphemeSet<'a> {
    ascii: Vec<char>,
    unicode: Vec<char>,
    emoji: Vec<&'a str>,
}

impl<'a> GraphemeSet<'a> {
    pub fn new_from_str(chars: &'a str) -> Self {
        let mut graphemes = chars
            .graphemes(true)
            .filter(|g| !is_ascii_control(g))
            .collect::<Vec<&str>>();
        graphemes.sort();
        graphemes.dedup();

        let mut ascii = vec![];
        let mut unicode = vec![];
        let mut emoji = vec![];

        for g in graphemes {
            if g.len() == 1 && g.is_ascii() {
                ascii.push(g.chars().next().unwrap());
            } else if emojis::get(g).is_some() {
                emoji.push(g);
            } else {
                debug_assert!(
                    g.chars().count() == 1,
                    "Non-emoji grapheme must be a single char: {g}"
                );

                let ch = g.chars().next().unwrap();
                unicode.push(ch);
            }
        }
        let non_emoji_glyphs = ascii.len() + unicode.len();
        assert!(
            non_emoji_glyphs <= 1024,
            "Too many unique graphemes: {non_emoji_glyphs}"
        );

        Self { ascii, unicode, emoji }
    }

    pub fn new(
        unicode_ranges: &[RangeInclusive<char>],
        emoji: &'a str,
    ) -> Self {
        let mut ascii = vec![];
        let mut unicode = vec![];
        for g in single_width_chars(unicode_ranges) {
            if g.is_ascii() {
                ascii.push(g);
            } else {
                unicode.push(g);
            }
        }

        let mut emoji = emoji.graphemes(true)
            .filter(|g| !is_ascii_control(g))
            .filter(|g| emojis::get(g).is_some())
            .collect::<Vec<&str>>();
        emoji.sort();
        emoji.dedup();

        let non_emoji_glyphs = ascii.len() + unicode.len();
        assert!(
            non_emoji_glyphs <= 1024,
            "Too many unique graphemes: {non_emoji_glyphs}"
        );

        let emoji_glyphs = emoji.len();
        assert!(
            emoji_glyphs <= 512,
            "Too many unique graphemes: {emoji_glyphs}"
        );

        Self { ascii, unicode, emoji }
    }

    pub(super) fn into_glyphs(self) -> Vec<Glyph> {
        let mut glyphs = Vec::new();

        // pre-assigned glyphs (in the range 0x000-0x07F)
        let mut used_ids = HashSet::new();
        for c in self.ascii.iter() {
            used_ids.insert(*c as u32); // \o/ fixed it
            for style in FontStyle::ALL {
                let s = c.to_compact_string();
                glyphs.push(Glyph::new(&s, style, (0, 0)));
            }
        }

        // unicode glyphs fill any gaps in the ASCII range (0x000-0x1FF)
        glyphs.extend(assign_missing_glyph_ids(used_ids, &self.unicode));

        // emoji glyphs are assigned IDs starting from 0x1000
        for (i, c) in self.emoji.iter().enumerate() {
            let id = i as u16 | Glyph::EMOJI_FLAG;
            let mut glyph = Glyph::new_with_id(id, c, FontStyle::Normal, (0, 0));
            glyph.is_emoji = true;
            glyphs.push(glyph);
        }

        glyphs.sort_by_key(|g| g.id);

        glyphs
    }
}

fn is_ascii_control(s: &str) -> bool {
    s.is_ascii() && (s.chars().next().unwrap() as u32) < 0x20
}

fn is_ascii_control_char(ch: char) -> bool {
    let ch = ch as u32;
    ch < 0x20 || ch == 0x7F
}

fn single_width_chars(ranges: &[RangeInclusive<char>]) -> Vec<char> {
    let chars: BTreeSet<char> = ranges
        .into_iter()
        .cloned()
        .flat_map(|r| r.into_iter())
        .filter(|&c| !is_ascii_control_char(c))
        .collect();

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
