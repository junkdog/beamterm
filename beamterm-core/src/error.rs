/// Error categories for the core rendering engine.
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
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

impl From<beamterm_data::SerializationError> for Error {
    fn from(err: beamterm_data::SerializationError) -> Self {
        Error::Data(err.message.into())
    }
}

#[cfg(feature = "native-dynamic-atlas")]
impl From<beamterm_rasterizer::Error> for Error {
    fn from(err: beamterm_rasterizer::Error) -> Self {
        Error::Resource(err.to_string())
    }
}

impl Error {
    // Shader errors
    pub(crate) fn shader_creation_failed(detail: &str) -> Self {
        Self::Shader(format!("Shader creation failed: {detail}"))
    }

    pub(crate) fn shader_program_creation_failed(detail: impl std::fmt::Display) -> Self {
        Self::Shader(format!("Shader program creation failed: {detail}"))
    }

    pub(crate) fn shader_compilation_failed(stage: impl std::fmt::Display, log: &str) -> Self {
        Self::Shader(format!("{stage} shader compilation failed: {log}"))
    }

    pub(crate) fn shader_link_failed(log: &str) -> Self {
        Self::Shader(format!("Shader linking failed: {log}"))
    }

    // Resource errors
    pub(crate) fn buffer_creation_failed(
        buffer_type: &str,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::Resource(format!("Failed to create {buffer_type} buffer: {detail}"))
    }

    pub(crate) fn vertex_array_creation_failed(detail: impl std::fmt::Display) -> Self {
        Self::Resource(format!("Failed to create vertex array object: {detail}"))
    }

    pub(crate) fn texture_creation_failed(detail: impl std::fmt::Display) -> Self {
        Self::Resource(format!("Failed to create texture: {detail}"))
    }

    pub(crate) fn uniform_location_failed(name: &str) -> Self {
        Self::Resource(format!("Failed to get uniform location: {name}"))
    }
}
