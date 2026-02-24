use std::fmt::Debug;

use glow::HasContext;

use crate::error::Error;

#[derive(Debug)]
pub(crate) struct ShaderProgram {
    pub(crate) program: glow::Program,
}

impl ShaderProgram {
    pub(crate) fn create(
        gl: &glow::Context,
        vertex_source: &str,
        fragment_source: &str,
    ) -> Result<Self, Error> {
        let program =
            unsafe { gl.create_program() }.map_err(|_| Error::shader_program_creation_failed())?;

        // compile shaders
        let vertex_shader = compile_shader(gl, ShaderType::Vertex, vertex_source)?;
        let fragment_shader = compile_shader(gl, ShaderType::Fragment, fragment_source)?;

        // attach shaders and link program
        unsafe {
            gl.attach_shader(program, vertex_shader);
            gl.attach_shader(program, fragment_shader);
            gl.link_program(program);
        }

        // delete shaders (no longer needed after linking)
        unsafe {
            gl.delete_shader(vertex_shader);
            gl.delete_shader(fragment_shader);
        }

        // check link status after cleaning up shaders; delete program on failure
        if let Err(e) = check_link_status(gl, program) {
            unsafe { gl.delete_program(program) };
            return Err(e);
        }

        Ok(ShaderProgram { program })
    }

    /// Use the shader program.
    pub(crate) fn use_program(&self, gl: &glow::Context) {
        unsafe { gl.use_program(Some(self.program)) };
    }

    /// Deletes the shader program, releasing the GPU resource.
    pub(crate) fn delete(&self, gl: &glow::Context) {
        unsafe { gl.delete_program(self.program) };
    }
}

fn compile_shader(
    gl: &glow::Context,
    shader_type: ShaderType,
    source: &str,
) -> Result<glow::Shader, Error> {
    let gl_shader_type: u32 = shader_type.into();
    let shader = unsafe { gl.create_shader(gl_shader_type) }
        .map_err(|_| Error::shader_creation_failed("unknown error"))?;

    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
    }

    if !unsafe { gl.get_shader_compile_status(shader) } {
        let log = unsafe { gl.get_shader_info_log(shader) };
        unsafe { gl.delete_shader(shader) };
        let stage = if gl_shader_type == glow::VERTEX_SHADER { "vertex" } else { "fragment" };
        return Err(Error::shader_compilation_failed(stage, log));
    }

    Ok(shader)
}

fn check_link_status(gl: &glow::Context, program: glow::Program) -> Result<(), Error> {
    let status = unsafe { gl.get_program_link_status(program) };
    if !status {
        let log = unsafe { gl.get_program_info_log(program) };
        return Err(Error::shader_link_failed(log));
    }

    Ok(())
}

/// Enum representing the type of shader.
enum ShaderType {
    Vertex,
    Fragment,
}

impl From<ShaderType> for u32 {
    fn from(val: ShaderType) -> Self {
        use ShaderType::*;

        match val {
            Vertex => glow::VERTEX_SHADER,
            Fragment => glow::FRAGMENT_SHADER,
        }
    }
}
