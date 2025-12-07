// This example shows the full range of RGB colors that can be displayed in the browser.

mod wave_effect;

use ratzilla::ratatui::Terminal;
use ratzilla::backend::webgl2::{FontAtlasData, WebGl2Backend, WebGl2BackendOptions};
use ratzilla::ratatui::style::{Color, Style};
use ratzilla::ratatui::widgets::{Block, Widget};
use ratzilla::WebRenderer;
use tachyonfx::{EffectRenderer, IntoEffect};
use wave_effect::WaveInterference;

fn get_query_param(name: &str) -> Option<String> {
    web_sys::window()?
        .location()
        .search()
        .ok()
        .and_then(|search| {
            let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
            params.get(name)
        })
}

fn main() -> std::io::Result<()> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    let font_atlas = match get_query_param("atlas_size").as_deref() {
        Some("10") => hack_10pt(),
        _ => FontAtlasData::default(),
    };

    let backend = WebGl2Backend::new_with_options(
        WebGl2BackendOptions::new()
            .font_atlas(font_atlas)
            .measure_performance(true)
            .grid_id("container")
            .enable_console_debug_api()
    )?;
    let terminal = Terminal::new(backend)?;

    let mut effect = WaveInterference::new().into_effect();
    let mut last_tick = web_time::Instant::now();

    terminal.draw_web(move |frame| {
        let now = web_time::Instant::now();
        let elapsed = now.duration_since(last_tick);
        last_tick = now;

        frame.render_effect(&mut effect, frame.area(), elapsed.into());
    });
    Ok(())
}

fn hack_10pt() -> FontAtlasData {
    FontAtlasData::from_binary(include_bytes!("../data/hack-10pt.atlas")).unwrap()
}