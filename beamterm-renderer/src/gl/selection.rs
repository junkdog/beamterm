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
    query: Rc<RefCell<Option<CellQuery>>>,
}

/// Tracks terminal dimensions for coordinate calculations.
///
/// Maintains the current terminal size in cells and provides shared access
/// for components that need to convert between pixel and cell coordinates.
pub(crate) struct TerminalDimensions {
    size: Rc<RefCell<(u16, u16)>>,
}

impl SelectionTracker {
    /// Creates a new selection tracker with no active selection.
    pub(super) fn new() -> Self {
        Self { query: Rc::new(RefCell::new(None)) }
    }

    /// Clears the current selection.
    ///
    /// Removes any active selection from the terminal grid.
    pub(crate) fn clear(&self) {
        *self.query.borrow_mut() = None;
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
        self.query
            .borrow()
            .as_ref()
            .map_or(SelectionMode::default(), |q| q.mode)
    }

    /// Returns the current selection query or `None` if no selection is active.
    ///
    /// Safe version that doesn't panic when no selection exists.
    pub(crate) fn get_query(&self) -> Option<CellQuery> {
        *self.query.borrow()
    }

    /// Sets a new selection query.
    ///
    /// Replaces any existing selection with the provided query.
    pub(crate) fn set_query(&self, query: CellQuery) {
        *self.query.borrow_mut() = Some(query);
    }

    /// Updates the end position of the current selection.
    ///
    /// Used during mouse drag operations to extend the selection.
    /// Does nothing if no selection is active.
    pub(crate) fn update_selection_end(&self, end: (u16, u16)) {
        if let Some(query) = self.query.borrow_mut().as_mut() {
            *query = query.end(end);
        }
    }
}

impl TerminalDimensions {
    /// Creates a new terminal dimensions tracker.
    ///
    /// # Arguments
    /// * `cols` - Number of columns in the terminal
    /// * `rows` - Number of rows in the terminal
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { size: Rc::new(RefCell::new((cols, rows))) }
    }

    /// Updates the terminal dimensions.
    ///
    /// Should be called whenever the terminal is resized.
    pub fn set(&self, cols: u16, rows: u16) {
        *self.size.borrow_mut() = (cols, rows);
    }

    /// Returns the current terminal dimensions as (columns, rows).
    pub fn get(&self) -> (u16, u16) {
        *self.size.borrow()
    }

    /// Returns a cloned reference to the internal size storage.
    ///
    /// Used for sharing dimensions across closures and event handlers.
    pub fn clone_ref(&self) -> Rc<RefCell<(u16, u16)>> {
        self.size.clone()
    }
}
