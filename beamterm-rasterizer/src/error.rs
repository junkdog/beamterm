#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Font not found: {0}")]
    FontNotFound(String),

    #[error("Rasterization failed: {0}")]
    RasterizationFailed(String),

    #[error("No fonts loaded")]
    NoFontsLoaded,
}
