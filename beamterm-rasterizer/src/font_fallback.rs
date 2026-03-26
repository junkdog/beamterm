use fontdb::{Database, Family, ID, Query, Stretch, Style, Weight};
use swash::{FontRef, tag_from_bytes};

use crate::error::Error;

/// Controls how color-table fonts are prioritized during resolution.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorPreference {
    /// Prefer fonts with color tables (COLR/CPAL, CBDT, sbix). Used for emoji.
    PreferColor,
    /// Prefer fonts without color tables. Used for text-presentation characters
    /// to avoid accidentally picking up an emoji font fallback.
    AvoidColor,
}

struct LoadedFont {
    id: ID,
    index: u32,
    /// True if the font has COLR+CPAL or color bitmap (CBDT/sbix) tables.
    has_color_tables: bool,
}

impl LoadedFont {
    fn new(db: &Database, id: ID) -> Option<Self> {
        db.with_face_data(id, |data, index| {
            let has_color_tables = FontRef::from_index(data, index as usize)
                .map(|r| {
                    let has_colr = r.table(tag_from_bytes(b"COLR")).is_some()
                        && r.table(tag_from_bytes(b"CPAL")).is_some();
                    let has_cbdt = r.table(tag_from_bytes(b"CBDT")).is_some();
                    let has_sbix = r.table(tag_from_bytes(b"sbix")).is_some();
                    has_colr || has_cbdt || has_sbix
                })
                .unwrap_or(false);

            LoadedFont { id, index, has_color_tables }
        })
    }
}

/// Resolves fonts by family name with fallback support.
pub(crate) struct FontResolver {
    db: Database,
    /// Loaded font metadata in priority order (primary families first).
    fonts: Vec<LoadedFont>,
    /// Number of primary fonts (from constructor families).
    primary_count: usize,
}

impl FontResolver {
    /// Creates a font resolver with the given font families.
    ///
    /// Loads system fonts and resolves each family. At least one family
    /// must be found or an error is returned.
    pub(crate) fn new(font_families: &[&str]) -> Result<Self, Error> {
        let mut db = Database::new();
        db.load_system_fonts();

        let mut fonts: Vec<(ID, LoadedFont)> = Vec::new();

        for &family in font_families {
            // try all 4 style variants for each family
            for (weight, style) in [
                (Weight::NORMAL, Style::Normal),
                (Weight::BOLD, Style::Normal),
                (Weight::NORMAL, Style::Italic),
                (Weight::BOLD, Style::Italic),
            ] {
                let query = Query {
                    families: &[Family::Name(family)],
                    weight,
                    stretch: Stretch::Normal,
                    style,
                };

                if let Some(id) = db.query(&query)
                    && !fonts.iter().any(|f| f.0 == id)
                    && let Some(font) = LoadedFont::new(&db, id)
                {
                    fonts.push((id, font));
                }
            }
        }

        if fonts.is_empty() {
            return Err(Error::FontNotFound(font_families.join(", ")));
        }

        let primary_count = fonts.len();
        let fonts = fonts.into_iter().map(|(_, f)| f).collect();

        Ok(Self { db, fonts, primary_count })
    }

    /// Calls `f` with a [`FontRef`] for the primary font (normal weight, normal style).
    pub(crate) fn with_primary_font<T>(&self, f: impl FnOnce(FontRef<'_>) -> T) -> Option<T> {
        self.with_font(0, f)
    }

    /// Calls `f` with a [`FontRef`] for the font at the given index.
    pub(crate) fn with_font<T>(&self, idx: usize, f: impl FnOnce(FontRef<'_>) -> T) -> Option<T> {
        let font = self.fonts.get(idx)?;
        self.db
            .with_face_data(font.id, |data, _| {
                FontRef::from_index(data, font.index as usize).map(f)
            })
            .flatten()
    }

    /// Returns the number of primary fonts (loaded from the constructor
    /// families). Fonts at indices >= this value are system fallbacks.
    pub(crate) fn primary_count(&self) -> usize {
        self.primary_count
    }

    /// Returns true if any primary font contains the given character.
    #[cfg(test)]
    pub(crate) fn primary_has_char(&self, ch: char) -> bool {
        (0..self.primary_count).any(|idx| self.font_has_char(idx, ch))
    }

    /// Returns true if the font at `idx` contains the given character.
    fn font_has_char(&self, idx: usize, ch: char) -> bool {
        self.with_font(idx, |r| r.charmap().map(ch) != 0)
            .unwrap_or(false)
    }

    /// Resolves a font that contains the given character, trying primary fonts first.
    /// Prefers non-color fonts to avoid picking up emoji fallbacks for text characters.
    ///
    /// Returns the font index or `None` if no font covers the character.
    pub(crate) fn resolve_char(&mut self, ch: char) -> Option<usize> {
        self.resolve_char_inner(ch, ColorPreference::AvoidColor)
    }

    /// Like [`resolve_char`], but prefers fonts with color glyph support.
    pub(crate) fn resolve_color_char(&mut self, ch: char) -> Option<usize> {
        self.resolve_char_inner(ch, ColorPreference::PreferColor)
    }

    fn resolve_char_inner(&mut self, ch: char, pref: ColorPreference) -> Option<usize> {
        let mut first_match: Option<usize> = None;

        for idx in 0..self.fonts.len() {
            if self.font_has_char(idx, ch) {
                let dominated = match pref {
                    ColorPreference::PreferColor => !self.fonts[idx].has_color_tables,
                    ColorPreference::AvoidColor => self.fonts[idx].has_color_tables,
                };

                if !dominated {
                    return Some(idx);
                }
                if first_match.is_none() {
                    first_match = Some(idx);
                }
            }
        }

        // fallback: scan system fonts for one that covers this character
        if let Some(id) = self.find_fallback_font(ch) {
            let font = LoadedFont::new(&self.db, id)?;
            self.fonts.push(font);
            let idx = self.fonts.len() - 1;

            let dominated = match pref {
                ColorPreference::PreferColor => !self.fonts[idx].has_color_tables,
                ColorPreference::AvoidColor => self.fonts[idx].has_color_tables,
            };

            if !dominated {
                return Some(idx);
            }
            if first_match.is_none() {
                first_match = Some(idx);
            }
        }

        // no font matching preference found; fall back to first font that has the character
        first_match
    }

    /// Resolves the best font for the given character and style.
    ///
    /// For bold/italic, tries to find a matching style variant among the primary
    /// fonts. Falls back to the first font that has the character.
    pub(crate) fn resolve_styled(
        &mut self,
        ch: char,
        style: beamterm_data::FontStyle,
    ) -> Option<usize> {
        use beamterm_data::FontStyle;

        // determine preferred weight/style index
        let preferred_offset = match style {
            FontStyle::Normal => 0,
            FontStyle::Bold => 1,
            FontStyle::Italic => 2,
            FontStyle::BoldItalic => 3,
        };

        // check if the preferred style variant has the char
        if preferred_offset < self.primary_count && self.font_has_char(preferred_offset, ch) {
            return Some(preferred_offset);
        }

        // fall back to any font that has the character
        self.resolve_char(ch)
    }

    /// Returns the font family name for the font at the given index.
    pub(crate) fn font_family_name(&self, idx: usize) -> Option<String> {
        let font = self.fonts.get(idx)?;
        self.db
            .face(font.id)
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
    }

    fn find_fallback_font(&self, ch: char) -> Option<ID> {
        for face in self.db.faces() {
            let has_char = self.db.with_face_data(face.id, |data, index| {
                FontRef::from_index(data, index as usize)
                    .map(|font_ref| font_ref.charmap().map(ch) != 0)
                    .unwrap_or(false)
            });

            if has_char == Some(true) {
                return Some(face.id);
            }
        }

        None
    }
}
