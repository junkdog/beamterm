use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use tracing::{debug, error, info, warn};

use beamterm_atlas::{
    atlas_generator::AtlasFontGenerator,
    font_discovery::{FontDiscovery, FontFamily},
    glyph_bounds::GlyphBounds,
};
use crate::{
    event::FocusedWidget,
    theme::Theme,
    widgets::{
        font_display::{FontDisplay, FontDisplayState, GlyphImage},
        font_selector::{FontSelector, FontSelectorState},
        parameter_input::{ParameterInput, ParameterInputState},
    },
};

#[derive(Debug, Clone, PartialEq)]
struct FontParameters {
    font_family: FontFamily,
    font_size: f32,
    line_height: f32,
    underline: beamterm_data::LineDecoration,
    strikethrough: beamterm_data::LineDecoration,
}

pub struct UI {
    theme: Theme,

    // Widget states
    symbol_input: String,
    font_display_state: FontDisplayState,
    font_selector_state: FontSelectorState,

    // Parameter states
    font_size_state: ParameterInputState,
    line_height_state: ParameterInputState,
    underline_position_state: ParameterInputState,
    underline_thickness_state: ParameterInputState,
    strikethrough_position_state: ParameterInputState,
    strikethrough_thickness_state: ParameterInputState,

    // Focus management
    focused_widget: FocusedWidget,

    // Status message
    status_message: Option<String>,

    // Font discovery
    font_discovery: FontDiscovery,

    // Font rendering
    current_font_generator: Option<AtlasFontGenerator>,

    // Font parameter caching
    cached_font_parameters: Option<FontParameters>,
}

impl UI {
    pub fn new() -> Result<Self> {
        let font_discovery = FontDiscovery::new();
        let available_fonts = font_discovery.discover_complete_monospace_families();

        let mut ui = Self {
            theme: Theme::default(),
            symbol_input: "█".to_string(),
            font_display_state: FontDisplayState::default(),
            font_selector_state: FontSelectorState::new(available_fonts),

            // Initialize parameter states with CLI defaults
            font_size_state: ParameterInputState::new(15.0, 8.0, 72.0, 1.0, 1),
            line_height_state: ParameterInputState::new(1.0, 0.5, 3.0, 0.1, 1),
            underline_position_state: ParameterInputState::new(85.0, 0.0, 100.0, 5.0, 0),
            underline_thickness_state: ParameterInputState::new(5.0, 1.0, 20.0, 1.0, 0),
            strikethrough_position_state: ParameterInputState::new(50.0, 0.0, 100.0, 5.0, 0),
            strikethrough_thickness_state: ParameterInputState::new(5.0, 1.0, 20.0, 1.0, 0),

            focused_widget: FocusedWidget::SymbolInput,
            status_message: None,
            font_discovery,
            current_font_generator: None,

            // Initialize cached parameters as None
            cached_font_parameters: None,
        };

        ui.update_focus();
        ui.update_font_display()?;

        Ok(ui)
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let size = frame.area();

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Symbol input
                Constraint::Min(10),   // Main content
                Constraint::Length(3), // Status bar
            ])
            .split(size);

        // Render symbol input
        self.render_symbol_input(frame, chunks[0]);

        // Main content layout
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60), // Font display
                Constraint::Percentage(40), // Controls
            ])
            .split(chunks[1]);

        // Render font display
        self.render_font_display(frame, main_chunks[0]);

        // Render controls
        self.render_controls(frame, main_chunks[1]);

        // Render status bar
        self.render_status_bar(frame, chunks[2]);

        // Render font list if open
        if self.font_selector_state.show_list {
            self.render_font_list_popup(frame, size);
        }
    }

    fn render_symbol_input(&self, frame: &mut Frame, area: Rect) {
        let style = if matches!(self.focused_widget, FocusedWidget::SymbolInput) {
            self.theme.input_active
        } else {
            self.theme.input_inactive
        };

        let border_style = if matches!(self.focused_widget, FocusedWidget::SymbolInput) {
            self.theme.border_focused
        } else {
            self.theme.border_unfocused
        };

        let input_text = if self.symbol_input.is_empty() {
            "(empty)".to_string()
        } else {
            self.symbol_input.clone()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Symbol")
            .title_style(self.theme.parameter_label)
            .border_style(border_style);

        let paragraph = Paragraph::new(Line::from(Span::styled(input_text, style))).block(block);

        frame.render_widget(paragraph, area);
    }

    fn render_font_display(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Font Variants")
            .title_style(self.theme.variant_label)
            .border_style(self.theme.border_unfocused);

        let font_display = FontDisplay::new(
            &self.theme,
            &self.font_display_state.symbol,
            &self.font_display_state.rendered_variants,
        )
        .block(block);

        frame.render_widget(font_display, area);
    }

    fn render_controls(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Controls")
            .title_style(self.theme.variant_label)
            .border_style(self.theme.border_unfocused);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Controls layout
        let control_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Font selector
                Constraint::Length(2), // Font size
                Constraint::Length(2), // Line height
                Constraint::Length(2), // Zoom level
                Constraint::Length(1), // Separator
                Constraint::Length(2), // Underline position
                Constraint::Length(2), // Underline thickness
                Constraint::Length(2), // Strikethrough position
                Constraint::Length(2), // Strikethrough thickness
                Constraint::Min(0),    // Remaining space
            ])
            .split(inner);

        // Font selector
        let font_selector = FontSelector::new(&self.theme);
        frame.render_stateful_widget(
            font_selector,
            control_chunks[0],
            &mut self.font_selector_state,
        );

        // Parameter inputs
        let font_size_input = ParameterInput::new("Size", "pt", &self.theme);
        frame.render_stateful_widget(
            font_size_input,
            control_chunks[1],
            &mut self.font_size_state,
        );

        let line_height_input = ParameterInput::new("Line Height", "x", &self.theme);
        frame.render_stateful_widget(
            line_height_input,
            control_chunks[2],
            &mut self.line_height_state,
        );

        // Separator
        let separator = Paragraph::new("─── Decorations ───────").style(self.theme.parameter_label);
        frame.render_widget(separator, control_chunks[4]);

        let underline_pos_input = ParameterInput::new("U.Position", "%", &self.theme);
        frame.render_stateful_widget(
            underline_pos_input,
            control_chunks[5],
            &mut self.underline_position_state,
        );

        let underline_thick_input = ParameterInput::new("U.Thickness", "%", &self.theme);
        frame.render_stateful_widget(
            underline_thick_input,
            control_chunks[6],
            &mut self.underline_thickness_state,
        );

        let strike_pos_input = ParameterInput::new("S.Position", "%", &self.theme);
        frame.render_stateful_widget(
            strike_pos_input,
            control_chunks[7],
            &mut self.strikethrough_position_state,
        );

        let strike_thick_input = ParameterInput::new("S.Thickness", "%", &self.theme);
        frame.render_stateful_widget(
            strike_thick_input,
            control_chunks[8],
            &mut self.strikethrough_thickness_state,
        );
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let content = if let Some(ref msg) = self.status_message {
            Line::from(Span::styled(msg.clone(), self.theme.status_bar))
        } else {
            Line::from(vec![
                Span::styled("[", self.theme.status_bar),
                Span::styled("L", self.theme.shortcut_mnemonic),
                Span::styled("]ist fonts  [", self.theme.status_bar),
                Span::styled("S", self.theme.shortcut_mnemonic),
                Span::styled("]ave atlas  [", self.theme.status_bar),
                Span::styled("R", self.theme.shortcut_mnemonic),
                Span::styled("]eset defaults  [", self.theme.status_bar),
                Span::styled("Q", self.theme.shortcut_mnemonic),
                Span::styled("]uit", self.theme.status_bar),
            ])
        };

        let paragraph = Paragraph::new(content).style(self.theme.status_bar);

        frame.render_widget(paragraph, area);
    }

    fn render_font_list_popup(&mut self, frame: &mut Frame, area: Rect) {
        let popup_area = self.centered_rect(80, 60, area);

        frame.render_widget(Clear, popup_area);

        let font_selector = FontSelector::new(&self.theme);
        frame.render_stateful_widget(font_selector, popup_area, &mut self.font_selector_state);
    }

    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }

    // State management methods

    pub fn update_symbol(&mut self, symbol: String) -> Result<()> {
        debug!(
            old_symbol = %self.symbol_input,
            new_symbol = %symbol,
            "Updating preview symbol"
        );
        self.symbol_input = symbol;
        debug!("About to call update_font_display after symbol change");
        let result = self.update_font_display();
        debug!("Completed update_font_display after symbol change");
        result
    }

    pub fn toggle_font_list(&mut self) {
        debug!("Toggling font list visibility");
        self.font_selector_state.toggle_list();
    }

    pub fn next_widget(&mut self) {
        let old_widget = self.focused_widget;
        self.focused_widget = self.focused_widget.next();
        debug!(
            from = ?old_widget,
            to = ?self.focused_widget,
            "Moving focus to next widget"
        );
        self.update_focus();
    }

    pub fn prev_widget(&mut self) {
        let old_widget = self.focused_widget;
        self.focused_widget = self.focused_widget.prev();
        debug!(
            from = ?old_widget,
            to = ?self.focused_widget,
            "Moving focus to previous widget"
        );
        self.update_focus();
    }

    pub fn increment_current(&mut self) -> Result<()> {
        match self.focused_widget {
            FocusedWidget::FontSelector => {
                self.font_selector_state.next();
                self.update_font_display()?;
            },
            FocusedWidget::FontSize => {
                self.font_size_state.increment();
                self.update_font_display()?;
            },
            FocusedWidget::LineHeight => {
                self.line_height_state.increment();
                self.update_font_display()?;
            },
            FocusedWidget::UnderlinePosition => {
                self.underline_position_state.increment();
                self.update_font_display()?;
            },
            FocusedWidget::UnderlineThickness => {
                self.underline_thickness_state.increment();
                self.update_font_display()?;
            },
            FocusedWidget::StrikethroughPosition => {
                self.strikethrough_position_state.increment();
                self.update_font_display()?;
            },
            FocusedWidget::StrikethroughThickness => {
                self.strikethrough_thickness_state.increment();
                self.update_font_display()?;
            },
            _ => {},
        }
        Ok(())
    }

    pub fn decrement_current(&mut self) -> Result<()> {
        match self.focused_widget {
            FocusedWidget::FontSelector => {
                self.font_selector_state.previous();
                self.update_font_display()?;
            },
            FocusedWidget::FontSize => {
                self.font_size_state.decrement();
                self.update_font_display()?;
            },
            FocusedWidget::LineHeight => {
                self.line_height_state.decrement();
                self.update_font_display()?;
            },
            FocusedWidget::UnderlinePosition => {
                self.underline_position_state.decrement();
                self.update_font_display()?;
            },
            FocusedWidget::UnderlineThickness => {
                self.underline_thickness_state.decrement();
                self.update_font_display()?;
            },
            FocusedWidget::StrikethroughPosition => {
                self.strikethrough_position_state.decrement();
                self.update_font_display()?;
            },
            FocusedWidget::StrikethroughThickness => {
                self.strikethrough_thickness_state.decrement();
                self.update_font_display()?;
            },
            _ => {},
        }
        Ok(())
    }

    pub fn save_atlas(&mut self) -> Result<()> {
        self.status_message = Some("Atlas saving not implemented yet".to_string());
        Ok(())
    }

    pub fn reset_defaults(&mut self) -> Result<()> {
        info!("Resetting all font parameters to default values");
        self.font_size_state.value = 15.0;
        self.line_height_state.value = 1.0;
        self.underline_position_state.value = 85.0;
        self.underline_thickness_state.value = 5.0;
        self.strikethrough_position_state.value = 50.0;
        self.strikethrough_thickness_state.value = 5.0;

        debug!(
            font_size = self.font_size_state.value,
            line_height = self.line_height_state.value,
            "Reset parameters to defaults"
        );

        self.status_message = Some("Reset to defaults".to_string());
        self.update_font_display()
    }

    fn update_focus(&mut self) {
        // Reset all focus states
        self.font_selector_state.set_focused(false);
        self.font_size_state.set_focused(false);
        self.line_height_state.set_focused(false);
        self.underline_position_state.set_focused(false);
        self.underline_thickness_state.set_focused(false);
        self.strikethrough_position_state
            .set_focused(false);
        self.strikethrough_thickness_state
            .set_focused(false);

        // Set current focus
        match self.focused_widget {
            FocusedWidget::FontSelector => self.font_selector_state.set_focused(true),
            FocusedWidget::FontSize => self.font_size_state.set_focused(true),
            FocusedWidget::LineHeight => self.line_height_state.set_focused(true),
            FocusedWidget::UnderlinePosition => self.underline_position_state.set_focused(true),
            FocusedWidget::UnderlineThickness => self.underline_thickness_state.set_focused(true),
            FocusedWidget::StrikethroughPosition => self
                .strikethrough_position_state
                .set_focused(true),
            FocusedWidget::StrikethroughThickness => self
                .strikethrough_thickness_state
                .set_focused(true),
            _ => {},
        }
    }

    fn update_font_display(&mut self) -> Result<()> {
        use beamterm_data::{FontStyle, LineDecoration};

        debug!(
            symbol = %self.symbol_input,
            font_size = self.font_size_state.value,
            line_height = self.line_height_state.value,
            "Updating font display"
        );

        self.font_display_state.symbol = self.symbol_input.clone();

        // Get current font and parameters
        let selected_font = match self.font_selector_state.selected_font() {
            Some(font) => {
                info!(font_family = %font.name, "Using selected font");
                font.clone()
            },
            None => {
                warn!("No font selected");
                // Fallback to demo if no font selected
                // self.font_display_state.rendered_variants = vec![
                //     (FontStyle::Normal, self.create_fallback_image()),
                //     (FontStyle::Bold, self.create_fallback_image()),
                //     (FontStyle::Italic, self.create_fallback_image()),
                //     (FontStyle::BoldItalic, self.create_fallback_image()),
                // ];
                return Ok(());
            },
        };

        let font_size = self.font_size_state.value;
        let line_height = self.line_height_state.value;

        // Create line decorations from current parameters
        let underline = LineDecoration {
            position: self.underline_position_state.value / 100.0,
            thickness: self.underline_thickness_state.value / 100.0,
        };

        let strikethrough = LineDecoration {
            position: self.strikethrough_position_state.value / 100.0,
            thickness: self.strikethrough_thickness_state.value / 100.0,
        };

        // Create current font parameters
        let current_params = FontParameters {
            font_family: selected_font.clone(),
            font_size,
            line_height,
            underline,
            strikethrough,
        };

        // Create or update font generator if parameters changed
        let need_new_generator = self.current_font_generator.is_none()
            || self.cached_font_parameters.as_ref() != Some(&current_params);

        if need_new_generator {
            debug!("Creating new font generator");
            match AtlasFontGenerator::new_with_family(
                selected_font.clone(),
                font_size,
                line_height,
                underline,
                strikethrough,
            ) {
                Ok(generator) => {
                    info!(
                        font_family = %selected_font.name,
                        font_size = font_size,
                        line_height = line_height,
                        "Font generator created successfully"
                    );
                    self.current_font_generator = Some(generator);
                    self.cached_font_parameters = Some(current_params);
                    self.status_message = None;
                },
                Err(e) => {
                    error!(
                        font_family = %selected_font.name,
                        font_size = font_size,
                        error = %e,
                        "Failed to create font generator"
                    );
                    self.status_message = Some(format!("Font error: {e}"));
                    return Ok(());
                },
            }
        }

        // Render the symbol for each font style
        if let Some(font_generator) = &mut self.current_font_generator {
            let symbol = if self.symbol_input.is_empty() {
                "A".to_string()
            } else {
                self.symbol_input.clone()
            };

            let bounds = font_generator.calculate_optimized_cell_dimensions();

            let mut rendered_variants = Vec::new();
            for &style in FontStyle::ALL.iter() {
                let rendered_glyph = self.render_symbol_with_style(&symbol, style, bounds);
                rendered_variants.push((style, rendered_glyph));
            }
            debug!(
                symbol = %symbol,
                variant_count = rendered_variants.len(),
                "Updating font display with new rendered variants"
            );
            // Clear existing variants first to force refresh
            self.font_display_state.rendered_variants.clear();
            self.font_display_state.rendered_variants = rendered_variants;

            // StatefulProtocols are now created fresh each frame in Widget::render
            debug!(
                "Updated {} glyph variants - protocols will be created fresh each frame",
                self.font_display_state.rendered_variants.len()
            );
        }

        Ok(())
    }

    fn render_symbol_with_style(
        &mut self,
        symbol: &str,
        style: beamterm_data::FontStyle,
        bounds: GlyphBounds,
    ) -> GlyphImage {
        // Rasterize the glyph directly using the font generator
        if let Some(ref mut generator) = self.current_font_generator {
            // let bounds = generator.calculate_glyph_bounds();
            let glyph_bitmap = generator
                .rasterize_symbol(symbol, style, bounds)
                .debug_checkered();
            debug!(
                symbol = %symbol,
                style = ?style,
                bounds = ?bounds,
                bitmap_len = glyph_bitmap.data.len(),
                "Rasterized glyph directly"
            );

            self.status_message = Some(format!(
                "Rasterized '{symbol}' {style:?} ({}x{})",
                bounds.width(),
                bounds.height()
            ));

            FontDisplayState::create_glyph_image(
                &glyph_bitmap.data,
                bounds.width() as _,
                bounds.height() as _,
            )
        } else {
            panic!("No font generator available");
        }
    }
}
