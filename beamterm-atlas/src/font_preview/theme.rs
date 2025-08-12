use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    /// For keyboard shortcut hints like [L]ist
    pub shortcut_mnemonic: Style,
    /// Active input field
    pub input_active: Style,
    /// Inactive input field  
    pub input_inactive: Style,
    /// Font variant labels (Regular, Bold, etc.)
    pub variant_label: Style,
    /// Parameter name labels
    pub parameter_label: Style,
    /// Parameter values
    pub parameter_value: Style,
    /// Focused widget borders
    pub border_focused: Style,
    /// Unfocused widget borders
    pub border_unfocused: Style,
    /// Block-rendered glyphs
    pub block_glyph: Style,
    /// Preview canvas for rendering glyphs
    pub preview_canvas: Style,
    /// Bottom status bar
    pub status_bar: Style,
    /// Error messages
    pub error: Style,
    /// Success messages
    pub success: Style,
    /// Increment/decrement buttons
    pub button: Style,
    /// Focused increment/decrement buttons
    pub button_focused: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            shortcut_mnemonic: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),

            input_active: Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray),

            input_inactive: Style::default().fg(Color::Gray),

            variant_label: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),

            parameter_label: Style::default().fg(Color::White),

            parameter_value: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),

            border_focused: Style::default().fg(Color::Yellow),

            border_unfocused: Style::default().fg(Color::Gray),

            block_glyph: Style::default().fg(Color::White),

            preview_canvas: Style::default().bg(Color::Rgb(64, 64, 64)),

            status_bar: Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray),

            error: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),

            success: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),

            button: Style::default().fg(Color::White).bg(Color::Blue),

            button_focused: Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow),
        }
    }
}
