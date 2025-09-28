mod atlas_generator;
mod bitmap_font;
mod cli;
mod coordinate;
mod font_discovery;
mod glyph_bounds;
mod glyph_rasterizer;
mod glyph_set;
mod grapheme;
mod logging;
mod raster_config;

use beamterm_data::*;
use clap::Parser;
use color_eyre::eyre::{Context, Result};

use crate::{
    atlas_generator::AtlasFontGenerator,
    cli::Cli,
    font_discovery::FontDiscovery,
    glyph_set::GLYPHS,
    logging::{init_logging, LoggingConfig},
};

fn main() -> Result<()> {
    // panic hook
    color_eyre::install()?;

    // Initialize structured logging
    let logging_config = LoggingConfig::from_env();
    let (_guard, _reload_handle) =
        init_logging(logging_config).wrap_err("Failed to initialize logging")?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "beamterm-atlas starting up"
    );

    // parse command line arguments
    let cli = Cli::parse();

    // handle --list-fonts flag
    if cli.list_fonts {
        Cli::display_font_list();
        return Ok(());
    }

    // validate CLI arguments
    cli.validate()?;

    // discover available fonts
    let discovery = FontDiscovery::new();
    let available_fonts = discovery.discover_complete_monospace_families();

    if available_fonts.is_empty() {
        eprintln!("No complete monospace font families found!");
        eprintln!(
            "A complete font family must have: Regular, Bold, Italic, and Bold+Italic variants"
        );
        return Ok(());
    }

    // select font
    let selected_font = cli.select_font(&available_fonts)?;

    // print configuration summary
    cli.print_summary(&selected_font.name);

    let underline = LineDecoration::new(cli.underline_position, cli.underline_thickness / 100.0);
    let strikethrough = LineDecoration::new(
        cli.strikethrough_position,
        cli.strikethrough_thickness / 100.0,
    );

    // Generate the font
    let mut generator = AtlasFontGenerator::new_with_family(
        selected_font.clone(),
        cli.font_size,
        cli.line_height,
        underline,
        strikethrough,
    )?;

    let bitmap_font = generator.generate(GLYPHS);

    bitmap_font.save(&cli.output)?;

    let atlas = &bitmap_font.atlas_data;
    println!("\nBitmap font generated!");
    println!("Font family: {}", selected_font.name);
    println!("Font size: {:.3}", atlas.font_size);
    println!(
        "Texture size: {}x{}x{}",
        atlas.texture_dimensions.0, atlas.texture_dimensions.1, atlas.texture_dimensions.2
    );
    println!(
        "Cell size: {}x{}",
        bitmap_font.atlas_data.cell_size.0, bitmap_font.atlas_data.cell_size.1
    );
    println!("Total glyph count: {}", bitmap_font.atlas_data.glyphs.len());
    println!(
        "Glyph count per variant: {}/{} (emoji: {})",
        bitmap_font
            .atlas_data
            .glyphs
            .iter()
            .filter(|g| !g.is_emoji)
            .count()
            / FontStyle::ALL.len(),
        Glyph::GLYPH_ID_MASK + 1, // zero-based id/index
        bitmap_font
            .atlas_data
            .glyphs
            .iter()
            .filter(|g| g.is_emoji)
            .count()
    );
    println!(
        "Longest grapheme in bytes: {}",
        bitmap_font
            .atlas_data
            .glyphs
            .iter()
            .map(|g| g.symbol.len())
            .max()
            .unwrap_or(0)
    );

    // Check for missing glyphs if requested
    if cli.check_missing {
        report_missing_glyphs(&mut generator);
    }

    Ok(())
}

fn report_missing_glyphs(generator: &mut AtlasFontGenerator) {
    println!("\n🔍 Checking for missing glyphs...");
    let missing_report = generator.check_missing_glyphs(GLYPHS);

    if missing_report.missing_glyphs.is_empty() {
        println!(
            "✅ All {} glyphs are supported by font '{}'",
            missing_report.total_checked, missing_report.font_family_name
        );
    } else {
        println!(
            "⚠️  Found {} missing glyphs out of {} checked in font '{}':",
            missing_report.missing_glyphs.len(),
            missing_report.total_checked,
            missing_report.font_family_name
        );

        // Group missing glyphs by style for better readability
        for style in [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic]
        {
            let mut glyphs: Vec<_> = missing_report
                .missing_glyphs
                .iter()
                .filter(|g| g.style == style)
                .collect();

            if !glyphs.is_empty() {
                // Sort glyphs by symbol for consistent output
                glyphs.sort_by_key(|g| &g.symbol);

                println!("  {:?}:", style);

                // Print 8 glyphs per line
                for chunk in glyphs.chunks(8) {
                    let line = chunk
                        .iter()
                        .map(|glyph| {
                            let first_char = glyph.symbol.chars().next().unwrap_or('\0');
                            let codepoint = first_char as u32;

                            let display_symbol =
                                if first_char.is_control() || first_char.is_whitespace() {
                                    format!("U+{:04X}", codepoint)
                                } else {
                                    format!("'{}'", glyph.symbol)
                                };
                            format!("{} (0x{:04X})", display_symbol, codepoint)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("    {}", line);
                }
            }
        }

        let success_rate = ((missing_report.total_checked - missing_report.missing_glyphs.len())
            as f64
            / missing_report.total_checked as f64)
            * 100.0;

        println!("📊 Font coverage: {:.1}%", success_rate);
    }
}
