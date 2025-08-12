use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};

use crate::font_preview::theme::Theme;

#[derive(Default, Clone)]
pub struct ParameterInputState {
    pub value: f32,
    pub focused: bool,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub precision: usize,
}

impl ParameterInputState {
    pub fn new(value: f32, min: f32, max: f32, step: f32, precision: usize) -> Self {
        Self {
            value,
            focused: false,
            min,
            max,
            step,
            precision,
        }
    }
    
    pub fn increment(&mut self) {
        self.value = (self.value + self.step).min(self.max);
    }
    
    pub fn decrement(&mut self) {
        self.value = (self.value - self.step).max(self.min);
    }
    
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

pub struct ParameterInput<'a> {
    label: &'a str,
    unit: &'a str,
    theme: &'a Theme,
}

impl<'a> ParameterInput<'a> {
    pub fn new(label: &'a str, unit: &'a str, theme: &'a Theme) -> Self {
        Self {
            label,
            unit,
            theme,
        }
    }
}

impl<'a> StatefulWidget for ParameterInput<'a> {
    type State = ParameterInputState;
    
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Split into label and controls
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(12),  // Label
                Constraint::Length(8), // Value
                Constraint::Length(3), // [+]
                Constraint::Length(3), // [-] 
            ])
            .split(area);
        
        // Render label
        let label_style = if state.focused {
            self.theme.parameter_label.fg(self.theme.border_focused.fg.unwrap_or(ratatui::style::Color::Yellow))
        } else {
            self.theme.parameter_label
        };
        
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{}: ", self.label), label_style)
        ])).render(chunks[0], buf);
        
        // Render value
        let value_text = if self.unit == "%" {
            format!("[{:.0}]{}", state.value, self.unit)
        } else {
            format!("[{:.1}]{}", state.value, self.unit)
        };
        
        let value_style = if state.focused {
            self.theme.parameter_value
        } else {
            self.theme.parameter_value.fg(ratatui::style::Color::DarkGray)
        };
        
        Paragraph::new(Line::from(vec![
            Span::styled(value_text, value_style)
        ])).render(chunks[1], buf);
        
        // Render increment button
        let inc_style = if state.focused {
            self.theme.button_focused
        } else {
            self.theme.button
        };
        
        Paragraph::new(Line::from(vec![
            Span::styled("[+]", inc_style)
        ])).render(chunks[2], buf);
        
        // Render decrement button  
        let dec_style = if state.focused {
            self.theme.button_focused
        } else {
            self.theme.button
        };
        
        Paragraph::new(Line::from(vec![
            Span::styled("[-]", dec_style)
        ])).render(chunks[3], buf);
    }
}