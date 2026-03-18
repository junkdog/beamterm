// application //

use std::{
    io::Write,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use beamterm_core::{
    GlState, GlslVersion, NativeGlyphRasterizer, TerminalGrid, gl::DynamicFontAtlas,
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    keyboard::{Key, ModifiersState, NamedKey},
    window::WindowId,
};

use crate::{
    DEFAULT_FONT_SIZE, FONT_FAMILIES, MAX_FONT_SIZE, MIN_FONT_SIZE,
    color::DEFAULT_BG,
    gl::GlWindow,
    input::{ctrl_key_bytes, named_key_bytes},
    terminal::{TermCallbacks, drain_pty, sync_terminal},
};

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
}

const PTY_BUF_SIZE: usize = 4096;
const FRAME_INTERVAL: Duration = Duration::from_micros(16_667); // ~60fps

pub struct AppState {
    win: GlWindow,
    gl_state: GlState,
    grid: TerminalGrid,
    pub parser: vt100::Parser<TermCallbacks>,
    prev_cursor: (u16, u16),
    prev_show_cursor: bool,
    last_render: Instant,
    pty_master: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub pty_rx: mpsc::Receiver<(Box<[u8; PTY_BUF_SIZE]>, usize)>,
    pub buf_tx: mpsc::Sender<Box<[u8; PTY_BUF_SIZE]>>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    modifiers: ModifiersState,
    pub shell_exited: bool,
    font_size: f32,
}

fn create_atlas(
    gl: &glow::Context,
    font_size: f32,
    pixel_ratio: f32,
) -> DynamicFontAtlas<NativeGlyphRasterizer> {
    NativeGlyphRasterizer::new(FONT_FAMILIES, font_size * pixel_ratio)
        .and_then(|rasterizer| DynamicFontAtlas::new(gl, rasterizer, font_size, pixel_ratio))
        .expect("failed to create dynamic font atlas")
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let win = GlWindow::new(
            event_loop,
            "beamterm - terminal emulator  |  Shift+F1/F2: font size",
            (960, 600),
        );
        let gl_state = GlState::new(&win.gl);

        let atlas = create_atlas(&win.gl, DEFAULT_FONT_SIZE, win.pixel_ratio());

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
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .expect("failed to take pty writer"),
        ));

        // --- PTY reader thread ---
        //
        // A pool of reusable 4 KiB buffers avoids a heap allocation per read.
        // The reader thread takes a buffer from `buf_rx`, reads into it, then
        // sends `(buf, len)` to the event loop via `data_tx`.  After the event
        // loop has processed the data it returns the buffer via `buf_tx`.
        const PTY_BUF_COUNT: usize = 8;

        let (data_tx, rx) = mpsc::channel::<(Box<[u8; PTY_BUF_SIZE]>, usize)>();
        let (buf_tx, buf_rx) = mpsc::channel::<Box<[u8; PTY_BUF_SIZE]>>();

        // seed the pool
        for _ in 0..PTY_BUF_COUNT {
            let _ = buf_tx.send(Box::new([0u8; PTY_BUF_SIZE]));
        }

        thread::spawn(move || {
            let mut reader = reader;
            while let Ok(mut buf) = buf_rx.recv() {
                match reader.read(buf.as_mut()) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if data_tx.send((buf, n)).is_err() {
                            break;
                        }
                    },
                }
            }
        });

        let callbacks = TermCallbacks { pty_writer: Arc::clone(&writer) };
        let parser = vt100::Parser::new_with_callbacks(term_rows, term_cols, 0, callbacks);

        self.state = Some(AppState {
            win,
            gl_state,
            grid,
            parser,
            prev_cursor: (0, 0),
            prev_show_cursor: true,
            last_render: Instant::now(),
            pty_master: pair.master,
            pty_writer: writer.clone(),
            pty_rx: rx,
            buf_tx,
            _child: child,
            modifiers: ModifiersState::default(),
            shell_exited: false,
            font_size: DEFAULT_FONT_SIZE,
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

                // Shift+F1/F2: decrease/increase font size
                if state.modifiers.shift_key()
                    && let Key::Named(ref named) = event.logical_key
                {
                    let delta = match named {
                        NamedKey::F1 => Some(-1.0_f32),
                        NamedKey::F2 => Some(1.0_f32),
                        _ => None,
                    };

                    if let Some(delta) = delta {
                        let new_size =
                            (state.font_size + delta).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                        if (new_size - state.font_size).abs() > f32::EPSILON {
                            let (cols, rows) = change_font_size(state, new_size);
                            state.parser.screen_mut().set_size(rows, cols);
                            let _ = state.pty_master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                        return;
                    }
                }

                let app_cursor = state.parser.screen().application_cursor();
                let bytes = if state.modifiers.control_key() && !state.modifiers.alt_key() {
                    ctrl_key_bytes(&event.logical_key)
                } else if let Key::Named(ref named) = event.logical_key {
                    named_key_bytes(named, app_cursor)
                } else {
                    event.text.as_ref().map(|t| t.as_bytes().to_vec())
                };

                if let Some(bytes) = bytes {
                    let _ = state.pty_writer.lock().unwrap().write_all(&bytes);
                }
            },
            WindowEvent::RedrawRequested => {
                drain_pty(state);

                sync_terminal(
                    &mut state.grid,
                    &mut state.parser,
                    &mut state.prev_cursor,
                    &mut state.prev_show_cursor,
                );
                state
                    .grid
                    .flush_cells(&state.win.gl)
                    .expect("failed to flush cells");

                let (w, h) = state.grid.canvas_size();
                state.gl_state.viewport(&state.win.gl, 0, 0, w, h);
                state.gl_state.clear_color(
                    &state.win.gl,
                    ((DEFAULT_BG >> 16) & 0xff) as f32 / 255.0,
                    ((DEFAULT_BG >> 8) & 0xff) as f32 / 255.0,
                    (DEFAULT_BG & 0xff) as f32 / 255.0,
                    1.0,
                );

                unsafe {
                    use glow::HasContext;
                    state.win.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                state
                    .grid
                    .render(&state.win.gl, &mut state.gl_state)
                    .expect("failed to render grid");

                state.win.swap_buffers();
                state.last_render = Instant::now();
            },
            _ => {},
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            // drain PTY every iteration so DSR responses (cursor position
            // queries) are never delayed by event batching during resize
            let has_data = drain_pty(state);

            if state.shell_exited {
                if let Some(state) = self.state.take() {
                    state.grid.delete(&state.win.gl);
                }
                event_loop.exit();
                return;
            }

            if has_data {
                let now = Instant::now();
                if now.duration_since(state.last_render) >= FRAME_INTERVAL {
                    state.win.window.request_redraw();
                } else {
                    // data flowing but frame not due yet: keep draining
                    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
                }
            } else {
                // no pending data: poll at ~200Hz to stay responsive
                // without spinning the CPU
                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(5),
                ));
            }
        }
    }
}

fn change_font_size(state: &mut AppState, new_size: f32) -> (u16, u16) {
    state.font_size = new_size;
    let atlas = create_atlas(&state.win.gl, new_size, state.win.pixel_ratio());

    state
        .grid
        .replace_atlas(&state.win.gl, atlas.into());
    state.grid.terminal_size()
}
