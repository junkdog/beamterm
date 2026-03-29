use std::io::BufWriter;

use beamterm_data::FontAtlasData;
use color_eyre::eyre::Result;

pub(crate) fn dump_atlas_png(atlas: &FontAtlasData, path: &str) -> Result<()> {
    let (tw, th, layers) = atlas.texture_dimensions();
    let tw = tw as u32;
    let th = th as u32;
    let layers = layers as u32;
    let cs = atlas.cell_size();
    let cw = cs.width as u32;
    let ch = cs.height as u32;
    let padding = FontAtlasData::PADDING as u32;
    let glyphs_per_layer = 32u32;

    // Strip padding for display: show only the content area of each cell
    let content_w = cw - 2 * padding;
    let content_h = ch - 2 * padding;

    // Each layer is transposed from 1×32 (vertical) to 32×1 (horizontal) strip.
    // 4 horizontal strips per row, with gaps.
    let gap = 4u32;
    let strip_w = content_w * glyphs_per_layer;
    let strip_h = content_h;
    let cols = 4u32;
    let rows = layers.div_ceil(cols);

    // Ruler margins (2x scaled bitmap font: 6x10 per char + 2px spacing)
    let scale = 2u32;
    let char_w = 3 * scale + scale; // 8px per char (6px glyph + 2px gap)
    let char_h = 5 * scale; // 10px tall
    let left_margin = char_w * 7 + 4; // "0x0000" = 6 chars + padding
    let header_h = char_h * 2 + 10; // two lines of text + padding
    let ruler_h = char_h + 8; // offset labels + tick marks + padding
    let top_margin = header_h + ruler_h;

    // Style sections: (start_layer, label, section_tint, label_color)
    // Gruvbox dark palette tints for each section
    // Normal=0-31, Bold=32-63, Italic=64-95, BoldItalic=96-127, Emoji=128+
    let sections: Vec<(u32, &str, [u8; 3], [u8; 3])> = [
        (0, "normal", [0x1A, 0x30, 0x1A], [0xFA, 0xBD, 0x2F]), // gruvbox green tint
        (32, "bold", [0x38, 0x1A, 0x1A], [0xFA, 0xBD, 0x2F]),  // gruvbox red tint
        (64, "italic", [0x1A, 0x1A, 0x38], [0xFA, 0xBD, 0x2F]), // gruvbox blue tint
        (96, "bold italic", [0x30, 0x1A, 0x30], [0xFA, 0xBD, 0x2F]), // gruvbox purple tint
        (128, "emoji", [0x38, 0x28, 0x1A], [0xFA, 0xBD, 0x2F]), // gruvbox orange tint
    ]
    .into_iter()
    .filter(|(start, _, _, _)| *start < layers)
    .collect();

    let divider_h = char_h + 6; // section label height

    // Count dividers (skip divider for the very first section)
    let num_dividers = sections.len().saturating_sub(1) as u32;

    // Compute y-offset for a given display row, accounting for section dividers
    let row_y = |row: u32| -> u32 {
        let base_y = top_margin + row * (strip_h + gap);
        // Count how many section boundaries fall before this row
        let first_layer_of_row = row * cols;
        let dividers_above = sections
            .iter()
            .skip(1) // first section has no divider
            .filter(|(start, _, _, _)| *start <= first_layer_of_row)
            .count() as u32;
        base_y + dividers_above * divider_h
    };

    let content_area_w = strip_w * cols + (cols - 1) * gap;
    let content_area_h = strip_h * rows + (rows - 1) * gap + num_dividers * divider_h;
    let img_w = left_margin + content_area_w;
    let img_h = top_margin + content_area_h;

    // Fill with gruvbox bg0_h gap color
    let bg_color: [u8; 3] = [0x1D, 0x20, 0x21];
    let mut pixels = vec![0u8; (img_w * img_h * 4) as usize];
    fill_rect(&mut pixels, img_w, 0, 0, img_w, img_h, bg_color);

    // Darken ruler margins (gruvbox bg0)
    let ruler_bg: [u8; 3] = [0x28, 0x28, 0x28];
    fill_rect(&mut pixels, img_w, 0, 0, img_w, top_margin, ruler_bg);
    fill_rect(
        &mut pixels,
        img_w,
        0,
        top_margin,
        left_margin,
        img_h - top_margin,
        ruler_bg,
    );

    let ruler_color: [u8; 3] = [0xA8, 0x99, 0x84]; // gruvbox fg4
    let header_color: [u8; 3] = [0xEB, 0xDB, 0xB2]; // gruvbox fg1
    let tick_color: [u8; 3] = [0x66, 0x5C, 0x54]; // gruvbox bg3
    let section_line_color: [u8; 3] = [0x3C, 0x38, 0x36]; // gruvbox bg1

    // Draw header: font name, size, cell dimensions
    let header_text = format!(
        "{}  {:.1}pt  {}x{}px",
        atlas.font_name(),
        atlas.font_size(),
        content_w,
        content_h,
    );
    draw_text(
        &mut pixels,
        img_w,
        left_margin,
        3,
        &header_text,
        header_color,
        scale,
    );

    // Subtitle: "generated with" + command
    let subtitle_prefix = "generated with ";
    let subtitle_cmd = "beamterm-atlas --dump-png";
    let subtitle_color: [u8; 3] = [0x92, 0x83, 0x74]; // gruvbox fg4 (gray4)
    let subtitle_cmd_color: [u8; 3] = [0x68, 0x9D, 0x6A]; // gruvbox green
    let subtitle_y = 3 + char_h + 4;
    let prefix_w = subtitle_prefix.len() as u32 * (3 * scale + scale);
    draw_text(
        &mut pixels,
        img_w,
        left_margin,
        subtitle_y,
        subtitle_prefix,
        subtitle_color,
        scale,
    );
    draw_text(
        &mut pixels,
        img_w,
        left_margin + prefix_w,
        subtitle_y,
        subtitle_cmd,
        subtitle_cmd_color,
        scale,
    );

    // Draw top ruler: hex glyph offset labels every 8 glyphs, continuous across columns
    for col_idx in 0..cols {
        let strip_x0 = left_margin + col_idx * (strip_w + gap);
        let col_base = col_idx * glyphs_per_layer; // e.g., 0, 32, 64, 96
        for offset in (0..glyphs_per_layer).step_by(8) {
            let x = strip_x0 + offset * content_w;
            // Tick mark
            for ty in (top_margin - 4)..top_margin {
                set_pixel(&mut pixels, img_w, x, ty, tick_color);
            }
            // Hex offset label (continuous: 00, 08, ..., 20, 28, ..., 60, 68, ...)
            let glyph_offset = col_base + offset;
            let label = format!("{glyph_offset:02x}");
            draw_text(
                &mut pixels,
                img_w,
                x + 2,
                header_h + 2,
                &label,
                ruler_color,
                scale,
            );
        }
    }

    // Draw section dividers and tint section backgrounds
    for (i, &(start_layer, label, _tint, label_color)) in sections.iter().enumerate() {
        // Determine section layer range
        let end_layer = sections
            .get(i + 1)
            .map_or(layers, |(s, _, _, _)| *s);

        // Tint the cell background for this section's rows
        let section_tint = sections[i].2;
        for layer in start_layer..end_layer {
            let row = layer / cols;
            let col = layer % cols;
            let base_y = row_y(row);
            let base_x = left_margin + col * (strip_w + gap);
            fill_rect(
                &mut pixels,
                img_w,
                base_x,
                base_y,
                strip_w,
                strip_h,
                section_tint,
            );
        }

        if i == 0 {
            continue; // no divider before the first section
        }
        let section_row = start_layer / cols;
        let divider_y = row_y(section_row) - divider_h;

        // Fill divider background
        fill_rect(
            &mut pixels,
            img_w,
            0,
            divider_y,
            img_w,
            divider_h,
            section_line_color,
        );

        // Draw section label centered vertically in divider
        let label_y = divider_y + (divider_h.saturating_sub(char_h)) / 2;
        draw_text(
            &mut pixels,
            img_w,
            left_margin + 4,
            label_y,
            label,
            label_color,
            scale,
        );
    }

    // Draw left ruler: 0x-prefixed hex base glyph ID per display row
    let texture_data = atlas.texture_data();
    for row in 0..rows {
        let y = row_y(row);

        // Tick mark
        for tx in (left_margin - 4)..left_margin {
            set_pixel(&mut pixels, img_w, tx, y, tick_color);
        }

        // Base glyph ID = first layer of this row * 32 glyphs per layer
        let base_id = row * cols * 32;
        let label_y = y + strip_h.saturating_sub(char_h) / 2; // vertically center in strip
        let label = format!("0x{base_id:04x}");
        draw_text_right_aligned(
            &mut pixels,
            img_w,
            left_margin - 6,
            label_y,
            &label,
            ruler_color,
            scale,
        );
    }

    for layer in 0..layers {
        let col = layer % cols;
        let row = layer / cols;

        for glyph_idx in 0..glyphs_per_layer {
            // Source: glyph stacked vertically in the layer (skip padding)
            let src_glyph_y = glyph_idx * ch + padding;
            // Destination: glyph laid out horizontally in the strip (offset by margins)
            let dst_glyph_x = left_margin + col * (strip_w + gap) + glyph_idx * content_w;
            let dst_glyph_y = row_y(row);

            for y in 0..content_h {
                for x in 0..content_w {
                    let src_x = padding + x;
                    let src_y = src_glyph_y + y;
                    let src_idx = (layer * tw * th + src_y * tw + src_x) as usize * 4;
                    let dst_x = dst_glyph_x + x;
                    let dst_y = dst_glyph_y + y;
                    let dst_idx = (dst_y * img_w + dst_x) as usize * 4;

                    if src_idx + 4 <= texture_data.len() && dst_idx + 4 <= pixels.len() {
                        // Alpha-blend glyph onto black background
                        let a = texture_data[src_idx + 3] as u32;
                        let r = texture_data[src_idx] as u32 * a / 255;
                        let g = texture_data[src_idx + 1] as u32 * a / 255;
                        let b = texture_data[src_idx + 2] as u32 * a / 255;
                        pixels[dst_idx] = r as u8;
                        pixels[dst_idx + 1] = g as u8;
                        pixels[dst_idx + 2] = b as u8;
                        pixels[dst_idx + 3] = 0xFF;
                    }
                }
            }
        }
    }

    let file = std::fs::File::create(path)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create PNG file '{path}': {e}"))?;
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, img_w, img_h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder
        .write_header()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to write PNG header: {e}"))?;

    writer
        .write_image_data(&pixels)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to write PNG data: {e}"))?;

    println!("Atlas dumped to {path} ({img_w}x{img_h}, {layers} layers in {cols}x{rows} grid)");
    Ok(())
}

/// Set a single pixel in the RGBA buffer.
fn set_pixel(pixels: &mut [u8], img_w: u32, x: u32, y: u32, color: [u8; 3]) {
    let idx = (y * img_w + x) as usize * 4;
    if idx + 4 <= pixels.len() {
        pixels[idx] = color[0];
        pixels[idx + 1] = color[1];
        pixels[idx + 2] = color[2];
        pixels[idx + 3] = 0xFF;
    }
}

/// Fill a rectangle in the RGBA buffer.
fn fill_rect(pixels: &mut [u8], img_w: u32, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
    let rgba = [color[0], color[1], color[2], 0xFF];
    for dy in 0..h {
        for dx in 0..w {
            let idx = ((y + dy) * img_w + (x + dx)) as usize * 4;
            if idx + 4 <= pixels.len() {
                pixels[idx..idx + 4].copy_from_slice(&rgba);
            }
        }
    }
}

/// Draw text at (x, y) using a 3x5 bitmap font scaled by `scale`. Left-aligned.
fn draw_text(
    pixels: &mut [u8],
    img_w: u32,
    x: u32,
    y: u32,
    text: &str,
    color: [u8; 3],
    scale: u32,
) {
    let glyph_w = 3 * scale;
    let advance = glyph_w + scale; // glyph width + inter-char gap
    let mut cx = x;
    for ch in text.chars() {
        if ch == ' ' {
            cx += advance;
            continue;
        }
        if let Some(glyph) = bitmap_char(ch) {
            for (row_idx, row) in glyph.iter().enumerate() {
                for col_idx in 0..3u32 {
                    if row & (1 << (2 - col_idx)) != 0 {
                        // Draw a scale x scale block for each pixel
                        for sy in 0..scale {
                            for sx in 0..scale {
                                set_pixel(
                                    pixels,
                                    img_w,
                                    cx + col_idx * scale + sx,
                                    y + row_idx as u32 * scale + sy,
                                    color,
                                );
                            }
                        }
                    }
                }
            }
        }
        cx += advance;
    }
}

/// Draw text right-aligned so that the rightmost character ends at `right_x`.
fn draw_text_right_aligned(
    pixels: &mut [u8],
    img_w: u32,
    right_x: u32,
    y: u32,
    text: &str,
    color: [u8; 3],
    scale: u32,
) {
    let advance = 3 * scale + scale;
    let char_count = text.chars().count() as u32;
    let text_w = char_count * advance - scale; // total width minus trailing gap
    let x = right_x.saturating_sub(text_w);
    draw_text(pixels, img_w, x, y, text, color, scale);
}

/// 3x5 bitmap font for digits, letters, and punctuation.
/// Each row is a 3-bit mask (MSB = left pixel).
fn bitmap_char(ch: char) -> Option<[u8; 5]> {
    match ch.to_ascii_lowercase() {
        '0' => Some([0b111, 0b101, 0b101, 0b101, 0b111]),
        '1' => Some([0b010, 0b110, 0b010, 0b010, 0b111]),
        '2' => Some([0b111, 0b001, 0b111, 0b100, 0b111]),
        '3' => Some([0b111, 0b001, 0b111, 0b001, 0b111]),
        '4' => Some([0b101, 0b101, 0b111, 0b001, 0b001]),
        '5' => Some([0b111, 0b100, 0b111, 0b001, 0b111]),
        '6' => Some([0b111, 0b100, 0b111, 0b101, 0b111]),
        '7' => Some([0b111, 0b001, 0b001, 0b001, 0b001]),
        '8' => Some([0b111, 0b101, 0b111, 0b101, 0b111]),
        '9' => Some([0b111, 0b101, 0b111, 0b001, 0b111]),
        'a' => Some([0b010, 0b101, 0b111, 0b101, 0b101]),
        'b' => Some([0b110, 0b101, 0b110, 0b101, 0b110]),
        'c' => Some([0b011, 0b100, 0b100, 0b100, 0b011]),
        'd' => Some([0b110, 0b101, 0b101, 0b101, 0b110]),
        'e' => Some([0b111, 0b100, 0b110, 0b100, 0b111]),
        'f' => Some([0b111, 0b100, 0b110, 0b100, 0b100]),
        'g' => Some([0b011, 0b100, 0b101, 0b101, 0b011]),
        'h' => Some([0b101, 0b101, 0b111, 0b101, 0b101]),
        'i' => Some([0b111, 0b010, 0b010, 0b010, 0b111]),
        'j' => Some([0b001, 0b001, 0b001, 0b101, 0b010]),
        'k' => Some([0b101, 0b110, 0b100, 0b110, 0b101]),
        'l' => Some([0b100, 0b100, 0b100, 0b100, 0b111]),
        'm' => Some([0b101, 0b111, 0b111, 0b101, 0b101]),
        'n' => Some([0b101, 0b111, 0b111, 0b101, 0b101]),
        'o' => Some([0b010, 0b101, 0b101, 0b101, 0b010]),
        'p' => Some([0b110, 0b101, 0b110, 0b100, 0b100]),
        'q' => Some([0b010, 0b101, 0b101, 0b110, 0b011]),
        'r' => Some([0b110, 0b101, 0b110, 0b101, 0b101]),
        's' => Some([0b011, 0b100, 0b010, 0b001, 0b110]),
        't' => Some([0b111, 0b010, 0b010, 0b010, 0b010]),
        'u' => Some([0b101, 0b101, 0b101, 0b101, 0b011]),
        'v' => Some([0b101, 0b101, 0b101, 0b101, 0b010]),
        'w' => Some([0b101, 0b101, 0b111, 0b111, 0b101]),
        'x' => Some([0b101, 0b101, 0b010, 0b101, 0b101]),
        'y' => Some([0b101, 0b101, 0b010, 0b010, 0b010]),
        'z' => Some([0b111, 0b001, 0b010, 0b100, 0b111]),
        '.' => Some([0b000, 0b000, 0b000, 0b000, 0b010]),
        '-' => Some([0b000, 0b000, 0b111, 0b000, 0b000]),
        _ => None,
    }
}
