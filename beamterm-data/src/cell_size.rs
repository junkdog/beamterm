/// Dimensions of a terminal cell in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: i32,
    pub height: i32,
}

impl CellSize {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    /// Scales each dimension by the given factor, rounding to the nearest integer.
    pub fn scale(self, factor: f32) -> Self {
        Self {
            width: (self.width as f32 * factor).round() as i32,
            height: (self.height as f32 * factor).round() as i32,
        }
    }
}
