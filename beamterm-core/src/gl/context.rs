use glow::HasContext;

/// Manages simple GL state to reduce redundant state changes
#[derive(Debug)]
pub struct GlState {
    // Viewport dimensions
    viewport: [i32; 4], // [x, y, width, height]

    // Clear color
    clear_color: [f32; 4],

    // Blend function state
    blend_func: (u32, u32), // (src_factor, dst_factor)

    // Active texture unit
    active_texture_unit: u32,

    // Enabled vertex attribute arrays
    enabled_vertex_attribs: Vec<bool>,
}

impl GlState {
    /// Create a new GLState object with GL defaults
    pub fn new(gl: &glow::Context) -> Self {
        // Get max vertex attributes
        let max_vertex_attribs = unsafe { gl.get_parameter_i32(glow::MAX_VERTEX_ATTRIBS) as usize };

        Self {
            viewport: [0, 0, 0, 0],
            clear_color: [0.0, 0.0, 0.0, 0.0],
            blend_func: (glow::ONE, glow::ZERO), // Default blend function
            active_texture_unit: glow::TEXTURE0,
            enabled_vertex_attribs: vec![false; max_vertex_attribs],
        }
    }

    /// Set viewport dimensions
    pub fn viewport(
        &mut self,
        gl: &glow::Context,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> &mut Self {
        let new_viewport = [x, y, width, height];
        if self.viewport != new_viewport {
            unsafe { gl.viewport(x, y, width, height) };
            self.viewport = new_viewport;
        }
        self
    }

    /// Set clear color
    pub fn clear_color(&mut self, gl: &glow::Context, r: f32, g: f32, b: f32, a: f32) -> &mut Self {
        let new_color = [r, g, b, a];
        if self.clear_color != new_color {
            unsafe { gl.clear_color(r, g, b, a) };
            self.clear_color = new_color;
        }
        self
    }

    /// Set active texture unit
    pub fn active_texture(&mut self, gl: &glow::Context, texture_unit: u32) -> &mut Self {
        if self.active_texture_unit != texture_unit {
            unsafe { gl.active_texture(texture_unit) };
            self.active_texture_unit = texture_unit;
        }
        self
    }

    /// Enable or disable a vertex attribute array
    pub fn vertex_attrib_array(
        &mut self,
        gl: &glow::Context,
        index: u32,
        enable: bool,
    ) -> &mut Self {
        let idx = index as usize;
        if idx < self.enabled_vertex_attribs.len() && self.enabled_vertex_attribs[idx] != enable {
            if enable {
                unsafe { gl.enable_vertex_attrib_array(index) };
            } else {
                unsafe { gl.disable_vertex_attrib_array(index) };
            }
            self.enabled_vertex_attribs[idx] = enable;
        }
        self
    }

    /// Reset all tracked state to GL defaults
    pub fn reset(&mut self, gl: &glow::Context) {
        // Reset blend function
        if self.blend_func != (glow::ONE, glow::ZERO) {
            unsafe { gl.blend_func(glow::ONE, glow::ZERO) };
            self.blend_func = (glow::ONE, glow::ZERO);
        }

        // Reset texture unit
        if self.active_texture_unit != glow::TEXTURE0 {
            unsafe { gl.active_texture(glow::TEXTURE0) };
            self.active_texture_unit = glow::TEXTURE0;
        }

        // Reset vertex attributes
        for (idx, enabled) in self.enabled_vertex_attribs.iter_mut().enumerate() {
            if *enabled {
                unsafe { gl.disable_vertex_attrib_array(idx as u32) };
                *enabled = false;
            }
        }

        // Note: We don't reset viewport or clear_color as these are typically
        // set based on canvas dimensions or application needs
    }
}
