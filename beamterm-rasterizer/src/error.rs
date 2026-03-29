/// Errors that can occur during font rasterization.
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    /// No font found matching the requested family name.
    #[error("Font not found: {0}")]
    FontNotFound(String),

    /// Glyph rasterization or cell metrics computation failed.
    #[error("Rasterization failed: {0}")]
    RasterizationFailed(String),

    /// The font database contains no loaded fonts.
    #[error("No fonts loaded")]
    NoFontsLoaded,
}
