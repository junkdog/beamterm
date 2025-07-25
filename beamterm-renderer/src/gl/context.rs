use crate::gl::GL;

/// Manages simple WebGL state to reduce redundant state changes
#[derive(Debug)]
pub(super) struct GlState {
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
    /// Create a new GLState object with WebGL defaults
    pub fn new(gl: &GL) -> Self {
        // Get max vertex attributes
        let max_vertex_attribs = gl
            .get_parameter(GL::MAX_VERTEX_ATTRIBS)
            .unwrap()
            .as_f64()
            .unwrap() as usize;

        Self {
            viewport: [0, 0, 0, 0],
            clear_color: [0.0, 0.0, 0.0, 0.0],
            blend_func: (GL::ONE, GL::ZERO), // Default blend function
            active_texture_unit: GL::TEXTURE0,
            enabled_vertex_attribs: vec![false; max_vertex_attribs],
        }
    }

    /// Set viewport dimensions
    pub fn viewport(&mut self, gl: &GL, x: i32, y: i32, width: i32, height: i32) -> &mut Self {
        let new_viewport = [x, y, width, height];
        if self.viewport != new_viewport {
            gl.viewport(x, y, width, height);
            self.viewport = new_viewport;
        }
        self
    }

    /// Set clear color
    pub fn clear_color(&mut self, gl: &GL, r: f32, g: f32, b: f32, a: f32) -> &mut Self {
        let new_color = [r, g, b, a];
        if self.clear_color != new_color {
            gl.clear_color(r, g, b, a);
            self.clear_color = new_color;
        }
        self
    }

    /// Set active texture unit
    pub fn active_texture(&mut self, gl: &GL, texture_unit: u32) -> &mut Self {
        if self.active_texture_unit != texture_unit {
            gl.active_texture(texture_unit);
            self.active_texture_unit = texture_unit;
        }
        self
    }

    /// Enable or disable a vertex attribute array
    pub fn vertex_attrib_array(&mut self, gl: &GL, index: u32, enable: bool) -> &mut Self {
        let idx = index as usize;
        if idx < self.enabled_vertex_attribs.len() && self.enabled_vertex_attribs[idx] != enable {
            if enable {
                gl.enable_vertex_attrib_array(index);
            } else {
                gl.disable_vertex_attrib_array(index);
            }
            self.enabled_vertex_attribs[idx] = enable;
        }
        self
    }

    /// Reset all tracked state to WebGL defaults
    pub fn reset(&mut self, gl: &GL) {
        // Reset blend function
        if self.blend_func != (GL::ONE, GL::ZERO) {
            gl.blend_func(GL::ONE, GL::ZERO);
            self.blend_func = (GL::ONE, GL::ZERO);
        }

        // Reset texture unit
        if self.active_texture_unit != GL::TEXTURE0 {
            gl.active_texture(GL::TEXTURE0);
            self.active_texture_unit = GL::TEXTURE0;
        }

        // Reset vertex attributes
        for (idx, enabled) in self.enabled_vertex_attribs.iter_mut().enumerate() {
            if *enabled {
                gl.disable_vertex_attrib_array(idx as u32);
                *enabled = false;
            }
        }

        // Note: We don't reset viewport or clear_color as these are typically
        // set based on canvas dimensions or application needs
    }

    fn capability(&self, gl: &GL, capability: u32, enable: bool) {
        if enable {
            gl.enable(capability);
        } else {
            gl.disable(capability);
        }
    }
}
