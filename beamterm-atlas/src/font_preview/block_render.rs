use beamterm_data::FontStyle;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};

/// Converts a rasterized glyph bitmap to terminal block characters using ratatui Buffer
pub struct BlockRenderer {
    /// Width of each rendered character in terminal cells
    pub char_width: usize,
    /// Height of each rendered character in terminal cells  
    pub char_height: usize,
}

impl BlockRenderer {
    pub fn new(char_width: usize, char_height: usize) -> Self {
        Self { char_width, char_height }
    }

    /// Render a bitmap glyph directly to a ratatui Buffer
    pub fn render_to_buffer(
        &self,
        buffer: &mut Buffer,
        area: Rect,
        bitmap: &[u8],
        width: usize,
        height: usize,
        style: Style,
    ) {
        let scale_x = width as f32 / area.width as f32;
        let scale_y = height as f32 / area.height as f32;

        for y in 0..area.height {
            for x in 0..area.width {
                let coverage = self.sample_coverage_for_buffer(
                    bitmap, width, height, x as usize, y as usize, scale_x, scale_y,
                );

                let block_char = self.coverage_to_block(coverage);

                if block_char != ' ' {
                    if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                        cell.set_char(block_char).set_style(style);
                    }
                }
            }
        }
    }

    /// Sample coverage for direct buffer rendering
    fn sample_coverage_for_buffer(
        &self,
        bitmap: &[u8],
        width: usize,
        height: usize,
        cell_x: usize,
        cell_y: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> f32 {
        let samples = 2; // 2x2 subsampling for better quality
        let mut total_coverage = 0.0;
        let mut sample_count = 0;

        for sy in 0..samples {
            for sx in 0..samples {
                let offset_x = sx as f32 / samples as f32;
                let offset_y = sy as f32 / samples as f32;

                let sample_x = ((cell_x as f32 + offset_x) * scale_x) as usize;
                let sample_y = ((cell_y as f32 + offset_y) * scale_y) as usize;

                if sample_x < width && sample_y < height {
                    let pixel_idx = sample_y * width + sample_x;
                    if pixel_idx < bitmap.len() {
                        let alpha = if bitmap.len() == width * height * 4 {
                            // RGBA format
                            bitmap[pixel_idx * 4 + 3]
                        } else {
                            // Grayscale format
                            bitmap[pixel_idx]
                        };

                        total_coverage += alpha as f32 / 255.0;
                        sample_count += 1;
                    }
                }
            }
        }

        if sample_count > 0 {
            total_coverage / sample_count as f32
        } else {
            0.0
        }
    }

    /// Convert a bitmap glyph to block characters
    pub fn render_glyph(&self, bitmap: &[u8], width: usize, height: usize) -> Vec<String> {
        let mut result = Vec::new();

        // Calculate scaling factors
        let scale_x = width as f32 / self.char_width as f32;
        let scale_y = height as f32 / self.char_height as f32;

        for y in 0..self.char_height {
            let mut line = String::new();

            for x in 0..self.char_width {
                // Sample the bitmap at this position
                let sample_x = (x as f32 * scale_x) as usize;
                let sample_y = (y as f32 * scale_y) as usize;

                let block_char = if sample_x < width && sample_y < height {
                    let pixel_idx = sample_y * width + sample_x;
                    if pixel_idx < bitmap.len() {
                        // For RGBA, we use the alpha channel to determine coverage
                        let alpha = if bitmap.len() == width * height * 4 {
                            bitmap[pixel_idx * 4 + 3]
                        } else {
                            // Assume grayscale
                            bitmap[pixel_idx]
                        };

                        self.alpha_to_block(alpha)
                    } else {
                        ' '
                    }
                } else {
                    ' '
                };

                line.push(block_char);
            }

            result.push(line);
        }

        result
    }

    /// Convert alpha value to appropriate block character
    fn alpha_to_block(&self, alpha: u8) -> char {
        match alpha {
            0..=63 => ' ',    // Transparent
            64..=127 => '░',  // Light shade
            128..=191 => '▒', // Medium shade
            192..=223 => '▓', // Dark shade
            224..=255 => '█', // Full block
        }
    }

    /// Render a specific font style variant with better block selection
    pub fn render_glyph_advanced(&self, bitmap: &[u8], width: usize, height: usize) -> Vec<String> {
        let mut result = Vec::new();

        let scale_x = width as f32 / self.char_width as f32;
        let scale_y = height as f32 / self.char_height as f32;

        for y in 0..self.char_height {
            let mut line = String::new();

            for x in 0..self.char_width {
                // Sample multiple points for better quality
                let coverage = self.sample_coverage(bitmap, width, height, x, y, scale_x, scale_y);
                let block_char = self.coverage_to_block(coverage);
                line.push(block_char);
            }

            result.push(line);
        }

        result
    }

    /// Sample coverage in a cell area
    fn sample_coverage(
        &self,
        bitmap: &[u8],
        width: usize,
        height: usize,
        cell_x: usize,
        cell_y: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> f32 {
        let samples = 4; // 2x2 subsampling
        let mut total_coverage = 0.0;
        let mut sample_count = 0;

        for sy in 0..samples {
            for sx in 0..samples {
                let offset_x = sx as f32 / samples as f32;
                let offset_y = sy as f32 / samples as f32;

                let sample_x = ((cell_x as f32 + offset_x) * scale_x) as usize;
                let sample_y = ((cell_y as f32 + offset_y) * scale_y) as usize;

                if sample_x < width && sample_y < height {
                    let pixel_idx = sample_y * width + sample_x;
                    if pixel_idx < bitmap.len() {
                        let alpha = if bitmap.len() == width * height * 4 {
                            bitmap[pixel_idx * 4 + 3]
                        } else {
                            bitmap[pixel_idx]
                        };

                        total_coverage += alpha as f32 / 255.0;
                        sample_count += 1;
                    }
                }
            }
        }

        if sample_count > 0 {
            total_coverage / sample_count as f32
        } else {
            0.0
        }
    }

    /// Convert coverage percentage to block character
    fn coverage_to_block(&self, coverage: f32) -> char {
        match coverage {
            c if c < 0.125 => ' ',
            c if c < 0.375 => '░',
            c if c < 0.625 => '▒',
            c if c < 0.875 => '▓',
            _ => '█',
        }
    }
}

/// Helper function to create a demo glyph for testing
pub fn create_demo_glyph(symbol: char, style: FontStyle) -> Vec<String> {
    let renderer = BlockRenderer::new(8, 12);

    // Create a simple bitmap pattern for demo
    let width = 32;
    let height = 48;
    let mut bitmap = vec![0u8; width * height];

    // Draw a simple pattern based on the symbol
    for y in 8..40 {
        for x in 4..28 {
            let distance_from_edge =
                std::cmp::min(std::cmp::min(x - 4, 28 - x), std::cmp::min(y - 8, 40 - y));

            if distance_from_edge < 3 {
                bitmap[y * width + x] = 255;
            } else if distance_from_edge < 6 {
                bitmap[y * width + x] = 128;
            }
        }
    }

    renderer.render_glyph(&bitmap, width, height)
}
