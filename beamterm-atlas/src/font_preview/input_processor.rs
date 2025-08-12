use std::sync::mpsc::Sender;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::font_preview::{
    event::{Dispatcher, FontPreviewEvent},
    ui::UI,
};

pub trait InputProcessor {
    fn apply(&mut self, event: &FontPreviewEvent, ui: &mut UI) -> Result<()>;
    fn on_pop(&self) {}
    fn on_push(&self) {}
}

pub struct MainInputProcessor {
    sender: Sender<FontPreviewEvent>,
}

impl MainInputProcessor {
    pub fn new(sender: Sender<FontPreviewEvent>) -> Self {
        Self { sender }
    }
}

impl InputProcessor for MainInputProcessor {
    fn apply(&mut self, event: &FontPreviewEvent, ui: &mut UI) -> Result<()> {
        match event {
            FontPreviewEvent::Input(key) => {
                self.handle_key_event(*key, ui)?;
            },
            FontPreviewEvent::UpdateSymbol(symbol) => {
                ui.update_symbol(symbol.clone())?;
            },
            FontPreviewEvent::ToggleFontList => {
                ui.toggle_font_list();
            },
            FontPreviewEvent::NextWidget => {
                ui.next_widget();
            },
            FontPreviewEvent::PrevWidget => {
                ui.prev_widget();
            },
            FontPreviewEvent::Increment => {
                ui.increment_current()?;
            },
            FontPreviewEvent::Decrement => {
                ui.decrement_current()?;
            },
            FontPreviewEvent::SaveAtlas => {
                ui.save_atlas()?;
            },
            FontPreviewEvent::ResetDefaults => {
                ui.reset_defaults()?;
            },
            FontPreviewEvent::Resize(_, _) => {
                // Terminal will handle resize automatically
            },
            FontPreviewEvent::Tick => {
                // Regular updates
            },
            _ => {},
        }
        Ok(())
    }
}

impl MainInputProcessor {
    fn handle_key_event(&mut self, key: KeyEvent, _ui: &mut UI) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.sender.dispatch(FontPreviewEvent::Quit);
            },
            KeyCode::Char('l') => {
                self.sender
                    .dispatch(FontPreviewEvent::ToggleFontList);
            },
            KeyCode::Char('s') => {
                self.sender.dispatch(FontPreviewEvent::SaveAtlas);
            },
            KeyCode::Char('r') => {
                self.sender
                    .dispatch(FontPreviewEvent::ResetDefaults);
            },
            KeyCode::Tab => {
                self.sender.dispatch(FontPreviewEvent::NextWidget);
            },
            KeyCode::BackTab => {
                self.sender.dispatch(FontPreviewEvent::PrevWidget);
            },
            KeyCode::Up | KeyCode::Char('+') => {
                self.sender.dispatch(FontPreviewEvent::Increment);
            },
            KeyCode::Down | KeyCode::Char('-') => {
                self.sender.dispatch(FontPreviewEvent::Decrement);
            },
            KeyCode::Char(c) => {
                if c.is_alphanumeric() || c.is_ascii_punctuation() || c == ' ' {
                    tracing::debug!("Key pressed: '{}', updating symbol", c);
                    self.sender
                        .dispatch(FontPreviewEvent::UpdateSymbol(c.to_string()));
                }
            },
            KeyCode::Backspace => {
                self.sender
                    .dispatch(FontPreviewEvent::UpdateSymbol(String::new()));
            },
            _ => {},
        }
        Ok(())
    }
}
