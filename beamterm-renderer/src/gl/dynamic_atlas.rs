use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    ops::Not,
};

use beamterm_data::{DebugSpacePattern, FontAtlasData, FontStyle, Glyph, LineDecoration};
use compact_str::{CompactString, CompactStringExt, ToCompactString, format_compact};
use web_sys::WebGl2RenderingContext;

use super::{
    GL, atlas,
    atlas::{Atlas, GlyphSlot, GlyphTracker, SlotId},
    canvas_rasterizer::{CanvasRasterizer, RasterizedGlyph},
    glyph_cache::GlyphCache,
    texture::Texture,
};
use crate::error::Error;

/// Glyphs per layer (1x32 vertical grid)
const GLYPHS_PER_LAYER: usize = 32;
/// Total number of glyph slots (2048 normal + 2048 wide)
const TOTAL_SLOTS: usize = 4096;
/// Number of texture layers in the atlas
const NUM_LAYERS: i32 = (TOTAL_SLOTS / GLYPHS_PER_LAYER) as i32; // 128 layers

/// A dynamic texture atlas that rasterizes font glyphs on demand.
///
/// Unlike the static `FontAtlas` which loads pre-generated atlas data,
/// `DynamicFontAtlas` uses the browser's canvas API to rasterize glyphs
/// as they are encountered. This enables:
/// - Runtime font selection without pre-generation
/// - Support for any system font
/// - Automatic handling of emoji and complex scripts
///
/// # Architecture
/// The atlas uses a **WebGL 2D texture array** where:
/// - 128 layers × 32 glyphs per layer = 4096 total slots
/// - LRU-based slot allocation with eviction when full
/// - Double-width glyphs (emoji, CJK) occupy 2 consecutive slots
/// - Glyphs are rasterized on first use and cached in the texture
pub(crate) struct DynamicFontAtlas {
    /// The underlying WebGL texture array (fixed size, allocated once)
    texture: Texture,
    /// The canvas rasterizer for on-demand glyph generation
    rasterizer: CanvasRasterizer,
    /// Cache mapping graphemes to texture slots (RefCell for interior mutability)
    cache: RefCell<GlyphCache>,
    /// Reverse lookup: slot ID to grapheme symbol
    symbol_lookup: RefCell<HashMap<u16, CompactString>>,
    /// Tracks glyphs pending rasterization/upload
    glyphs_pending_upload: PendingUploads,
    /// The size of each character cell in physical pixels (without padding)
    physical_cell_size: (i32, i32),
    /// Tracks glyphs that were requested but couldn't be rasterized
    glyph_tracker: GlyphTracker,
    /// Underline configuration
    underline: LineDecoration,
    /// Strikethrough configuration
    strikethrough: LineDecoration,
    /// Debug pattern for space glyph (for pixel-perfect validation)
    debug_space_pattern: Option<DebugSpacePattern>,
    /// Base font size before pixel ratio scaling (user-specified)
    base_font_size: f32,
    /// Current pixel ratio for HiDPI rendering
    pixel_ratio: f32,
}

impl DynamicFontAtlas {
    /// Creates a new dynamic font atlas with the specified font settings.
    ///
    /// # Arguments
    /// * `gl` - WebGL2 rendering context
    /// * `font_family` - CSS font-family string (e.g., "'JetBrains Mono', monospace")
    /// * `font_size` - Base font size in logical pixels (before pixel ratio scaling)
    /// * `pixel_ratio` - Device pixel ratio for HiDPI rendering
    /// * `debug_space_pattern` - Optional checkered pattern for space glyph (for pixel-perfect validation)
    pub(crate) fn new(
        gl: &web_sys::WebGl2RenderingContext,
        font_family: &[CompactString],
        font_size: f32,
        pixel_ratio: f32,
        debug_space_pattern: Option<DebugSpacePattern>,
    ) -> Result<Self, Error> {
        let font_family_css = font_family
            .iter()
            .map(|s| format_compact!("'{s}'"))
            .join_compact(", ");

        // Scale font size by pixel ratio for crisp HiDPI rendering
        let effective_font_size = font_size * pixel_ratio;
        let rasterizer = CanvasRasterizer::new(&font_family_css, effective_font_size)?;
        let physical_cell_size = Self::measure_cell_size(&rasterizer)?;

        let padded_cell_size = (
            physical_cell_size.0 + FontAtlasData::PADDING * 2,
            physical_cell_size.1 + FontAtlasData::PADDING * 2,
        );
        let texture = Texture::for_dynamic_font_atlas(gl, GL::RGBA, padded_cell_size, NUM_LAYERS)?;

        let atlas = Self {
            texture,
            rasterizer,
            cache: RefCell::new(GlyphCache::new()),
            symbol_lookup: RefCell::new(HashMap::new()),
            glyphs_pending_upload: PendingUploads::new(),
            physical_cell_size,
            glyph_tracker: GlyphTracker::new(),
            underline: LineDecoration::new(0.9, 0.05), // near bottom, thin
            strikethrough: LineDecoration::new(0.5, 0.05), // middle, thin
            debug_space_pattern,
            base_font_size: font_size,
            pixel_ratio,
        };
        atlas.upload_ascii_glyphs(gl)?;

        Ok(atlas)
    }

    /// Uploads pending glyphs to the texture.
    ///
    /// Rasterizes and uploads glyphs in batches sized to fit the canvas height.
    /// Double-width glyphs (emoji, CJK) are split into left/right halves
    /// and uploaded to two consecutive slots.
    fn upload_pending_glyphs(&self, gl: &web_sys::WebGl2RenderingContext) -> Result<(), Error> {
        use super::canvas_rasterizer::RasterizedGlyph;

        if self.glyphs_pending_upload.is_empty() {
            return Ok(());
        }

        let pending = self
            .glyphs_pending_upload
            .take(self.rasterizer.max_batch_size());

        // build grapheme/style pairs for rasterization
        let graphemes: Vec<(&str, FontStyle)> = pending
            .iter()
            .map(|g| (g.key.as_str(), g.style))
            .collect();

        let rasterized = self.rasterizer.rasterize(&graphemes)?;

        self.upload_glyphs(gl, pending, rasterized)?;

        Ok(())
    }

    fn upload_ascii_glyphs(&self, gl: &web_sys::WebGl2RenderingContext) -> Result<(), Error> {
        let batch_size = self.rasterizer.max_batch_size();

        let all_pending: Vec<PendingGlyph> = (0x20u8..=0x7Eu8)
            .map(|b| PendingGlyph {
                slot: GlyphSlot::Normal(b as u16 - 0x20), // occupy first 95 slots
                key: CompactString::from_utf8([b]).expect("valid ascii"),
                style: FontStyle::Normal,
            })
            .collect();

        for batch in all_pending.chunks(batch_size) {
            let batch_vec: Vec<PendingGlyph> = batch.to_vec();

            // build grapheme/style pairs for rasterization
            let graphemes: Vec<(&str, FontStyle)> = batch_vec
                .iter()
                .map(|g| (g.key.as_str(), g.style))
                .collect();

            let rasterized = self.rasterizer.rasterize(&graphemes)?;

            self.upload_glyphs(gl, batch_vec, rasterized)?;
        }

        Ok(())
    }

    fn upload_glyphs(
        &self,
        gl: &WebGl2RenderingContext,
        pending: Vec<PendingGlyph>,
        rasterized: Vec<RasterizedGlyph>,
    ) -> Result<(), Error> {
        let padded_cell_size = (
            self.physical_cell_size.0 + FontAtlasData::PADDING * 2,
            self.physical_cell_size.1 + FontAtlasData::PADDING * 2,
        );
        let cell_w = padded_cell_size.0 as u32;
        let cell_h = padded_cell_size.1 as u32;

        // upload rasterized glyphs
        for (pending_glyph, glyph_data) in pending.iter().zip(rasterized.iter()) {
            // Replace space glyph with checkered pattern if debug mode is enabled
            let glyph_data = if pending_glyph.key == " " {
                if let Some(pattern) = self.debug_space_pattern {
                    std::borrow::Cow::Owned(generate_checkered_glyph(cell_w, cell_h, pattern))
                } else {
                    std::borrow::Cow::Borrowed(glyph_data)
                }
            } else {
                std::borrow::Cow::Borrowed(glyph_data)
            };

            if pending_glyph.slot.is_double_width() {
                // split double-width glyph into left and right halves
                let (left, right) = split_double_width_glyph(&glyph_data, cell_w, cell_h);
                let slot_id = pending_glyph.slot.slot_id() & Glyph::EMOJI_FLAG.not();
                self.texture
                    .upload_glyph(gl, slot_id, padded_cell_size, &left)?;
                self.texture
                    .upload_glyph(gl, slot_id + 1, padded_cell_size, &right)?;
            } else {
                self.texture.upload_glyph(
                    gl,
                    pending_glyph.slot.slot_id(),
                    padded_cell_size,
                    &glyph_data,
                )?;
            }
        }
        Ok(())
    }

    fn measure_cell_size(rasterizer: &CanvasRasterizer) -> Result<(i32, i32), Error> {
        let reference_glyphs = rasterizer.rasterize(&[("█", FontStyle::Normal)])?;

        if let Some(g) = reference_glyphs.first() {
            Ok((
                g.width as i32 - FontAtlasData::PADDING * 2,
                g.height as i32 - FontAtlasData::PADDING * 2,
            ))
        } else {
            Err(Error::rasterizer_empty_reference_glyph())
        }
    }
}

impl Atlas for DynamicFontAtlas {
    fn get_glyph_id(&self, key: &str, style_bits: u16) -> Option<u16> {
        self.resolve_glyph_slot(key, style_bits)
            .map(|slot| slot.slot_id())
    }

    fn get_base_glyph_id(&self, key: &str) -> Option<u16> {
        self.cache
            .borrow_mut()
            .get(key, FontStyle::Normal)
            .map(|slot| slot.slot_id())
    }

    fn cell_size(&self) -> (i32, i32) {
        self.physical_cell_size
    }

    fn bind(&self, gl: &WebGl2RenderingContext, texture_unit: u32) {
        self.texture.bind(gl, texture_unit);
    }

    fn underline(&self) -> LineDecoration {
        self.underline
    }

    fn strikethrough(&self) -> LineDecoration {
        self.strikethrough
    }

    fn get_symbol(&self, glyph_id: u16) -> Option<CompactString> {
        // ASCII characters (slots 0-94) are directly mapped: slot_id = codepoint - 0x20
        // This matches upload_ascii_glyphs() which assigns slots 0-94 for 0x20-0x7E
        if glyph_id < 95 {
            let ch = (glyph_id + 0x20) as u8 as char;
            Some(ch.to_compact_string())
        } else {
            self.symbol_lookup
                .borrow()
                .get(&glyph_id)
                .cloned()
        }
    }

    fn glyph_tracker(&self) -> &GlyphTracker {
        &self.glyph_tracker
    }

    fn glyph_count(&self) -> u32 {
        self.cache.borrow().len() as u32
    }

    fn flush(&self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        while !self.glyphs_pending_upload.is_empty() {
            self.upload_pending_glyphs(gl)?;
        }

        Ok(())
    }

    fn recreate_texture(&mut self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        self.texture.delete(gl);

        let padded_cell_size = (
            self.physical_cell_size.0 + FontAtlasData::PADDING * 2,
            self.physical_cell_size.1 + FontAtlasData::PADDING * 2,
        );
        self.texture = Texture::for_dynamic_font_atlas(gl, GL::RGBA, padded_cell_size, NUM_LAYERS)?;

        self.cache.borrow_mut().clear();
        self.symbol_lookup.borrow_mut().clear();

        self.upload_ascii_glyphs(gl)?;

        Ok(())
    }

    fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str)) {
        for (glyph_id, symbol) in self.symbol_lookup.borrow().iter() {
            f(*glyph_id, symbol.as_str());
        }
    }

    fn resolve_glyph_slot(&self, key: &str, style_bits: u16) -> Option<GlyphSlot> {
        let font_variant = FontStyle::from_u16(style_bits & FontStyle::MASK);
        let styling = style_bits & (Glyph::STRIKETHROUGH_FLAG | Glyph::UNDERLINE_FLAG);

        let mut cache = self.cache.borrow_mut();
        if let Some(glyph) = cache.get(key, font_variant) {
            return Some(glyph.with_styling(styling));
        }

        // glyph not present, insert and mark for upload
        let (slot, _) = cache.insert(key, font_variant);

        // add reverse lookup
        self.symbol_lookup
            .borrow_mut()
            .insert(slot.slot_id(), CompactString::new(key));

        self.glyphs_pending_upload.add(PendingGlyph {
            slot,
            key: CompactString::new(key),
            style: font_variant,
        });

        Some(slot.with_styling(styling))
    }

    /// Returns `0x0FFF` for flat 12-bit slot addressing.
    ///
    /// The dynamic atlas uses sequential slot assignment (4096 total slots) rather
    /// than the encoded glyph ID scheme. Emoji are tracked via `GlyphSlot::Emoji`
    /// variant instead of a flag bit, so we only need 12 bits for the slot index.
    fn base_lookup_mask(&self) -> u32 {
        atlas::DYNAMIC_ATLAS_LOOKUP_MASK
    }

    fn delete(&self, gl: &WebGl2RenderingContext) {
        self.texture.delete(gl);
    }

    fn update_pixel_ratio(
        &mut self,
        gl: &WebGl2RenderingContext,
        pixel_ratio: f32,
    ) -> Result<f32, Error> {
        // Skip if ratio hasn't changed
        if (self.pixel_ratio - pixel_ratio).abs() < f32::EPSILON {
            return Ok(pixel_ratio);
        }

        self.pixel_ratio = pixel_ratio;

        // Recreate rasterizer with new effective font size
        let effective_font_size = self.base_font_size * pixel_ratio;
        self.rasterizer =
            CanvasRasterizer::new(self.rasterizer.font_family(), effective_font_size)?;

        // Recalculate cell size from new rasterizer
        self.physical_cell_size = Self::measure_cell_size(&self.rasterizer)?;

        // Recreate texture with new dimensions
        self.texture.delete(gl);
        let padded_cell_size = (
            self.physical_cell_size.0 + FontAtlasData::PADDING * 2,
            self.physical_cell_size.1 + FontAtlasData::PADDING * 2,
        );
        self.texture = Texture::for_dynamic_font_atlas(gl, GL::RGBA, padded_cell_size, NUM_LAYERS)?;

        // Clear cache, lookups, and missing glyph tracker, then re-upload ASCII glyphs
        self.cache.borrow_mut().clear();
        self.symbol_lookup.borrow_mut().clear();
        self.glyph_tracker.clear();
        self.upload_ascii_glyphs(gl)?;

        Ok(pixel_ratio)
    }

    fn cell_scale_for_dpr(&self, _pixel_ratio: f32) -> f32 {
        // Dynamic atlas cell_size() returns physical size, so no additional scaling needed
        1.0
    }

    fn texture_cell_size(&self) -> (i32, i32) {
        // Physical cell size is the texture dimension
        self.physical_cell_size
    }
}

impl std::fmt::Debug for DynamicFontAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicFontAtlas")
            .field("physical_cell_size", &self.physical_cell_size)
            .field("cache", &*self.cache.borrow())
            .finish_non_exhaustive()
    }
}

struct PendingUploads {
    glyphs: RefCell<BTreeSet<PendingGlyph>>,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct PendingGlyph {
    slot: GlyphSlot,
    key: CompactString,
    style: FontStyle,
}

impl PendingUploads {
    fn new() -> Self {
        Self { glyphs: RefCell::new(BTreeSet::new()) }
    }

    fn add(&self, glyph: PendingGlyph) {
        self.glyphs.borrow_mut().insert(glyph);
    }

    fn take(&self, count: usize) -> Vec<PendingGlyph> {
        let mut glyphs = self.glyphs.borrow_mut();
        let mut pending_upload = Vec::with_capacity(count.min(glyphs.len()));

        for _ in 0..count {
            if let Some(glyph) = glyphs.pop_last() {
                pending_upload.push(glyph);
            } else {
                break;
            }
        }

        pending_upload
    }

    fn is_empty(&self) -> bool {
        self.glyphs.borrow().is_empty()
    }
}

/// Generates a checkered glyph pattern for validating pixel-perfect rendering.
fn generate_checkered_glyph(
    width: u32,
    height: u32,
    pattern: DebugSpacePattern,
) -> RasterizedGlyph {
    let bytes_per_pixel = 4usize;
    let mut pixels = vec![0u8; (width * height) as usize * bytes_per_pixel];

    for y in 0..height {
        for x in 0..width {
            let is_white = match pattern {
                DebugSpacePattern::OnePixel => (x + y) % 2 == 0,
                DebugSpacePattern::TwoByTwo => ((x / 2) + (y / 2)) % 2 == 0,
            };

            if is_white {
                let idx = ((y * width + x) as usize) * bytes_per_pixel;
                pixels[idx] = 0xff; // R
                pixels[idx + 1] = 0xff; // G
                pixels[idx + 2] = 0xff; // B
                pixels[idx + 3] = 0xff; // A
            }
            // Black pixels (alpha=0) are already initialized to 0
        }
    }

    RasterizedGlyph::new(pixels, width, height)
}

/// Splits a double-width glyph into left and right halves.
///
/// The input glyph should be 2× cell_w wide. Each half will be cell_w × cell_h.
/// Each half gets padding on its inner edge (where the split occurs).
fn split_double_width_glyph(
    glyph: &super::canvas_rasterizer::RasterizedGlyph,
    cell_w: u32,
    cell_h: u32,
) -> (RasterizedGlyph, RasterizedGlyph) {
    use beamterm_data::FontAtlasData;

    use super::canvas_rasterizer::RasterizedGlyph;

    let bytes_per_pixel = 4usize;
    let padding = FontAtlasData::PADDING as usize;
    let content_w = (cell_w as usize).saturating_sub(2 * padding);

    let mut left_pixels = vec![0u8; (cell_w * cell_h) as usize * bytes_per_pixel];
    let mut right_pixels = vec![0u8; (cell_w * cell_h) as usize * bytes_per_pixel];

    let src_row_stride = glyph.width as usize * bytes_per_pixel;
    let dst_row_stride = cell_w as usize * bytes_per_pixel;

    // calculate source content region (excluding outer padding from source)
    let src_content_start = padding;
    let src_content_width = (glyph.width as usize).saturating_sub(2 * padding);

    // split the content in half
    let left_content_width = src_content_width / 2;
    let right_content_width = src_content_width - left_content_width;

    for row in 0..cell_h.min(glyph.height) as usize {
        let src_row_start = row * src_row_stride;
        let dst_row_start = row * dst_row_stride;

        // copy left half content (with padding on left from source, padding on right is zeros)
        // destination: [padding][content][padding]
        // source left half: [padding][left_content]
        for col in 0..padding {
            // copy left padding from source
            let src_idx = src_row_start + col * bytes_per_pixel;
            let dst_idx = dst_row_start + col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() {
                left_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }
        for col in 0..left_content_width.min(content_w) {
            // copy left content
            let src_col = src_content_start + col;
            let dst_col = padding + col;
            let src_idx = src_row_start + src_col * bytes_per_pixel;
            let dst_idx = dst_row_start + dst_col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() {
                left_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }
        // right padding of left half stays as zeros (inner edge)

        // copy right half content (padding on left is zeros, padding on right from source)
        // destination: [padding][content][padding]
        // source right half: [right_content][padding]
        // left padding of right half stays as zeros (inner edge)
        for col in 0..right_content_width.min(content_w) {
            // copy right content
            let src_col = src_content_start + left_content_width + col;
            let dst_col = padding + col;
            let src_idx = src_row_start + src_col * bytes_per_pixel;
            let dst_idx = dst_row_start + dst_col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() {
                right_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }
        for col in 0..padding {
            // copy right padding from source
            let src_col = glyph.width as usize - padding + col;
            let dst_col = cell_w as usize - padding + col;
            let src_idx = src_row_start + src_col * bytes_per_pixel;
            let dst_idx = dst_row_start + dst_col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() && dst_idx + 4 <= right_pixels.len() {
                right_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }
    }

    (
        RasterizedGlyph { pixels: left_pixels, width: cell_w, height: cell_h },
        RasterizedGlyph {
            pixels: right_pixels,
            width: cell_w,
            height: cell_h,
        },
    )
}
