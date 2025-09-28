use std::{fs::File, io::Write};

use beamterm_data::FontAtlasData;
use color_eyre::Report;

/// Represents a bitmap font with all its associated metadata
#[derive(Debug)]
pub struct BitmapFont {
    /// The properties of the font
    pub(crate) atlas_data: FontAtlasData,
}

impl BitmapFont {
    /// Save bitmap font and metadata to a file
    pub fn save(&self, path: &str) -> Result<(), Report> {
        let metadata = &self.atlas_data;
        let mut file = File::create(path)?;
        Write::write_all(&mut file, &metadata.to_binary())?;

        Ok(())
    }
}
