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
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(8);
    let mut received = false;

    loop {
        match state.pty_rx.try_recv() {
            Ok(data) => {
                received = true;
                state.parser.process(&data);
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
    parser: &vt100::Parser<TermCallbacks>,
    prev_screen: &mut Option<vt100::Screen>,
) {
    let screen = parser.screen();
    let cursor = screen.cursor_position();
    let show_cursor = !screen.hide_cursor();

    match prev_screen.as_ref() {
        None => {
            // full update: first frame or after resize/font change
            grid.update_cells(
                screen
                    .visible_rows()
                    .enumerate()
                    .flat_map(|(row, vt_row)| {
                        vt_row
                            .cells()
                            .enumerate()
                            .map(move |(col, cell)| {
                                let is_cursor = show_cursor
                                    && row == cursor.0 as usize
                                    && col == cursor.1 as usize;

                                cell_data(cell, is_cursor)
                            })
                    }),
            )
            .expect("failed to update cells");
        },
        Some(prev) => {
            // diff update: only emit changed cells
            let prev_cursor = prev.cursor_position();
            let prev_show = !prev.hide_cursor();

            // cell content diff
            grid.update_cells_by_position(
                screen
                    .visible_rows()
                    .zip(prev.visible_rows())
                    .enumerate()
                    .flat_map(|(row, (cur_row, prev_row))| diff_row(row, cur_row, prev_row)),
            )
            .expect("failed to update cells");

            // cursor overlay: repaint cells where cursor appeared or disappeared
            let cursor_cells = [(show_cursor, cursor), (prev_show, prev_cursor)];
            grid.update_cells_by_position(cursor_cells.into_iter().filter_map(
                |(visible, (row, col))| {
                    let cell = screen.cell(row, col)?;
                    let is_cursor = visible && row == cursor.0 && col == cursor.1;
                    Some((col, row, cell_data(cell, is_cursor)))
                },
            ))
            .expect("failed to update cursor cells");
        },
    }

    *prev_screen = Some(screen.clone());
}

/// Yields `(col, row, CellData)` for cells that differ between the current
/// and previous row.
fn diff_row<'a>(
    row: usize,
    cur_row: &'a vt100::Row,
    prev_row: &'a vt100::Row,
) -> impl Iterator<Item = (u16, u16, CellData<'a>)> {
    cur_row
        .cells()
        .zip(prev_row.cells())
        .enumerate()
        .filter_map(move |(col, (cell, prev_cell))| {
            if cell != prev_cell {
                Some((col as u16, row as u16, cell_data(cell, false)))
            } else {
                None
            }
        })
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
