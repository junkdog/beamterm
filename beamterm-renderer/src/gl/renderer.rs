use web_sys::HtmlCanvasElement;

use crate::{
    error::Error,
    gl::{context::GlState, GL},
    js,
};

/// Rendering context that provides access to WebGL state.
pub(super) struct RenderContext<'a> {
    pub gl: &'a web_sys::WebGl2RenderingContext,
    pub state: &'a mut GlState,
}

/// High-level WebGL2 renderer for terminal-style applications.
///
/// The `Renderer` manages the WebGL2 rendering context, canvas, and provides
/// a simplified interface for rendering drawable objects. It handles frame
/// management, viewport setup, and coordinate system transformations.
#[derive(Debug)]
pub struct Renderer {
    gl: web_sys::WebGl2RenderingContext,
    canvas: web_sys::HtmlCanvasElement,
    state: GlState,
    canvas_padding_color: (f32, f32, f32),
}

impl Renderer {
    /// Creates a new renderer by querying for a canvas element with the given ID.
    ///
    /// This method searches the DOM for a canvas element with the specified ID,
    /// initializes a WebGL2 context, and sets up the renderer with orthographic
    /// projection matching the canvas dimensions.
    ///
    /// # Parameters
    /// * `canvas_id` - CSS selector for the canvas element (e.g., "canvas" or "#my-canvas")
    ///
    /// # Returns
    /// * `Ok(Renderer)` - Successfully created renderer
    /// * `Err(Error)` - Failed to find canvas, create WebGL context, or initialize renderer
    ///
    /// # Errors
    /// * `Error::UnableToRetrieveCanvas` - Canvas element not found
    /// * `Error::FailedToRetrieveWebGl2RenderingContext` - WebGL2 not supported or failed to initialize
    pub fn create(canvas_id: &str) -> Result<Self, Error> {
        let canvas = js::get_canvas_by_id(canvas_id)?;
        Self::create_with_canvas(canvas)
    }

    /// Sets the background color for the canvas area outside the terminal grid.
    ///
    /// When the canvas dimensions don't align perfectly with the terminal cell grid,
    /// there may be unused pixels around the edges. This color fills those padding
    /// areas to maintain a consistent appearance.
    pub fn canvas_padding_color(mut self, color: u32) -> Self {
        let r = ((color >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color >> 8) & 0xFF) as f32 / 255.0;
        let b = (color & 0xFF) as f32 / 255.0;
        self.canvas_padding_color = (r, g, b);
        self
    }

    /// Creates a new renderer from an existing HTML canvas element.
    ///
    /// This method takes ownership of an existing canvas element and initializes
    /// the WebGL2 context and renderer state. Useful when you already have a
    /// reference to the canvas element.
    ///
    /// # Parameters
    /// * `canvas` - HTML canvas element to use for rendering
    ///
    /// # Returns
    /// * `Ok(Renderer)` - Successfully created renderer
    /// * `Err(Error)` - Failed to create WebGL context or initialize renderer
    pub fn create_with_canvas(canvas: HtmlCanvasElement) -> Result<Self, Error> {
        let (width, height) = (canvas.width(), canvas.height());

        // initialize WebGL context
        let gl = js::get_webgl2_context(&canvas)?;
        let state = GlState::new(&gl);

        let mut renderer = Self {
            gl,
            canvas,
            state,
            canvas_padding_color: (0.0, 0.0, 0.0),
        };
        renderer.resize(width as _, height as _);
        Ok(renderer)
    }

    /// Resizes the canvas and updates the viewport.
    ///
    /// This method changes the canvas resolution and adjusts the WebGL viewport
    /// to match. The projection matrix is automatically updated to maintain
    /// proper coordinate mapping.
    ///
    /// # Parameters
    /// * `width` - New canvas width in pixels
    /// * `height` - New canvas height in pixels
    pub fn resize(&mut self, width: i32, height: i32) {
        self.canvas.set_width(width as u32);
        self.canvas.set_height(height as u32);
        self.state.viewport(&self.gl, 0, 0, width, height);
    }

    /// Clears the framebuffer with the specified color.
    ///
    /// Sets the clear color and clears both the color and depth buffers.
    /// Color components should be in the range [0.0, 1.0].
    ///
    /// # Parameters
    /// * `r` - Red component (0.0 to 1.0)
    /// * `g` - Green component (0.0 to 1.0)
    /// * `b` - Blue component (0.0 to 1.0)
    pub fn clear(&mut self, r: f32, g: f32, b: f32) {
        self.state.clear_color(&self.gl, r, g, b, 1.0);
        self.gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);
    }

    /// Begins a new rendering frame.
    pub fn begin_frame(&mut self) {
        let (r, g, b) = self.canvas_padding_color;
        self.clear(r, g, b);
    }

    /// Renders a drawable object.
    ///
    /// This method calls the drawable's prepare, draw, and cleanup methods
    /// in sequence, providing it with a render context containing.
    ///
    /// # Parameters
    /// * `drawable` - Object implementing the `Drawable` trait
    #[allow(private_bounds)]
    pub fn render(&mut self, drawable: &impl Drawable) {
        let mut context = RenderContext { gl: &self.gl, state: &mut self.state };

        drawable.prepare(&mut context);
        drawable.draw(&mut context);
        drawable.cleanup(&mut context);
    }

    /// Ends the current rendering frame.
    ///
    /// This method finalizes the frame rendering. In future versions, this
    /// may handle buffer swapping or other post-rendering operations.
    pub fn end_frame(&mut self) {
        // swap buffers (todo)
    }

    /// Returns a reference to the WebGL2 rendering context.
    pub fn gl(&self) -> &GL {
        &self.gl
    }

    /// Returns a mutable reference to the WebGL2 rendering context.
    pub fn canvas(&self) -> &HtmlCanvasElement {
        &self.canvas
    }

    /// Returns the current canvas dimensions as a tuple.
    ///
    /// # Returns
    /// Tuple containing (width, height) in pixels
    pub fn canvas_size(&self) -> (i32, i32) {
        (self.canvas.width() as i32, self.canvas.height() as i32)
    }
}

/// Trait for objects that can be rendered by the renderer.
pub(super) trait Drawable {
    /// Prepares the object for rendering.
    ///
    /// This method should set up all necessary OpenGL state, bind shaders,
    /// textures, and vertex data required for rendering.
    ///
    /// # Parameters
    /// * `context` - Mutable reference to the render context
    fn prepare(&self, context: &mut RenderContext);

    /// Performs the actual rendering.
    ///
    /// This method should issue draw calls to render the object. All necessary
    /// state should already be set up from the `prepare()` call.
    ///
    /// # Parameters
    /// * `context` - Mutable reference to the render context
    fn draw(&self, context: &mut RenderContext);

    /// Cleans up after rendering.
    ///
    /// This method should restore OpenGL state and unbind any resources
    /// that were bound during `prepare()`. This ensures proper cleanup
    /// for subsequent rendering operations.
    ///
    /// # Parameters
    /// * `context` - Mutable reference to the render context
    fn cleanup(&self, context: &mut RenderContext);
}
