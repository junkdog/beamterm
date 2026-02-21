/// A position in the terminal grid, specified by column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub col: u16,
    pub row: u16,
}

impl CursorPosition {
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
