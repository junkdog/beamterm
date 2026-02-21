use beamterm_core::gl::{Drawable, GlState, RenderContext};
use glow::HasContext;
use web_sys::HtmlCanvasElement;

use crate::{error::Error, js};

/// High-level WebGL2 renderer for terminal-style applications.
///
/// The `Renderer` manages the WebGL2 rendering context, canvas, and provides
/// a simplified interface for rendering drawable objects. It handles frame
/// management, viewport setup, and coordinate system transformations.
pub struct Renderer {
    gl: glow::Context,
    raw_gl: web_sys::WebGl2RenderingContext, // for is_context_lost() only
    canvas: web_sys::HtmlCanvasElement,
    state: GlState,
    canvas_padding_color: (f32, f32, f32),
    logical_size_px: (i32, i32),
    pixel_ratio: f32,
    auto_resize_canvas_css: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("canvas_padding_color", &self.canvas_padding_color)
            .field("logical_size_px", &self.logical_size_px)
            .field("pixel_ratio", &self.pixel_ratio)
            .field("auto_resize_canvas_css", &self.auto_resize_canvas_css)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Creates a new renderer by querying for a canvas element with the given ID.
    pub fn create(canvas_id: &str, auto_resize_canvas_css: bool) -> Result<Self, Error> {
        let canvas = js::get_canvas_by_id(canvas_id)?;
        Self::create_with_canvas(canvas, auto_resize_canvas_css)
    }

    /// Sets the background color for the canvas area outside the terminal grid.
    pub fn canvas_padding_color(mut self, color: u32) -> Self {
        let r = ((color >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color >> 8) & 0xFF) as f32 / 255.0;
        let b = (color & 0xFF) as f32 / 255.0;
        self.canvas_padding_color = (r, g, b);
        self
    }

    /// Creates a new renderer from an existing HTML canvas element.
    pub fn create_with_canvas(
        canvas: HtmlCanvasElement,
        auto_resize_canvas_css: bool,
    ) -> Result<Self, Error> {
        let (width, height) = (canvas.width() as i32, canvas.height() as i32);

        // initialize WebGL context
        let (gl, raw_gl) = js::create_glow_context(&canvas)?;
        let state = GlState::new(&gl);

        let mut renderer = Self {
            gl,
            raw_gl,
            canvas,
            state,
            canvas_padding_color: (0.0, 0.0, 0.0),
            logical_size_px: (width, height),
            pixel_ratio: 1.0,
            auto_resize_canvas_css,
        };
        renderer.resize(width as _, height as _);
        Ok(renderer)
    }

    /// Resizes the canvas and updates the viewport.
    pub fn resize(&mut self, width: i32, height: i32) {
        self.logical_size_px = (width, height);
        let (w, h) = self.physical_size();

        self.canvas.set_width(w as u32);
        self.canvas.set_height(h as u32);

        if self.auto_resize_canvas_css {
            let _ = self
                .canvas
                .style()
                .set_property("width", &format!("{width}px"));
            let _ = self
                .canvas
                .style()
                .set_property("height", &format!("{height}px"));
        }

        self.state.viewport(&self.gl, 0, 0, w, h);
    }

    /// Clears the framebuffer with the specified color.
    pub fn clear(&mut self, r: f32, g: f32, b: f32) {
        self.state.clear_color(&self.gl, r, g, b, 1.0);
        unsafe {
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT)
        };
    }

    /// Begins a new rendering frame.
    pub fn begin_frame(&mut self) {
        let (r, g, b) = self.canvas_padding_color;
        self.clear(r, g, b);
    }

    /// Renders a drawable object.
    pub fn render(&mut self, drawable: &impl Drawable) {
        let mut context = RenderContext { gl: &self.gl, state: &mut self.state };

        drawable.prepare(&mut context);
        drawable.draw(&mut context);
        drawable.cleanup(&mut context);
    }

    /// Ends the current rendering frame.
    pub fn end_frame(&mut self) {
        // swap buffers (todo)
    }

    /// Returns a reference to the glow rendering context.
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// Returns a reference to the HTML canvas element.
    pub fn canvas(&self) -> &HtmlCanvasElement {
        &self.canvas
    }

    /// Returns the current canvas dimensions as a tuple.
    pub fn canvas_size(&self) -> (i32, i32) {
        self.logical_size()
    }

    /// Returns the logical size of the canvas in pixels.
    pub fn logical_size(&self) -> (i32, i32) {
        self.logical_size_px
    }

    /// Returns the physical size of the canvas in pixels, taking into account the device
    /// pixel ratio.
    pub fn physical_size(&self) -> (i32, i32) {
        let (w, h) = self.logical_size_px;
        (
            (w as f32 * self.pixel_ratio).round() as i32,
            (h as f32 * self.pixel_ratio).round() as i32,
        )
    }

    /// Checks if the WebGL context has been lost.
    pub fn is_context_lost(&self) -> bool {
        self.raw_gl.is_context_lost()
    }

    /// Restores the WebGL context after a context loss event.
    pub fn restore_context(&mut self) -> Result<(), Error> {
        let (gl, raw_gl) = js::create_glow_context(&self.canvas)?;
        self.state = GlState::new(&gl);
        self.gl = gl;
        self.raw_gl = raw_gl;

        // Restore viewport
        let (width, height) = self.canvas_size();
        self.state.viewport(&self.gl, 0, 0, width, height);

        Ok(())
    }

    /// Sets the pixel ratio.
    pub(crate) fn set_pixel_ratio(&mut self, pixel_ratio: f32) {
        self.pixel_ratio = pixel_ratio;
    }
}
