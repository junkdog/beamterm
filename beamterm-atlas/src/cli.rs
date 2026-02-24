use std::{ops::RangeInclusive, path::PathBuf};

use beamterm_data::DebugSpacePattern;
use clap::Parser;
use color_eyre::{Report, eyre::eyre};

use crate::font_discovery::{FontDiscovery, FontFamily};

#[derive(Parser, Debug)]
#[command(
    name = "beamterm-atlas",
    about = "Font atlas generator for beamterm terminal renderer",
    long_about = "Generates GPU-optimized texture arrays from TTF/OTF fonts for high-performance terminal rendering"
)]
pub struct Cli {
    /// Font selection: name (partial match) or 1-based index
    #[arg(value_name = "FONT", required_unless_present = "list_fonts")]
    pub font: Option<String>,

    /// Emoji font family name to use for emoji glyphs
    #[arg(long, value_name = "FONT", default_value = "Noto Color Emoji")]
    pub emoji_font: String,

    /// File containing symbols (including emoji) to include in the atlas (optional if ranges cover all needed symbols)
    #[arg(long, value_parser = validate_file_exists)]
    pub symbols_file: Option<PathBuf>,

    /// Unicode ranges in hex format (e.g., 0x2580..0x259F) from which to include glyphs. ASCII
    /// (0x20-0x7F) is always included.
    #[arg(short, long = "range", value_parser = parse_unicode_range)]
    pub ranges: Vec<RangeInclusive<char>>,

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

    /// Replace space glyph with a checkered pattern to validate pixel-perfect rendering.
    /// Use "1" for 1px checkers or "2" for 2x2 pixel checkers.
    #[arg(long, value_name = "SIZE", value_parser = parse_debug_space_pattern)]
    pub debug_space_pattern: Option<DebugSpacePattern>,
}

fn parse_debug_space_pattern(s: &str) -> Result<DebugSpacePattern, String> {
    match s {
        "1" | "1px" => Ok(DebugSpacePattern::OnePixel),
        "2" | "2x2" => Ok(DebugSpacePattern::TwoByTwo),
        _ => Err(format!(
            "Invalid pattern '{s}'. Use '1' (or '1px') for 1px checkers, '2' (or '2x2') for 2x2 checkers"
        )),
    }
}

impl Cli {
    /// Selects a font based on the CLI arguments and available fonts
    pub fn select_font<'a>(
        &self,
        available_fonts: &'a [FontFamily],
    ) -> Result<&'a FontFamily, Report> {
        if available_fonts.is_empty() {
            return Err(eyre!("No complete monospace font families found!"));
        }

        let font = self
            .font
            .as_ref()
            .ok_or_else(|| eyre!("Font selection required"))?;

        // Try parsing as index first (1-based)
        if let Ok(idx) = font.parse::<usize>() {
            if idx > 0 && idx <= available_fonts.len() {
                return Ok(&available_fonts[idx - 1]);
            } else {
                return Err(eyre!(
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
            .ok_or_else(|| eyre!("Font '{font}' not found"))
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
    pub fn validate(&self) -> Result<(), Report> {
        if self.font_size <= 0.0 {
            return Err(eyre!("Font size must be positive"));
        }

        if self.line_height <= 0.0 {
            return Err(eyre!("Line height must be positive"));
        }

        // Validate position values are in [0.0, 1.0]
        if self.underline_position < 0.0 || self.underline_position > 1.0 {
            return Err(eyre!("Underline position must be between 0.0 and 1.0"));
        }

        if self.strikethrough_position < 0.0 || self.strikethrough_position > 1.0 {
            return Err(eyre!("Strikethrough position must be between 0.0 and 1.0"));
        }

        // Validate thickness values are reasonable percentages
        if self.underline_thickness <= 0.0 || self.underline_thickness > 100.0 {
            return Err(eyre!(
                "Underline thickness must be between 0 and 100 percent"
            ));
        }

        if self.strikethrough_thickness <= 0.0 || self.strikethrough_thickness > 100.0 {
            return Err(eyre!(
                "Strikethrough thickness must be between 0 and 100 percent"
            ));
        }

        Ok(())
    }

    pub fn read_symbols_file(&self) -> Result<String, Report> {
        match &self.symbols_file {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|e| eyre!("Failed to read symbols file '{}': {}", path.display(), e)),
            None => Ok(String::new()),
        }
    }

    /// Prints a summary of the configuration
    pub fn print_summary(&self, font_name: &str) {
        println!("\nGenerating font atlas:");
        println!("  Font: {font_name}");
        println!("  Emoji font: {}", self.emoji_font);
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

fn parse_unicode_range(s: &str) -> Result<RangeInclusive<char>, String> {
    if let Some((start_str, end_str)) = s.split_once("..") {
        let start_code = parse_hex(start_str.trim())
            .map_err(|e| format!("Invalid start value '{start_str}': {e}"))?;
        let end_code =
            parse_hex(end_str.trim()).map_err(|e| format!("Invalid end value '{end_str}': {e}"))?;

        let start_char = char::from_u32(start_code)
            .ok_or_else(|| format!("Invalid Unicode code point: 0x{start_code:x}"))?;
        let end_char = char::from_u32(end_code)
            .ok_or_else(|| format!("Invalid Unicode code point: 0x{end_code:x}"))?;

        if start_code > end_code {
            return Err(format!(
                "Start value (0x{start_code:x}) cannot be greater than end value (0x{end_code:x})"
            ));
        }

        Ok(start_char..=end_char)
    } else {
        Err(format!(
            "Invalid range format '{s}'. Expected format: 0x20..0x7f"
        ))
    }
}

fn parse_hex(s: &str) -> Result<u32, String> {
    s.strip_prefix("0x")
        .ok_or_else(|| format!("Expected hexadecimal format (0x...), got: {s}"))
        .map(|hex_str| u32::from_str_radix(hex_str, 16))?
        .map_err(|_| format!("Invalid hexadecimal number: {s}"))
}

fn validate_file_exists(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    match () {
        _ if !path.exists() => Err(format!("Input file does not exist: {s}")),
        _ if !path.is_file() => Err(format!("Path is not a file: {s}")),
        _ => Ok(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_validation() {
        let cli = Cli {
            font: Some("test".to_string()),
            emoji_font: "Noto Color Emoji".to_string(),
            symbols_file: Some(PathBuf::from("/dev/null")),
            ranges: vec![],
            font_size: 15.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 0.85,
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
            debug_space_pattern: None,
        };

        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_invalid_font_size() {
        let cli = Cli {
            font: Some("test".to_string()),
            emoji_font: "Noto Color Emoji".to_string(),
            symbols_file: None,
            ranges: vec![],
            font_size: -1.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 0.85,
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
            debug_space_pattern: None,
        };

        assert!(cli.validate().is_err());
    }

    #[test]
    fn test_invalid_position() {
        let cli = Cli {
            font: Some("test".to_string()),
            emoji_font: "Noto Color Emoji".to_string(),
            symbols_file: None,
            ranges: vec![],
            font_size: 15.0,
            line_height: 1.0,
            output: "test.atlas".to_string(),
            underline_position: 1.5, // Invalid: > 1.0
            underline_thickness: 5.0,
            strikethrough_position: 0.5,
            strikethrough_thickness: 5.0,
            list_fonts: false,
            check_missing: false,
            debug_space_pattern: None,
        };

        assert!(cli.validate().is_err());
    }
}
