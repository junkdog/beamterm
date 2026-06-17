//! Glyph cache with partitioned regions for normal and double-width glyphs.
//!
//! - Two LRU caches: one for normal glyphs, one for double-width (emoji/CJK)
//! - O(1) lookup, insert, and eviction
//! - No bitmap needed - each region allocates sequentially then evicts LRU

use beamterm_data::FontStyle;
use compact_str::CompactString;
use lru::LruCache;
use unicode_width::UnicodeWidthStr;

use crate::{
    gl::atlas::{GLYPH_SLOT_MASK, GlyphSlot, SlotId},
    is_emoji,
};

/// Pre-allocated slots for normal-styled ASCII glyphs (0x20..0x7E)
pub(crate) const ASCII_SLOTS: u16 = 0x7E - 0x20 + 1; // 95 slots for ASCII (0x20..0x7E)

/// Normal glyphs: slots 0..2048
pub(crate) const NORMAL_CAPACITY: usize = 2048;
/// Double-width glyphs: slots 2048..6144 (2048 glyphs x 2 slots each)
pub(crate) const WIDE_CAPACITY: usize = 2048;
const WIDE_BASE: SlotId = NORMAL_CAPACITY as SlotId;

/// Smallest ligature width handled by the dedicated ligature pools.
///
/// Two-cell ligatures reuse the [wide region](GlyphCache::wide) (same stride),
/// so the pools only cover widths 3..=[`MAX_LIGATURE_CELLS`].
pub(crate) const MIN_LIGATURE_CELLS: u8 = 3;
/// Largest ligature width that can be stored as a single multi-cell glyph.
pub const MAX_LIGATURE_CELLS: u8 = 8;
/// Number of size-classed ligature pools (one per width 3..=8).
const NUM_LIGATURE_POOLS: usize = (MAX_LIGATURE_CELLS - MIN_LIGATURE_CELLS + 1) as usize;

/// First slot of the ligature region (immediately after the wide region).
const LIGATURE_BASE: SlotId = (NORMAL_CAPACITY + WIDE_CAPACITY * 2) as SlotId;
/// Glyph capacity per ligature pool, indexed by `width - MIN_LIGATURE_CELLS`.
///
/// Width-3 ligatures are the most common (`===`, `!==`, `>>=`, `...`); wider
/// ones are rarer, so capacity tapers off. Total slots consumed:
/// `sum(cap[w] * width[w])` must stay within the 13-bit (8192-slot) address space.
const LIGATURE_POOL_GLYPHS: [SlotId; NUM_LIGATURE_POOLS] = [96, 64, 48, 40, 32, 24];

/// One-past-the-last texture slot used by any region.
///
/// The dynamic atlas texture must allocate enough layers to cover this many
/// slots. Derived from the region layout so the texture and the cache cannot
/// drift apart.
pub(crate) const TOTAL_SLOTS: SlotId = {
    let mut total = LIGATURE_BASE;
    let mut i = 0;
    while i < NUM_LIGATURE_POOLS {
        total += LIGATURE_POOL_GLYPHS[i] * (i as SlotId + MIN_LIGATURE_CELLS as SlotId);
        i += 1;
    }
    total
};

/// Emoji flag for the dynamic atlas (bit 15).
///
/// Unlike the static atlas which uses `Glyph::EMOJI_FLAG` (bit 12) as part of
/// the texture slot address, the dynamic atlas stores the emoji flag in bit 15
/// — outside the 13-bit slot mask (0x1FFF) — so that the full 8192-slot address
/// space is available for non-emoji wide glyphs.
pub(crate) const DYNAMIC_EMOJI_FLAG: u16 = 0x8000;

pub(crate) type CacheKey = (CompactString, FontStyle);

/// Glyph cache with separate regions for normal and double-width glyphs.
///
/// - Normal region: slots 0-2047 (2048 single-width glyphs)
/// - Wide region: slots 2048-6143 (2048 double-width glyphs, 2 slots each)
pub(crate) struct GlyphCache {
    /// LRU for normal (single-width) glyphs
    normal: LruCache<CacheKey, GlyphSlot>,
    /// LRU for double-width glyphs
    wide: LruCache<CacheKey, GlyphSlot>,
    /// Size-classed LRU pools for ligatures spanning 3..=8 cells.
    ligature: [LruCache<CacheKey, GlyphSlot>; NUM_LIGATURE_POOLS],
    /// Next slot in normal region (0-2047)
    normal_next: SlotId,
    /// Next index in wide region (starts at 2048)
    wide_next: SlotId,
    /// Next slot in each ligature pool.
    ligature_next: [SlotId; NUM_LIGATURE_POOLS],
    /// First slot of each ligature pool.
    ligature_base: [SlotId; NUM_LIGATURE_POOLS],
    /// One-past-the-last slot of each ligature pool.
    ligature_end: [SlotId; NUM_LIGATURE_POOLS],
}

impl GlyphCache {
    pub(crate) fn new() -> Self {
        let mut ligature_base = [0; NUM_LIGATURE_POOLS];
        let mut ligature_end = [0; NUM_LIGATURE_POOLS];
        let mut base = LIGATURE_BASE;
        for pool in 0..NUM_LIGATURE_POOLS {
            let width = pool as SlotId + MIN_LIGATURE_CELLS as SlotId;
            ligature_base[pool] = base;
            base += LIGATURE_POOL_GLYPHS[pool] * width;
            ligature_end[pool] = base;
        }
        debug_assert!(
            u32::from(base) <= GLYPH_SLOT_MASK + 1,
            "ligature region overflows slot address space"
        );

        Self {
            normal: LruCache::unbounded(),
            wide: LruCache::unbounded(),
            ligature: std::array::from_fn(|_| LruCache::unbounded()),
            normal_next: ASCII_SLOTS,
            wide_next: WIDE_BASE,
            ligature_next: ligature_base,
            ligature_base,
            ligature_end,
        }
    }

    /// Gets the slot for a ligature glyph of the given cell width, marking it
    /// recently used. `cells` must be in `MIN_LIGATURE_CELLS..=MAX_LIGATURE_CELLS`.
    pub(crate) fn get_ligature(
        &mut self,
        key: &str,
        style: FontStyle,
        cells: u8,
    ) -> Option<GlyphSlot> {
        let pool = Self::ligature_pool(cells)?;
        let cache_key = (CompactString::new(key), style);
        self.ligature[pool].get(&cache_key).copied()
    }

    /// Inserts a ligature glyph spanning `cells` cells, returning its slot and
    /// the evicted key (if any). Allocates `cells` consecutive slots.
    pub(crate) fn insert_ligature(
        &mut self,
        key: &str,
        style: FontStyle,
        cells: u8,
    ) -> Option<(GlyphSlot, Option<CacheKey>)> {
        let pool = Self::ligature_pool(cells)?;
        let cache_key = (CompactString::new(key), style);

        if let Some(&slot) = self.ligature[pool].get(&cache_key) {
            return Some((slot, None));
        }

        let width = cells as SlotId;
        let (idx, evicted) = if self.ligature_next[pool] + width <= self.ligature_end[pool] {
            let idx = self.ligature_next[pool];
            self.ligature_next[pool] += width;
            (idx, None)
        } else {
            let (evicted_key, evicted_slot) = self.ligature[pool]
                .pop_lru()
                .expect("ligature pool should not be empty when full");
            (evicted_slot.slot_id(), Some(evicted_key))
        };

        let slot = GlyphSlot::Ligature(idx, cells);
        self.ligature[pool].put(cache_key, slot);
        Some((slot, evicted))
    }

    /// Returns the pool index for a ligature of the given width, or `None` if
    /// the width is outside the supported ligature range.
    fn ligature_pool(cells: u8) -> Option<usize> {
        if (MIN_LIGATURE_CELLS..=MAX_LIGATURE_CELLS).contains(&cells) {
            Some((cells - MIN_LIGATURE_CELLS) as usize)
        } else {
            None
        }
    }

    /// Gets the slot for a glyph, marking it as recently used.
    pub(crate) fn get(&mut self, key: &str, style: FontStyle) -> Option<GlyphSlot> {
        // ascii glyphs with normal font styles are always allocated (outside cache)
        if key.len() == 1 && style == FontStyle::Normal {
            Some(GlyphSlot::Normal(
                (key.chars().next().unwrap() as SlotId).saturating_sub(0x20),
            ))
        } else if key.len() == 1 {
            // ascii glyphs are always single-width
            let cache_key = (CompactString::new(key), style);
            self.normal.get(&cache_key).copied()
        } else if is_emoji(key) {
            // emoji glyphs disregard style
            let cache_key = (CompactString::new(key), FontStyle::Normal);
            self.wide.get(&cache_key).copied()
        } else {
            let cache_key = (CompactString::new(key), style);
            if key.width() == 2 {
                // double-width glyphs
                self.wide.get(&cache_key).copied()
            } else {
                // normal glyphs
                self.normal.get(&cache_key).copied()
            }
        }
    }

    /// Inserts a glyph, returning its slot. Evicts LRU if region is full.
    #[cfg(test)]
    fn insert(&mut self, key: &str, style: FontStyle) -> (GlyphSlot, Option<CacheKey>) {
        self.insert_ex(key, style, false)
    }

    /// Inserts a glyph with an explicit double-width override.
    ///
    /// When `force_wide` is true, the glyph is placed in the wide region
    /// regardless of unicode-width. This is used for PUA glyphs (e.g. Nerd
    /// Font icons) whose advance width exceeds one cell.
    pub(crate) fn insert_ex(
        &mut self,
        key: &str,
        style: FontStyle,
        force_wide: bool,
    ) -> (GlyphSlot, Option<CacheKey>) {
        // avoid inserting ASCII normal glyphs into cache
        if key.len() == 1 && style == FontStyle::Normal && !force_wide {
            let slot =
                GlyphSlot::Normal((key.chars().next().unwrap() as SlotId).saturating_sub(0x20));
            return (slot, None);
        }

        let cache_key = (CompactString::new(key), style);
        let is_emoji = is_emoji(key);

        if is_emoji || key.width() == 2 || force_wide {
            // Check if already present
            if let Some(&slot) = self.wide.get(&cache_key) {
                return (slot, None);
            }

            // Allocate or evict
            let (idx, evicted) =
                if (self.wide_next as usize) < (NORMAL_CAPACITY + WIDE_CAPACITY * 2) {
                    let idx = self.wide_next;
                    self.wide_next += 2;
                    (idx, None)
                } else {
                    let (evicted_key, evicted_slot) = self
                        .wide
                        .pop_lru()
                        .expect("wide cache should not be empty when full");
                    (evicted_slot.slot_id(), Some(evicted_key))
                };

            let slot = if is_emoji {
                GlyphSlot::Emoji(idx | DYNAMIC_EMOJI_FLAG)
            } else {
                GlyphSlot::Wide(idx)
            };

            self.wide.put(cache_key, slot);

            (slot, evicted)
        } else {
            // Check if already present
            if let Some(&slot) = self.normal.get(&cache_key) {
                return (slot, None);
            }

            // Allocate or evict
            let (slot, evicted) = if (self.normal_next as usize) < NORMAL_CAPACITY {
                let slot = self.normal_next;
                self.normal_next += 1;
                (GlyphSlot::Normal(slot), None)
            } else {
                let (evicted_key, evicted_slot) = self
                    .normal
                    .pop_lru()
                    .expect("normal cache should not be empty when full");
                (evicted_slot, Some(evicted_key))
            };

            self.normal.put(cache_key, slot);
            (slot, evicted)
        }
    }

    /// Returns total number of cached glyphs.
    pub(crate) fn len(&self) -> usize {
        self.normal.len()
            + self.wide.len()
            + self
                .ligature
                .iter()
                .map(LruCache::len)
                .sum::<usize>()
    }

    /// Clears all cached glyphs.
    pub(crate) fn clear(&mut self) {
        self.normal.clear();
        self.wide.clear();
        for pool in &mut self.ligature {
            pool.clear();
        }

        self.normal_next = ASCII_SLOTS;
        self.wide_next = WIDE_BASE;
        self.ligature_next = self.ligature_base;
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for GlyphCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphCache")
            .field("normal", &self.normal.len())
            .field("wide", &self.wide.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: FontStyle = FontStyle::Normal;

    // First normal slot after reserved ASCII slots (0-94)
    const FIRST_NORMAL_SLOT: SlotId = ASCII_SLOTS; // 95

    // Emoji slots include DYNAMIC_EMOJI_FLAG (0x8000)
    const EMOJI_SLOT_BASE: SlotId = WIDE_BASE | DYNAMIC_EMOJI_FLAG; // 2048 | 0x8000 = 34816

    #[test]
    fn test_ascii_fast_path() {
        // ASCII characters with Normal style use the fast path in get()
        // They return slot = char - 0x20, without using the cache
        let mut cache = GlyphCache::new();

        // 'A' = 0x41, so slot = 0x41 - 0x20 = 33
        assert_eq!(cache.get("A", S), Some(GlyphSlot::Normal(33)));
        // ' ' = 0x20, so slot = 0
        assert_eq!(cache.get(" ", S), Some(GlyphSlot::Normal(0)));
        // '~' = 0x7E, so slot = 0x7E - 0x20 = 94
        assert_eq!(cache.get("~", S), Some(GlyphSlot::Normal(94)));
    }

    #[test]
    fn test_normal_insert_get() {
        let mut cache = GlyphCache::new();

        // Non-ASCII single-width character (uses cache, not fast path)
        let (slot, evicted) = cache.insert("\u{2192}", S);
        assert_eq!(slot, GlyphSlot::Normal(FIRST_NORMAL_SLOT));
        assert!(evicted.is_none());

        assert_eq!(
            cache.get("\u{2192}", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
        assert!(cache.get("\u{2190}", S).is_none());
    }

    #[test]
    fn test_wide_insert_get() {
        let mut cache = GlyphCache::new();

        let (slot1, _) = cache.insert("\u{1F680}", S);
        let (slot2, _) = cache.insert("\u{1F3AE}", S);

        // Emoji slots start at WIDE_BASE with DYNAMIC_EMOJI_FLAG, each takes 2 slots
        assert_eq!(slot1, GlyphSlot::Emoji(EMOJI_SLOT_BASE));
        assert_eq!(slot2, GlyphSlot::Emoji(EMOJI_SLOT_BASE + 2));

        assert_eq!(
            cache.get("\u{1F680}", S),
            Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE))
        );
        assert_eq!(
            cache.get("\u{1F3AE}", S),
            Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE + 2))
        );
    }

    #[test]
    fn test_wide_cjk() {
        let mut cache = GlyphCache::new();

        let (slot1, _) = cache.insert("\u{4E2D}", S);
        let (slot2, _) = cache.insert("\u{6587}", S);

        // CJK wide slots start at WIDE_BASE (no emoji flag), each takes 2 slots
        assert_eq!(slot1, GlyphSlot::Wide(WIDE_BASE));
        assert_eq!(slot2, GlyphSlot::Wide(WIDE_BASE + 2));

        assert_eq!(cache.get("\u{4E2D}", S), Some(GlyphSlot::Wide(WIDE_BASE)));
        assert_eq!(
            cache.get("\u{6587}", S),
            Some(GlyphSlot::Wide(WIDE_BASE + 2))
        );
    }

    #[test]
    fn test_mixed_insert() {
        let mut cache = GlyphCache::new();

        // Use non-ASCII chars to test cache behavior (ASCII uses fast path)
        let (s1, _) = cache.insert("\u{2192}", S);
        let (s2, _) = cache.insert("\u{1F680}", S);
        let (s3, _) = cache.insert("\u{2190}", S);

        assert_eq!(s1, GlyphSlot::Normal(FIRST_NORMAL_SLOT));
        assert_eq!(s2, GlyphSlot::Emoji(EMOJI_SLOT_BASE));
        assert_eq!(s3, GlyphSlot::Normal(FIRST_NORMAL_SLOT + 1));

        assert_eq!(
            cache.get("\u{2192}", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
        assert_eq!(
            cache.get("\u{1F680}", S),
            Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE))
        );
        assert_eq!(
            cache.get("\u{2190}", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT + 1))
        );
    }

    #[test]
    fn test_style_differentiation() {
        let mut cache = GlyphCache::new();

        // ASCII with Normal style uses fast path (not cache)
        let (slot1, _) = cache.insert("A", FontStyle::Normal);
        // ASCII with Bold style uses cache (not fast path which is Normal-only)
        let (slot2, _) = cache.insert("A", FontStyle::Bold);

        // Normal uses fast path: 'A' = 0x41 - 0x20 = 33
        assert_eq!(slot1, GlyphSlot::Normal(33));
        // Bold goes through cache
        assert_eq!(slot2, GlyphSlot::Normal(FIRST_NORMAL_SLOT));

        // get() for Normal style uses fast path: 'A' = 0x41 - 0x20 = 33
        assert_eq!(
            cache.get("A", FontStyle::Normal),
            Some(GlyphSlot::Normal(33))
        );
        // get() for Bold uses cache
        assert_eq!(
            cache.get("A", FontStyle::Bold),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
    }

    #[test]
    fn test_ligature_pools_are_width_classed() {
        let mut cache = GlyphCache::new();

        let (s3, _) = cache.insert_ligature("===", S, 3).unwrap();
        let (s3b, _) = cache.insert_ligature("!==", S, 3).unwrap();
        let (s4, _) = cache.insert_ligature("<==>", S, 4).unwrap();

        // width-3 pool: consecutive entries are `width` slots apart
        assert!(matches!(s3, GlyphSlot::Ligature(_, 3)));
        assert_eq!(s3b.slot_id(), s3.slot_id() + 3);
        // width-4 lives in a different pool, after the width-3 region
        assert!(matches!(s4, GlyphSlot::Ligature(_, 4)));
        assert!(s4.slot_id() >= LIGATURE_BASE);

        // lookups round-trip
        assert_eq!(cache.get_ligature("===", S, 3), Some(s3));
        assert_eq!(cache.get_ligature("<==>", S, 4), Some(s4));
        // wrong width class doesn't find it
        assert_eq!(cache.get_ligature("===", S, 4), None);
    }

    #[test]
    fn test_ligature_width_out_of_range() {
        let mut cache = GlyphCache::new();
        // width 2 is handled by the wide region, not the ligature pools
        assert!(cache.insert_ligature("=>", S, 2).is_none());
        // width 9 exceeds MAX_LIGATURE_CELLS
        assert!(cache.insert_ligature("=========", S, 9).is_none());
    }

    #[test]
    fn test_ligature_eviction_reuses_slots() {
        let mut cache = GlyphCache::new();
        let cap = LIGATURE_POOL_GLYPHS[0] as usize; // width-3 pool capacity

        // fill the width-3 pool exactly
        for i in 0..cap {
            let (_, evicted) = cache
                .insert_ligature(&format!("l3-{i}"), S, 3)
                .unwrap();
            assert!(evicted.is_none(), "no eviction while filling");
        }
        // next insert must evict the LRU entry and reuse its slot
        let (slot, evicted) = cache.insert_ligature("overflow", S, 3).unwrap();
        assert_eq!(evicted, Some((CompactString::new("l3-0"), S)));
        assert!(matches!(slot, GlyphSlot::Ligature(_, 3)));
    }

    #[test]
    fn test_reinsert_existing() {
        let mut cache = GlyphCache::new();

        // Use non-ASCII to test cache reinsert behavior
        let (slot1, _) = cache.insert("\u{2192}", S);
        let (slot2, evicted) = cache.insert("\u{2192}", S);

        assert_eq!(slot1, slot2);
        assert!(evicted.is_none());
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.get("\u{2192}", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
    }
}
