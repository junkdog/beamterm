//! Terminal emulator demo using beamterm-core for GPU-accelerated rendering.
//!
//! Spawns a PTY with the user's default shell and renders it using
//! beamterm's OpenGL 3.3 pipeline. Uses `vt100` for terminal emulation
//! and `portable-pty` for PTY management.
//!
//! Run with:
//! ```sh
//! cargo run -p terminal-emulator
//! ```

use std::{
    io::{Read, Write},
    num::NonZeroU32,
    sync::mpsc,
    thread,
};

use beamterm_core::{
    CellData, FontAtlasData, FontStyle, GlState, GlslVersion, GlyphEffect, StaticFontAtlas,
    TerminalGrid,
};
use glutin::surface::GlSurface;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::WindowId,
};

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .expect("event loop failed");
}

//  color palette (dracula)

const DEFAULT_FG: u32 = 0x00_f8_f8_f2;
const DEFAULT_BG: u32 = 0x00_28_2a_36;

#[rustfmt::skip]
const ANSI_COLORS: [u32; 16] = [
    0x21_22_2c, // 0  black
    0xff_55_55, // 1  red
    0x50_fa_7b, // 2  green
    0xf1_fa_8c, // 3  yellow
    0xbd_93_f9, // 4  blue
    0xff_79_c6, // 5  magenta
    0x8b_e9_fd, // 6  cyan
    0xf8_f8_f2, // 7  white
    0x62_72_a4, // 8  bright black
    0xff_6e_6e, // 9  bright red
    0x69_ff_94, // 10 bright green
    0xff_ff_a5, // 11 bright yellow
    0xd6_ac_ff, // 12 bright blue
    0xff_92_df, // 13 bright magenta
    0xa4_ff_ff, // 14 bright cyan
    0xff_ff_ff, // 15 bright white
];

const COLOR_CUBE_VALUES: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

fn color_to_rgb(color: vt100::Color, default: u32) -> u32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => match i {
            0..=15 => ANSI_COLORS[i as usize],
            16..=231 => {
                let i = i - 16;
                let r = COLOR_CUBE_VALUES[(i / 36) as usize] as u32;
                let g = COLOR_CUBE_VALUES[((i / 6) % 6) as usize] as u32;
                let b = COLOR_CUBE_VALUES[(i % 6) as usize] as u32;
                (r << 16) | (g << 8) | b
            },
            232..=255 => {
                let v = 8 + 10 * (i - 232) as u32;
                (v << 16) | (v << 8) | v
            },
        },
        vt100::Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
    }
}

fn dim_color(c: u32) -> u32 {
    let r = ((c >> 16) & 0xff) / 2;
    let g = ((c >> 8) & 0xff) / 2;
    let b = (c & 0xff) / 2;
    (r << 16) | (g << 8) | b
}

// cell conversion //

fn convert_cell(cell: &'_ vt100::Cell, is_cursor: bool) -> CellData<'_> {
    let contents = cell.contents();
    let symbol = if contents.is_empty() { " " } else { contents };

    let mut fg = color_to_rgb(cell.fgcolor(), DEFAULT_FG);
    let mut bg = color_to_rgb(cell.bgcolor(), DEFAULT_BG);

    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.dim() {
        fg = dim_color(fg);
    }
    if is_cursor {
        std::mem::swap(&mut fg, &mut bg);
    }

    let style = match (cell.bold(), cell.italic()) {
        (true, true) => FontStyle::BoldItalic,
        (true, false) => FontStyle::Bold,
        (false, true) => FontStyle::Italic,
        (false, false) => FontStyle::Normal,
    };

    let effect = if cell.underline() { GlyphEffect::Underline } else { GlyphEffect::None };

    CellData::new(symbol, style, effect, fg, bg)
}

fn space() -> CellData<'static> {
    CellData::new(
        " ",
        FontStyle::Normal,
        GlyphEffect::None,
        DEFAULT_FG,
        DEFAULT_BG,
    )
}

// terminal sync //

fn sync_terminal(grid: &mut TerminalGrid, parser: &vt100::Parser) {
    let screen = parser.screen();
    let (term_cols, term_rows) = grid.terminal_size();
    let cols = term_cols as usize;
    let rows = term_rows as usize;
    let cursor = screen.cursor_position();

    grid.update_cells((0..rows).flat_map(|row| {
        (0..cols).map(move |col| {
            let is_cursor = row == cursor.0 as usize && col == cursor.1 as usize;

            match screen.cell(row as u16, col as u16) {
                Some(cell) if !cell.is_wide_continuation() => convert_cell(cell, is_cursor),
                _ => {
                    if is_cursor {
                        CellData::new(
                            " ",
                            FontStyle::Normal,
                            GlyphEffect::None,
                            DEFAULT_BG,
                            DEFAULT_FG,
                        )
                    } else {
                        space()
                    }
                },
            }
        })
    }))
    .expect("failed to update cells");
}

// input mapping //

fn ctrl_key_bytes(key: &Key) -> Option<Vec<u8>> {
    if let Key::Character(ch) = key {
        let c = ch.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]);
        }
    }
    None
}

fn named_key_bytes(key: &NamedKey) -> Option<Vec<u8>> {
    #[rustfmt::skip]
    let seq: &[u8] = match key {
        NamedKey::Enter      => b"\r",
        NamedKey::Backspace  => b"\x7f",
        NamedKey::Tab        => b"\t",
        NamedKey::Escape     => b"\x1b",
        NamedKey::Space      => b" ",
        NamedKey::ArrowUp    => b"\x1b[A",
        NamedKey::ArrowDown  => b"\x1b[B",
        NamedKey::ArrowRight => b"\x1b[C",
        NamedKey::ArrowLeft  => b"\x1b[D",
        NamedKey::Home       => b"\x1b[H",
        NamedKey::End        => b"\x1b[F",
        NamedKey::PageUp     => b"\x1b[5~",
        NamedKey::PageDown   => b"\x1b[6~",
        NamedKey::Delete     => b"\x1b[3~",
        NamedKey::Insert     => b"\x1b[2~",
        _ => return None,
    };
    Some(seq.to_vec())
}

// application //

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

struct AppState {
    win: GlWindow,
    gl_state: GlState,
    grid: TerminalGrid,
    parser: vt100::Parser,
    pty_master: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Box<dyn Write + Send>,
    pty_rx: mpsc::Receiver<Vec<u8>>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    modifiers: ModifiersState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let win = GlWindow::new(event_loop, "beamterm - terminal emulator", (960, 600));
        let gl_state = GlState::new(&win.gl);

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

        let (term_cols, term_rows) = grid.terminal_size();

        // --- PTY setup ---
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: term_rows,
                cols: term_cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to open pty");

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .expect("failed to spawn shell");
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .expect("failed to clone pty reader");
        let writer = pair
            .master
            .take_writer()
            .expect("failed to take pty writer");

        // --- PTY reader thread ---
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    },
                }
            }
        });

        let parser = vt100::Parser::new(term_rows, term_cols, 0);

        self.state = Some(AppState {
            win,
            gl_state,
            grid,
            parser,
            pty_master: pair.master,
            pty_writer: writer,
            pty_rx: rx,
            _child: child,
            modifiers: ModifiersState::default(),
        });
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

                    let (cols, rows) = state.grid.terminal_size();
                    state.parser.screen_mut().set_size(rows, cols);
                    let _ = state.pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });

                    state.win.window.request_redraw();
                }
            },
            WindowEvent::ModifiersChanged(mods) => {
                state.modifiers = mods.state();
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if !event.state.is_pressed() {
                    return;
                }

                let bytes = if state.modifiers.control_key() && !state.modifiers.alt_key() {
                    ctrl_key_bytes(&event.logical_key)
                } else if let Key::Named(ref named) = event.logical_key {
                    named_key_bytes(named)
                } else {
                    event.text.as_ref().map(|t| t.as_bytes().to_vec())
                };

                if let Some(bytes) = bytes {
                    let _ = state.pty_writer.write_all(&bytes);
                }
            },
            WindowEvent::RedrawRequested => {
                // drain PTY output
                while let Ok(data) = state.pty_rx.try_recv() {
                    state.parser.process(&data);
                }

                sync_terminal(&mut state.grid, &state.parser);
                state
                    .grid
                    .flush_cells(&state.win.gl)
                    .expect("failed to flush cells");

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

// glutin / winit boilerplate //

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
