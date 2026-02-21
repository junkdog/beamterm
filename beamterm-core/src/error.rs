/// Error categories for the core rendering engine.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Shader compilation, linking, or program creation errors.
    #[error("Shader error: {0}")]
    Shader(String),

    /// GL resource creation or management errors.
    #[error("Resource error: {0}")]
    Resource(String),

    /// External data loading or parsing errors.
    #[error("Data error: {0}")]
    Data(String),
}

impl Error {
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

    pub(crate) fn uniform_location_failed(name: &str) -> Self {
        Self::Resource(format!("Failed to get uniform location: {name}"))
    }
}
