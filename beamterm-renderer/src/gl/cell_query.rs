use compact_str::{CompactString, CompactStringExt};

use crate::gl::TerminalGrid;

/// Configuration for querying and extracting text from terminal cells.
///
/// Defines the selection mode, coordinate range, and text processing options
/// for extracting content from the terminal grid.
#[derive(Debug, Clone, Copy, Default)]
pub struct CellQuery {
    pub(crate) mode: SelectionMode,
    pub(super) start: Option<(u16, u16)>,
    pub(super) end: Option<(u16, u16)>,
    pub(super) trim_trailing_whitespace: bool,
    /// Hash of the selected content when selection was completed.
    /// Used to detect if underlying content has changed.
    pub(crate) content_hash: Option<u64>,
}

/// Defines how cells are selected in the terminal grid.
#[derive(Debug, Clone, Copy, Default)]
pub enum SelectionMode {
    /// Rectangular selection of cells.
    ///
    /// Selects all cells within the rectangle defined by start and end points.
    #[default]
    Block,
    /// Linear selection following text flow.
    ///
    /// Selects cells from start to end following line wrapping, similar to
    /// standard text selection in terminals.
    Linear,
}

/// Zero-allocation iterator over terminal cell indices.
///
/// Provides efficient iteration over selected cells without allocating
/// intermediate collections.
#[derive(Debug)]
pub enum CellIterator {
    /// Iterator for block (rectangular) selections.
    Block(BlockCellIterator),
    /// Iterator for linear (text-flow) selections.
    Linear(LinearCellIterator),
}

/// Iterator for block (rectangular) cell selection.
///
/// Iterates over cells row by row within the rectangular region defined
/// by start and end coordinates.
#[derive(Debug)]
pub struct BlockCellIterator {
    cols: u16,
    start: (u16, u16),
    end: (u16, u16),
    current: (u16, u16),
    finished: bool,
}

/// Iterator for linear cell selection.
///
/// Iterates over cells following text flow from start to end position,
/// wrapping at line boundaries like standard text selection.
#[derive(Debug)]
pub struct LinearCellIterator {
    cols: u16,
    current_idx: usize,
    end_idx: usize,
    finished: bool,
}

/// Creates a new cell query with the specified selection mode.
///
/// # Example
/// ```
/// use beamterm_renderer::{select, SelectionMode};
///
/// let query = select(SelectionMode::Block)
///     .start((0, 0))
///     .end((10, 5))
///     .trim_trailing_whitespace(true);
/// ```
pub fn select(mode: SelectionMode) -> CellQuery {
    CellQuery { mode, ..CellQuery::default() }
}

impl CellQuery {
    /// Sets the starting position for the selection.
    ///
    /// # Arguments
    /// * `start` - Starting coordinates as (column, row)
    pub fn start(mut self, start: (u16, u16)) -> Self {
        self.start = Some(start);
        self
    }

    /// Sets the ending position for the selection.
    ///
    /// # Arguments
    /// * `end` - Ending coordinates as (column, row)
    pub fn end(mut self, end: (u16, u16)) -> Self {
        self.end = Some(end);
        self
    }

    /// Checks if the query has no selection range defined.
    pub fn is_empty(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }

    /// Returns the normalized selection range if both start and end are defined.
    ///
    /// The returned range has coordinates ordered so that the first tuple
    /// contains the minimum coordinates and the second contains the maximum.
    pub fn range(&self) -> Option<((u16, u16), (u16, u16))> {
        if let (Some(start), Some(end)) = (self.start, self.end) {
            Some((
                (start.0.min(end.0), start.1.min(end.1)),
                (start.0.max(end.0), start.1.max(end.1)),
            ))
        } else {
            None
        }
    }

    /// Configures whether to remove trailing whitespace from each line.
    ///
    /// When enabled, spaces at the end of each selected line are removed
    /// from the extracted text.
    pub fn trim_trailing_whitespace(mut self, enabled: bool) -> Self {
        self.trim_trailing_whitespace = enabled;
        self
    }

    /// Sets the content hash for this query.
    ///
    /// The hash is computed from the selected cells when the selection
    /// is completed, and used to detect if the underlying content changes.
    pub(crate) fn with_content_hash(mut self, hash: u64) -> Self {
        self.content_hash = Some(hash);
        self
    }
}

impl Iterator for CellIterator {
    type Item = (usize, bool); // (cell_index, needs_newline_after)

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            CellIterator::Block(iter) => iter.next(),
            CellIterator::Linear(iter) => iter.next(),
        }
    }
}

impl Iterator for BlockCellIterator {
    type Item = (usize, bool);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.current.1 > self.end.1 {
            return None;
        }

        let idx = self.current.1 as usize * self.cols as usize + self.current.0 as usize;

        // Check if we need a newline after this cell
        let is_end_of_row = self.current.0 == self.end.0;
        let is_last_row = self.current.1 == self.end.1;
        let needs_newline = is_end_of_row && !is_last_row;

        // Advance to next position
        if self.current.0 < self.end.0 {
            self.current.0 += 1;
        } else {
            self.current.0 = self.start.0;
            self.current.1 += 1;
            if self.current.1 > self.end.1 {
                self.finished = true;
            }
        }

        Some((idx, needs_newline))
    }
}

impl Iterator for LinearCellIterator {
    type Item = (usize, bool);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.current_idx > self.end_idx {
            return None;
        }

        let idx = self.current_idx;

        // Check if we need a newline before this cell (except for the first cell)
        let is_row_start = idx.is_multiple_of(self.cols as usize);
        let is_first_cell = idx == self.current_idx;
        let needs_newline_before = is_row_start && !is_first_cell;

        self.current_idx += 1;
        if self.current_idx > self.end_idx {
            self.finished = true;
        }

        // Check if NEXT cell will need a newline before it
        let needs_newline_after = if self.current_idx <= self.end_idx {
            self.current_idx
                .is_multiple_of(self.cols as usize)
        } else {
            false
        };

        Some((idx, needs_newline_after))
    }
}

impl BlockCellIterator {
    /// Creates a new block iterator with bounds checking.
    ///
    /// Ensures coordinates are within terminal bounds and properly ordered.
    fn new(cols: u16, start: (u16, u16), end: (u16, u16), max_cells: usize) -> Self {
        // Bounds checking and coordinate ordering
        let start = (
            start.0.min(cols.saturating_sub(1)),
            start
                .1
                .min((max_cells / cols as usize).saturating_sub(1) as u16),
        );
        let end = (
            end.0.min(cols.saturating_sub(1)),
            end.1
                .min((max_cells / cols as usize).saturating_sub(1) as u16),
        );
        let (start, end) = if start > end { (end, start) } else { (start, end) };

        Self {
            cols,
            start,
            end,
            current: start,
            finished: start.1 > end.1,
        }
    }
}

impl LinearCellIterator {
    /// Creates a new linear iterator with bounds checking.
    ///
    /// Converts coordinates to linear indices and ensures they're within bounds.
    fn new(cols: u16, start: (u16, u16), end: (u16, u16), max_cells: usize) -> Self {
        let cols_usize = cols as usize;

        // Bounds checking and coordinate ordering
        let start = (
            start.0.min(cols.saturating_sub(1)),
            start
                .1
                .min((max_cells / cols_usize).saturating_sub(1) as u16),
        );
        let end = (
            end.0.min(cols.saturating_sub(1)),
            end.1
                .min((max_cells / cols_usize).saturating_sub(1) as u16),
        );
        let (start, end) = if start > end { (end, start) } else { (start, end) };

        let start_idx = start.1 as usize * cols_usize + start.0 as usize;
        let end_idx = end.1 as usize * cols_usize + end.0 as usize;
        let end_idx = end_idx.min(max_cells.saturating_sub(1));
        let start_idx = start_idx.min(end_idx);

        Self {
            cols,
            current_idx: start_idx,
            end_idx,
            finished: start_idx > end_idx,
        }
    }
}

impl TerminalGrid {
    /// Zero-allocation iterator over cell indices for a given selection range and mode.
    ///
    /// Creates an efficient iterator that yields cell indices and newline indicators
    /// without allocating intermediate collections.
    ///
    /// # Returns
    /// Iterator yielding (cell_index, needs_newline_after) tuples.
    pub fn cell_iter(&self, selection: CellQuery) -> CellIterator {
        let cols = self.terminal_size().0;
        let max_cells = self.cell_count();

        let (start, end) = selection.range().unwrap_or_default();

        match selection.mode {
            SelectionMode::Block => {
                CellIterator::Block(BlockCellIterator::new(cols, start, end, max_cells))
            },
            SelectionMode::Linear => {
                CellIterator::Linear(LinearCellIterator::new(cols, start, end, max_cells))
            },
        }
    }

    /// Extracts text content from the terminal based on the selection query.
    ///
    /// Retrieves the text within the selection range, optionally trimming
    /// trailing whitespace from each line based on the query configuration.
    ///
    /// # Arguments
    /// * `selection` - Query defining the selection range and options
    ///
    /// # Returns
    /// The selected text as a `CompactString`, or empty string if no selection.
    pub fn get_text(&self, selection: CellQuery) -> CompactString {
        if selection.is_empty() {
            return CompactString::const_new("");
        }

        let text = self.get_symbols(self.cell_iter(selection));

        if selection.trim_trailing_whitespace {
            text.lines().map(str::trim_end).join_compact("\n")
        } else {
            text
        }
    }
}
