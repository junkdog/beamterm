use crate::gl::context::GlState;

/// Rendering context that provides access to GL state.
pub struct RenderContext<'a> {
    pub gl: &'a glow::Context,
    pub state: &'a mut GlState,
}

/// Trait for objects that can be rendered.
pub trait Drawable {
    /// Prepares the object for rendering.
    ///
    /// This method should set up all necessary OpenGL state, bind shaders,
    /// textures, and vertex data required for rendering.
    fn prepare(&self, context: &mut RenderContext);

    /// Performs the actual rendering.
    ///
    /// This method should issue draw calls to render the object. All necessary
    /// state should already be set up from the `prepare()` call.
    fn draw(&self, context: &mut RenderContext);

    /// Cleans up after rendering.
    ///
    /// This method should restore OpenGL state and unbind any resources
    /// that were bound during `prepare()`. This ensures proper cleanup
    /// for subsequent rendering operations.
    fn cleanup(&self, context: &mut RenderContext);
}
