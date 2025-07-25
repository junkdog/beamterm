use std::collections::HashSet;

use beamterm_data::{FontStyle, Glyph};
use unicode_segmentation::UnicodeSegmentation;

pub struct GraphemeSet<'a> {
    ascii: Vec<&'a str>,
    unicode: Vec<&'a str>,
    emoji: Vec<&'a str>,
}

impl<'a> GraphemeSet<'a> {
    pub fn new(chars: &'a str) -> Self {
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
                ascii.push(g);
            } else if emojis::get(g).is_some() {
                emoji.push(g);
            } else {
                unicode.push(g);
            }
        }
        let non_emoji_glyphs = ascii.len() + unicode.len();
        assert!(
            non_emoji_glyphs <= 512,
            "Too many unique graphemes: {non_emoji_glyphs}"
        );

        Self { ascii, unicode, emoji }
    }

    pub(super) fn into_glyphs(self) -> Vec<Glyph> {
        let mut glyphs = Vec::new();

        // pre-assigned glyphs (in the range 0x000-0x07F)
        let mut used_ids = HashSet::new();
        for c in self.ascii.iter() {
            used_ids.insert(c.chars().next().unwrap() as u16);
            for style in FontStyle::ALL {
                glyphs.push(Glyph::new(c, style, (0, 0)));
            }
        }

        // unicode glyphs fill any gaps in the ASCII range (0x000-0x1FF)
        glyphs.extend(assign_missing_glyph_ids(used_ids, &self.unicode));

        // emoji glyphs are assigned IDs starting from 0x800
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

fn assign_missing_glyph_ids(used_ids: HashSet<u16>, symbols: &[&str]) -> Vec<Glyph> {
    let mut next_id: i32 = -1; // initial value to -1
    let mut next_glyph_id = || {
        let mut id = next_id;
        while id == -1 || used_ids.contains(&(id as u16)) {
            id += 1;
        }

        next_id = id + 1;
        id as u16
    };

    symbols
        .iter()
        .flat_map(|c| {
            let base_id = next_glyph_id();
            [
                Glyph::new_with_id(base_id, c, FontStyle::Normal, (0, 0)),
                Glyph::new_with_id(base_id, c, FontStyle::Bold, (0, 0)),
                Glyph::new_with_id(base_id, c, FontStyle::Italic, (0, 0)),
                Glyph::new_with_id(base_id, c, FontStyle::BoldItalic, (0, 0)),
            ]
        })
        .collect()
}
