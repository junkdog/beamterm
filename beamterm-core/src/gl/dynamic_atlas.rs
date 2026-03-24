use std::{collections::HashMap, ops::Not};

use beamterm_data::{CellSize, DebugSpacePattern, FontAtlasData, FontStyle, Glyph, LineDecoration};
use compact_str::{CompactString, ToCompactString};

use super::{
    atlas::{self, Atlas, GlyphSlot, GlyphTracker},
    glyph_cache::{ASCII_SLOTS, DYNAMIC_EMOJI_FLAG, GlyphCache, NORMAL_CAPACITY, WIDE_CAPACITY},
    glyph_rasterizer::GlyphRasterizer,
    texture::{RasterizedGlyph, Texture},
};
use crate::Error;

/// Glyphs per layer (1x32 vertical grid)
const GLYPHS_PER_LAYER: usize = 32;
/// Total number of glyph slots (2048 normal + 4096 wide = 2048 double-width glyphs)
const TOTAL_SLOTS: usize = 6144;
/// Number of texture layers in the atlas
const NUM_LAYERS: i32 = (TOTAL_SLOTS / GLYPHS_PER_LAYER) as i32; // 192 layers

/// A dynamic texture atlas that rasterizes font glyphs on demand.
///
/// Generic over the rasterization backend (`R`), enabling both native (swash+fontdb)
/// and WASM (Canvas API) backends with shared atlas logic.
///
/// # Architecture
/// - 192 layers x 32 glyphs per layer = 6144 total slots
/// - LRU-based slot allocation with eviction when full
/// - Double-width glyphs (emoji, CJK) occupy 2 consecutive slots
/// - Glyphs are rasterized on first use and cached in the texture
pub struct DynamicFontAtlas<R: GlyphRasterizer> {
    texture: Texture,
    rasterizer: R,
    cache: GlyphCache,
    symbol_lookup: HashMap<u16, CompactString>,
    glyphs_pending_upload: PendingUploads,
    physical_cell_size: CellSize,
    glyph_tracker: GlyphTracker,
    underline: LineDecoration,
    strikethrough: LineDecoration,
    debug_space_pattern: Option<DebugSpacePattern>,
    base_font_size: f32,
    pixel_ratio: f32,
}

impl<R: GlyphRasterizer> DynamicFontAtlas<R> {
    /// Creates a new dynamic font atlas.
    ///
    /// # Arguments
    /// * `gl` - glow rendering context
    /// * `rasterizer` - platform-specific glyph rasterizer
    /// * `base_font_size` - font size in logical pixels (before pixel ratio scaling)
    /// * `pixel_ratio` - device pixel ratio for HiDPI rendering
    pub fn new(
        gl: &glow::Context,
        rasterizer: R,
        base_font_size: f32,
        pixel_ratio: f32,
    ) -> Result<Self, Error> {
        Self::with_debug_spaces(gl, rasterizer, base_font_size, pixel_ratio, None)
    }

    /// Creates a new dynamic font atlas with optional debug space pattern.
    pub fn with_debug_spaces(
        gl: &glow::Context,
        rasterizer: R,
        base_font_size: f32,
        pixel_ratio: f32,
        debug_space_pattern: Option<DebugSpacePattern>,
    ) -> Result<Self, Error> {
        let physical_cell_size = rasterizer.cell_size();
        let underline = rasterizer.underline();
        let strikethrough = rasterizer.strikethrough();

        let padded_cell_size = CellSize::new(
            physical_cell_size.width + FontAtlasData::PADDING * 2,
            physical_cell_size.height + FontAtlasData::PADDING * 2,
        );
        let texture = Texture::for_dynamic_font_atlas(gl, padded_cell_size, NUM_LAYERS)?;

        let mut atlas = Self {
            texture,
            rasterizer,
            cache: GlyphCache::new(),
            symbol_lookup: HashMap::new(),
            glyphs_pending_upload: PendingUploads::new(),
            physical_cell_size,
            glyph_tracker: GlyphTracker::new(),
            underline,
            strikethrough,
            debug_space_pattern,
            base_font_size,
            pixel_ratio,
        };
        atlas.upload_ascii_glyphs(gl)?;

        Ok(atlas)
    }

    fn upload_ascii_glyphs(&mut self, gl: &glow::Context) -> Result<(), Error> {
        let all_pending: Vec<PendingGlyph> = (0x20u8..=0x7Eu8)
            .map(|b| PendingGlyph {
                slot: GlyphSlot::Normal(b as u16 - 0x20),
                key: CompactString::from_utf8([b]).expect("valid ascii"),
                style: FontStyle::Normal,
            })
            .collect();

        let batch_size = self.rasterizer.max_batch_size();
        for batch in all_pending.chunks(batch_size) {
            self.rasterize_and_upload(gl, batch.to_vec())?;
        }

        Ok(())
    }

    fn upload_pending_glyphs(&mut self, gl: &glow::Context) -> Result<(), Error> {
        if self.glyphs_pending_upload.is_empty() {
            return Ok(());
        }

        let batch_size = self.rasterizer.max_batch_size();
        let pending = self.glyphs_pending_upload.take(batch_size);
        self.rasterize_and_upload(gl, pending)
    }

    fn rasterize_and_upload(
        &mut self,
        gl: &glow::Context,
        pending: Vec<PendingGlyph>,
    ) -> Result<(), Error> {
        let padded_cell_size = CellSize::new(
            self.physical_cell_size.width + FontAtlasData::PADDING * 2,
            self.physical_cell_size.height + FontAtlasData::PADDING * 2,
        );
        let cell_w = padded_cell_size.width as u32;
        let cell_h = padded_cell_size.height as u32;

        let graphemes: Vec<(&str, FontStyle)> = pending
            .iter()
            .map(|g| (g.key.as_str(), g.style))
            .collect();

        let rasterized = self.rasterizer.rasterize_batch(&graphemes)?;

        for (pending_glyph, glyph_data) in pending.iter().zip(rasterized.iter()) {
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
                let (left, right) = split_double_width_glyph(&glyph_data, cell_w, cell_h);
                let slot_id = pending_glyph.slot.slot_id() & DYNAMIC_EMOJI_FLAG.not();
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
}

impl<R: GlyphRasterizer> atlas::sealed::Sealed for DynamicFontAtlas<R> {}

impl<R: GlyphRasterizer> Atlas for DynamicFontAtlas<R> {
    fn get_glyph_id(&mut self, key: &str, style_bits: u16) -> Option<u16> {
        self.resolve_glyph_slot(key, style_bits)
            .map(|slot| slot.slot_id())
    }

    fn get_base_glyph_id(&mut self, key: &str) -> Option<u16> {
        self.cache
            .get(key, FontStyle::Normal)
            .map(|slot| slot.slot_id())
    }

    fn cell_size(&self) -> CellSize {
        self.physical_cell_size
    }

    fn bind(&self, gl: &glow::Context) {
        self.texture.bind(gl);
    }

    fn underline(&self) -> LineDecoration {
        self.underline
    }

    fn strikethrough(&self) -> LineDecoration {
        self.strikethrough
    }

    fn get_symbol(&self, glyph_id: u16) -> Option<CompactString> {
        if glyph_id < ASCII_SLOTS {
            let ch = (glyph_id + 0x20) as u8 as char;
            Some(ch.to_compact_string())
        } else {
            self.symbol_lookup.get(&glyph_id).cloned()
        }
    }

    fn get_ascii_char(&self, glyph_id: u16) -> Option<char> {
        if glyph_id < ASCII_SLOTS {
            Some((glyph_id + 0x20) as u8 as char)
        } else {
            self.get_symbol(glyph_id)
                .map(|s| s.chars().next().unwrap())
                .filter(|&ch| ch.is_ascii())
        }
    }

    fn glyph_tracker(&self) -> &GlyphTracker {
        &self.glyph_tracker
    }

    fn glyph_count(&self) -> u32 {
        self.cache.len() as u32
    }

    fn flush(&mut self, gl: &glow::Context) -> Result<(), Error> {
        self.glyphs_pending_upload.cap_to_capacity();
        while !self.glyphs_pending_upload.is_empty() {
            self.upload_pending_glyphs(gl)?;
        }
        Ok(())
    }

    fn recreate_texture(&mut self, gl: &glow::Context) -> Result<(), Error> {
        self.texture.delete(gl);

        let padded_cell_size = CellSize::new(
            self.physical_cell_size.width + FontAtlasData::PADDING * 2,
            self.physical_cell_size.height + FontAtlasData::PADDING * 2,
        );
        self.texture = Texture::for_dynamic_font_atlas(gl, padded_cell_size, NUM_LAYERS)?;

        self.cache.clear();
        self.symbol_lookup.clear();
        self.glyph_tracker.clear();

        self.upload_ascii_glyphs(gl)?;

        Ok(())
    }

    fn for_each_symbol(&self, f: &mut dyn FnMut(u16, &str)) {
        for (glyph_id, symbol) in self.symbol_lookup.iter() {
            f(*glyph_id, symbol.as_str());
        }
    }

    fn resolve_glyph_slot(&mut self, key: &str, style_bits: u16) -> Option<GlyphSlot> {
        let font_variant = FontStyle::from_u16(style_bits & FontStyle::MASK).ok()?;
        let styling = style_bits & (Glyph::STRIKETHROUGH_FLAG | Glyph::UNDERLINE_FLAG);

        if let Some(glyph) = self.cache.get(key, font_variant) {
            return Some(glyph.with_styling(styling));
        }

        // check if the font's advance width indicates this is a double-width
        // glyph (e.g. Nerd Font icons) even though unicode-width returns 1
        let force_wide = self.rasterizer.is_double_width(key);

        // glyph not present, insert and mark for upload
        let (slot, _) = self
            .cache
            .insert_ex(key, font_variant, force_wide);

        // add reverse lookup
        self.symbol_lookup
            .insert(slot.slot_id(), CompactString::new(key));

        self.glyphs_pending_upload.add(PendingGlyph {
            slot,
            key: CompactString::new(key),
            style: font_variant,
        });

        Some(slot.with_styling(styling))
    }

    fn emoji_bit(&self) -> u32 {
        15
    }

    fn delete(&self, gl: &glow::Context) {
        self.texture.delete(gl);
    }

    fn update_pixel_ratio(&mut self, gl: &glow::Context, pixel_ratio: f32) -> Result<f32, Error> {
        if (self.pixel_ratio - pixel_ratio).abs() < f32::EPSILON {
            return Ok(pixel_ratio);
        }

        self.pixel_ratio = pixel_ratio;

        let effective_font_size = self.base_font_size * pixel_ratio;
        self.rasterizer
            .update_font_size(effective_font_size)?;

        self.physical_cell_size = self.rasterizer.cell_size();
        self.underline = self.rasterizer.underline();
        self.strikethrough = self.rasterizer.strikethrough();

        self.texture.delete(gl);
        let padded_cell_size = CellSize::new(
            self.physical_cell_size.width + FontAtlasData::PADDING * 2,
            self.physical_cell_size.height + FontAtlasData::PADDING * 2,
        );
        self.texture = Texture::for_dynamic_font_atlas(gl, padded_cell_size, NUM_LAYERS)?;

        self.cache.clear();
        self.symbol_lookup.clear();
        self.glyph_tracker.clear();
        self.upload_ascii_glyphs(gl)?;

        Ok(pixel_ratio)
    }

    fn cell_scale_for_dpr(&self, _pixel_ratio: f32) -> f32 {
        1.0
    }

    fn texture_cell_size(&self) -> CellSize {
        self.physical_cell_size
    }
}

impl<R: GlyphRasterizer> std::fmt::Debug for DynamicFontAtlas<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicFontAtlas")
            .field("physical_cell_size", &self.physical_cell_size)
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

struct PendingUploads {
    normal: Vec<PendingGlyph>,
    wide: Vec<PendingGlyph>,
}

#[derive(Clone)]
struct PendingGlyph {
    slot: GlyphSlot,
    key: CompactString,
    style: FontStyle,
}

impl PendingUploads {
    fn new() -> Self {
        Self { normal: Vec::new(), wide: Vec::new() }
    }

    fn add(&mut self, glyph: PendingGlyph) {
        match glyph.slot {
            GlyphSlot::Normal(_) => self.normal.push(glyph),
            GlyphSlot::Wide(_) | GlyphSlot::Emoji(_) => self.wide.push(glyph),
        }
    }

    /// Discards pending glyphs that exceed the LRU capacity per region.
    ///
    /// Only the most recently added glyphs (tail of each vec) are kept,
    /// since earlier entries have already been evicted from the cache.
    /// Wide capacity is 2048 glyphs, each occupying 2 consecutive texture slots.
    fn cap_to_capacity(&mut self) {
        let normal_cap = NORMAL_CAPACITY - ASCII_SLOTS as usize;
        if self.normal.len() > normal_cap {
            let excess = self.normal.len() - normal_cap;
            self.normal.drain(0..excess);
        }
        if self.wide.len() > WIDE_CAPACITY {
            let excess = self.wide.len() - WIDE_CAPACITY;
            self.wide.drain(0..excess);
        }
    }

    fn take(&mut self, count: usize) -> Vec<PendingGlyph> {
        let total = self.normal.len() + self.wide.len();
        let to_take = count.min(total);
        let mut result = Vec::with_capacity(to_take);

        while result.len() < to_take {
            if let Some(g) = self.wide.pop() {
                result.push(g);
            } else if let Some(g) = self.normal.pop() {
                result.push(g);
            } else {
                break;
            }
        }

        result
    }

    fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.wide.is_empty()
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
        }
    }

    RasterizedGlyph::new(pixels, width, height)
}

/// Splits a double-width glyph into left and right halves.
///
/// Each half will be `cell_w` x `cell_h`. Padding from the source glyph is preserved
/// on the outer edges; the inner split edges get zero padding.
fn split_double_width_glyph(
    glyph: &RasterizedGlyph,
    cell_w: u32,
    cell_h: u32,
) -> (RasterizedGlyph, RasterizedGlyph) {
    let bytes_per_pixel = 4usize;
    let padding = FontAtlasData::PADDING as usize;
    let content_w = (cell_w as usize).saturating_sub(2 * padding);

    let mut left_pixels = vec![0u8; (cell_w * cell_h) as usize * bytes_per_pixel];
    let mut right_pixels = vec![0u8; (cell_w * cell_h) as usize * bytes_per_pixel];

    let src_row_stride = glyph.width as usize * bytes_per_pixel;
    let dst_row_stride = cell_w as usize * bytes_per_pixel;

    let src_content_start = padding;
    let src_content_width = (glyph.width as usize).saturating_sub(2 * padding);
    let left_content_width = src_content_width / 2;
    let right_content_width = src_content_width - left_content_width;

    for row in 0..cell_h.min(glyph.height) as usize {
        let src_row_start = row * src_row_stride;
        let dst_row_start = row * dst_row_stride;

        // left half: [padding][content][padding]
        for col in 0..padding {
            let src_idx = src_row_start + col * bytes_per_pixel;
            let dst_idx = dst_row_start + col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() {
                left_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }
        for col in 0..left_content_width.min(content_w) {
            let src_col = src_content_start + col;
            let dst_col = padding + col;
            let src_idx = src_row_start + src_col * bytes_per_pixel;
            let dst_idx = dst_row_start + dst_col * bytes_per_pixel;
            if src_idx + 4 <= glyph.pixels.len() {
                left_pixels[dst_idx..dst_idx + 4]
                    .copy_from_slice(&glyph.pixels[src_idx..src_idx + 4]);
            }
        }

        // right half: [padding][content][padding]
        for col in 0..right_content_width.min(content_w) {
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
        RasterizedGlyph::new(left_pixels, cell_w, cell_h),
        RasterizedGlyph::new(right_pixels, cell_w, cell_h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gl::glyph_cache::ASCII_SLOTS;

    fn normal_glyph(slot: u16, key: &str) -> PendingGlyph {
        PendingGlyph {
            slot: GlyphSlot::Normal(slot),
            key: CompactString::new(key),
            style: FontStyle::Normal,
        }
    }

    fn wide_glyph(slot: u16, key: &str) -> PendingGlyph {
        PendingGlyph {
            slot: GlyphSlot::Wide(slot),
            key: CompactString::new(key),
            style: FontStyle::Normal,
        }
    }

    #[test]
    fn cap_to_capacity_is_noop_when_under_limit() {
        let mut uploads = PendingUploads::new();
        uploads.add(normal_glyph(100, "a"));
        uploads.add(wide_glyph(2048, "\u{4E2D}"));

        uploads.cap_to_capacity();

        assert_eq!(uploads.normal.len(), 1);
        assert_eq!(uploads.wide.len(), 1);
    }

    #[test]
    fn cap_to_capacity_trims_oldest_normal_glyphs() {
        let mut uploads = PendingUploads::new();
        let normal_cap = NORMAL_CAPACITY - ASCII_SLOTS as usize;

        // fill beyond capacity: oldest entries should be dropped
        for i in 0..(normal_cap + 3) as u16 {
            uploads.add(normal_glyph(i, &format!("n{i}")));
        }

        uploads.cap_to_capacity();

        assert_eq!(uploads.normal.len(), normal_cap);
        // the 3 oldest entries (n0, n1, n2) should have been drained;
        // the first remaining entry should be n3
        assert_eq!(uploads.normal[0].key.as_str(), "n3");
    }

    #[test]
    fn cap_to_capacity_trims_oldest_wide_glyphs() {
        let mut uploads = PendingUploads::new();

        for i in 0..(WIDE_CAPACITY + 5) as u16 {
            uploads.add(wide_glyph(2048 + i * 2, &format!("w{i}")));
        }

        uploads.cap_to_capacity();

        assert_eq!(uploads.wide.len(), WIDE_CAPACITY);
        // the 5 oldest entries (w0..w4) should have been drained
        assert_eq!(uploads.wide[0].key.as_str(), "w5");
    }

    #[test]
    fn cap_to_capacity_trims_regions_independently() {
        let mut uploads = PendingUploads::new();
        let normal_cap = NORMAL_CAPACITY - ASCII_SLOTS as usize;

        // overflow normal, keep wide under limit
        for i in 0..(normal_cap + 2) as u16 {
            uploads.add(normal_glyph(i, &format!("n{i}")));
        }
        uploads.add(wide_glyph(2048, "w0"));

        uploads.cap_to_capacity();

        assert_eq!(uploads.normal.len(), normal_cap);
        assert_eq!(uploads.wide.len(), 1); // wide untouched
    }

    #[test]
    fn take_prioritizes_wide_glyphs() {
        let mut uploads = PendingUploads::new();
        uploads.add(normal_glyph(100, "n0"));
        uploads.add(wide_glyph(2048, "w0"));
        uploads.add(normal_glyph(101, "n1"));
        uploads.add(wide_glyph(2050, "w1"));

        let batch = uploads.take(3);

        assert_eq!(batch.len(), 3);
        // wide glyphs taken first (popped from back: w1, w0), then normal
        assert_eq!(batch[0].key.as_str(), "w1");
        assert_eq!(batch[1].key.as_str(), "w0");
        assert_eq!(batch[2].key.as_str(), "n1");
    }
}
