use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::{font_discovery::FontFamily, font_preview::theme::Theme};

#[derive(Default)]
pub struct FontSelectorState {
    pub fonts: Vec<FontFamily>,
    pub selected: Option<usize>,
    pub focused: bool,
    pub show_list: bool,
    pub list_state: ListState,
}

impl FontSelectorState {
    pub fn new(fonts: Vec<FontFamily>) -> Self {
        let mut state = Self {
            fonts,
            selected: Some(0),
            focused: false,
            show_list: false,
            list_state: ListState::default(),
        };

        if !state.fonts.is_empty() {
            state.list_state.select(Some(0));
        }

        state
    }

    pub fn selected_font(&self) -> Option<&FontFamily> {
        self.selected.and_then(|i| self.fonts.get(i))
    }

    pub fn next(&mut self) {
        if self.fonts.is_empty() {
            return;
        }

        let next_index = match self.selected {
            Some(i) => (i + 1) % self.fonts.len(),
            None => 0,
        };

        self.selected = Some(next_index);
        self.list_state.select(Some(next_index));
    }

    pub fn previous(&mut self) {
        if self.fonts.is_empty() {
            return;
        }

        let prev_index = match self.selected {
            Some(i) => {
                if i == 0 {
                    self.fonts.len() - 1
                } else {
                    i - 1
                }
            },
            None => 0,
        };

        self.selected = Some(prev_index);
        self.list_state.select(Some(prev_index));
    }

    pub fn toggle_list(&mut self) {
        self.show_list = !self.show_list;
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

pub struct FontSelector<'a> {
    theme: &'a Theme,
}

impl<'a> FontSelector<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl<'a> StatefulWidget for FontSelector<'a> {
    type State = FontSelectorState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if state.show_list {
            self.render_list(area, buf, state);
        } else {
            self.render_dropdown(area, buf, state);
        }
    }
}

impl<'a> FontSelector<'a> {
    fn render_dropdown(&self, area: Rect, buf: &mut Buffer, state: &FontSelectorState) {
        let font_name = state
            .selected_font()
            .map(|f| f.name.as_str())
            .unwrap_or("No fonts available");

        let style = if state.focused { self.theme.input_active } else { self.theme.input_inactive };

        let border_style = if state.focused {
            self.theme.border_focused
        } else {
            self.theme.border_unfocused
        };

        let text = Line::from(vec![
            Span::styled(format!("{font_name} "), style),
            Span::styled("▼", self.theme.parameter_label),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        Paragraph::new(text)
            .block(block)
            .render(area, buf);
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer, state: &mut FontSelectorState) {
        let items: Vec<ListItem> = state
            .fonts
            .iter()
            .enumerate()
            .map(|(i, font)| {
                let style = if Some(i) == state.selected {
                    self.theme.input_active
                } else {
                    self.theme.input_inactive
                };

                ListItem::new(Line::from(Span::styled(&font.name, style)))
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Select Font")
            .title_style(self.theme.variant_label)
            .border_style(self.theme.border_focused);

        let list = List::new(items)
            .block(block)
            .highlight_style(self.theme.input_active);

        StatefulWidget::render(list, area, buf, &mut state.list_state);
    }
}
