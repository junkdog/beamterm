use color_eyre::Result;
use std::sync::mpsc::Sender;

use crate::font_preview::{
    event::FontPreviewEvent,
    input_processor::{InputProcessor, MainInputProcessor},
    ui::UI,
};

pub struct InputMultiplexer {
    sender: Sender<FontPreviewEvent>,
    processors: Vec<Box<dyn InputProcessor>>,
}

impl InputMultiplexer {
    pub fn new(sender: Sender<FontPreviewEvent>) -> Self {
        let mut multiplexer = Self { 
            sender: sender.clone(), 
            processors: Vec::new() 
        };
        
        // Add the main input processor as the base layer
        multiplexer.push(Box::new(MainInputProcessor::new(sender)));
        
        multiplexer
    }

    pub fn push(&mut self, processor: Box<dyn InputProcessor>) {
        self.processors.push(processor);
        if let Some(processor) = self.processors.last() {
            processor.on_push()
        }
    }

    pub fn pop_processor(&mut self) {
        if let Some(processor) = self.processors.last() {
            processor.on_pop();
        }
        // Don't pop the last processor (main input processor)
        if self.processors.len() > 1 {
            self.processors.pop();
        }
    }

    pub fn apply(&mut self, event: &FontPreviewEvent, ui: &mut UI) -> Result<()> {
        // Handle modal events that should trigger processor stack changes
        match event {
            // Future modal events would be handled here
            // FontPreviewEvent::SettingsDialogOpen => {
            //     self.push(Box::new(SettingsDialogProcessor::new(self.sender.clone())));
            // },
            // FontPreviewEvent::SettingsDialogClose => self.pop_processor(),
            _ => {}
        }

        // Apply the event to the top processor
        if let Some(processor) = self.processors.last_mut() {
            processor.apply(event, ui)?;
        }
        
        Ok(())
    }
}