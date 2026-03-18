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
/// Returns `true` if any data was received.
pub fn drain_pty(state: &mut AppState) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(4);
    let mut received = false;

    loop {
        match state.pty_rx.try_recv() {
            Ok((buf, len)) => {
                received = true;
                state.parser.process(&buf[..len]);
                let _ = state.buf_tx.send(buf);
                if std::time::Instant::now() >= deadline {
                    break;
                }
            },
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                state.shell_exited = true;
                break;
            },
        }
    }

    received
}

pub fn sync_terminal(
    grid: &mut TerminalGrid,
    parser: &mut vt100::Parser<TermCallbacks>,
    prev_cursor: &mut (u16, u16),
    prev_show_cursor: &mut bool,
) {
    let screen = parser.screen();
    let cursor = screen.cursor_position();
    let show_cursor = !screen.hide_cursor();
    let (rows, _cols) = screen.size();

    let dirty = parser.screen_mut().take_dirty();
    if !dirty.any() && cursor == *prev_cursor && show_cursor == *prev_show_cursor {
        return;
    }

    let screen = parser.screen();

    // update dirty rows
    grid.update_cells_by_position(dirty.iter(rows).flat_map(|row_idx| {
        let vt_row = screen.visible_row(row_idx).unwrap();
        vt_row
            .cells()
            .enumerate()
            .map(move |(col, cell)| {
                let is_cursor = show_cursor && row_idx == cursor.0 && col as u16 == cursor.1;
                (col as u16, row_idx, cell_data(cell, is_cursor))
            })
    }))
    .expect("failed to update cells");

    // cursor overlay: repaint cells where cursor was and where it is now,
    // unless already covered by dirty rows
    let cursor_cells = [(show_cursor, cursor), (*prev_show_cursor, *prev_cursor)];
    grid.update_cells_by_position(
        cursor_cells
            .into_iter()
            .filter_map(|(visible, (row, col))| {
                if dirty.is_dirty(row) {
                    return None; // already handled above
                }
                let cell = screen.cell(row, col)?;
                let is_cursor = visible && row == cursor.0 && col == cursor.1;
                Some((col, row, cell_data(cell, is_cursor)))
            }),
    )
    .expect("failed to update cursor cells");

    *prev_cursor = cursor;
    *prev_show_cursor = show_cursor;
}

fn cell_data(cell: &vt100::Cell, is_cursor: bool) -> CellData<'_> {
    if cell.is_wide_continuation() {
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
    } else {
        convert_cell(cell, is_cursor)
    }
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
        if c == 'n' {
            let code = params.first().and_then(|p| p.first()).copied();
            match code {
                // DSR - Device Status Report: \x1b[5n → respond "OK"
                Some(5) => {
                    let _ = self
                        .pty_writer
                        .lock()
                        .unwrap()
                        .write_all(b"\x1b[0n");
                },
                // DSR - Cursor Position Report: \x1b[6n → respond with position
                Some(6) => {
                    let (row, col) = screen.cursor_position();
                    let response = format!("\x1b[{};{}R", row + 1, col + 1);
                    let _ = self
                        .pty_writer
                        .lock()
                        .unwrap()
                        .write_all(response.as_bytes());
                },
                _ => {},
            }
        }
    }
}
