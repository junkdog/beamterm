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
    pub(super) fn drain(&mut self) -> DirtyChunkIter {
        let dirty = self.dirty & self.active_mask();
        self.dirty = 0;
        DirtyChunkIter { dirty, total_cells: self.total_cells }
    }
}

/// Iterator over contiguous runs of dirty chunks, yielding `(start, end)` cell ranges.
pub(super) struct DirtyChunkIter {
    dirty: u64,
    total_cells: usize,
}

impl Iterator for DirtyChunkIter {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.dirty == 0 {
            return None;
        }

        let start_chunk = self.dirty.trailing_zeros() as usize;
        let run_len = (!(self.dirty >> start_chunk)).trailing_zeros() as usize;

        let start = start_chunk * DirtyRegions::CHUNK_SIZE;
        let end = ((start_chunk + run_len) * DirtyRegions::CHUNK_SIZE).min(self.total_cells);

        // clear the contiguous run of bits
        self.dirty &= !(((1u64 << run_len) - 1) << start_chunk);

        Some((start, end))
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
}
