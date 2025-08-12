use std::{io, panic};

use color_eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};

use crate::font_preview::event::{EventHandler, FontPreviewEvent};

pub type CrosstermTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub struct Tui {
    terminal: CrosstermTerminal,
    events: EventHandler,
}

impl Tui {
    pub fn new(terminal: CrosstermTerminal, events: EventHandler) -> Self {
        Self { terminal, events }
    }

    pub fn draw(&mut self, render_ui: impl FnOnce(&mut Frame)) -> Result<()> {
        self.terminal.draw(render_ui)?;
        Ok(())
    }

    pub fn receive_events<F>(&self, mut f: F)
    where
        F: FnMut(FontPreviewEvent),
    {
        let mut apply_event = |e| f(e);

        apply_event(self.events.next().unwrap());
        while let Some(event) = self.events.try_next() {
            apply_event(event)
        }
    }

    pub fn enter(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;

        crossterm::execute!(io::stderr(), EnterAlternateScreen, EnableMouseCapture)?;

        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic| {
            Self::reset().expect("failed to reset the terminal");
            panic_hook(panic);
        }));

        self.terminal.hide_cursor()?;
        self.terminal.clear()?;
        Ok(())
    }

    fn reset() -> Result<()> {
        terminal::disable_raw_mode()?;
        crossterm::execute!(io::stderr(), LeaveAlternateScreen, DisableMouseCapture)?;
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        Self::reset()?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}