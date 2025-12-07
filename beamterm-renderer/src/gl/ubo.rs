use std::fmt::Debug;

use crate::{
    error::Error,
    gl::{GL, ShaderProgram, buffer_upload_struct},
};

#[derive(Debug)]
pub(super) struct UniformBufferObject {
    buffer: web_sys::WebGlBuffer,
    binding_point: u32,
}

impl UniformBufferObject {
    pub fn new(gl: &GL, binding_point: u32) -> Result<Self, Error> {
        let buffer = gl
            .create_buffer()
            .ok_or(Error::buffer_creation_failed("ubo"))?;

        Ok(Self { buffer, binding_point })
    }

    pub fn bind(&self, gl: &GL) {
        gl.bind_buffer(GL::UNIFORM_BUFFER, Some(&self.buffer));
    }

    pub fn unbind(&self, gl: &GL) {
        gl.bind_buffer(GL::UNIFORM_BUFFER, None);
    }

    pub(crate) fn bind_to_shader(
        &self,
        gl: &GL,
        shader: &ShaderProgram,
        block_name: &'static str,
    ) -> Result<(), Error> {
        let block_index = gl.get_uniform_block_index(&shader.program, block_name);
        if block_index == GL::INVALID_INDEX {
            return Err(Error::uniform_location_failed(block_name));
        }

        gl.uniform_block_binding(&shader.program, block_index, self.binding_point);
        gl.bind_buffer_base(GL::UNIFORM_BUFFER, self.binding_point, Some(&self.buffer));

        Ok(())
    }

    pub fn upload_data<T>(&self, gl: &GL, data: &T) {
        self.bind(gl);
        buffer_upload_struct(gl, GL::UNIFORM_BUFFER, data, GL::STATIC_DRAW);
        self.unbind(gl);
    }
}
