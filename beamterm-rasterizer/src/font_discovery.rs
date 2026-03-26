use std::collections::HashMap;

use fontdb::{Database, ID, Style, Weight};

/// A font family with all 4 required style variants.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFamily {
    pub name: String,
    pub fonts: FontVariants,
}

/// Font IDs for each style variant within a family.
#[derive(Debug, Clone, PartialEq)]
pub struct FontVariants {
    pub regular: ID,
    pub bold: ID,
    pub italic: ID,
    pub bold_italic: ID,
}

/// Discovers and enumerates system fonts.
pub struct FontDiscovery {
    db: Database,
}

impl FontDiscovery {
    pub fn new() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();

        Self { db }
    }

    /// Discovers all monospaced font families that have all 4 required variants
    /// (Regular, Bold, Italic, Bold+Italic).
    pub fn discover_complete_monospace_families(&self) -> Vec<FontFamily> {
        let mut families: HashMap<String, HashMap<(Weight, Style), ID>> = HashMap::new();

        // group fonts by family name
        for face in self.db.faces().filter(|f| f.monospaced) {
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

    /// Find a font family by name (partial match, case-insensitive).
    /// Returns the actual font family name if found.
    pub fn find_font(&self, font_name: &str) -> Option<String> {
        let font_name_lower = font_name.to_lowercase();

        self.db.faces().find_map(|face| {
            face.families
                .iter()
                .find(|(name, _)| name.to_lowercase().contains(&font_name_lower))
                .map(|(name, _)| name.clone())
        })
    }

    /// Get all unique font family names in the system (sorted).
    pub fn list_all_fonts(&self) -> Vec<String> {
        let mut font_names: Vec<String> = self
            .db
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();

        font_names.sort();
        font_names.dedup();
        font_names
    }
}

impl Default for FontDiscovery {
    fn default() -> Self {
        Self::new()
    }
}
