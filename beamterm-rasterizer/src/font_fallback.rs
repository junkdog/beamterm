use fontdb::{Database, Family, ID, Query, Stretch, Style, Weight};
use swash::FontRef;

use crate::error::Error;

/// A loaded font kept alive so swash can borrow from it.
struct LoadedFont {
    data: Vec<u8>,
    index: u32,
}

impl LoadedFont {
    fn as_font_ref(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index as usize)
    }

    /// Returns true if this font contains the given character.
    fn has_char(&self, ch: char) -> bool {
        self.as_font_ref()
            .map(|r| r.charmap().map(ch) != 0)
            .unwrap_or(false)
    }
}

/// Resolves fonts by family name with fallback support.
pub(crate) struct FontResolver {
    db: Database,
    /// Loaded font data in priority order (primary families first).
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

        let mut fonts = Vec::new();

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
                    && !fonts.iter().any(|f: &(ID, LoadedFont)| f.0 == id)
                    && let Some(font) = Self::load_font(&db, id)
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

    /// Returns a font reference for the primary font (normal weight, normal style).
    pub(crate) fn primary_font(&self) -> FontRef<'_> {
        self.fonts[0]
            .as_font_ref()
            .expect("primary font always valid")
    }

    /// Returns the number of primary fonts (loaded from the constructor
    /// families). Fonts at indices >= this value are system fallbacks.
    pub(crate) fn primary_count(&self) -> usize {
        self.primary_count
    }

    /// Returns true if any primary font contains the given character.
    #[cfg(test)]
    pub(crate) fn primary_has_char(&self, ch: char) -> bool {
        self.fonts[..self.primary_count]
            .iter()
            .any(|f| f.has_char(ch))
    }

    /// Resolves a font that contains the given character, trying primary fonts first.
    ///
    /// Returns `(FontRef, font_index)` or `None` if no font covers the character.
    pub(crate) fn resolve_char(&mut self, ch: char) -> Option<(FontRef<'_>, usize)> {
        // find index of a loaded font that has the char
        let found_idx = self
            .fonts
            .iter()
            .enumerate()
            .find(|(_, font)| font.has_char(ch))
            .map(|(idx, _)| idx);

        if let Some(idx) = found_idx {
            let font_ref = self.fonts[idx].as_font_ref()?;
            return Some((font_ref, idx));
        }

        // fallback: scan system fonts for one that covers this character
        let id = self.find_fallback_font(ch)?;
        let font = Self::load_font(&self.db, id)?;
        self.fonts.push(font);
        let idx = self.fonts.len() - 1;
        let font_ref = self.fonts[idx].as_font_ref()?;
        Some((font_ref, idx))
    }

    /// Resolves the best font for the given character and style.
    ///
    /// For bold/italic, tries to find a matching style variant among the primary
    /// fonts. Falls back to the first font that has the character.
    pub(crate) fn resolve_styled(
        &mut self,
        ch: char,
        style: beamterm_data::FontStyle,
    ) -> Option<(FontRef<'_>, usize)> {
        use beamterm_data::FontStyle;

        // determine preferred weight/style index
        let preferred_offset = match style {
            FontStyle::Normal => 0,
            FontStyle::Bold => 1,
            FontStyle::Italic => 2,
            FontStyle::BoldItalic => 3,
        };

        // check if the preferred style variant has the char (without borrowing self mutably)
        let preferred_has_char =
            preferred_offset < self.primary_count && self.fonts[preferred_offset].has_char(ch);

        if preferred_has_char {
            let font_ref = self.fonts[preferred_offset].as_font_ref()?;
            return Some((font_ref, preferred_offset));
        }

        // fall back to any font that has the character
        self.resolve_char(ch)
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

    fn load_font(db: &Database, id: ID) -> Option<LoadedFont> {
        db.with_face_data(id, |data, index| LoadedFont { data: data.to_vec(), index })
    }
}
