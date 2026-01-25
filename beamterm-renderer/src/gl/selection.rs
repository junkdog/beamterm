use std::{
    cell::{RefCell, RefMut},
    fmt::{Debug, Formatter},
    rc::Rc,
};

use compact_str::CompactString;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

use crate::{
    Error,
    gl::{
        TerminalGrid,
        cell_query::{CellQuery, SelectionMode, select},
    },
};

/// Tracks the active text selection in the terminal grid.
///
/// Manages the current selection query and provides methods to update or clear
/// the selection. Uses interior mutability to allow shared access across
/// multiple components.
#[derive(Debug, Clone)]
pub(crate) struct SelectionTracker {
    inner: Rc<RefCell<SelectionTrackerInner>>,
}

#[derive(Debug, Default)]
struct SelectionTrackerInner {
    query: Option<CellQuery>,
}

/// Tracks terminal metrics for coordinate calculations.
///
/// Maintains both terminal dimensions (cols, rows) and cell size (width, height)
/// in a single shared structure. Used by mouse handlers to convert between
/// pixel and cell coordinates.
pub(crate) struct TerminalMetrics {
    inner: Rc<RefCell<TerminalMetricsInner>>,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalMetricsInner {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl SelectionTracker {
    /// Creates a new selection tracker with no active selection.
    pub(super) fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(SelectionTrackerInner::default())),
        }
    }

    /// Clears the current selection.
    ///
    /// Removes any active selection from the terminal grid.
    pub(crate) fn clear(&self) {
        *self.inner.borrow_mut() = SelectionTrackerInner::default();
    }

    /// Returns the current selection query.
    ///
    /// # Panics
    /// Panics if no selection is active. This is internal-only API where
    /// a selection is guaranteed to exist when called.
    pub(crate) fn query(&self) -> CellQuery {
        self.get_query()
            .expect("query to be a value due to internal-only usage")
    }

    /// Returns the selection mode of the current query.
    ///
    /// Defaults to the default selection mode if no query is active.
    pub(super) fn mode(&self) -> SelectionMode {
        self.inner
            .borrow()
            .query
            .as_ref()
            .map_or(SelectionMode::default(), |q| q.mode)
    }

    /// Returns the current selection query or `None` if no selection is active.
    ///
    /// Safe version that doesn't panic when no selection exists.
    pub(crate) fn get_query(&self) -> Option<CellQuery> {
        self.inner.borrow().query
    }

    /// Sets a new selection query.
    ///
    /// Replaces any existing selection with the provided query.
    pub(crate) fn set_query(&self, query: CellQuery) {
        self.inner.borrow_mut().query = Some(query);
    }

    /// Updates the end position of the current selection.
    ///
    /// Used during mouse drag operations to extend the selection.
    pub(crate) fn update_selection_end(&self, end: (u16, u16)) {
        if let Some(query) = &mut self.inner.borrow_mut().query {
            *query = query.end(end);
        }
    }

    /// Sets the content hash on the current query.
    ///
    /// The hash is stored with the query to detect if underlying content changes.
    pub(crate) fn set_content_hash(&self, hash: u64) {
        if let Some(query) = &mut self.inner.borrow_mut().query {
            *query = query.with_content_hash(hash);
        }
    }
}

impl TerminalMetrics {
    /// Creates a new terminal metrics tracker.
    ///
    /// # Arguments
    /// * `cols` - Number of columns in the terminal
    /// * `rows` - Number of rows in the terminal
    /// * `cell_width` - Cell width in CSS pixels (can be fractional)
    /// * `cell_height` - Cell height in CSS pixels (can be fractional)
    pub fn new(cols: u16, rows: u16, cell_width: f32, cell_height: f32) -> Self {
        Self {
            inner: Rc::new(RefCell::new(TerminalMetricsInner {
                cols,
                rows,
                cell_width,
                cell_height,
            })),
        }
    }

    /// Updates the terminal metrics.
    ///
    /// Should be called whenever the terminal is resized or the font atlas changes.
    pub fn set(&self, cols: u16, rows: u16, cell_width: f32, cell_height: f32) {
        *self.inner.borrow_mut() = TerminalMetricsInner { cols, rows, cell_width, cell_height };
    }

    /// Returns all metrics: (cols, rows, cell_width, cell_height).
    pub fn get(&self) -> (u16, u16, f32, f32) {
        let inner = self.inner.borrow();
        (inner.cols, inner.rows, inner.cell_width, inner.cell_height)
    }

    /// Returns a cloned reference to the internal metrics storage.
    ///
    /// Used for sharing metrics across closures and event handlers.
    pub fn clone_ref(&self) -> Rc<RefCell<TerminalMetricsInner>> {
        self.inner.clone()
    }
}
