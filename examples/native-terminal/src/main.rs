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
    CellData, FontAtlasData, FontStyle, GlState, GlslVersion, GlyphEffect, StaticFontAtlas,
    TerminalGrid,
};
use glutin::surface::GlSurface;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
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
    win: GlWindow,
    gl_state: GlState,
    grid: TerminalGrid,
}

impl AppState {
    fn refresh(&mut self) {
        populate_demo_content(&mut self.grid);
        self.grid
            .flush_cells(&self.win.gl)
            .expect("failed to flush cells");
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        // --- windowing + GL context (see GlWindow below) ---
        let win = GlWindow::new(event_loop, "beamterm - native OpenGL 3.3", (960, 600));
        let gl_state = GlState::new(&win.gl);

        // --- beamterm setup ---
        let atlas_data = FontAtlasData::default();
        let atlas = StaticFontAtlas::load(&win.gl, atlas_data).expect("failed to load font atlas");

        let grid = TerminalGrid::new(
            &win.gl,
            atlas.into(),
            win.physical_size(),
            win.pixel_ratio(),
            &GlslVersion::Gl330,
        )
        .expect("failed to create terminal grid");

        self.state = Some(AppState { win, gl_state, grid });
        self.state.as_mut().unwrap().refresh();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            if let Some(state) = self.state.take() {
                state.grid.delete(&state.win.gl);
            }
            event_loop.exit();
            return;
        }

        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.win.resize_surface(new_size);

                    let _ = state.grid.resize(
                        &state.win.gl,
                        (new_size.width as i32, new_size.height as i32),
                        state.win.pixel_ratio(),
                    );

                    state.refresh();
                    state.win.window.request_redraw();
                }
            },
            WindowEvent::RedrawRequested => {
                let (w, h) = state.grid.canvas_size();
                state.gl_state.viewport(&state.win.gl, 0, 0, w, h);
                state
                    .gl_state
                    .clear_color(&state.win.gl, 0.0, 0.0, 0.0, 1.0);

                unsafe {
                    use glow::HasContext;
                    state.win.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                state
                    .grid
                    .render(&state.win.gl, &mut state.gl_state)
                    .expect("failed to render grid");

                state.win.swap_buffers();
            },
            _ => {},
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.win.window.request_redraw();
        }
    }
}

// ── color scheme ─────────────────────────────────────────────────────

const BG: u32 = 0x00_28_2a_36; // dracula background
const FG: u32 = 0x00_f8_f8_f2; // dracula foreground
const BORDER_FG: u32 = 0x00_62_72_a4; // dracula comment

// ── demo content ─────────────────────────────────────────────────────

/// Populates the terminal grid with colorful demo content.
fn populate_demo_content(grid: &mut TerminalGrid) {
    let (cols, rows) = grid.terminal_size();
    let cols = cols as usize;
    let rows = rows as usize;

    grid.update_cells(
        (0..rows).flat_map(|row| (0..cols).map(move |col| build_cell(row, col, cols, rows))),
    )
    .expect("failed to update cells");
}

fn build_cell(row: usize, col: usize, cols: usize, rows: usize) -> CellData<'static> {
    match row {
        // Title bar
        0 => {
            let title = " beamterm - native OpenGL 3.3 ";
            text_or_space(
                title,
                col,
                FontStyle::Bold,
                GlyphEffect::None,
                FG,
                0x00_44_47_5a,
            )
        },
        1 => space(BG),

        // Color palette
        2 => color_palette_cell(col),
        3 | 5 | 12 => space(BG),

        // Gradient bar
        4 => gradient_cell(col, cols),

        // Style demos
        6 => style_demo(
            col,
            "Normal text  ",
            FontStyle::Normal,
            GlyphEffect::None,
            FG,
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
            BORDER_FG,
        ),

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
    cell(ch, (r << 16) | (g << 8) | b, BG)
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
        return char_at(
            label,
            col,
            FontStyle::Normal,
            GlyphEffect::None,
            BORDER_FG,
            BG,
        );
    }

    let offset = col - label.len();
    let idx = offset / 8;
    let pos = offset % 8;

    if idx < PALETTE.len() {
        let (color, name) = PALETTE[idx];
        char_at(name, pos, FontStyle::Bold, GlyphEffect::None, BG, color)
    } else {
        space(BG)
    }
}

fn style_demo(
    col: usize,
    text: &'static str,
    style: FontStyle,
    effect: GlyphEffect,
    fg: u32,
) -> CellData<'static> {
    if col < 2 {
        space(BG)
    } else {
        text_or_space(text, col - 2, style, effect, fg, BG)
    }
}

fn border_or_fill(row: usize, col: usize, _cols: usize, rows: usize) -> CellData<'static> {
    match (col, row) {
        (0, r) if r == rows - 1 => cell("└", BORDER_FG, BG),
        (0, _) => cell("│", BORDER_FG, BG),
        (_, r) if r == rows - 1 => cell("─", BORDER_FG, BG),
        _ => {
            // Subtle checkerboard
            let dark = (col + row).is_multiple_of(2);
            space(if dark { BG } else { 0x00_2c_2e_3a })
        },
    }
}

fn cell(ch: &'static str, fg: u32, bg: u32) -> CellData<'static> {
    CellData::new(ch, FontStyle::Normal, GlyphEffect::None, fg, bg)
}

fn space(bg: u32) -> CellData<'static> {
    cell(" ", FG, bg)
}

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

fn text_or_space(
    text: &'static str,
    index: usize,
    style: FontStyle,
    effect: GlyphEffect,
    fg: u32,
    bg: u32,
) -> CellData<'static> {
    if index < text.len() {
        char_at(text, index, style, effect, fg, bg)
    } else {
        space(bg)
    }
}

// ── glutin / winit boilerplate ───────────────────────────────────────

use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
    },
    display::{GetGlDisplay, GlDisplay},
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::{
    dpi::LogicalSize,
    window::{Window, WindowAttributes},
};

struct GlWindow {
    window: Window,
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
    gl: glow::Context,
}

impl GlWindow {
    fn new(event_loop: &ActiveEventLoop, title: &str, size: (u32, u32)) -> Self {
        let window_attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(size.0, size.1));

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

        let inner = window.inner_size();
        let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
            .build(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
                NonZeroU32::new(inner.width).unwrap(),
                NonZeroU32::new(inner.height).unwrap(),
            );

        let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &surface_attrs) }
            .expect("failed to create GL surface");

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("failed to make GL context current");

        let _ = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| gl_display.get_proc_address(name))
        };

        Self { window, gl_context, gl_surface, gl }
    }

    fn physical_size(&self) -> (i32, i32) {
        let s = self.window.inner_size();
        (s.width as i32, s.height as i32)
    }

    fn pixel_ratio(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    fn resize_surface(&self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(new_size.width).unwrap(),
            NonZeroU32::new(new_size.height).unwrap(),
        );
    }

    fn swap_buffers(&self) {
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("failed to swap buffers");
    }
}
