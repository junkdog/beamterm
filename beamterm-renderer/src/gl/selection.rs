use std::{cell::RefCell, rc::Rc};

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

impl TerminalMetrics {
    /// Creates a new terminal metrics tracker.
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
    pub fn set(&self, cols: u16, rows: u16, cell_width: f32, cell_height: f32) {
        *self.inner.borrow_mut() = TerminalMetricsInner { cols, rows, cell_width, cell_height };
    }

    /// Returns all metrics: (cols, rows, cell_width, cell_height).
    pub fn get(&self) -> (u16, u16, f32, f32) {
        let inner = self.inner.borrow();
        (inner.cols, inner.rows, inner.cell_width, inner.cell_height)
    }

    /// Returns a cloned reference to the internal metrics storage.
    pub fn clone_ref(&self) -> Rc<RefCell<TerminalMetricsInner>> {
        self.inner.clone()
    }
}
