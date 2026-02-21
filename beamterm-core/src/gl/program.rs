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
        check_link_status(gl, program)?;

        // delete shaders (no longer needed after linking)
        unsafe {
            gl.delete_shader(vertex_shader);
            gl.delete_shader(fragment_shader);
        }

        Ok(ShaderProgram { program })
    }

    /// Use the shader program.
    pub(crate) fn use_program(&self, gl: &glow::Context) {
        unsafe { gl.use_program(Some(self.program)) };
    }
}

fn compile_shader(
    gl: &glow::Context,
    shader_type: ShaderType,
    source: &str,
) -> Result<glow::Shader, Error> {
    let shader = unsafe { gl.create_shader(shader_type.into()) }
        .map_err(|_| Error::shader_creation_failed("unknown error"))?;

    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
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
