// cell conversion //

use std::{
    io::Write,
    sync::{Arc, Mutex, mpsc},
};

use beamterm_core::{CellData, FontStyle, GlyphEffect, TerminalGrid};

use crate::{
    app::AppState,
    color::{DEFAULT_BG, DEFAULT_FG, color_to_rgb, dim_color},
};

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

// terminal sync //

const SPACE: CellData<'static> = CellData::new_with_style_bits(" ", 0, DEFAULT_FG, DEFAULT_BG);

/// Drain all pending PTY output and feed it to the vt100 parser.
/// This must be called after resize events so that DSR responses
/// (cursor position queries from TUI apps) are processed promptly.
pub fn drain_pty(state: &mut AppState) {
    loop {
        match state.pty_rx.try_recv() {
            Ok(data) => state.parser.process(&data),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                state.shell_exited = true;
                break;
            },
        }
    }
}

pub fn sync_terminal(grid: &mut TerminalGrid, parser: &vt100::Parser<TermCallbacks>) {
    let screen = parser.screen();
    let (term_cols, term_rows) = grid.terminal_size();
    let cols = term_cols as usize;
    let rows = term_rows as usize;
    let cursor = screen.cursor_position();
    let show_cursor = !screen.hide_cursor();

    grid.update_cells((0..rows).flat_map(|row| {
        (0..cols).map(move |col| {
            let is_cursor = show_cursor && row == cursor.0 as usize && col == cursor.1 as usize;

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
                        SPACE
                    }
                },
            }
        })
    }))
    .expect("failed to update cells");
}

// terminal callbacks //

/// Handles escape sequences that require a response written back to the PTY
/// (e.g. DSR/cursor position reports needed by ratatui and other TUI apps).
pub struct TermCallbacks {
    pub pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl vt100::Callbacks for TermCallbacks {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        _i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        // DSR - Device Status Report: \x1b[6n → respond with cursor position
        if c == 'n' && params.first().and_then(|p| p.first()) == Some(&6) {
            let (row, col) = screen.cursor_position();
            let response = format!("\x1b[{};{}R", row + 1, col + 1);
            let _ = self
                .pty_writer
                .lock()
                .unwrap()
                .write_all(response.as_bytes());
        }
    }
}
