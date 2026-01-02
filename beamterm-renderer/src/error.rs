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

    // Shader errors
    pub(crate) fn shader_creation_failed(detail: &str) -> Self {
        Self::Shader(format!("Shader creation failed: {detail}"))
    }

    pub(crate) fn shader_program_creation_failed() -> Self {
        Self::Shader("Shader program creation failed".to_string())
    }

    pub(crate) fn shader_link_failed(log: String) -> Self {
        Self::Shader(format!("Shader linking failed: {log}"))
    }

    // Resource errors
    pub(crate) fn buffer_creation_failed(buffer_type: &str) -> Self {
        Self::Resource(format!("Failed to create {buffer_type} buffer"))
    }

    pub(crate) fn vertex_array_creation_failed() -> Self {
        Self::Resource("Failed to create vertex array object".to_string())
    }

    pub(crate) fn texture_creation_failed() -> Self {
        Self::Resource("Failed to create texture".to_string())
    }

    pub(crate) fn rasterizer_canvas_creation_failed() -> Self {
        Self::Resource("Failed to create texture offscreen canvas for rasterization".to_string())
    }

    pub(crate) fn rasterizer_failed() -> Self {
        Self::Resource("Failed to rasterize glyphs to offscreen canvas".to_string())
    }

    pub(crate) fn uniform_location_failed(name: &str) -> Self {
        Self::Resource(format!("Failed to get uniform location: {name}"))
    }
}
