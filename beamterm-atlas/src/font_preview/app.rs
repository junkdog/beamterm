use std::{
    io::{stdout, Stdout},
    time::{Duration, Instant},
};

use color_eyre::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyCode,
        KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::font_preview::ui::UI;

#[derive(Debug, Clone)]
pub enum Event {
    Input(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Tick,
}

#[derive(Debug, Clone)]
pub enum Action {
    UpdateSymbol(String),
    ChangeFontSize(f32),
    ChangeLineHeight(f32),
    SelectFont(usize),
    ToggleFontList,
    AdjustUnderlinePosition(f32),
    AdjustUnderlineThickness(f32),
    AdjustStrikethroughPosition(f32),
    AdjustStrikethroughThickness(f32),
    SaveAtlas,
    ResetDefaults,
    Quit,
    NextWidget,
    PrevWidget,
    Increment,
    Decrement,
}

pub struct FontPreviewApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    ui: UI,
    should_quit: bool,
    last_tick: Instant,
    tick_rate: Duration,
}

impl FontPreviewApp {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let ui = UI::new()?;

        Ok(Self {
            terminal,
            ui,
            should_quit: false,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(250),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        while !self.should_quit {
            self.terminal.draw(|f| self.ui.render(f))?;

            let timeout = self
                .tick_rate
                .checked_sub(self.last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            self.handle_key_event(key)?;
                        }
                    },
                    CrosstermEvent::Resize(w, h) => {
                        self.handle_event(Event::Resize(w, h))?;
                    },
                    _ => {},
                }
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                self.handle_event(Event::Tick)?;
                self.last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        self.handle_event(Event::Input(key))
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Input(key) => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.handle_action(Action::Quit)?;
                },
                KeyCode::Char('l') => {
                    self.handle_action(Action::ToggleFontList)?;
                },
                KeyCode::Char('s') => {
                    self.handle_action(Action::SaveAtlas)?;
                },
                KeyCode::Char('r') => {
                    self.handle_action(Action::ResetDefaults)?;
                },
                KeyCode::Tab => {
                    self.handle_action(Action::NextWidget)?;
                },
                KeyCode::BackTab => {
                    self.handle_action(Action::PrevWidget)?;
                },
                KeyCode::Up | KeyCode::Char('+') => {
                    self.handle_action(Action::Increment)?;
                },
                KeyCode::Down | KeyCode::Char('-') => {
                    self.handle_action(Action::Decrement)?;
                },
                KeyCode::Char(c) => {
                    if c.is_alphanumeric() || c.is_ascii_punctuation() || c == ' ' {
                        tracing::debug!("Key pressed: '{}', updating symbol", c);
                        self.handle_action(Action::UpdateSymbol(c.to_string()))?;
                    }
                },
                KeyCode::Backspace => {
                    self.handle_action(Action::UpdateSymbol(String::new()))?;
                },
                _ => {},
            },
            Event::Resize(_, _) => {
                // Terminal will handle resize automatically
            },
            Event::Tick => {
                // Regular updates
            },
        }

        Ok(())
    }

    fn handle_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => {
                self.should_quit = true;
            },
            Action::UpdateSymbol(symbol) => {
                self.ui.update_symbol(symbol)?;
            },
            Action::ToggleFontList => {
                self.ui.toggle_font_list();
            },
            Action::NextWidget => {
                self.ui.next_widget();
            },
            Action::PrevWidget => {
                self.ui.prev_widget();
            },
            Action::Increment => {
                self.ui.increment_current()?;
            },
            Action::Decrement => {
                self.ui.decrement_current()?;
            },
            Action::SaveAtlas => {
                self.ui.save_atlas()?;
            },
            Action::ResetDefaults => {
                self.ui.reset_defaults()?;
            },
            _ => {
                // Handle other actions
            },
        }

        Ok(())
    }
}

impl Drop for FontPreviewApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}
