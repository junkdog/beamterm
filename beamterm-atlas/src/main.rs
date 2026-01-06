mod atlas_generator;
mod bitmap_font;
mod cli;
mod coordinate;
mod font_discovery;
mod glyph_bounds;
mod glyph_rasterizer;
mod grapheme;
mod logging;
mod raster_config;

use beamterm_data::*;
use clap::Parser;
use color_eyre::eyre::{Context, Result};

use crate::{
    atlas_generator::{AtlasFontGenerator, FallbackGlyphStats},
    cli::Cli,
    font_discovery::FontDiscovery,
    logging::{LoggingConfig, init_logging},
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

    // Validate and resolve emoji font name
    let emoji_font_name = resolve_emoji_font_name(&cli.emoji_font, discovery)?;
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
        emoji_font_name,
        cli.font_size,
        cli.line_height,
        underline,
        strikethrough,
    )?;

    let ranges = if cli.ranges.is_empty() {
        default_unicode_ranges()
    } else {
        cli.ranges.clone()
    };

    let additional_symbols = cli.read_symbols_file()?;
    let (bitmap_font, fallback_stats) = generator.generate(&ranges, &additional_symbols);
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
    let rasterized_glyphs = bitmap_font.atlas_data.glyphs;
    println!("Total glyph count: {}", rasterized_glyphs.len());
    println!(
        "Glyph count per variant: {} (emoji: {})",
        rasterized_glyphs
            .iter()
            .filter(|g| !g.is_emoji)
            .count()
            / FontStyle::ALL.len(),
        rasterized_glyphs
            .iter()
            .filter(|g| g.is_emoji)
            .count()
            / 2 // each emoji occupies two glyphs
    );
    println!(
        "Longest grapheme in bytes: {}",
        rasterized_glyphs
            .iter()
            .map(|g| g.symbol.len())
            .max()
            .unwrap_or(0)
    );

    // Report fallback glyphs if any
    report_fallback_glyphs(&fallback_stats);

    // Check for missing glyphs if requested
    if cli.check_missing {
        report_missing_glyphs(&mut generator, &ranges, &additional_symbols);
    }

    Ok(())
}

fn report_fallback_glyphs(stats: &FallbackGlyphStats) {
    if stats.fallback_glyphs.is_empty() {
        return;
    }

    println!(
        "\n⚠️  {} glyphs used fallback fonts (out of {} total):",
        stats.fallback_glyphs.len(),
        stats.total_glyphs
    );

    // Group by fallback font name
    let mut by_font: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
    for glyph in &stats.fallback_glyphs {
        by_font
            .entry(&glyph.fallback_font_name)
            .or_default()
            .push(glyph);
    }

    for (font_name, glyphs) in by_font {
        println!("  From '{}':", font_name);

        // Group by style within each font
        for style in [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic] {
            let style_glyphs: Vec<_> = glyphs
                .iter()
                .filter(|g| g.style == style)
                .collect();

            if !style_glyphs.is_empty() {
                println!("    {:?} ({}):", style, style_glyphs.len());

                // Print up to 74 glyphs per line
                for chunk in style_glyphs.chunks(74) {
                    let symbols: String = chunk
                        .iter()
                        .map(|g| {
                            let ch = g.symbol.chars().next().unwrap_or('\0');
                            if ch.is_control() || ch.is_whitespace() {
                                '·'
                            } else {
                                ch
                            }
                        })
                        .collect();
                    println!("      {}", symbols);
                }
            }
        }
    }

    // Report font dimensions in a table
    if let Some(primary) = stats.primary_font_dimensions {
        // Find max font name length for alignment
        let max_name_len = stats
            .fallback_font_dimensions
            .iter()
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0)
            .max("Primary".len());

        // Count glyphs per font+style
        let count_glyphs = |font: &str, style: FontStyle| -> usize {
            stats
                .fallback_glyphs
                .iter()
                .filter(|g| g.fallback_font_name == font && g.style == style)
                .count()
        };

        println!("\n  Font dimensions and glyph metrics (█):");
        println!(
            "    {:<width$}  {:>5}  {:>4}  {:>4}  {:>5}  {:>5}  {:>5}  {:>5}",
            "Font", "Size", "Δw", "Δh", "Norm", "Bold", "Ital", "B+I",
            width = max_name_len
        );
        println!(
            "    {:-<width$}  {:->5}  {:->4}  {:->4}  {:->5}  {:->5}  {:->5}  {:->5}",
            "", "", "", "", "", "", "", "",
            width = max_name_len
        );
        println!(
            "    {:<width$}  {:>2}x{:<2}  {:>4}  {:>4}  {:>5}  {:>5}  {:>5}  {:>5}",
            "Primary", primary.width, primary.height, "-", "-", "-", "-", "-", "-",
            width = max_name_len
        );

        let mut total_normal = 0usize;
        let mut total_bold = 0usize;
        let mut total_italic = 0usize;
        let mut total_bold_italic = 0usize;

        for (font_name, dims) in &stats.fallback_font_dimensions {
            let width_diff = dims.width - primary.width;
            let height_diff = dims.height - primary.height;

            let fmt_diff = |diff: i32| -> String {
                match diff.cmp(&0) {
                    std::cmp::Ordering::Greater => format!("+{}", diff),
                    std::cmp::Ordering::Less => format!("{}", diff),
                    std::cmp::Ordering::Equal => "0".to_string(),
                }
            };

            let normal = count_glyphs(font_name, FontStyle::Normal);
            let bold = count_glyphs(font_name, FontStyle::Bold);
            let italic = count_glyphs(font_name, FontStyle::Italic);
            let bold_italic = count_glyphs(font_name, FontStyle::BoldItalic);

            total_normal += normal;
            total_bold += bold;
            total_italic += italic;
            total_bold_italic += bold_italic;

            println!(
                "    {:<width$}  {:>2}x{:<2}  {:>4}  {:>4}  {:>5}  {:>5}  {:>5}  {:>5}",
                font_name,
                dims.width,
                dims.height,
                fmt_diff(width_diff),
                fmt_diff(height_diff),
                normal,
                bold,
                italic,
                bold_italic,
                width = max_name_len
            );
        }

        // Print totals row
        println!(
            "    {:-<width$}  {:->5}  {:->4}  {:->4}  {:->5}  {:->5}  {:->5}  {:->5}",
            "", "", "", "", "", "", "", "",
            width = max_name_len
        );
        println!(
            "    {:<width$}  {:>5}  {:>4}  {:>4}  {:>5}  {:>5}  {:>5}  {:>5}",
            "Total", "", "", "",
            total_normal,
            total_bold,
            total_italic,
            total_bold_italic,
            width = max_name_len
        );
    }
}

fn resolve_emoji_font_name(emoji_font: &str, discovery: FontDiscovery) -> Result<String> {
    let emoji_font_name = match discovery.find_font(emoji_font) {
        Some(exact_name) => {
            if exact_name != emoji_font {
                println!("✓ Found emoji font: {exact_name} (matched: {emoji_font})",);
            } else {
                println!("✓ Found emoji font: {}", exact_name);
            }
            exact_name
        },
        None => {
            eprintln!("❌ Emoji font '{emoji_font}' not found in system fonts");

            // Suggest emoji fonts
            let all_fonts = discovery.list_all_fonts();
            let emoji_fonts: Vec<_> = all_fonts
                .iter()
                .filter(|name| {
                    let lower = name.to_lowercase();
                    lower.contains("emoji") || lower.contains("noto color")
                })
                .collect();

            if !emoji_fonts.is_empty() {
                eprintln!("\nAvailable emoji fonts:");
                for font in emoji_fonts {
                    eprintln!("  - {}", font);
                }
            }

            return Err(color_eyre::eyre::eyre!(
                "Emoji font '{emoji_font}' not found"
            ));
        },
    };
    Ok(emoji_font_name)
}

fn report_missing_glyphs(
    generator: &mut AtlasFontGenerator,
    ranges: &[std::ops::RangeInclusive<char>],
    additional_symbols: &str,
) {
    println!("\n🔍 Checking for missing glyphs...");
    let missing_report = generator.check_missing_glyphs(ranges, additional_symbols);

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

fn default_unicode_ranges() -> Vec<std::ops::RangeInclusive<char>> {
    vec![
        '\u{00A0}'..='\u{00FF}', // Latin-1 Supplement
        '\u{0100}'..='\u{017F}', // Latin Extended-A
        '\u{2300}'..='\u{232F}', // Miscellaneous Technical
        '\u{2350}'..='\u{23FF}', // Miscellaneous Technical
        '\u{2500}'..='\u{257F}', // Box Drawing
        '\u{2580}'..='\u{259F}', // Block Elements
        '\u{25A0}'..='\u{25CF}', // Geometric Shapes (excerpt)
        '\u{25E2}'..='\u{25FF}', // Geometric Shapes (excerpt)
        '\u{2800}'..='\u{28FF}', // Braille Patterns
    ]
}
