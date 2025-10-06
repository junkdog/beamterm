use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use beamterm_data::{FontAtlasData, FontStyle, Glyph};
use compact_str::{CompactString, ToCompactString};
use web_sys::console;

use crate::{error::Error, gl::GL};

/// A texture atlas containing font glyphs for efficient WebGL text rendering.
///
/// `FontAtlas` manages a WebGL 2D texture array where each layer contains a single
/// character glyph. This design enables efficient instanced rendering of text by
/// allowing the GPU to select the appropriate character layer for each rendered cell.
///
/// # Architecture
/// The atlas uses a **WebGL 2D texture array** where:
/// - Each layer contains one character glyph
/// - ASCII characters use their ASCII value as the layer index
/// - Non-ASCII characters are stored in a hash map for layer lookup
/// - All glyphs have uniform cell dimensions for consistent spacing
#[derive(Debug)]
pub struct FontAtlas {
    /// The underlying texture
    texture: crate::gl::texture::Texture,
    /// Symbol to 3d texture index
    glyph_coords: HashMap<CompactString, u16>,
    /// Base glyph identifier to symbol mapping
    symbol_lookup: HashMap<u16, CompactString>,
    /// The size of each character cell in pixels
    cell_size: (i32, i32),
    /// The number of slices in the atlas texture
    num_slices: u32,
    /// Underline configuration
    underline: beamterm_data::LineDecoration,
    /// Strikethrough configuration  
    strikethrough: beamterm_data::LineDecoration,
    /// Tracks glyphs that were requested but not found in the atlas
    glyph_tracker: GlyphTracker,
}

impl FontAtlas {
    /// Loads the default embedded font atlas.
    pub fn load_default(gl: &web_sys::WebGl2RenderingContext) -> Result<Self, Error> {
        let config = FontAtlasData::default();
        Self::load(gl, config)
    }

    /// Creates a TextureAtlas from a grid of equal-sized cells
    pub fn load(
        gl: &web_sys::WebGl2RenderingContext,
        config: FontAtlasData,
    ) -> Result<Self, Error> {
        let texture = crate::gl::texture::Texture::from_font_atlas_data(gl, GL::RGBA, &config)?;
        let num_slices = config.texture_dimensions.2;

        let texture_layers = config
            .glyphs
            .iter()
            .map(|g| g.id as i32)
            .max()
            .unwrap_or(0)
            + 1;

        let (cell_width, cell_height) = config.cell_size;
        let mut layers = HashMap::new();
        let mut symbol_lookup = HashMap::new();

        // we only store the normal-styled glyphs (incl emoji) in the atlas lookup,
        // as the correct layer id can be derived from the base glyph id plus font style.
        //
        // emoji are (currently all) double-width and occupy two consecutive glyph ids,
        // but we only store the first id in the lookup.
        config.glyphs.iter()
            .filter(|g| g.style == FontStyle::Normal) // only normal style glyphs
            .filter(|g| !g.is_ascii())                // only non-ascii glyphs
            .for_each(|g| {
                symbol_lookup.insert(g.id, g.symbol.clone());
                layers.insert(g.symbol.clone(), g.id);
            });

        Ok(Self {
            texture,
            glyph_coords: layers,
            symbol_lookup,
            cell_size: (cell_width, cell_height),
            num_slices: num_slices as u32,
            underline: config.underline,
            strikethrough: config.strikethrough,
            glyph_tracker: GlyphTracker::new(),
        })
    }

    /// Binds the atlas texture to the specified texture unit
    pub fn bind(&self, gl: &web_sys::WebGl2RenderingContext, texture_unit: u32) {
        self.texture.bind(gl, texture_unit);
    }

    pub fn cell_size(&self) -> (i32, i32) {
        let (w, h) = self.cell_size;
        (
            w - 2 * FontAtlasData::PADDING,
            h - 2 * FontAtlasData::PADDING,
        )
    }

    /// Returns the underline configuration
    pub fn underline(&self) -> beamterm_data::LineDecoration {
        self.underline
    }

    /// Returns the strikethrough configuration
    pub fn strikethrough(&self) -> beamterm_data::LineDecoration {
        self.strikethrough
    }

    /// Returns the symbol for the given glyph ID, if it exists
    pub fn get_symbol(&self, glyph_id: u16) -> Option<Cow<'_, str>> {
        let base_glyph_id = glyph_id & (Glyph::GLYPH_ID_MASK | Glyph::EMOJI_FLAG);

        if (0x20..0x80).contains(&base_glyph_id) {
            // ASCII characters are directly mapped to their code point
            let ch = base_glyph_id as u8 as char;
            Some(Cow::from(ch.to_compact_string()))
        } else {
            self.symbol_lookup
                .get(&base_glyph_id)
                .map(|s| Cow::from(s.as_str()))
        }
    }

    /// Returns the base glyph identifier for the given key
    pub fn get_base_glyph_id(&self, key: &str) -> Option<u16> {
        if key.len() == 1 {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii() {
                // 0x00..0x7f double as layer
                let id = ch as u16;
                return Some(id);
            }
        }

        match self.glyph_coords.get(key) {
            Some(id) => Some(*id),
            None => {
                self.glyph_tracker.record_missing(key);
                None
            },
        }
    }

    /// Returns a reference to the glyph tracker for accessing missing glyphs.
    pub fn glyph_tracker(&self) -> &GlyphTracker {
        &self.glyph_tracker
    }

    /// Returns the total number of glyphs available in the atlas.
    /// This includes ASCII characters (0x20..0x80) plus non-ASCII glyphs.
    pub(crate) fn glyph_count(&self) -> u32 {
        // ASCII printable characters: 0x20..0x80 (96 characters)
        let ascii_count = 0x80 - 0x20;
        // Non-ASCII glyphs stored in symbol_lookup
        let non_ascii_count = self.symbol_lookup.len() as u32;
        ascii_count + non_ascii_count
    }

    pub(crate) fn get_symbol_lookup(&self) -> &HashMap<u16, CompactString> {
        &self.symbol_lookup
    }
}

/// Tracks glyphs that were requested but not found in the font atlas.
#[derive(Debug, Default)]
pub struct GlyphTracker {
    missing: RefCell<HashSet<CompactString>>,
}

impl GlyphTracker {
    /// Creates a new empty glyph tracker.
    pub fn new() -> Self {
        Self { missing: RefCell::new(HashSet::new()) }
    }

    /// Records a glyph as missing.
    pub fn record_missing(&self, glyph: &str) {
        self.missing.borrow_mut().insert(glyph.into());
    }

    /// Returns a copy of all missing glyphs.
    pub fn missing_glyphs(&self) -> HashSet<CompactString> {
        self.missing.borrow().clone()
    }

    /// Clears all tracked missing glyphs.
    pub fn clear(&self) {
        self.missing.borrow_mut().clear();
    }

    /// Returns the number of unique missing glyphs.
    pub fn len(&self) -> usize {
        self.missing.borrow().len()
    }

    /// Returns true if no glyphs are missing.
    pub fn is_empty(&self) -> bool {
        self.missing.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_tracker() {
        let tracker = GlyphTracker::new();

        // Initially empty
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);

        // Record some missing glyphs
        tracker.record_missing("🎮");
        tracker.record_missing("🎯");
        tracker.record_missing("🎮"); // Duplicate

        assert!(!tracker.is_empty());
        assert_eq!(tracker.len(), 2); // Only unique glyphs

        // Check the missing glyphs
        let missing = tracker.missing_glyphs();
        assert!(missing.contains(&CompactString::new("🎮")));
        assert!(missing.contains(&CompactString::new("🎯")));

        // Clear and verify
        tracker.clear();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }
}
