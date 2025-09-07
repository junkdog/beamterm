use clap::Parser;

use crate::font_discovery::{FontDiscovery, FontFamily};

#[derive(Parser, Debug)]
#[command(
    name = "beamterm-atlas",
    about = "Font atlas generator for beamterm WebGL terminal renderer",
    long_about = "Generates GPU-optimized texture arrays from TTF/OTF fonts for high-performance terminal rendering"
)]
pub struct Cli {
    /// Font selection: name (partial match) or 1-based index
    #[arg(value_name = "FONT", required_unless_present = "list_fonts")]
    pub font: Option<String>,

    /// Font size in points
    #[arg(short = 's', long, default_value = "15.0", value_name = "SIZE")]
    pub font_size: f32,

    /// Line height multiplier
    #[arg(short = 'l', long, default_value = "1.0", value_name = "MULTIPLIER")]
    pub line_height: f32,

    /// Output file path
    #[arg(
        short = 'o',
        long,
        default_value = "./bitmap_font.atlas",
        value_name = "PATH"
    )]
    pub output: String,

    /// Underline position (0.0 = top, 1.0 = bottom of cell)
    #[arg(long, default_value = "0.85", value_name = "FRACTION")]
    pub underline_position: f32,

    /// Underline thickness as percentage of cell height
    #[arg(long, default_value = "5.0", value_name = "PERCENT")]
    pub underline_thickness: f32,

    /// Strikethrough position (0.0 = top, 1.0 = bottom of cell)
    #[arg(long, default_value = "0.5", value_name = "FRACTION")]
    pub strikethrough_position: f32,

    /// Strikethrough thickness as percentage of cell height  
    #[arg(long, default_value = "5.0", value_name = "PERCENT")]
    pub strikethrough_thickness: f32,

    /// List available fonts and exit
    #[arg(short = 'L', long)]
    pub list_fonts: bool,

    /// Check for missing glyphs and show detailed coverage report
    #[arg(long)]
    pub check_missing: bool,
}

impl Cli {
    /// Selects a font based on the CLI arguments and available fonts
    pub fn select_font<'a>(
        &self,
        available_fonts: &'a [FontFamily],
    ) -> Result<&'a FontFamily, String> {
        if available_fonts.is_empty() {
            return Err("No complete monospace font families found!".to_string());
        }

        let font = self
            .font
            .as_ref()
            .ok_or("Font selection required")?;

        // Try parsing as index first (1-based)
        if let Ok(idx) = font.parse::<usize>() {
            if idx > 0 && idx <= available_fonts.len() {
                return Ok(&available_fonts[idx - 1]);
            } else {
                return Err(format!(
                    "Font index {} out of range (1-{})",
                    idx,
                    available_fonts.len()
                ));
            }
        }

        // Try to find by name (case-insensitive partial match)
        available_fonts
            .iter()
            .find(|f| {
                f.name
                    .to_lowercase()
                    .contains(&font.to_lowercase())
            })
            .ok_or_else(|| format!("Font '{font}' not found"))
    }

    /// Displays the list of available fonts
    pub fn display_font_list() {
        println!("Discovering monospace fonts...");
        let discovery = FontDiscovery::new();
        let available_fonts = discovery.discover_complete_monospace_families();

        if available_fonts.is_empty() {
            println!("No complete monospace font families found!");
            println!(
                "A complete font family must have: Regular, Bold, Italic, and Bold+Italic variants"
            );
            return;
        }

        println!("\nAvailable monospace fonts with all variants:");
        println!("{:<4} Font Name", "ID");
        println!("{}", "-".repeat(50));

        for (i, font) in available_fonts.iter().enumerate() {
            println!("{:<4} {}", i + 1, font.name);
        }

        println!("\nTotal: {} font families", available_fonts.len());
    }

    /// Validates the CLI arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.font_size <= 0.0 {
            return Err("Font size must be positive".to_string());
        }

        if self.line_height <= 0.0 {
            return Err("Line height must be positive".to_string());
        }

        // Validate position values are in [0.0, 1.0]
        if self.underline_position < 0.0 || self.underline_position > 1.0 {
            return Err("Underline position must be between 0.0 and 1.0".to_string());
        }

        if self.strikethrough_position < 0.0 || self.strikethrough_position > 1.0 {
            return Err("Strikethrough position must be between 0.0 and 1.0".to_string());
        }

        // Validate thickness values are reasonable percentages
        if self.underline_thickness <= 0.0 || self.underline_thickness > 100.0 {
            return Err("Underline thickness must be between 0 and 100 percent".to_string());
        }

        if self.strikethrough_thickness <= 0.0 || self.strikethrough_thickness > 100.0 {
            return Err("Strikethrough thickness must be between 0 and 100 percent".to_string());
        }

        Ok(())
    }

    /// Prints a summary of the configuration
    pub fn print_summary(&self, font_name: &str) {
        println!("\nGenerating font atlas:");
        println!("  Font: {font_name}");
        println!("  Size: {}pt", self.font_size);
        println!("  Line height: {}x", self.line_height);
        println!("  Output: {}", self.output);

        if self.underline_thickness != 5.0 || self.underline_position != 0.85 {
            println!(
                "  Underline: {}% thick at {:.0}% height",
                self.underline_thickness,
                self.underline_position * 100.0
            );
        }

        if self.strikethrough_thickness != 5.0 || self.strikethrough_position != 0.5 {
            println!(
                "  Strikethrough: {}% thick at {:.0}% height",
                self.strikethrough_thickness,
                self.strikethrough_position * 100.0
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_validation() {
        let cli = Cli {
            font: Some("test".to_string()),
            font_size: 15.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 0.85,
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
        };

        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_invalid_font_size() {
        let cli = Cli {
            font: Some("test".to_string()),
            font_size: -1.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 0.85,
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
        };

        assert!(cli.validate().is_err());
    }

    #[test]
    fn test_invalid_position() {
        let cli = Cli {
            font: Some("test".to_string()),
            font_size: 15.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 1.5, // Invalid: > 1.0
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
        };

        assert!(cli.validate().is_err());
    }
}
