//! Terminal emulator demo using beamterm-core for GPU-accelerated rendering.
//!
//! Spawns a PTY with the user's default shell and renders it using
//! beamterm's OpenGL 3.3 pipeline with a native dynamic font atlas
//! (swash+fontdb). Uses `vt100` for terminal emulation and `portable-pty`
//! for PTY management.
//!
//! Run with:
//! ```sh
//! cargo run -p terminal-emulator
//! ```

mod app;
mod color;
mod gl;
mod input;
mod terminal;
use winit::event_loop::EventLoop;

use crate::app::App;

pub const FONT_FAMILIES: &[&str] = &["Hack Nerd Font Mono", "Hack", "Noto Color Emoji"];
pub const DEFAULT_FONT_SIZE: f32 = 16.0;
pub const MIN_FONT_SIZE: f32 = 6.0;
pub const MAX_FONT_SIZE: f32 = 48.0;

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .expect("event loop failed");
}
