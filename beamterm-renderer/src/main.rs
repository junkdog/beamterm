use beamterm_renderer::{Error, Terminal};

mod error;
mod gl;
mod js;
mod mat4;

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run().unwrap()
}

fn run() -> Result<(), Error> {
    let mut terminal = Terminal::builder("canvas")
        .fallback_glyph(" ")
        .build()?;

    terminal.render_frame()?;

    Ok(())
}
