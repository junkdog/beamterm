use std::fmt::Debug;

use glow::HasContext;

use crate::{
    error::Error,
    gl::{ShaderProgram, buffer_upload_struct},
};

#[derive(Debug)]
pub(crate) struct UniformBufferObject {
    buffer: glow::Buffer,
    binding_point: u32,
}

impl UniformBufferObject {
    pub fn new(gl: &glow::Context, binding_point: u32) -> Result<Self, Error> {
        let buffer =
            unsafe { gl.create_buffer() }.map_err(|e| Error::buffer_creation_failed("ubo", e))?;

        Ok(Self { buffer, binding_point })
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe { gl.bind_buffer(glow::UNIFORM_BUFFER, Some(self.buffer)) };
    }

    pub fn unbind(&self, gl: &glow::Context) {
        unsafe { gl.bind_buffer(glow::UNIFORM_BUFFER, None) };
    }

    pub(crate) fn bind_to_shader(
        &self,
        gl: &glow::Context,
        shader: &ShaderProgram,
        block_name: &'static str,
    ) -> Result<(), Error> {
        let block_index = unsafe { gl.get_uniform_block_index(shader.program, block_name) }
            .ok_or(Error::uniform_location_failed(block_name))?;

        unsafe {
            gl.uniform_block_binding(shader.program, block_index, self.binding_point);
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, self.binding_point, Some(self.buffer));
        }

        Ok(())
    }

    pub fn upload_data<T>(&self, gl: &glow::Context, data: &T) {
        self.bind(gl);
        unsafe { buffer_upload_struct(gl, glow::UNIFORM_BUFFER, data, glow::STATIC_DRAW) };
        self.unbind(gl);
    }

    /// Deletes the UBO, releasing the GPU resource.
    pub(crate) fn delete(&self, gl: &glow::Context) {
        unsafe { gl.delete_buffer(self.buffer) };
    }
}
