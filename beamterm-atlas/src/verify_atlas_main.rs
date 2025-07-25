// Create another binary in bitmap-font/src/bin/view_atlas_grid.rs

use std::fmt::Write;

use beamterm_data::{FontAtlasData, Glyph};
use colored::Colorize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let atlas = FontAtlasData::default();

    println!("=== Font Atlas Grid Viewer ===");
    println!(
        "Texture: {}x{}x{} (4x4 cells per slice)",
        atlas.texture_dimensions.0, atlas.texture_dimensions.1, atlas.texture_dimensions.2
    );

    // Calculate total number of slices
    let max_slice = atlas
        .glyphs
        .iter()
        .max_by_key(|g| g.id)
        .unwrap()
        .id as usize
        / 16;

    // Display slices two per row
    for slice_pair in (0..=max_slice).step_by(2) {
        let slice_left = slice_pair;
        let slice_right = if slice_pair < max_slice { Some(slice_pair + 1) } else { None };

        println!(
            "\n=== Slice {} {} ===",
            slice_left,
            slice_right.map_or(String::new(), |s| format!("& {s}"))
        );

        render_slice_pair(&atlas, slice_left, slice_right)?;
    }

    Ok(())
}

fn find_glyph_symbol(atlas: &FontAtlasData, slice: u16, pos: u16) -> Option<&Glyph> {
    let glyph_id = slice << 4 | pos;
    atlas.glyphs.iter().find(|g| g.id == glyph_id)
}

fn render_slice_pair(
    atlas: &FontAtlasData,
    left_slice: usize,
    right_slice: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_width = atlas.cell_size.0 as usize * 4;
    let display_height = atlas.cell_size.1 as usize * 4;

    let mut output = String::new();

    // Draw top border with column markers for both slices
    write!(&mut output, "   ").ok();
    // Left slice markers
    for x in 0..display_width {
        if x % atlas.cell_size.0 as usize == 0 {
            write!(
                &mut output,
                "{}",
                (x / atlas.cell_size.0 as usize)
                    .to_string()
                    .blue()
            )
            .ok();
        } else {
            write!(&mut output, " ").ok();
        }
    }

    if right_slice.is_some() {
        write!(&mut output, "  ").ok(); // Gap between slices
                                        // Right slice markers
        for x in 0..display_width {
            if x % atlas.cell_size.0 as usize == 0 {
                write!(
                    &mut output,
                    "{}",
                    (x / atlas.cell_size.0 as usize)
                        .to_string()
                        .blue()
                )
                .ok();
            } else {
                write!(&mut output, " ").ok();
            }
        }
    }
    writeln!(&mut output).ok();

    // Process pixels in pairs for half-block rendering
    for y in (0..display_height).step_by(2) {
        // Draw row marker
        if y % atlas.cell_size.1 as usize == 0 {
            write!(
                &mut output,
                "{:2} ",
                (y / atlas.cell_size.1 as usize)
                    .to_string()
                    .blue()
            )
            .ok();
        } else {
            write!(&mut output, "   ").ok();
        }

        // Render left slice
        render_slice_row(atlas, left_slice, y, &mut output);

        // Render right slice if present
        if let Some(right) = right_slice {
            write!(&mut output, "  ").ok(); // Gap between slices
            render_slice_row(atlas, right, y, &mut output);
        }

        writeln!(&mut output).ok();
    }

    print!("{output}");
    Ok(())
}

fn render_slice_row(atlas: &FontAtlasData, slice: usize, y: usize, output: &mut String) {
    let slice_height = atlas.texture_dimensions.1 as usize;
    let slice_width = atlas.texture_dimensions.0 as usize;
    let slice_offset = slice * slice_width * slice_height;
    let display_width = atlas.cell_size.0 as usize * 4;
    let display_height = atlas.cell_size.1 as usize * 4;

    for x in 0..display_width {
        let idx_top = slice_offset + y * slice_width + x;
        let idx_bottom = if y + 1 < display_height {
            slice_offset + (y + 1) * slice_width + x
        } else {
            idx_top
        };

        let pixel_top = if 4 * idx_top < atlas.texture_data.len() {
            (atlas.texture_data[idx_top * 4] as u32) << 24
                | (atlas.texture_data[idx_top * 4 + 1] as u32) << 16
                | (atlas.texture_data[idx_top * 4 + 2] as u32) << 8
                | (atlas.texture_data[idx_top * 4 + 3] as u32)
        } else {
            0x000000
        };

        let pixel_bottom: u32 =
            if 4 * idx_bottom < atlas.texture_data.len() && y + 1 < display_height {
                (atlas.texture_data[idx_bottom * 4] as u32) << 24
                    | (atlas.texture_data[idx_bottom * 4 + 1] as u32) << 16
                    | (atlas.texture_data[idx_bottom * 4 + 2] as u32) << 8
                    | (atlas.texture_data[idx_bottom * 4 + 3] as u32)
            } else {
                0x000000
            };

        let a_top = pixel_top & 0xFF;
        let a_bottom = pixel_bottom & 0xFF;

        // Determine which half-block character to use
        match (a_top > 0, a_bottom > 0) {
            (true, true) => {
                let (r1, g1, b1) = rgb_components(pixel_top);
                let (r2, g2, b2) = rgb_components(pixel_bottom);

                let px = "▀".truecolor(r1, g1, b1).on_truecolor(r2, g2, b2);

                write!(output, "{px}").ok();
            },
            (true, false) => {
                // Top half-block only
                let (r, g, b) = rgb_components(pixel_top);
                write!(output, "{}", "▀".truecolor(r, g, b)).ok();
            },
            (false, true) => {
                // Bottom half-block only
                let (r, g, b) = rgb_components(pixel_bottom);
                write!(output, "{}", "▄".truecolor(r, g, b)).ok();
            },
            (false, false) => {
                // Empty pixel
                let on_h_grid = x % atlas.cell_size.0 as usize == 0;
                let on_v_grid_top = y % atlas.cell_size.1 as usize == 0;
                let on_v_grid_bottom = (y + 1) % atlas.cell_size.1 as usize == 0;

                if on_h_grid && on_v_grid_top {
                    // Top pixel is at cell start
                    let y_pos = y / atlas.cell_size.1 as usize;
                    let x_pos = x / atlas.cell_size.0 as usize;
                    // Position within the 4x4 grid of this slice
                    let pos = y_pos * 4 + x_pos;

                    if let Some(glyph) = find_glyph_symbol(atlas, slice as u16, pos as u16) {
                        let ch = glyph.symbol.chars().next().unwrap_or(' ');
                        write!(output, "{}", ch.to_string().truecolor(0xfe, 0x80, 0x19)).ok();
                    } else {
                        write!(output, "{}", "+".bright_black()).ok();
                    }
                } else if on_h_grid && on_v_grid_bottom && y + 1 < display_height {
                    // Bottom pixel is at cell start
                    let y_pos = (y + 1) / atlas.cell_size.1 as usize;
                    let x_pos = x / atlas.cell_size.0 as usize;
                    let pos = y_pos * 4 + x_pos;

                    if let Some(glyph) = find_glyph_symbol(atlas, slice as u16, pos as u16) {
                        let ch = glyph.symbol.chars().next().unwrap_or(' ');
                        write!(output, "{}", ch.to_string().truecolor(0xfe, 0x80, 0x19)).ok();
                    } else {
                        write!(output, "+").ok();
                    }
                } else if on_h_grid {
                    write!(output, "|").ok();
                } else if on_v_grid_top || on_v_grid_bottom {
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
