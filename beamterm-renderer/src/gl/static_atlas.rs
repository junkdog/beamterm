use std::{borrow::Cow, collections::HashMap};

use beamterm_data::{FontAtlasData, FontStyle, Glyph, LineDecoration};
use compact_str::{CompactString, ToCompactString};
use web_sys::WebGl2RenderingContext;

use super::{
    GL, atlas,
    atlas::{Atlas, FontAtlas, GlyphSlot, GlyphTracker},
};
use crate::error::Error;

/// A texture atlas containing font glyphs for efficient WebGL text rendering.
///
/// `StaticFontAtlas` manages a WebGL 2D texture array where each layer contains a single
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
pub(crate) struct StaticFontAtlas {
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
    /// The last assigned halfwidth base glyph ID, before fullwidth
    last_halfwidth_base_glyph_id: u16,
    /// Retained atlas data for context loss recovery
    atlas_data: FontAtlasData,
}

impl StaticFontAtlas {
    /// Loads the default embedded font atlas.
    fn load_default(gl: &web_sys::WebGl2RenderingContext) -> Result<Self, Error> {
        let config = FontAtlasData::default();
        Self::load(gl, config)
    }

    /// Creates a TextureAtlas from a grid of equal-sized cells
    pub(crate) fn load(
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
            last_halfwidth_base_glyph_id: config.max_halfwidth_base_glyph_id,
            symbol_lookup,
            cell_size: (cell_width, cell_height),
            num_slices: num_slices as u32,
            underline: config.underline,
            strikethrough: config.strikethrough,
            glyph_tracker: GlyphTracker::new(),
            atlas_data: config,
        })
    }
}

impl Atlas for StaticFontAtlas {
    fn get_glyph_id(&self, key: &str, style_bits: u16) -> Option<u16> {
        let base_id = self.get_base_glyph_id(key)?;
        Some(base_id | style_bits)
    }

    /// Returns the base glyph identifier for the given key
    fn get_base_glyph_id(&self, key: &str) -> Option<u16> {
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

    fn cell_size(&self) -> (i32, i32) {
        let (w, h) = self.cell_size;
        (
            w - 2 * FontAtlasData::PADDING,
            h - 2 * FontAtlasData::PADDING,
        )
    }

    fn bind(&self, gl: &web_sys::WebGl2RenderingContext, texture_unit: u32) {
        self.texture.bind(gl, texture_unit);
    }

    /// Returns the underline configuration
    fn underline(&self) -> beamterm_data::LineDecoration {
        self.underline
    }

    /// Returns the strikethrough configuration
    fn strikethrough(&self) -> beamterm_data::LineDecoration {
        self.strikethrough
    }

    /// Returns the symbol for the given glyph ID, if it exists
    fn get_symbol(&self, glyph_id: u16) -> Option<CompactString> {
        let base_glyph_id = if glyph_id & Glyph::EMOJI_FLAG != 0 {
            glyph_id & Glyph::GLYPH_ID_EMOJI_MASK
        } else {
            glyph_id & Glyph::GLYPH_ID_MASK
        };

        if (0x20..0x80).contains(&base_glyph_id) {
            // ASCII characters are directly mapped to their code point
            let ch = base_glyph_id as u8 as char;
            Some(ch.to_compact_string())
        } else {
            self.symbol_lookup.get(&base_glyph_id).cloned()
        }
    }

    fn glyph_tracker(&self) -> &GlyphTracker {
        &self.glyph_tracker
    }

    fn glyph_count(&self) -> u32 {
        // ASCII printable characters: 0x20..0x80 (96 characters)
        let ascii_count = 0x80 - 0x20;
        // Non-ASCII glyphs stored in symbol_lookup
        let non_ascii_count = self.symbol_lookup.len() as u32;
        ascii_count + non_ascii_count
    }

    fn flush(&self, _gl: &WebGl2RenderingContext) -> Result<(), Error> {
        Ok(()) // static atlas has no pending glyphs
    }

    /// Recreates the GPU texture after a WebGL context loss.
    ///
    /// This method rebuilds the texture from the retained atlas data. All glyph
    /// mappings and other CPU-side state are preserved; only the GPU texture
    /// handle is recreated.
    ///
    /// # Parameters
    /// * `gl` - The new WebGL2 rendering context
    ///
    /// # Returns
    /// * `Ok(())` - Texture successfully recreated
    /// * `Err(Error)` - Failed to create texture
    fn recreate_texture(&mut self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        // Delete old texture if it exists (may be invalid after context loss)
        self.texture.delete(gl);

        // Recreate texture from retained atlas data
        self.texture =
            crate::gl::texture::Texture::from_font_atlas_data(gl, GL::RGBA, &self.atlas_data)?;

        Ok(())
    }

    fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str)) {
        // ASCII printable characters (0x20..0x80)
        for code in 0x20u16..0x80 {
            let ch = code as u8 as char;
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            f(code, s);
        }
        // Non-ASCII glyphs from symbol lookup
        for (glyph_id, symbol) in &self.symbol_lookup {
            f(*glyph_id, symbol.as_str());
        }
    }

    fn resolve_glyph_slot(&self, key: &str, style_bits: u16) -> Option<GlyphSlot> {
        if key.len() == 1 {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii() {
                // 0x00..0x7f double as layer
                let id = ch as u16;
                return Some(GlyphSlot::Normal(id | style_bits));
            }
        }

        match self.glyph_coords.get(key) {
            Some(base_glyph_id) => {
                let id = base_glyph_id | style_bits;
                if *base_glyph_id >= self.last_halfwidth_base_glyph_id {
                    Some(GlyphSlot::Wide(id))
                } else if id & Glyph::EMOJI_FLAG != 0 {
                    Some(GlyphSlot::Emoji(id))
                } else {
                    Some(GlyphSlot::Normal(id))
                }
            },
            None => {
                self.glyph_tracker.record_missing(key);
                None
            },
        }
    }

    /// Returns `0x1FFF` to support the full glyph encoding from `beamterm-atlas`.
    ///
    /// This 13-bit mask includes the emoji flag (bit 12) so that emoji base IDs
    /// can be extracted correctly for symbol lookup and texture coordinate calculation.
    fn base_lookup_mask(&self) -> u32 {
        atlas::STATIC_ATLAS_LOOKUP_MASK
    }

    fn delete(&self, gl: &WebGl2RenderingContext) {
        self.texture.delete(gl);
    }

    fn update_pixel_ratio(
        &mut self,
        _gl: &WebGl2RenderingContext,
        pixel_ratio: f32,
    ) -> Result<f32, Error> {
        // Static atlas doesn't need to do anything - cell scaling is handled by the grid
        Ok(pixel_ratio)
    }

    fn cell_scale_for_dpr(&self, pixel_ratio: f32) -> f32 {
        // snap to specific scale values to avoid arbitrary fractional scaling
        if pixel_ratio <= 0.5 { 0.5 } else { pixel_ratio.round().max(1.0) }
    }

    fn texture_cell_size(&self) -> (i32, i32) {
        // Static atlas texture size equals cell_size (fixed resolution)
        self.cell_size()
    }
}
