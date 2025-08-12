use std::{fmt::Debug, sync::mpsc, thread};
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};

#[derive(Debug, Clone)]
pub enum FontPreviewEvent {
    Input(KeyEvent),
    Resize(u16, u16),
    Tick,
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

pub trait Dispatcher {
    fn dispatch(&self, event: FontPreviewEvent);
}

impl Dispatcher for mpsc::Sender<FontPreviewEvent> {
    fn dispatch(&self, event: FontPreviewEvent) {
        let _ = self.send(event);
    }
}

#[derive(Debug)]
pub struct EventHandler {
    sender: mpsc::Sender<FontPreviewEvent>,
    receiver: mpsc::Receiver<FontPreviewEvent>,
    _handler: thread::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate: std::time::Duration) -> Self {
        let (sender, receiver) = mpsc::channel();

        let handler = {
            let sender = sender.clone();
            thread::spawn(move || {
                let mut last_tick = std::time::Instant::now();
                loop {
                    let timeout = tick_rate
                        .checked_sub(last_tick.elapsed())
                        .unwrap_or(tick_rate);

                    if event::poll(timeout).expect("unable to poll for events") {
                        Self::apply_event(&sender);
                    }

                    if last_tick.elapsed() >= tick_rate {
                        sender.dispatch(FontPreviewEvent::Tick);
                        last_tick = std::time::Instant::now();
                    }
                }
            })
        };

        Self { sender, receiver, _handler: handler }
    }

    pub fn sender(&self) -> mpsc::Sender<FontPreviewEvent> {
        self.sender.clone()
    }

    pub fn next(&self) -> Result<FontPreviewEvent, mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn try_next(&self) -> Option<FontPreviewEvent> {
        self.receiver.try_recv().ok()
    }

    fn apply_event(sender: &mpsc::Sender<FontPreviewEvent>) {
        match event::read().expect("unable to read event") {
            CrosstermEvent::Key(e) if e.kind == KeyEventKind::Press => {
                sender.send(FontPreviewEvent::Input(e))
            }
            CrosstermEvent::Resize(w, h) => {
                sender.send(FontPreviewEvent::Resize(w, h))
            }
            _ => Ok(()),
        }
        .expect("failed to send event")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FocusedWidget {
    SymbolInput = 0,
    FontSelector,
    FontSize,
    LineHeight,
    UnderlinePosition,
    UnderlineThickness,
    StrikethroughPosition,
    StrikethroughThickness,
}

const WIDGET_COUNT: u32 = 8;


impl FocusedWidget {
    pub fn next(self) -> Self {
        let next_idx = (self.to_u8() + 1) % WIDGET_COUNT as u8;
        Self::from_u8(next_idx)
    }
    
    pub fn prev(self) -> Self {
        let prev_idx = WIDGET_COUNT + self.to_u8() as u32 - 1;
        Self::from_u8((prev_idx % WIDGET_COUNT) as _)

    }

    const fn from_u8(discriminant: u8) -> Self {
        // SAFETY: This is safe because FocusedWidget is repr(u8) and we ensure
        // the discriminant value is within the valid range (0-8)
        let discriminant = discriminant as u32;
        unsafe { std::mem::transmute((discriminant % WIDGET_COUNT) as u8) }
    }

    const fn to_u8(&self) -> u8 {
        // SAFETY: Because FocusedWidget is repr(u8), we can safely cast to u8
        *self as u8
    }
}