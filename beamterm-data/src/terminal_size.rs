/// Dimensions of a terminal grid in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    /// Column count.
    pub cols: u16,
    /// Row count.
    pub rows: u16,
}

impl TerminalSize {
    /// Creates a new terminal size with the given column and row counts.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}
