//! Native OpenGL 3.3 terminal rendering example.
//!
//! Demonstrates beamterm-core rendering to a native desktop window using
//! glutin (OpenGL context) + winit (windowing). This proves the full
//! rendering pipeline works on native OpenGL 3.3 Core Profile.
//!
//! Run with:
//! ```sh
//! cargo run -p beamterm-core --example native_terminal
//! ```

use std::num::NonZeroU32;

use beamterm_core::{
    CellData, Drawable, FontAtlasData, FontStyle, GlState, GlslVersion, GlyphEffect, RenderContext,
    StaticFontAtlas, TerminalGrid,
};
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
    },
    display::{GetGlDisplay, GlDisplay},
    surface::{GlSurface, Surface, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .expect("event loop failed");
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

struct AppState {
    window: Window,
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
    gl: glow::Context,
    gl_state: GlState,
    grid: TerminalGrid,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = WindowAttributes::default()
            .with_title("beamterm - native OpenGL 3.3")
            .with_inner_size(LogicalSize::new(960, 600));

        let config_template = ConfigTemplateBuilder::new().with_alpha_size(8);

        let (window, gl_config) =
            DisplayBuilder::new()
                .with_window_attributes(Some(window_attrs))
                .build(event_loop, config_template, |configs| {
                    configs
                        .reduce(|accum, config| {
                            if config.num_samples() > accum.num_samples() { config } else { accum }
                        })
                        .unwrap()
                })
                .expect("failed to build display");

        let window = window.expect("failed to create window");
        let gl_display = gl_config.display();

        // Request OpenGL 3.3 Core
        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
            ));

        let not_current_context = unsafe { gl_display.create_context(&gl_config, &context_attrs) }
            .expect("failed to create GL context");

        let size = window.inner_size();
        let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
            .build(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            );

        let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &surface_attrs) }
            .expect("failed to create GL surface");

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("failed to make GL context current");

        // Try vsync, but don't fail if unsupported
        let _ = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        // Create glow context from glutin's GL loader
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| gl_display.get_proc_address(name))
        };

        let gl_state = GlState::new(&gl);

        // Load the default embedded font atlas
        let atlas_data = FontAtlasData::default();
        let atlas = StaticFontAtlas::load(&gl, atlas_data).expect("failed to load font atlas");

        let pixel_ratio = window.scale_factor() as f32;
        let physical_size = (size.width as i32, size.height as i32);

        let mut grid = TerminalGrid::new(
            &gl,
            atlas.into(),
            physical_size,
            pixel_ratio,
            &GlslVersion::Gl330,
        )
        .expect("failed to create terminal grid");

        populate_demo_content(&mut grid, &gl);
        grid.flush_cells(&gl)
            .expect("failed to flush cells");

        self.state = Some(AppState { window, gl_context, gl_surface, gl, gl_state, grid });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.gl_surface.resize(
                        &state.gl_context,
                        NonZeroU32::new(new_size.width).unwrap(),
                        NonZeroU32::new(new_size.height).unwrap(),
                    );

                    let pixel_ratio = state.window.scale_factor() as f32;
                    let _ = state.grid.resize(
                        &state.gl,
                        (new_size.width as i32, new_size.height as i32),
                        pixel_ratio,
                    );

                    populate_demo_content(&mut state.grid, &state.gl);
                    state
                        .grid
                        .flush_cells(&state.gl)
                        .expect("failed to flush cells");
                    state.window.request_redraw();
                }
            },
            WindowEvent::RedrawRequested => {
                let (w, h) = state.grid.canvas_size();
                state.gl_state.viewport(&state.gl, 0, 0, w, h);
                state
                    .gl_state
                    .clear_color(&state.gl, 0.0, 0.0, 0.0, 1.0);

                unsafe {
                    use glow::HasContext;
                    state.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                let mut ctx = RenderContext { gl: &state.gl, state: &mut state.gl_state };

                state.grid.prepare(&mut ctx)
                    .expect("failed to prepare grid");
                state.grid.draw(&mut ctx);
                state.grid.cleanup(&mut ctx);

                state
                    .gl_surface
                    .swap_buffers(&state.gl_context)
                    .expect("failed to swap buffers");
            },
            _ => {},
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

/// Populates the terminal grid with colorful demo content.
fn populate_demo_content(grid: &mut TerminalGrid, gl: &glow::Context) {
    let (cols, rows) = grid.terminal_size();
    let cols = cols as usize;
    let rows = rows as usize;

    grid.update_cells(
        gl,
        (0..rows).flat_map(|row| (0..cols).map(move |col| build_cell(row, col, cols, rows))),
    )
    .expect("failed to update cells");
}

fn build_cell(row: usize, col: usize, cols: usize, rows: usize) -> CellData<'static> {
    let bg = 0x00_28_2a_36; // dracula background

    match row {
        // Title bar
        0 => {
            let title = " beamterm - native OpenGL 3.3 ";
            if col < title.len() {
                char_at(
                    title,
                    col,
                    FontStyle::Bold,
                    GlyphEffect::None,
                    0x00_f8_f8_f2,
                    0x00_44_47_5a,
                )
            } else {
                space(bg)
            }
        },
        1 => space(bg),

        // Color palette
        2 => color_palette_cell(col),
        3 => space(bg),

        // Gradient bar
        4 => gradient_cell(col, cols),
        5 => space(bg),

        // Style demos
        6 => style_demo(
            col,
            "Normal text  ",
            FontStyle::Normal,
            GlyphEffect::None,
            0x00_f8_f8_f2,
        ),
        7 => style_demo(
            col,
            "Bold text    ",
            FontStyle::Bold,
            GlyphEffect::None,
            0x00_ff_b8_6c,
        ),
        8 => style_demo(
            col,
            "Italic text  ",
            FontStyle::Italic,
            GlyphEffect::None,
            0x00_8b_e9_fd,
        ),
        9 => style_demo(
            col,
            "Bold+Italic  ",
            FontStyle::BoldItalic,
            GlyphEffect::None,
            0x00_ff_79_c6,
        ),
        10 => style_demo(
            col,
            "Underlined   ",
            FontStyle::Normal,
            GlyphEffect::Underline,
            0x00_50_fa_7b,
        ),
        11 => style_demo(
            col,
            "Strikethrough",
            FontStyle::Normal,
            GlyphEffect::Strikethrough,
            0x00_62_72_a4,
        ),
        12 => space(bg),

        // Border + checkerboard fill
        _ => border_or_fill(row, col, cols, rows),
    }
}

const GRADIENT: &[&str] = &[" ", "░", "▒", "▓", "█", "▓", "▒", "░"];

fn gradient_cell(col: usize, cols: usize) -> CellData<'static> {
    let ch = GRADIENT[col % GRADIENT.len()];
    let t = col as f32 / cols.max(1) as f32;
    let r = (t * 255.0) as u32;
    let g = ((1.0 - t) * 200.0) as u32;
    let b = 180_u32;
    CellData::new(
        ch,
        FontStyle::Normal,
        GlyphEffect::None,
        (r << 16) | (g << 8) | b,
        0x00_28_2a_36,
    )
}

const PALETTE: &[(u32, &str)] = &[
    (0x00_ff_55_55, "Red     "),
    (0x00_ff_b8_6c, "Orange  "),
    (0x00_f1_fa_8c, "Yellow  "),
    (0x00_50_fa_7b, "Green   "),
    (0x00_8b_e9_fd, "Cyan    "),
    (0x00_bd_93_f9, "Purple  "),
    (0x00_ff_79_c6, "Pink    "),
    (0x00_f8_f8_f2, "White   "),
];

fn color_palette_cell(col: usize) -> CellData<'static> {
    let label = " Dracula palette: ";
    if col < label.len() {
        char_at(
            label,
            col,
            FontStyle::Normal,
            GlyphEffect::None,
            0x00_62_72_a4,
            0x00_28_2a_36,
        )
    } else {
        let offset = col - label.len();
        let block_width = 8;
        let idx = offset / block_width;
        let pos = offset % block_width;

        if idx < PALETTE.len() {
            let (color, name) = PALETTE[idx];
            char_at(
                name,
                pos,
                FontStyle::Bold,
                GlyphEffect::None,
                0x00_28_2a_36,
                color,
            )
        } else {
            space(0x00_28_2a_36)
        }
    }
}

fn style_demo(
    col: usize,
    text: &'static str,
    style: FontStyle,
    effect: GlyphEffect,
    fg: u32,
) -> CellData<'static> {
    let prefix_len = 2;
    if col < prefix_len {
        space(0x00_28_2a_36)
    } else {
        let text_col = col - prefix_len;
        if text_col < text.len() {
            char_at(text, text_col, style, effect, fg, 0x00_28_2a_36)
        } else {
            space(0x00_28_2a_36)
        }
    }
}

fn border_or_fill(row: usize, col: usize, _cols: usize, rows: usize) -> CellData<'static> {
    let border_fg = 0x00_62_72_a4;
    let bg = 0x00_28_2a_36;

    if col == 0 && row == rows - 1 {
        CellData::new("└", FontStyle::Normal, GlyphEffect::None, border_fg, bg)
    } else if col == 0 {
        CellData::new("│", FontStyle::Normal, GlyphEffect::None, border_fg, bg)
    } else if row == rows - 1 {
        CellData::new("─", FontStyle::Normal, GlyphEffect::None, border_fg, bg)
    } else {
        // Subtle checkerboard
        let dark = (col + row).is_multiple_of(2);
        space(if dark { 0x00_28_2a_36 } else { 0x00_2c_2e_3a })
    }
}

fn space(bg: u32) -> CellData<'static> {
    CellData::new(" ", FontStyle::Normal, GlyphEffect::None, 0x00_f8_f8_f2, bg)
}

/// Returns a CellData for a single ASCII character at `index` within `text`.
fn char_at(
    text: &'static str,
    index: usize,
    style: FontStyle,
    effect: GlyphEffect,
    fg: u32,
    bg: u32,
) -> CellData<'static> {
    CellData::new(&text[index..index + 1], style, effect, fg, bg)
}
