use std::{cell::RefCell, collections::HashSet, fmt::Debug};

use compact_str::CompactString;

use crate::{DynamicFontAtlas, Error, StaticFontAtlas};

pub(super) type SlotId = u16;
pub(super) const STATIC_ATLAS_LOOKUP_MASK: u32 = 0x1FFF;
pub(super) const DYNAMIC_ATLAS_LOOKUP_MASK: u32 = 0x0FFF;

/// Trait defining the interface for font atlases.
pub(crate) trait Atlas {
    /// Returns the glyph identifier for the given key and style bits
    fn get_glyph_id(&self, key: &str, style_bits: u16) -> Option<u16>;

    /// Returns the base glyph identifier for the given key
    fn get_base_glyph_id(&self, key: &str) -> Option<u16>;

    /// Returns the height of the atlas in pixels.
    fn cell_size(&self) -> (i32, i32);

    /// Binds the font atlas texture to the specified texture unit.
    fn bind(&self, gl: &web_sys::WebGl2RenderingContext, texture_unit: u32);

    /// Returns the underline configuration
    fn underline(&self) -> beamterm_data::LineDecoration;

    /// Returns the strikethrough configuration
    fn strikethrough(&self) -> beamterm_data::LineDecoration;

    /// Returns the symbol for the given glyph ID, if it exists
    fn get_symbol(&self, glyph_id: u16) -> Option<CompactString>;

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
    fn flush(&self, gl: &web_sys::WebGl2RenderingContext) -> Result<(), Error>;

    /// Recreates the GPU texture after a WebGL context loss.
    ///
    /// This clears the cache - glyphs will be re-rasterized on next access.
    fn recreate_texture(&mut self, gl: &web_sys::WebGl2RenderingContext) -> Result<(), Error>;

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
    fn resolve_glyph_slot(&self, key: &str, style_bits: u16) -> Option<GlyphSlot>;

    /// Returns the bitmask for extracting the base glyph ID from a styled glyph ID.
    ///
    /// The glyph ID encodes both the base glyph index and style/effect flags. This mask
    /// isolates the base ID portion, which is used for texture coordinate calculation
    /// and symbol reverse-lookup.
    ///
    /// # Implementation Differences
    ///
    /// - **`StaticFontAtlas`** returns `0x1FFF` (13 bits):
    ///   - Bits 0-9: Base glyph ID (1024 values for regular glyphs)
    ///   - Bits 10-11: Reserved for font style derivation
    ///   - Bit 12: Emoji flag (included in mask for emoji base ID extraction)
    ///   - Supports the full glyph ID encoding scheme from `beamterm-atlas`
    ///
    /// - **`DynamicFontAtlas`** returns `0x0FFF` (12 bits):
    ///   - Uses a flat slot-based addressing (4096 total slots)
    ///   - Emoji are tracked via `GlyphSlot::Emoji` variant rather than a flag bit
    ///   - Simpler addressing since glyphs are assigned sequentially at runtime
    fn base_lookup_mask(&self) -> u32;

    /// Deletes the GPU texture resources associated with this atlas.
    ///
    /// This method must be called before dropping the atlas to properly clean up
    /// WebGL resources. Failing to call this will leak GPU memory.
    fn delete(&self, gl: &web_sys::WebGl2RenderingContext);

    /// Updates the pixel ratio for HiDPI rendering.
    ///
    /// Returns the effective pixel ratio that should be used for viewport scaling.
    /// Each atlas implementation decides how to handle the ratio:
    ///
    /// - **Static atlas**: Returns exact ratio, no internal work needed
    /// - **Dynamic atlas**: Returns exact ratio, reinitializes with scaled font size
    fn update_pixel_ratio(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        pixel_ratio: f32,
    ) -> Result<f32, Error>;

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
    fn texture_cell_size(&self) -> (i32, i32);
}

pub(crate) struct FontAtlas {
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
    pub(super) fn new(inner: impl Atlas + 'static) -> Self {
        Self { inner: Box::new(inner) }
    }

    pub(crate) fn get_glyph_id(&self, key: &str, style_bits: u16) -> Option<u16> {
        self.inner.get_glyph_id(key, style_bits)
    }

    pub(crate) fn get_base_glyph_id(&self, key: &str) -> Option<u16> {
        self.inner.get_base_glyph_id(key)
    }

    pub(crate) fn cell_size(&self) -> (i32, i32) {
        self.inner.cell_size()
    }

    pub(crate) fn bind(&self, gl: &web_sys::WebGl2RenderingContext, texture_unit: u32) {
        self.inner.bind(gl, texture_unit)
    }

    pub(crate) fn underline(&self) -> beamterm_data::LineDecoration {
        self.inner.underline()
    }

    pub(crate) fn strikethrough(&self) -> beamterm_data::LineDecoration {
        self.inner.strikethrough()
    }

    pub(crate) fn get_symbol(&self, glyph_id: u16) -> Option<CompactString> {
        self.inner.get_symbol(glyph_id)
    }

    pub(crate) fn glyph_tracker(&self) -> &GlyphTracker {
        self.inner.glyph_tracker()
    }

    pub(crate) fn glyph_count(&self) -> u32 {
        self.inner.glyph_count()
    }

    pub(crate) fn recreate_texture(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
    ) -> Result<(), Error> {
        self.inner.recreate_texture(gl)
    }

    pub(crate) fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str)) {
        self.inner.for_each_symbol(f)
    }

    pub(crate) fn resolve_glyph_slot(&self, key: &str, style_bits: u16) -> Option<GlyphSlot> {
        self.inner.resolve_glyph_slot(key, style_bits)
    }

    pub(crate) fn flush(&self, gl: &web_sys::WebGl2RenderingContext) -> Result<(), Error> {
        self.inner.flush(gl)
    }

    pub(super) fn base_lookup_mask(&self) -> u32 {
        self.inner.base_lookup_mask()
    }

    pub(super) fn space_glyph_id(&self) -> u16 {
        self.get_glyph_id(" ", 0x0)
            .expect("space glyph exists in every font atlas")
    }

    /// Deletes the GPU texture resources associated with this atlas.
    pub(crate) fn delete(&self, gl: &web_sys::WebGl2RenderingContext) {
        self.inner.delete(gl)
    }

    /// Updates the pixel ratio for HiDPI rendering.
    ///
    /// Returns the effective pixel ratio to use for viewport scaling.
    pub(crate) fn update_pixel_ratio(
        &mut self,
        gl: &web_sys::WebGl2RenderingContext,
        pixel_ratio: f32,
    ) -> Result<f32, Error> {
        self.inner.update_pixel_ratio(gl, pixel_ratio)
    }

    /// Returns the cell scale factor for layout calculations.
    pub(crate) fn cell_scale_for_dpr(&self, pixel_ratio: f32) -> f32 {
        self.inner.cell_scale_for_dpr(pixel_ratio)
    }

    /// Returns the texture cell size in physical pixels.
    pub(crate) fn texture_cell_size(&self) -> (i32, i32) {
        self.inner.texture_cell_size()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum GlyphSlot {
    Normal(SlotId),
    Wide(SlotId),
    Emoji(SlotId),
}

impl GlyphSlot {
    pub fn slot_id(&self) -> SlotId {
        match *self {
            GlyphSlot::Normal(id) | GlyphSlot::Wide(id) | GlyphSlot::Emoji(id) => id,
        }
    }

    pub fn with_styling(self, style_bits: u16) -> Self {
        use GlyphSlot::*;
        match self {
            Normal(id) => Normal(id | style_bits),
            Wide(id) => Wide(id | style_bits),
            Emoji(id) => Emoji(id | style_bits),
        }
    }

    /// Returns true if this is a double-width glyph (emoji or wide CJK).
    pub fn is_double_width(&self) -> bool {
        matches!(self, GlyphSlot::Wide(_) | GlyphSlot::Emoji(_))
    }
}

/// Tracks glyphs that were requested but not found in the font atlas.
#[derive(Debug, Default)]
pub(crate) struct GlyphTracker {
    missing: RefCell<HashSet<CompactString>>,
}

impl GlyphTracker {
    /// Creates a new empty glyph tracker.
    pub(crate) fn new() -> Self {
        Self { missing: RefCell::new(HashSet::new()) }
    }

    /// Records a glyph as missing.
    pub(crate) fn record_missing(&self, glyph: &str) {
        self.missing.borrow_mut().insert(glyph.into());
    }

    /// Returns a copy of all missing glyphs.
    pub(crate) fn missing_glyphs(&self) -> HashSet<CompactString> {
        self.missing.borrow().clone()
    }

    /// Clears all tracked missing glyphs.
    pub(crate) fn clear(&self) {
        self.missing.borrow_mut().clear();
    }

    /// Returns the number of unique missing glyphs.
    pub(crate) fn len(&self) -> usize {
        self.missing.borrow().len()
    }

    /// Returns true if no glyphs are missing.
    pub(crate) fn is_empty(&self) -> bool {
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
