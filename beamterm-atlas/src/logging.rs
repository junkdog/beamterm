use std::path::PathBuf;

use color_eyre::Report;
use directories::ProjectDirs;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter::EnvFilter, fmt, layer::SubscriberExt, reload, util::SubscriberInitExt, Layer,
};

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Log level for file output
    pub file_level: Level,
    /// Log level for console output
    pub console_level: Level,
    /// Directory where log files should be written
    pub log_dir: Option<PathBuf>,
    /// Whether to enable JSON formatted logs for structured output
    pub json_format: bool,
    /// Maximum number of log files to keep for rotation
    #[allow(dead_code)]
    pub max_files: Option<usize>,
    /// Whether this is for a TUI application (disables console logging)
    pub is_tui: bool,
}

/// Handle for dynamically updating log levels
pub struct LoggingReloadHandle {
    file_reload_handle: Option<reload::Handle<EnvFilter, tracing_subscriber::Registry>>,
    console_reload_handle: Option<reload::Handle<EnvFilter, tracing_subscriber::Registry>>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            file_level: Level::DEBUG,
            console_level: Level::WARN,
            log_dir: Some(Self::default_log_dir()),
            json_format: false,
            max_files: Some(10),
            is_tui: false,
        }
    }
}

impl LoggingConfig {
    /// Get the OS-appropriate default log directory
    pub fn default_log_dir() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "beamterm") {
            // Use the cache directory for logs (more appropriate for temporary/log files)
            // On Linux: ~/.cache/beamterm
            // On macOS: ~/Library/Caches/beamterm
            // On Windows: %LOCALAPPDATA%\beamterm\cache
            proj_dirs.cache_dir().to_path_buf()
        } else {
            // Fallback to current directory if we can't determine OS directories
            PathBuf::from("beamterm-logs")
        }
    }

    /// Create logging configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        // Override log levels from environment
        if let Ok(level) = std::env::var("BEAMTERM_LOG_LEVEL") {
            if let Ok(parsed_level) = level.parse::<Level>() {
                config.file_level = parsed_level;
                config.console_level = parsed_level;
            }
        }

        // Override file log level specifically
        if let Ok(level) = std::env::var("BEAMTERM_FILE_LOG_LEVEL") {
            if let Ok(parsed_level) = level.parse::<Level>() {
                config.file_level = parsed_level;
            }
        }

        // Override console log level specifically
        if let Ok(level) = std::env::var("BEAMTERM_CONSOLE_LOG_LEVEL") {
            if let Ok(parsed_level) = level.parse::<Level>() {
                config.console_level = parsed_level;
            }
        }

        // Override log directory from environment
        if let Ok(log_dir) = std::env::var("BEAMTERM_LOG_DIR") {
            config.log_dir = Some(PathBuf::from(log_dir));
        }

        // Disable file logging if requested
        if std::env::var("BEAMTERM_NO_FILE_LOGS").is_ok() {
            config.log_dir = None;
        }

        // Enable JSON format for structured logging
        if std::env::var("BEAMTERM_JSON_LOGS").is_ok() {
            config.json_format = true;
        }

        config
    }

    /// Create configuration optimized for font preview TUI
    #[allow(dead_code)]
    pub fn for_font_preview() -> Self {
        Self {
            file_level: Level::DEBUG,
            console_level: Level::INFO,
            log_dir: Some(Self::default_log_dir()),
            json_format: false, // Human-readable for development
            max_files: Some(5), // Keep fewer files for development
            is_tui: true,       // Disables console logging to prevent TUI interference
        }
    }
}

/// Initialize the logging system with the given configuration
pub fn init_logging(
    config: LoggingConfig,
) -> Result<(Option<WorkerGuard>, LoggingReloadHandle), Report> {
    let mut layers = vec![];
    let mut guard = None;
    let mut reload_handle = LoggingReloadHandle {
        file_reload_handle: None,
        console_reload_handle: None,
    };

    // Create file logging layer if log directory is specified
    if let Some(log_dir) = &config.log_dir {
        // Ensure log directory exists
        std::fs::create_dir_all(log_dir)?;

        let file_appender = tracing_appender::rolling::daily(log_dir, "beamterm-atlas.log");
        let (non_blocking, file_guard) = tracing_appender::non_blocking(file_appender);
        guard = Some(file_guard);

        let file_filter = EnvFilter::builder()
            .with_default_directive(config.file_level.into())
            .from_env_lossy()
            .add_directive("cosmic_text::buffer=off".parse().unwrap());

        let (file_layer, file_reload) = reload::Layer::new(file_filter);
        reload_handle.file_reload_handle = Some(file_reload);

        let file_layer = if config.json_format {
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_filter(file_layer)
                .boxed()
        } else {
            fmt::layer()
                .with_writer(non_blocking)
                .with_filter(file_layer)
                .boxed()
        };

        layers.push(file_layer);
    }

    // Create console logging layer only if not a TUI application
    if !config.is_tui {
        let console_filter = EnvFilter::builder()
            .with_default_directive(config.console_level.into())
            .from_env_lossy()
            .add_directive("cosmic_text::buffer=off".parse().unwrap());

        let (console_layer, console_reload) = reload::Layer::new(console_filter);
        reload_handle.console_reload_handle = Some(console_reload);

        let console_layer = fmt::layer()
            .with_target(false) // Hide module paths for cleaner console output
            .with_filter(console_layer)
            .boxed();

        layers.push(console_layer);
    }

    // Initialize the subscriber with all layers
    let subscriber = tracing_subscriber::registry().with(layers);
    subscriber.init();

    Ok((guard, reload_handle))
}

/// Convenience macro for logging with structured fields related to font operations
#[macro_export]
macro_rules! log_font_operation {
    ($level:expr, $message:expr, $($field:ident = $value:expr),*) => {
        match $level {
            tracing::Level::ERROR => tracing::error!($message, $($field = $value),*),
            tracing::Level::WARN => tracing::warn!($message, $($field = $value),*),
            tracing::Level::INFO => tracing::info!($message, $($field = $value),*),
            tracing::Level::DEBUG => tracing::debug!($message, $($field = $value),*),
            tracing::Level::TRACE => tracing::trace!($message, $($field = $value),*),
        }
    };
}

/// Convenience macro for logging font preview UI events
#[macro_export]
macro_rules! log_ui_event {
    ($message:expr) => {
        tracing::debug!("{}", $message);
    };
    ($message:expr, $($field:ident = $value:expr),*) => {
        tracing::debug!($message, $($field = $value),*);
    };
}

/// Convenience macro for logging font generation metrics
#[macro_export]
macro_rules! log_font_metrics {
    ($message:expr, $($field:ident = $value:expr),*) => {
        tracing::info!($message, $($field = $value),*);
    };
}
