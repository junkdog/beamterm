use std::{collections::HashSet, fmt::Debug};

use compact_str::CompactString;

use crate::Error;

/// Prevents external implementations of the [`Atlas`] trait.
///
/// This module is not part of the public API.
#[doc(hidden)]
pub mod sealed {
    /// Sealed marker trait. Cannot be implemented outside of beamterm crates.
    pub trait Sealed {}
}

pub type SlotId = u16;
/// Bitmask for extracting the base glyph slot from a styled glyph ID.
///
/// Both static and dynamic atlases use 13 bits (0x1FFF) for texture addressing.
/// The emoji flag lives above this mask: bit 12 for static atlas (naturally part
/// of slot address for emoji at slots >= 4096), bit 15 for dynamic atlas.
pub(crate) const GLYPH_SLOT_MASK: u32 = 0x1FFF;

/// Trait defining the interface for font atlases.
///
/// This trait is **sealed** and cannot be implemented outside of beamterm crates.
///
/// Methods that may mutate internal state (glyph resolution, cache updates,
/// texture uploads) take `&mut self`. Read-only accessors take `&self`.
pub trait Atlas: sealed::Sealed {
    /// Returns the glyph identifier for the given key and style bits.
    ///
    /// May mutate internal state (e.g., LRU promotion in dynamic atlases,
    /// recording missing glyphs in static atlases).
    fn get_glyph_id(&mut self, key: &str, style_bits: u16) -> Option<u16>;

    /// Returns the base glyph identifier for the given key.
    ///
    /// May mutate internal state (e.g., LRU promotion, missing glyph tracking).
    fn get_base_glyph_id(&mut self, key: &str) -> Option<u16>;

    /// Returns the height of the atlas in pixels.
    fn cell_size(&self) -> beamterm_data::CellSize;

    /// Binds the font atlas texture to the currently active texture unit.
    fn bind(&self, gl: &glow::Context);

    /// Returns the underline configuration
    fn underline(&self) -> beamterm_data::LineDecoration;

    /// Returns the strikethrough configuration
    fn strikethrough(&self) -> beamterm_data::LineDecoration;

    /// Returns the symbol for the given glyph ID, if it exists
    fn get_symbol(&self, glyph_id: u16) -> Option<CompactString>;

    /// Returns the ASCII character for the given glyph ID, if it represents an ASCII char.
    ///
    /// This is an optimized path for URL detection that avoids string allocation.
    fn get_ascii_char(&self, glyph_id: u16) -> Option<char>;

    /// Returns a reference to the glyph tracker for accessing missing glyphs.
    fn glyph_tracker(&self) -> &GlyphTracker;

    /// Returns the number of glyphs currently in the atlas.
    fn glyph_count(&self) -> u32;

    /// Flushes any pending glyph data to the GPU texture.
    ///
    /// For dynamic atlases, this rasterizes and uploads queued glyphs that were
    /// allocated during [`resolve_glyph_slot`] calls. Must be called after the
    /// atlas texture is bound and before rendering.
    ///
    /// For static atlases, this is a no-op since all glyphs are pre-loaded.
    ///
    /// # Errors
    /// Returns an error if texture upload fails.
    fn flush(&mut self, gl: &glow::Context) -> Result<(), Error>;

    /// Recreates the GPU texture after a context loss.
    ///
    /// This clears the cache - glyphs will be re-rasterized on next access.
    ///
    /// # Errors
    /// Returns an error if GPU texture creation fails.
    fn recreate_texture(&mut self, gl: &glow::Context) -> Result<(), Error>;

    /// Iterates over all glyph ID to symbol mappings.
    ///
    /// Calls the provided closure for each (glyph_id, symbol) pair in the atlas.
    /// This is used for debugging and exposing the atlas contents to JavaScript.
    fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str));

    /// Resolves a glyph to its texture slot.
    ///
    /// For static atlases, performs a lookup and returns `None` if not found.
    ///
    /// For dynamic atlases, allocates a slot if missing and queues for upload.
    /// The slot is immediately valid, but [`flush`] must be called before
    /// rendering to populate the texture.
    fn resolve_glyph_slot(&mut self, key: &str, style_bits: u16) -> Option<GlyphSlot>;

    /// Returns the bit position used for emoji detection in the fragment shader.
    ///
    /// The glyph ID encodes the base slot index (bits 0-12, masked by `0x1FFF`)
    /// plus effect/flag bits above that. The emoji bit tells the shader to use
    /// texture color (emoji) vs foreground color (regular text).
    ///
    /// - **`StaticFontAtlas`** returns `12`: emoji are at slots >= 4096, so bit 12
    ///   is naturally set in their slot address.
    /// - **`DynamicFontAtlas`** returns `15`: emoji flag is stored in bit 15,
    ///   outside the 13-bit slot mask, leaving bits 13-14 for underline/strikethrough.
    fn emoji_bit(&self) -> u32;

    /// Deletes the GPU texture resources associated with this atlas.
    ///
    /// This method must be called before dropping the atlas to properly clean up
    /// GPU resources. Failing to call this will leak GPU memory.
    fn delete(&self, gl: &glow::Context);

    /// Updates the pixel ratio for HiDPI rendering.
    ///
    /// Returns the effective pixel ratio that should be used for viewport scaling.
    /// Each atlas implementation decides how to handle the ratio:
    ///
    /// - **Static atlas**: Returns exact ratio, no internal work needed
    /// - **Dynamic atlas**: Returns exact ratio, reinitializes with scaled font size
    ///
    /// # Errors
    /// Returns an error if GPU texture recreation fails during reinitialization.
    fn update_pixel_ratio(&mut self, gl: &glow::Context, pixel_ratio: f32) -> Result<f32, Error>;

    /// Returns the cell scale factor for layout calculations at the given DPR.
    ///
    /// This determines how cells from `cell_size()` should be scaled for layout:
    ///
    /// - **Static atlas**: Returns snapped scale values (0.5, 1.0, 2.0, 3.0, etc.)
    ///   to avoid arbitrary fractional scaling of pre-rasterized glyphs.
    ///   DPR <= 0.5 snaps to 0.5, otherwise rounds to nearest integer (minimum 1.0).
    /// - **Dynamic atlas**: Returns `1.0` - glyphs are re-rasterized at the exact DPR,
    ///   so `cell_size()` already returns the correctly-scaled physical size
    ///
    /// # Contract
    ///
    /// - Return value is always >= 0.5
    /// - The effective cell size for layout is `cell_size() * cell_scale_for_dpr(dpr)`
    /// - Static atlases use snapped scaling to preserve glyph sharpness
    /// - Dynamic atlases handle DPR internally via re-rasterization
    fn cell_scale_for_dpr(&self, pixel_ratio: f32) -> f32;

    /// Returns the texture cell size in physical pixels (for fragment shader calculations).
    ///
    /// This is used for computing padding fractions in the shader, which need to be
    /// based on the actual texture dimensions rather than logical layout dimensions.
    ///
    /// - **Static atlas**: Same as `cell_size()` (texture is at fixed resolution)
    /// - **Dynamic atlas**: Physical cell size (before dividing by pixel_ratio)
    fn texture_cell_size(&self) -> beamterm_data::CellSize;
}

pub struct FontAtlas {
    inner: Box<dyn Atlas>,
}

impl<A: Atlas + 'static> From<A> for FontAtlas {
    fn from(atlas: A) -> Self {
        FontAtlas::new(atlas)
    }
}

impl Debug for FontAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontAtlas")
            .finish_non_exhaustive()
    }
}

impl FontAtlas {
    pub fn new(inner: impl Atlas + 'static) -> Self {
        Self { inner: Box::new(inner) }
    }

    pub fn get_glyph_id(&mut self, key: &str, style_bits: u16) -> Option<u16> {
        self.inner.get_glyph_id(key, style_bits)
    }

    pub fn get_base_glyph_id(&mut self, key: &str) -> Option<u16> {
        self.inner.get_base_glyph_id(key)
    }

    #[must_use]
    pub fn cell_size(&self) -> beamterm_data::CellSize {
        self.inner.cell_size()
    }

    pub fn bind(&self, gl: &glow::Context) {
        self.inner.bind(gl);
    }

    #[must_use]
    pub fn underline(&self) -> beamterm_data::LineDecoration {
        self.inner.underline()
    }

    #[must_use]
    pub fn strikethrough(&self) -> beamterm_data::LineDecoration {
        self.inner.strikethrough()
    }

    #[must_use]
    pub fn get_symbol(&self, glyph_id: u16) -> Option<CompactString> {
        self.inner.get_symbol(glyph_id)
    }

    #[must_use]
    pub fn get_ascii_char(&self, glyph_id: u16) -> Option<char> {
        self.inner.get_ascii_char(glyph_id)
    }

    #[must_use]
    pub fn glyph_tracker(&self) -> &GlyphTracker {
        self.inner.glyph_tracker()
    }

    #[must_use]
    pub fn glyph_count(&self) -> u32 {
        self.inner.glyph_count()
    }

    /// Recreates the GPU texture after a context loss.
    ///
    /// # Errors
    /// Returns an error if GPU texture creation fails.
    pub fn recreate_texture(&mut self, gl: &glow::Context) -> Result<(), Error> {
        self.inner.recreate_texture(gl)
    }

    pub fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str)) {
        self.inner.for_each_symbol(f);
    }

    pub fn resolve_glyph_slot(&mut self, key: &str, style_bits: u16) -> Option<GlyphSlot> {
        self.inner.resolve_glyph_slot(key, style_bits)
    }

    /// Flushes pending glyph data to the GPU texture.
    ///
    /// # Errors
    /// Returns an error if texture upload fails.
    pub fn flush(&mut self, gl: &glow::Context) -> Result<(), Error> {
        self.inner.flush(gl)
    }

    pub(crate) fn emoji_bit(&self) -> u32 {
        self.inner.emoji_bit()
    }

    pub(crate) fn space_glyph_id(&mut self) -> u16 {
        self.get_glyph_id(" ", 0x0)
            .expect("space glyph exists in every font atlas")
    }

    /// Deletes the GPU texture resources associated with this atlas.
    pub fn delete(&self, gl: &glow::Context) {
        self.inner.delete(gl);
    }

    /// Updates the pixel ratio for HiDPI rendering.
    ///
    /// Returns the effective pixel ratio to use for viewport scaling.
    ///
    /// # Errors
    /// Returns an error if GPU texture recreation fails during reinitialization.
    pub fn update_pixel_ratio(
        &mut self,
        gl: &glow::Context,
        pixel_ratio: f32,
    ) -> Result<f32, Error> {
        self.inner.update_pixel_ratio(gl, pixel_ratio)
    }

    /// Returns the cell scale factor for layout calculations.
    #[must_use]
    pub fn cell_scale_for_dpr(&self, pixel_ratio: f32) -> f32 {
        self.inner.cell_scale_for_dpr(pixel_ratio)
    }

    /// Returns the texture cell size in physical pixels.
    #[must_use]
    pub fn texture_cell_size(&self) -> beamterm_data::CellSize {
        self.inner.texture_cell_size()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum GlyphSlot {
    Normal(SlotId),
    Wide(SlotId),
    Emoji(SlotId),
}

impl GlyphSlot {
    #[must_use]
    pub fn slot_id(&self) -> SlotId {
        match *self {
            GlyphSlot::Normal(id) | GlyphSlot::Wide(id) | GlyphSlot::Emoji(id) => id,
        }
    }

    #[must_use]
    pub fn with_styling(self, style_bits: u16) -> Self {
        use GlyphSlot::*;
        match self {
            Normal(id) => Normal(id | style_bits),
            Wide(id) => Wide(id | style_bits),
            Emoji(id) => Emoji(id | style_bits),
        }
    }

    /// Returns true if this is a double-width glyph (emoji or wide CJK).
    #[must_use]
    pub fn is_double_width(&self) -> bool {
        matches!(self, GlyphSlot::Wide(_) | GlyphSlot::Emoji(_))
    }
}

/// Tracks glyphs that were requested but not found in the font atlas.
#[derive(Debug, Default)]
pub struct GlyphTracker {
    missing: HashSet<CompactString>,
}

impl GlyphTracker {
    /// Creates a new empty glyph tracker.
    #[must_use]
    pub fn new() -> Self {
        Self { missing: HashSet::new() }
    }

    /// Records a glyph as missing.
    pub fn record_missing(&mut self, glyph: &str) {
        self.missing.insert(glyph.into());
    }

    /// Returns a copy of all missing glyphs.
    #[must_use]
    pub fn missing_glyphs(&self) -> HashSet<CompactString> {
        self.missing.clone()
    }

    /// Clears all tracked missing glyphs.
    pub fn clear(&mut self) {
        self.missing.clear();
    }

    /// Returns the number of unique missing glyphs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.missing.len()
    }

    /// Returns true if no glyphs are missing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_tracker() {
        let mut tracker = GlyphTracker::new();

        // Initially empty
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);

        // Record some missing glyphs
        tracker.record_missing("\u{1F3AE}");
        tracker.record_missing("\u{1F3AF}");
        tracker.record_missing("\u{1F3AE}"); // Duplicate

        assert!(!tracker.is_empty());
        assert_eq!(tracker.len(), 2); // Only unique glyphs

        // Check the missing glyphs
        let missing = tracker.missing_glyphs();
        assert!(missing.contains(&CompactString::new("\u{1F3AE}")));
        assert!(missing.contains(&CompactString::new("\u{1F3AF}")));

        // Clear and verify
        tracker.clear();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }
}
