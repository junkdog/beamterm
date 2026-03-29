use std::{cell::Cell, fmt::Debug, rc::Rc};

use crate::gl::cell_query::CellQuery;

/// Tracks the active text selection in the terminal grid.
///
/// Manages the current selection query and provides methods to update or clear
/// the selection. Uses `Cell` for interior mutability since `CellQuery` is `Copy`.
#[derive(Clone)]
pub struct SelectionTracker {
    inner: Rc<Cell<Option<CellQuery>>>,
}

impl Debug for SelectionTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionTracker")
            .field("query", &self.inner.get())
            .finish()
    }
}

impl SelectionTracker {
    /// Creates a new selection tracker with no active selection.
    pub(crate) fn new() -> Self {
        Self { inner: Rc::new(Cell::new(None)) }
    }

    /// Clears the current selection.
    ///
    /// Removes any active selection from the terminal grid.
    pub fn clear(&self) {
        self.inner.set(None);
    }

    /// Returns the current selection query.
    ///
    /// # Panics
    /// Panics if no selection is active. This is internal-only API where
    /// a selection is guaranteed to exist when called.
    #[must_use]
    pub fn query(&self) -> CellQuery {
        self.get_query()
            .expect("query to be a value due to internal-only usage")
    }

    /// Returns the current selection query or `None` if no selection is active.
    ///
    /// Safe version that doesn't panic when no selection exists.
    #[must_use]
    pub fn get_query(&self) -> Option<CellQuery> {
        self.inner.get()
    }

    /// Sets a new selection query.
    ///
    /// Replaces any existing selection with the provided query.
    pub fn set_query(&self, query: CellQuery) {
        self.inner.set(Some(query));
    }

    /// Updates the end position of the current selection.
    ///
    /// Used during mouse drag operations to extend the selection.
    pub fn update_selection_end(&self, end: (u16, u16)) {
        if let Some(query) = self.inner.get() {
            self.inner.set(Some(query.end(end)));
        }
    }

    /// Sets the content hash on the current query.
    ///
    /// The hash is stored with the query to detect if underlying content changes.
    pub fn set_content_hash(&self, hash: u64) {
        if let Some(query) = self.inner.get() {
            self.inner
                .set(Some(query.with_content_hash(hash)));
        }
    }
}
