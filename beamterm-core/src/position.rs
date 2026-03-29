/// A position in the terminal grid, specified by column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    /// Column index (zero-based).
    pub col: u16,
    /// Row index (zero-based).
    pub row: u16,
}

impl CursorPosition {
    /// Creates a new cursor position at the given column and row.
    #[must_use]
    pub fn new(col: u16, row: u16) -> Self {
        Self { col, row }
    }

    pub(crate) fn move_left(self, distance: u16) -> Option<CursorPosition> {
        self.col
            .checked_sub(distance)
            .map(|col| CursorPosition::new(col, self.row))
    }

    pub(crate) fn move_right(self, distance: u16, row_length: u16) -> Option<CursorPosition> {
        self.col
            .checked_add(distance)
            .map(|col| CursorPosition::new(col, self.row))
            .filter(|&pos| pos.col < row_length)
    }
}
