mod app;
mod event;
mod input_multiplexer;
mod input_processor;
mod theme;
mod tui;
mod ui;
mod widgets;

use std::panic;

use app::FontPreviewApp;
use beamterm_atlas::logging::{init_logging, LoggingConfig};
use color_eyre::Result;
use crossterm::{
    event::DisableMouseCapture,
    terminal::{self, LeaveAlternateScreen},
};

fn main() -> Result<()> {
    // Install color-eyre first to get its panic hook
    color_eyre::install()?;

    // Now wrap the color-eyre panic hook with our terminal reset
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Reset terminal first
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), LeaveAlternateScreen, DisableMouseCapture);

        // Then call the original color-eyre hook
        original_hook(panic_info);
    }));

    // Initialize structured logging
    let logging_config = LoggingConfig::for_font_preview();
    let (_guard, _reload_handle) = init_logging(logging_config)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to initialize logging: {}", e))?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Font Preview starting up"
    );

    let mut app = FontPreviewApp::new()?;
    tracing::info!("Font Preview app initialized successfully");

    let result = app.run();

    match &result {
        Ok(_) => tracing::info!("Font Preview app completed successfully"),
        Err(e) => tracing::error!(error = %e, "Font Preview app failed"),
    }

    result
}
