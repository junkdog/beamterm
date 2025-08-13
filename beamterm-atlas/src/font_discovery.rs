use std::collections::HashMap;

use cosmic_text::{fontdb, FontSystem, Style, Weight};

#[derive(Debug, Clone, PartialEq)]
pub struct FontFamily {
    pub name: String,
    pub fonts: FontVariants,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontVariants {
    pub regular: fontdb::ID,
    pub bold: fontdb::ID,
    pub italic: fontdb::ID,
    pub bold_italic: fontdb::ID,
}

pub struct FontDiscovery {
    font_system: FontSystem,
}

impl FontDiscovery {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();

        // load system fonts
        let db = font_system.db_mut();
        db.load_system_fonts();

        Self { font_system }
    }

    /// Discovers all monospaced font families that have all 4 required variants
    pub fn discover_complete_monospace_families(&self) -> Vec<FontFamily> {
        let db = self.font_system.db();
        let mut families: HashMap<String, HashMap<(Weight, Style), fontdb::ID>> = HashMap::new();

        // group fonts by family name
        for face in db.faces().filter(|f| f.monospaced) {
            let family_name = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            let variants = families.entry(family_name).or_default();

            // map the font properties to our required variants
            let key = (face.weight, face.style);
            variants.insert(key, face.id);
        }

        // filter families that have all 4 required variants
        let mut complete_families = Vec::new();

        for (name, variants) in families {
            let regular = variants.get(&(Weight::NORMAL, Style::Normal));
            let bold = variants.get(&(Weight::BOLD, Style::Normal));
            let italic = variants.get(&(Weight::NORMAL, Style::Italic));
            let bold_italic = variants.get(&(Weight::BOLD, Style::Italic));

            if let (Some(&regular), Some(&bold), Some(&italic), Some(&bold_italic)) =
                (regular, bold, italic, bold_italic)
            {
                complete_families.push(FontFamily {
                    name,
                    fonts: FontVariants { regular, bold, italic, bold_italic },
                });
            }
        }

        complete_families.sort_by(|a, b| a.name.cmp(&b.name));
        complete_families
    }

    /// Loads a specific font family into the generator's font system
    pub fn load_font_family(
        font_system: &mut FontSystem,
        family: &FontFamily,
    ) -> Result<(), String> {
        let db = font_system.db();

        let all_fonts = [
            family.fonts.regular,
            family.fonts.bold,
            family.fonts.italic,
            family.fonts.bold_italic,
        ];

        // verify all fonts exist
        for id in all_fonts.into_iter() {
            if db.face(id).is_none() {
                return Err(format!("Font ID {id} not found in database"));
            }
        }

        Ok(())
    }

    pub fn into_font_system(self) -> FontSystem {
        self.font_system
    }
}

impl Default for FontDiscovery {
    fn default() -> Self {
        Self::new()
    }
}
