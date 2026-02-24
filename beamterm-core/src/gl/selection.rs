use std::{cell::RefCell, fmt::Debug, rc::Rc};

use crate::gl::cell_query::CellQuery;

/// Tracks the active text selection in the terminal grid.
///
/// Manages the current selection query and provides methods to update or clear
/// the selection. Uses interior mutability to allow shared access across
/// multiple components.
#[derive(Debug, Clone)]
pub struct SelectionTracker {
    inner: Rc<RefCell<SelectionTrackerInner>>,
}

#[derive(Debug, Default)]
struct SelectionTrackerInner {
    query: Option<CellQuery>,
}

impl SelectionTracker {
    /// Creates a new selection tracker with no active selection.
    pub(crate) fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(SelectionTrackerInner::default())),
        }
    }

    /// Clears the current selection.
    ///
    /// Removes any active selection from the terminal grid.
    pub fn clear(&self) {
        *self.inner.borrow_mut() = SelectionTrackerInner::default();
    }

    /// Returns the current selection query.
    ///
    /// # Panics
    /// Panics if no selection is active. This is internal-only API where
    /// a selection is guaranteed to exist when called.
    pub fn query(&self) -> CellQuery {
        self.get_query()
            .expect("query to be a value due to internal-only usage")
    }

    /// Returns the current selection query or `None` if no selection is active.
    ///
    /// Safe version that doesn't panic when no selection exists.
    pub fn get_query(&self) -> Option<CellQuery> {
        self.inner.borrow().query
    }

    /// Sets a new selection query.
    ///
    /// Replaces any existing selection with the provided query.
    pub fn set_query(&self, query: CellQuery) {
        self.inner.borrow_mut().query = Some(query);
    }

    /// Updates the end position of the current selection.
    ///
    /// Used during mouse drag operations to extend the selection.
    pub fn update_selection_end(&self, end: (u16, u16)) {
        if let Some(query) = &mut self.inner.borrow_mut().query {
            *query = query.end(end);
        }
    }

    /// Sets the content hash on the current query.
    ///
    /// The hash is stored with the query to detect if underlying content changes.
    pub fn set_content_hash(&self, hash: u64) {
        if let Some(query) = &mut self.inner.borrow_mut().query {
            *query = query.with_content_hash(hash);
        }
    }
}
