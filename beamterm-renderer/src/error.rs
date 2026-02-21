/// Error categories.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Failed to initialize WebGL context or retrieve DOM elements.
    #[error("Initialization error: {0}")]
    Initialization(String),

    /// Shader compilation, linking, or program creation errors.
    #[error("Shader error: {0}")]
    Shader(String),

    /// WebGL resource creation or management errors.
    #[error("Resource error: {0}")]
    Resource(String),

    /// External data loading or parsing errors.
    #[error("Data error: {0}")]
    Data(String),

    /// Event listener errors, related to mouse input handling.
    #[error("Event listener error: {0}")]
    Callback(String),

    /// Canvas rasterization errors during dynamic font atlas generation.
    #[error("Rasterization error: {0}")]
    Rasterization(String),
}

impl Error {
    // Helper constructors for common error scenarios

    // Initialization errors
    pub(crate) fn window_not_found() -> Self {
        Self::Initialization("Unable to retrieve window".to_string())
    }

    pub(crate) fn document_not_found() -> Self {
        Self::Initialization("Unable to retrieve document".to_string())
    }

    pub(crate) fn canvas_not_found() -> Self {
        Self::Initialization("Unable to retrieve canvas".to_string())
    }

    pub(crate) fn webgl_context_failed() -> Self {
        Self::Initialization("Failed to retrieve WebGL2 rendering context".to_string())
    }

    pub(crate) fn canvas_context_failed() -> Self {
        Self::Initialization("Failed to retrieve canvas rendering context".to_string())
    }

    pub(crate) fn rasterizer_canvas_creation_failed(detail: impl std::fmt::Display) -> Self {
        Self::Rasterization(format!("Failed to create offscreen canvas: {detail}"))
    }

    pub(crate) fn rasterizer_context_failed() -> Self {
        Self::Rasterization("Failed to get 2d rendering context from offscreen canvas".to_string())
    }

    pub(crate) fn rasterizer_missing_font_family() -> Self {
        Self::Rasterization("font_family must be set before rasterizing".to_string())
    }

    pub(crate) fn rasterizer_missing_font_size() -> Self {
        Self::Rasterization("font_size must be set before rasterizing".to_string())
    }

    pub(crate) fn rasterizer_fill_text_failed(
        grapheme: &str,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::Rasterization(format!("Failed to render glyph '{grapheme}': {detail}"))
    }

    pub(crate) fn rasterizer_get_image_data_failed(detail: impl std::fmt::Display) -> Self {
        Self::Rasterization(format!("Failed to read pixel data from canvas: {detail}"))
    }

    pub(crate) fn rasterizer_measure_failed(detail: impl std::fmt::Display) -> Self {
        Self::Rasterization(format!("Failed to measure cell metrics: {detail}"))
    }

    pub(crate) fn rasterizer_empty_reference_glyph() -> Self {
        Self::Rasterization(
            "Reference glyph rasterization produced no pixels; cannot determine cell size"
                .to_string(),
        )
    }
}

impl From<beamterm_core::Error> for Error {
    fn from(err: beamterm_core::Error) -> Self {
        match err {
            beamterm_core::Error::Shader(msg) => Error::Shader(msg),
            beamterm_core::Error::Resource(msg) => Error::Resource(msg),
            beamterm_core::Error::Data(msg) => Error::Data(msg),
        }
    }
}

impl From<Error> for wasm_bindgen::JsValue {
    fn from(err: Error) -> Self {
        wasm_bindgen::JsValue::from_str(&err.to_string())
    }
}
