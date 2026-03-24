// Create another binary in bitmap-font/src/bin/view_atlas_grid.rs

use std::{fmt::Write, fs, path::PathBuf};

use beamterm_data::{FontAtlasData, Glyph};
use clap::Parser;
use colored::Colorize;

#[derive(Parser)]
#[command(name = "verify-atlas")]
#[command(about = "Visualize font atlas texture slices in the terminal")]
struct Cli {
    /// Path to the .atlas file to verify
    #[arg(value_name = "ATLAS_FILE")]
    atlas_path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let data = fs::read(&cli.atlas_path)?;
    let atlas = FontAtlasData::from_binary(&data)?;

    let (tw, th, tl) = atlas.texture_dimensions();
    let cs = atlas.cell_size();

    println!("=== Font Atlas Grid Viewer ===");
    println!("Texture: {tw}x{th}x{tl} (1x32 vertical cells per layer)");
    println!("Cell size: {}x{}", cs.width, cs.height);

    // Calculate total number of slices
    let max_slice = atlas
        .glyphs()
        .iter()
        .max_by_key(|g| g.id())
        .unwrap()
        .id() as usize
        / 32;

    // Display each layer (32 cells vertical, displayed in rows of 8 for readability)
    for layer in 0..=max_slice {
        println!("\n=== Layer {layer} ===");
        render_layer(&atlas, layer)?;
    }

    Ok(())
}

fn find_glyph_symbol(atlas: &FontAtlasData, layer: u16, pos: u16) -> Option<&Glyph> {
    let glyph_id = (layer << 5) | pos; // 32 glyphs per layer (shift by 5 = multiply by 32)
    atlas.glyphs().iter().find(|g| g.id() == glyph_id)
}

fn render_layer(atlas: &FontAtlasData, layer: usize) -> Result<(), Box<dyn std::error::Error>> {
    let cells_per_row = 16; // Display 16 cells per row
    let rows = 32 / cells_per_row; // 2 rows of 16 cells
    let cs = atlas.cell_size();
    let cell_width = cs.width as usize;
    let display_width = cell_width * cells_per_row;
    let cell_height = cs.height as usize;

    // Display each row of 16 cells
    for row in 0..rows {
        let start_cell = row * cells_per_row;

        println!("  Cells {}-{}", start_cell, start_cell + cells_per_row - 1);

        let mut output = String::new();

        // Column markers
        write!(&mut output, "   ").ok();
        for x in 0..display_width {
            if x % cell_width == 0 {
                let col = x / cell_width;
                write!(&mut output, "{}", format!("{col:X}").blue()).ok(); // Hex for 0-F
            } else {
                write!(&mut output, " ").ok();
            }
        }
        writeln!(&mut output).ok();

        // Process pixels in pairs for half-block rendering
        for y in (0..cell_height).step_by(2) {
            // Row marker
            write!(&mut output, "   ").ok();

            // Render 8 cells from this display row
            for cell_offset in 0..cells_per_row {
                let cell_pos = start_cell + cell_offset;
                render_cell(atlas, layer, cell_pos, y, &mut output);
            }

            writeln!(&mut output).ok();
        }

        print!("{output}");
    }

    Ok(())
}

fn render_cell(
    atlas: &FontAtlasData,
    layer: usize,
    cell_pos: usize,
    y: usize,
    output: &mut String,
) {
    let (layer_width, layer_height, _) = atlas.texture_dimensions();
    let layer_height = layer_height as usize;
    let layer_width = layer_width as usize;
    let layer_offset = layer * layer_width * layer_height;
    let cs = atlas.cell_size();
    let cell_width = cs.width as usize;
    let cell_height = cs.height as usize;
    let texture_data = atlas.texture_data();

    // Vertical layout: calculate y offset for this cell in the texture
    let cell_y_offset = cell_pos * cell_height;

    for x in 0..cell_width {
        let texture_y_top = cell_y_offset + y;
        let texture_y_bottom = cell_y_offset + y + 1;
        let idx_top = layer_offset + texture_y_top * layer_width + x;
        let idx_bottom = layer_offset + texture_y_bottom * layer_width + x;

        let pixel_top = if 4 * idx_top < texture_data.len() {
            (texture_data[idx_top * 4] as u32) << 24
                | (texture_data[idx_top * 4 + 1] as u32) << 16
                | (texture_data[idx_top * 4 + 2] as u32) << 8
                | (texture_data[idx_top * 4 + 3] as u32)
        } else {
            0x000000
        };

        let pixel_bottom = if 4 * idx_bottom < texture_data.len() {
            (texture_data[idx_bottom * 4] as u32) << 24
                | (texture_data[idx_bottom * 4 + 1] as u32) << 16
                | (texture_data[idx_bottom * 4 + 2] as u32) << 8
                | (texture_data[idx_bottom * 4 + 3] as u32)
        } else {
            0x000000
        };

        let a_top = pixel_top & 0xFF;
        let a_bottom = pixel_bottom & 0xFF;

        match (a_top > 0, a_bottom > 0) {
            (true, true) => {
                let (r1, g1, b1) = rgb_components(pixel_top);
                let (r2, g2, b2) = rgb_components(pixel_bottom);
                let px = "▀".truecolor(r1, g1, b1).on_truecolor(r2, g2, b2);
                write!(output, "{px}").ok();
            },
            (true, false) => {
                let (r, g, b) = rgb_components(pixel_top);
                write!(output, "{}", "▀".truecolor(r, g, b)).ok();
            },
            (false, true) => {
                let (r, g, b) = rgb_components(pixel_bottom);
                write!(output, "{}", "▄".truecolor(r, g, b)).ok();
            },
            (false, false) => {
                // Show glyph symbol at cell boundary
                if x == 0 && y == 0 {
                    if let Some(glyph) = find_glyph_symbol(atlas, layer as u16, cell_pos as u16) {
                        let ch = glyph.symbol().chars().next().unwrap_or(' ');
                        write!(output, "{}", ch.to_string().truecolor(0xfe, 0x80, 0x19)).ok();
                    } else {
                        write!(output, "{}", "+".bright_black()).ok();
                    }
                } else if x == 1 && y == 0 {
                    // Check if glyph symbol was double-width; if so, skip this column
                    let is_double_width = find_glyph_symbol(atlas, layer as u16, cell_pos as u16)
                        .and_then(|g| g.symbol().chars().next())
                        .and_then(unicode_width::UnicodeWidthChar::width)
                        .is_some_and(|w| w > 1);
                    if !is_double_width {
                        write!(output, "-").ok();
                    }
                } else if x == 0 {
                    write!(output, "|").ok();
                } else if y == 0 {
                    write!(output, "-").ok();
                } else {
                    write!(output, " ").ok();
                }
            },
        }
    }
}

fn rgb_components(color: u32) -> (u8, u8, u8) {
    let a = color & 0xFF;

    let r = ((((color >> 24) & 0xFF) * a) >> 8) as u8;
    let g = ((((color >> 16) & 0xFF) * a) >> 8) as u8;
    let b = ((((color >> 8) & 0xFF) * a) >> 8) as u8;
    (r, g, b)
}
