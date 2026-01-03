use beamterm_data::{FontAtlasData, FontStyle};
use beamterm_renderer::{CanvasRasterizer, RasterizedGlyph};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

const FONT_FAMILY: &str = "'Hack', 'Noto Sans Mono'";
const FONT_SIZE: f32 = 14.940;
// const FONT_SIZE: f32 = 15.0;
const PADDING: i32 = FontAtlasData::PADDING;
const ZOOM: u32 = 4; // 4x pixel zoom for easier inspection

fn main() {
    console_error_panic_hook::set_once();
    if let Err(e) = run() {
        web_sys::console::error_1(&e);
    }
}

fn run() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let canvas = document
        .get_element_by_id("output")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    // Disable image smoothing for crisp pixel scaling
    ctx.set_image_smoothing_enabled(false);

    let rasterizer = CanvasRasterizer::new(FONT_FAMILY, FONT_SIZE)?;

    // Measure font metrics directly from canvas
    ctx.set_font(&format!("{}px {}", FONT_SIZE, FONT_FAMILY));
    ctx.set_text_baseline("top");
    let metrics = ctx.measure_text("█")?;

    web_sys::console::log_1(&"=== Font Metrics (from canvas) ===".into());
    web_sys::console::log_1(&format!("Font: {} @ {}px", FONT_FAMILY, FONT_SIZE).into());
    web_sys::console::log_1(&format!("ZOOM: {}x", ZOOM).into());
    web_sys::console::log_1(&format!("width: {:.2}", metrics.width()).into());
    web_sys::console::log_1(
        &format!(
            "actual_bounding_box_ascent: {:.2}",
            metrics.actual_bounding_box_ascent()
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!(
            "actual_bounding_box_descent: {:.2}",
            metrics.actual_bounding_box_descent()
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!(
            "font_bounding_box_ascent: {:.2}",
            metrics.font_bounding_box_ascent()
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!(
            "font_bounding_box_descent: {:.2}",
            metrics.font_bounding_box_descent()
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!(
            "actual height (ascent+descent): {:.2}",
            metrics.actual_bounding_box_ascent() + metrics.actual_bounding_box_descent()
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!(
            "font height (ascent+descent): {:.2}",
            metrics.font_bounding_box_ascent() + metrics.font_bounding_box_descent()
        )
        .into(),
    );
    web_sys::console::log_1(&"".into());

    // First, measure cell size by rasterizing the reference glyph
    let reference = rasterizer.rasterize(&[("█", FontStyle::Normal)])?;

    let ref_glyph = &reference[0];
    let padded_cell_w = ref_glyph.width as i32;
    let padded_cell_h = ref_glyph.height as i32;
    let unpadded_cell_w = padded_cell_w - 2 * PADDING;
    let unpadded_cell_h = padded_cell_h - 2 * PADDING;

    web_sys::console::log_1(&"=== Cell Size Debug ===".into());
    web_sys::console::log_1(&format!("PADDING constant: {}px", PADDING).into());
    web_sys::console::log_1(
        &format!(
            "Reference glyph (█) dimensions: {}x{}",
            ref_glyph.width, ref_glyph.height
        )
        .into(),
    );
    web_sys::console::log_1(
        &format!("Padded cell size: {}x{}", padded_cell_w, padded_cell_h).into(),
    );
    web_sys::console::log_1(
        &format!(
            "Unpadded cell size: {}x{}",
            unpadded_cell_w, unpadded_cell_h
        )
        .into(),
    );
    web_sys::console::log_1(&"".into());

    let test_glyphs: &[(&str, FontStyle)] = &[
        ("█", FontStyle::Normal),
        ("A", FontStyle::Normal),
        ("B", FontStyle::Bold),
        ("C", FontStyle::Italic),
        ("g", FontStyle::Normal),
        ("y", FontStyle::Normal),
        ("→", FontStyle::Normal),
        ("░", FontStyle::Normal),
        ("─", FontStyle::Normal), // box drawing horizontal
        ("│", FontStyle::Normal), // box drawing vertical
        ("┌", FontStyle::Normal), // box drawing corner
        ("╔", FontStyle::Normal), // box drawing double corner
        // Emoji (double-width)
        ("🚀", FontStyle::Normal), // rocket
        ("😀", FontStyle::Normal), // grinning face
        ("🎉", FontStyle::Normal), // party popper
        ("❤", FontStyle::Normal),  // red heart (may be single-width on some systems)
        ("👨‍👩‍👧", FontStyle::Normal), // family ZWJ sequence
        // CJK (double-width)
        ("中", FontStyle::Normal),
        ("日", FontStyle::Normal),
    ];

    let glyphs = rasterizer.rasterize(test_glyphs)?;

    // Clear canvas
    ctx.set_fill_style_str("#1a1a2e");
    ctx.fill_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);

    let cols = 6;
    let spacing = 8u32; // spacing between cells (in zoomed pixels)

    web_sys::console::log_1(&"=== Glyph Details ===".into());

    for (i, ((grapheme, style), glyph)) in test_glyphs.iter().zip(glyphs.iter()).enumerate() {
        let col = i % cols;
        let row = i / cols;

        // Cell dimensions at zoom scale
        let cell_w = (padded_cell_w as u32) * ZOOM + spacing;
        let cell_h = (padded_cell_h as u32) * ZOOM + spacing;

        let x = (col as u32) * cell_w + spacing;
        let y = (row as u32) * cell_h + spacing;

        // Draw cell boundary (outer edge in red)
        ctx.set_stroke_style_str("#ff0000");
        ctx.set_line_width(1.0);
        ctx.stroke_rect(
            x as f64 - 0.5,
            y as f64 - 0.5,
            (glyph.width * ZOOM) as f64 + 1.0,
            (glyph.height * ZOOM) as f64 + 1.0,
        );

        // Draw padding boundary (inner edge in green)
        ctx.set_stroke_style_str("#00ff00");
        ctx.stroke_rect(
            (x + PADDING as u32 * ZOOM) as f64 - 0.5,
            (y + PADDING as u32 * ZOOM) as f64 - 0.5,
            ((glyph.width as i32 - 2 * PADDING) as u32 * ZOOM) as f64 + 1.0,
            ((glyph.height as i32 - 2 * PADDING) as u32 * ZOOM) as f64 + 1.0,
        );

        // Draw the glyph at 4x zoom
        draw_glyph_zoomed(&ctx, glyph, x, y, ZOOM)?;

        // Draw grid lines to show individual pixels
        ctx.set_stroke_style_str("rgba(255, 255, 255, 0.15)");
        ctx.set_line_width(0.5);
        for px in 0..=glyph.width {
            let line_x = x + px * ZOOM;
            ctx.begin_path();
            ctx.move_to(line_x as f64, y as f64);
            ctx.line_to(line_x as f64, (y + glyph.height * ZOOM) as f64);
            ctx.stroke();
        }
        for py in 0..=glyph.height {
            let line_y = y + py * ZOOM;
            ctx.begin_path();
            ctx.move_to(x as f64, line_y as f64);
            ctx.line_to((x + glyph.width * ZOOM) as f64, line_y as f64);
            ctx.stroke();
        }

        let empty = if glyph.is_empty() { " (EMPTY)" } else { "" };
        let double_width = if glyph.width > ref_glyph.width { " (2x)" } else { "" };
        web_sys::console::log_1(
            &format!(
                "[{:2}] '{}' {:?} -> {}x{} (inner: {}x{}){}{}",
                i,
                grapheme,
                style,
                glyph.width,
                glyph.height,
                glyph.width as i32 - 2 * PADDING,
                glyph.height as i32 - 2 * PADDING,
                double_width,
                empty
            )
            .into(),
        );
    }

    web_sys::console::log_1(&"".into());
    web_sys::console::log_1(&format!("Total: {} glyphs rasterized", glyphs.len()).into());

    // Verify all glyphs have consistent height
    let heights: Vec<_> = glyphs.iter().map(|g| g.height).collect();
    let all_same_height = heights.iter().all(|&h| h == heights[0]);
    web_sys::console::log_1(
        &format!(
            "Height consistency: {} (heights: {:?})",
            if all_same_height { "OK" } else { "MISMATCH!" },
            heights
        )
        .into(),
    );

    Ok(())
}

/// Draw a glyph at the specified zoom level, pixel-perfect
fn draw_glyph_zoomed(
    ctx: &CanvasRenderingContext2d,
    glyph: &RasterizedGlyph,
    x: u32,
    y: u32,
    zoom: u32,
) -> Result<(), JsValue> {
    // Draw each pixel as a zoomed rectangle
    for py in 0..glyph.height {
        for px in 0..glyph.width {
            let idx = ((py * glyph.width + px) * 4) as usize;
            let r = glyph.pixels[idx];
            let g = glyph.pixels[idx + 1];
            let b = glyph.pixels[idx + 2];
            let a = glyph.pixels[idx + 3];

            if a > 0 {
                ctx.set_fill_style_str(&format!("rgba({}, {}, {}, {})", r, g, b, a as f64 / 255.0));
                ctx.fill_rect(
                    (x + px * zoom) as f64,
                    (y + py * zoom) as f64,
                    zoom as f64,
                    zoom as f64,
                );
            }
        }
    }
    Ok(())
}
