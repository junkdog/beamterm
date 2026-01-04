//! Glyph cache with partitioned regions for normal and double-width glyphs.
//!
//! - Two LRU caches: one for normal glyphs, one for double-width (emoji/CJK)
//! - O(1) lookup, insert, and eviction
//! - No bitmap needed - each region allocates sequentially then evicts LRU

use beamterm_data::{FontStyle, Glyph};
use compact_str::CompactString;
use lru::LruCache;
use unicode_width::UnicodeWidthStr;

use crate::{
    gl::atlas::{GlyphSlot, SlotId},
    terminal::is_double_width,
};

/// Pre-allocated slots for normal-styled ASCII glyphs (0x20..0x7E)
const ASCII_SLOTS: u16 = 0x7E - 0x20 + 1; // 95 slots for ASCII (0x20..0x7E)

/// Normal glyphs: slots 0..2048
const NORMAL_CAPACITY: usize = 2048;
/// Double-width glyphs: slots 2048..4096 (1024 glyphs × 2 slots each)
const WIDE_CAPACITY: usize = 1024;
const WIDE_BASE: SlotId = NORMAL_CAPACITY as SlotId;

pub(super) type CacheKey = (CompactString, FontStyle);

/// Glyph cache with separate regions for normal and double-width glyphs.
///
/// - Normal region: slots 0-2047 (2048 single-width glyphs)
/// - Wide region: slots 2048-4095 (1024 double-width glyphs, 2 slots each)
pub(super) struct GlyphCache {
    /// LRU for normal (single-width) glyphs
    normal: LruCache<CacheKey, GlyphSlot>,
    /// LRU for double-width glyphs
    wide: LruCache<CacheKey, GlyphSlot>,
    /// Next slot in normal region (0-2047)
    normal_next: SlotId,
    /// Next index in wide region (starts at 2048)
    wide_next: SlotId,
}

impl GlyphCache {
    pub(super) fn new() -> Self {
        Self {
            normal: LruCache::unbounded(),
            wide: LruCache::unbounded(),
            normal_next: ASCII_SLOTS,
            wide_next: WIDE_BASE,
        }
    }

    /// Gets the slot for a glyph, marking it as recently used.
    pub(super) fn get(&mut self, key: &str, style: FontStyle) -> Option<GlyphSlot> {
        let cache_key = (CompactString::new(key), style);

        match () {
            // ascii glyphs with normal font styles are always allocated (outside cache)
            _ if key.len() == 1 && style == FontStyle::Normal => Some(GlyphSlot::Normal(
                (key.chars().next().unwrap() as SlotId).saturating_sub(0x20),
            )),

            // ascii glyphs are always single-width
            _ if key.len() == 1 => self.normal.get(&cache_key).copied(),

            // emoji glyphs disregard style
            _ if emojis::get(key).is_some() => self
                .wide
                .get(&(CompactString::new(key), FontStyle::Normal))
                .copied(),

            // double-width glyphs
            _ if key.width() == 2 => self.wide.get(&cache_key).copied(),

            // normal glyphs
            _ => self.normal.get(&cache_key).copied(),
        }
    }

    /// Inserts a glyph, returning its slot. Evicts LRU if region is full.
    pub(super) fn insert(&mut self, key: &str, style: FontStyle) -> (GlyphSlot, Option<CacheKey>) {
        // avoid inserting ASCII normal glyphs into cache
        if key.len() == 1 && style == FontStyle::Normal {
            let slot =
                GlyphSlot::Normal((key.chars().next().unwrap() as SlotId).saturating_sub(0x20));
            return (slot, None);
        }

        let cache_key = (CompactString::new(key), style);
        let is_emoji = emojis::get(key).is_some();
        let double_width = is_emoji || key.width() == 2;

        if is_double_width(key) {
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
                GlyphSlot::Emoji(idx | Glyph::EMOJI_FLAG)
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
    pub(super) fn len(&self) -> usize {
        self.normal.len() + self.wide.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.wide.is_empty()
    }

    /// Clears all cached glyphs.
    pub(super) fn clear(&mut self) {
        self.normal.clear();
        self.wide.clear();

        self.normal_next = ASCII_SLOTS;
        self.wide_next = WIDE_BASE;
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

    // Emoji slots include EMOJI_FLAG (0x1000)
    const EMOJI_SLOT_BASE: SlotId = WIDE_BASE | Glyph::EMOJI_FLAG; // 2048 | 4096 = 6144

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
        let (slot, evicted) = cache.insert("→", S);
        assert_eq!(slot, GlyphSlot::Normal(FIRST_NORMAL_SLOT));
        assert!(evicted.is_none());

        assert_eq!(
            cache.get("→", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
        assert!(cache.get("←", S).is_none());
    }

    #[test]
    fn test_wide_insert_get() {
        let mut cache = GlyphCache::new();

        let (slot1, _) = cache.insert("🚀", S);
        let (slot2, _) = cache.insert("🎮", S);

        // Emoji slots start at WIDE_BASE with EMOJI_FLAG, each takes 2 slots
        assert_eq!(slot1, GlyphSlot::Emoji(EMOJI_SLOT_BASE));
        assert_eq!(slot2, GlyphSlot::Emoji(EMOJI_SLOT_BASE + 2));

        assert_eq!(cache.get("🚀", S), Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE)));
        assert_eq!(
            cache.get("🎮", S),
            Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE + 2))
        );
    }

    #[test]
    fn test_wide_cjk() {
        let mut cache = GlyphCache::new();

        let (slot1, _) = cache.insert("中", S);
        let (slot2, _) = cache.insert("文", S);

        // CJK wide slots start at WIDE_BASE (no emoji flag), each takes 2 slots
        assert_eq!(slot1, GlyphSlot::Wide(WIDE_BASE));
        assert_eq!(slot2, GlyphSlot::Wide(WIDE_BASE + 2));

        assert_eq!(cache.get("中", S), Some(GlyphSlot::Wide(WIDE_BASE)));
        assert_eq!(cache.get("文", S), Some(GlyphSlot::Wide(WIDE_BASE + 2)));
    }

    #[test]
    fn test_mixed_insert() {
        let mut cache = GlyphCache::new();

        // Use non-ASCII chars to test cache behavior (ASCII uses fast path)
        let (s1, _) = cache.insert("→", S);
        let (s2, _) = cache.insert("🚀", S);
        let (s3, _) = cache.insert("←", S);

        assert_eq!(s1, GlyphSlot::Normal(FIRST_NORMAL_SLOT));
        assert_eq!(s2, GlyphSlot::Emoji(EMOJI_SLOT_BASE));
        assert_eq!(s3, GlyphSlot::Normal(FIRST_NORMAL_SLOT + 1));

        assert_eq!(
            cache.get("→", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
        assert_eq!(cache.get("🚀", S), Some(GlyphSlot::Emoji(EMOJI_SLOT_BASE)));
        assert_eq!(
            cache.get("←", S),
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
    fn test_reinsert_existing() {
        let mut cache = GlyphCache::new();

        // Use non-ASCII to test cache reinsert behavior
        let (slot1, _) = cache.insert("→", S);
        let (slot2, evicted) = cache.insert("→", S);

        assert_eq!(slot1, slot2);
        assert!(evicted.is_none());
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.get("→", S),
            Some(GlyphSlot::Normal(FIRST_NORMAL_SLOT))
        );
    }
}
