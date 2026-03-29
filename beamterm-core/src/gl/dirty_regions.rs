/// Tracks which chunks of the cell buffer need uploading to the GPU.
///
/// Uses a `u64` bitmask where each bit represents a chunk of 1024 cells.
/// Adjacent dirty chunks are merged into contiguous uploads via [`drain()`](Self::drain).
#[derive(Debug)]
pub(super) struct DirtyRegions {
    dirty: u64,
    total_cells: usize,
}

impl DirtyRegions {
    const CHUNK_SHIFT: u32 = 10; // 1024 cells per chunk
    const CHUNK_SIZE: usize = 1 << Self::CHUNK_SHIFT;

    pub(super) fn new(total_cells: usize) -> Self {
        debug_assert!(total_cells > 0, "requires a non-zero sized terminal");
        Self { dirty: 0, total_cells }
    }

    pub(super) fn is_clean(&self) -> bool {
        self.dirty == 0
    }

    /// Marks the chunk containing `cell_index` as dirty.
    pub(super) fn mark(&mut self, cell_index: usize) {
        self.dirty |= 1u64 << ((cell_index >> Self::CHUNK_SHIFT) & 0b0011_1111);
    }

    /// Marks all chunks as dirty (used for bulk updates / context loss).
    pub(super) fn mark_all(&mut self) {
        self.dirty = u64::MAX;
    }

    /// Returns a bitmask covering only the chunks that contain actual cells.
    fn active_mask(&self) -> u64 {
        let active_chunks = self.total_cells.div_ceil(Self::CHUNK_SIZE);
        if active_chunks >= 64 { u64::MAX } else { (1u64 << active_chunks) - 1 }
    }

    /// Returns true if every active chunk is dirty.
    pub(super) fn is_all_active_dirty(&self) -> bool {
        let mask = self.active_mask();
        self.dirty & mask == mask
    }

    pub(super) fn clear(&mut self) {
        self.dirty = 0;
    }

    /// Takes the dirty bits and clears them, returning an iterator
    /// over contiguous dirty `(start_cell, end_cell)` ranges.
    ///
    /// When the terminal exceeds 64 chunks (65 536 cells), multiple chunks
    /// alias the same bit. The iterator walks all actual chunks and checks
    /// the aliased bit, so every dirty region is uploaded — aliased chunks
    /// may cause redundant uploads but never missed ones.
    pub(super) fn drain(&mut self) -> DirtyChunkIter {
        let dirty = self.dirty;
        self.dirty = 0;
        let total_chunks = self.total_cells.div_ceil(Self::CHUNK_SIZE);
        DirtyChunkIter {
            dirty,
            total_cells: self.total_cells,
            current_chunk: 0,
            total_chunks,
        }
    }
}

/// Iterator over contiguous runs of dirty chunks, yielding `(start, end)` cell ranges.
///
/// Walks all actual chunks (not just bit positions 0–63), checking each
/// chunk's aliased bit in the `u64` mask. This correctly handles terminals
/// larger than 64 × 1024 = 65 536 cells.
pub(super) struct DirtyChunkIter {
    dirty: u64,
    total_cells: usize,
    current_chunk: usize,
    total_chunks: usize,
}

impl Iterator for DirtyChunkIter {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_chunk < self.total_chunks {
            if self.dirty & (1u64 << (self.current_chunk & 63)) != 0 {
                let start_chunk = self.current_chunk;
                self.current_chunk += 1;
                while self.current_chunk < self.total_chunks
                    && self.dirty & (1u64 << (self.current_chunk & 63)) != 0
                {
                    self.current_chunk += 1;
                }
                let start = start_chunk * DirtyRegions::CHUNK_SIZE;
                let end = (self.current_chunk * DirtyRegions::CHUNK_SIZE).min(self.total_cells);
                return Some((start, end));
            }
            self.current_chunk += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_on_creation() {
        let dr = DirtyRegions::new(10_000);
        assert!(dr.is_clean());
    }

    #[test]
    fn mark_single_cell() {
        let mut dr = DirtyRegions::new(10_000);
        dr.mark(42);
        assert!(!dr.is_clean());
        let ranges: Vec<_> = dr.drain().collect();
        assert_eq!(ranges, vec![(0, 1024)]);
    }

    #[test]
    fn mark_adjacent_chunks_merge() {
        let mut dr = DirtyRegions::new(10_000);
        dr.mark(500); // chunk 0
        dr.mark(1500); // chunk 1
        dr.mark(2500); // chunk 2
        let ranges: Vec<_> = dr.drain().collect();
        assert_eq!(ranges, vec![(0, 3072)]);
    }

    #[test]
    fn mark_separated_chunks() {
        let mut dr = DirtyRegions::new(10_000);
        dr.mark(0); // chunk 0
        dr.mark(5000); // chunk 4
        let ranges: Vec<_> = dr.drain().collect();
        assert_eq!(ranges, vec![(0, 1024), (4096, 5120)]);
    }

    #[test]
    fn all_dirty_detection() {
        let mut dr = DirtyRegions::new(10_000); // 10 chunks
        dr.mark_all();
        assert!(dr.is_all_active_dirty());
    }

    #[test]
    fn drain_clamps_to_total_cells() {
        let mut dr = DirtyRegions::new(1500); // 2 chunks, last chunk partially filled
        dr.mark(1400); // chunk 1
        let ranges: Vec<_> = dr.drain().collect();
        assert_eq!(ranges, vec![(1024, 1500)]); // clamped to total_cells
    }

    #[test]
    fn drain_resets_dirty_state() {
        let mut dr = DirtyRegions::new(10_000);
        dr.mark(42);
        let _ = dr.drain().count();
        assert!(dr.is_clean());
    }

    #[test]
    fn all_dirty_single_range() {
        let mut dr = DirtyRegions::new(2048); // exactly 2 chunks
        dr.mark_all();
        let ranges: Vec<_> = dr.drain().collect();
        // all-dirty produces contiguous run from drain
        assert_eq!(ranges, vec![(0, 2048)]);
    }

    #[test]
    fn aliased_chunk_beyond_64k_is_uploaded() {
        // 70 chunks = 71680 cells; chunk 65 aliases to bit 1
        let mut dr = DirtyRegions::new(71_680);
        dr.mark(66_000); // chunk 64 (aliases bit 0)
        let ranges: Vec<_> = dr.drain().collect();
        assert!(
            ranges
                .iter()
                .any(|(s, e)| *s <= 66_000 && *e > 66_000),
            "expected a range covering cell 66000, got {ranges:?}"
        );
    }

    #[test]
    fn aliased_chunk_also_uploads_lower_alias() {
        // marking chunk 64 (bit 0) should also upload chunk 0
        let mut dr = DirtyRegions::new(71_680);
        dr.mark(66_000); // chunk 64, aliases to bit 0
        let ranges: Vec<_> = dr.drain().collect();
        // bit 0 is set, so both chunk 0 and chunk 64 should be uploaded
        assert!(
            ranges.iter().any(|(s, _)| *s == 0),
            "expected chunk 0 (aliased) to be uploaded, got {ranges:?}"
        );
        assert!(
            ranges
                .iter()
                .any(|(s, e)| *s <= 65_536 && *e > 65_536),
            "expected chunk 64 to be uploaded, got {ranges:?}"
        );
    }

    #[test]
    fn adjacent_aliased_chunks_merge() {
        let mut dr = DirtyRegions::new(71_680);
        dr.mark(65_536); // chunk 64 (bit 0)
        dr.mark(66_560); // chunk 65 (bit 1)
        let ranges: Vec<_> = dr.drain().collect();
        // chunks 0-1 and 64-65 should each be merged into contiguous ranges
        assert!(
            ranges.iter().any(|(s, e)| *s == 0 && *e >= 2048),
            "expected chunks 0-1 merged, got {ranges:?}"
        );
        assert!(
            ranges
                .iter()
                .any(|(s, e)| *s == 65_536 && *e >= 67_584),
            "expected chunks 64-65 merged, got {ranges:?}"
        );
    }
}
