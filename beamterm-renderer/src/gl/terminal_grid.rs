use std::{cmp::min, fmt::Debug};

use beamterm_data::{FontAtlasData, FontStyle, GlyphEffect};
use web_sys::{console, WebGl2RenderingContext};

use crate::{
    error::Error,
    gl::{
        buffer_upload_array, ubo::UniformBufferObject, Drawable, FontAtlas, RenderContext,
        ShaderProgram, GL,
    },
    mat4::Mat4,
};

/// A high-performance terminal grid renderer using instanced rendering.
///
/// `TerminalGrid` renders a grid of terminal cells using WebGL2 instanced drawing.
/// Each cell can display a character from a font atlas with customizable foreground
/// and background colors. The renderer uses a 2D texture array to efficiently
/// store glyph data and supports real-time updates of cell content.
#[derive(Debug)]
pub struct TerminalGrid {
    /// Shader program for rendering the terminal cells.
    shader: ShaderProgram,
    /// Terminal cell instance data
    cells: Vec<CellDynamic>,
    /// Terminal size in cells
    terminal_size: (u16, u16),
    /// Size of the canvas in pixels
    canvas_size_px: (i32, i32),
    /// Buffers for the terminal grid
    buffers: TerminalBuffers,
    /// shared state for the vertex shader
    ubo_vertex: UniformBufferObject,
    /// shared state for the fragment shader
    ubo_fragment: UniformBufferObject,
    /// Font atlas for rendering text.
    atlas: FontAtlas,
    /// Uniform location for the texture sampler.
    sampler_loc: web_sys::WebGlUniformLocation,
}

#[derive(Debug)]
struct TerminalBuffers {
    vao: web_sys::WebGlVertexArrayObject,
    vertices: web_sys::WebGlBuffer,
    instance_pos: web_sys::WebGlBuffer,
    instance_cell: web_sys::WebGlBuffer,
    indices: web_sys::WebGlBuffer,
}

impl TerminalBuffers {
    fn upload_instance_data<T>(&self, gl: &WebGl2RenderingContext, cell_data: &[T]) {
        gl.bind_vertex_array(Some(&self.vao));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.instance_cell));

        buffer_upload_array(gl, GL::ARRAY_BUFFER, cell_data, GL::DYNAMIC_DRAW);

        gl.bind_vertex_array(None);
    }
}

impl TerminalGrid {
    const FRAGMENT_GLSL: &'static str = include_str!("../shaders/cell.frag");
    const VERTEX_GLSL: &'static str = include_str!("../shaders/cell.vert");

    pub fn new(
        gl: &WebGl2RenderingContext,
        atlas: FontAtlas,
        screen_size: (i32, i32),
    ) -> Result<Self, Error> {
        // create and setup the Vertex Array Object
        let vao = create_vao(gl)?;
        gl.bind_vertex_array(Some(&vao));

        // prepare vertex, index and instance buffers
        let cell_size = atlas.cell_size();
        let (cols, rows) = (screen_size.0 / cell_size.0, screen_size.1 / cell_size.1);

        let fill_glyphs = Self::fill_glyphs(&atlas);
        let cell_data = create_terminal_cell_data(cols, rows, &fill_glyphs);
        let cell_pos = CellStatic::create_grid(cols, rows);
        let buffers = setup_buffers(gl, vao, &cell_pos, &cell_data, cell_size)?;

        // unbind VAO to prevent accidental modification
        gl.bind_vertex_array(None);

        // setup shader and uniform data
        let shader = ShaderProgram::create(gl, Self::VERTEX_GLSL, Self::FRAGMENT_GLSL)?;
        shader.use_program(gl);

        let ubo_vertex = UniformBufferObject::new(gl, CellVertexUbo::BINDING_POINT)?;
        ubo_vertex.bind_to_shader(gl, &shader, "VertUbo")?;
        let ubo_fragment = UniformBufferObject::new(gl, CellFragmentUbo::BINDING_POINT)?;
        ubo_fragment.bind_to_shader(gl, &shader, "FragUbo")?;

        let sampler_loc = gl
            .get_uniform_location(&shader.program, "u_sampler")
            .ok_or(Error::uniform_location_failed("u_sampler"))?;

        console::log_2(&"terminal cells".into(), &cell_data.len().into());

        let (cols, rows) = (screen_size.0 / cell_size.0, screen_size.1 / cell_size.1);
        console::log_1(&format!("terminal size {cols}x{rows}").into());
        let grid = Self {
            shader,
            terminal_size: (cols as u16, rows as u16),
            canvas_size_px: screen_size,
            cells: cell_data,
            buffers,
            ubo_vertex,
            ubo_fragment,
            atlas,
            sampler_loc,
        };

        grid.upload_ubo_data(gl);

        Ok(grid)
    }

    pub fn cell_size(&self) -> (i32, i32) {
        self.atlas.cell_size()
    }

    pub fn terminal_size(&self) -> (u16, u16) {
        self.terminal_size
    }

    /// Uploads uniform buffer data for screen and cell dimensions.
    ///
    /// This method updates the shader uniform buffer with the current screen
    /// size and cell dimensions. Must be called when the screen size changes
    /// or when initializing the grid.
    ///
    /// # Parameters
    /// * `gl` - WebGL2 rendering context
    pub fn upload_ubo_data(&self, gl: &WebGl2RenderingContext) {
        let cell_size = self.cell_size();

        let vertex_ubo = CellVertexUbo::new(self.canvas_size_px, cell_size);
        self.ubo_vertex.upload_data(gl, &vertex_ubo);

        let fragment_ubo = CellFragmentUbo::new(cell_size);
        self.ubo_fragment.upload_data(gl, &fragment_ubo);
    }

    /// Returns the total number of cells in the terminal grid.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Updates the content of terminal cells with new data.
    ///
    /// This method efficiently updates the dynamic instance buffer with new
    /// cell data. The iterator must provide exactly the same number of cells
    /// as the grid contains, in row-major order.
    ///
    /// # Parameters
    /// * `gl` - WebGL2 rendering context
    /// * `cells` - Iterator providing `CellData` for each cell in the grid
    ///
    /// # Returns
    /// * `Ok(())` - Successfully updated cell data
    /// * `Err(Error)` - Failed to update buffer or other WebGL error
    pub fn update_cells<'a>(
        &mut self,
        gl: &WebGl2RenderingContext,
        cells: impl Iterator<Item = CellData<'a>>,
    ) -> Result<(), Error> {
        // update instance buffer with new cell data
        let atlas = &self.atlas;

        let fallback_glyph = atlas.get_glyph_coord(" ", FontStyle::Normal).unwrap_or(0);
        self.cells.iter_mut().zip(cells).for_each(|(cell, data)| {
            let glyph_id = atlas.get_base_glyph_id(data.symbol).unwrap_or(fallback_glyph);

            *cell = CellDynamic::new(glyph_id | data.style_bits, data.fg, data.bg);
        });

        self.buffers.upload_instance_data(gl, &self.cells);

        Ok(())
    }

    pub fn resize(
        &mut self,
        gl: &WebGl2RenderingContext,
        canvas_size: (i32, i32),
    ) -> Result<(), Error> {
        self.canvas_size_px = canvas_size;

        // update the UBO with new screen size
        self.upload_ubo_data(gl);

        let cell_size = self.atlas.cell_size();
        let cols = canvas_size.0 / cell_size.0;
        let rows = canvas_size.1 / cell_size.1;
        if self.terminal_size == (cols as u16, rows as u16) {
            return Ok(()); // no change in terminal size
        }

        // update buffers; bind VAO to ensure correct state
        gl.bind_vertex_array(Some(&self.buffers.vao));

        // delete old cell instance buffers
        gl.delete_buffer(Some(&self.buffers.instance_cell));
        gl.delete_buffer(Some(&self.buffers.instance_pos));

        // resize cell data vector
        let current_size = (self.terminal_size.0 as i32, self.terminal_size.1 as i32);
        let cell_data = resize_cell_grid(&self.cells, current_size, (cols, rows));
        self.cells = cell_data;

        let cell_pos = CellStatic::create_grid(cols, rows);

        // re-create buffers with new data
        self.buffers.instance_cell = create_dynamic_instance_buffer(gl, &self.cells)?;
        self.buffers.instance_pos = create_static_instance_buffer(gl, &cell_pos)?;

        // unbind VAO
        gl.bind_vertex_array(None);

        self.terminal_size = (cols as u16, rows as u16);

        Ok(())
    }

    fn fill_glyphs(atlas: &FontAtlas) -> Vec<u16> {
        [
            ("🤫", FontStyle::Normal),
            ("🙌", FontStyle::Normal),
            ("n", FontStyle::Normal),
            ("o", FontStyle::Normal),
            ("r", FontStyle::Normal),
            ("m", FontStyle::Normal),
            ("a", FontStyle::Normal),
            ("l", FontStyle::Normal),
            ("b", FontStyle::Bold),
            ("o", FontStyle::Bold),
            ("l", FontStyle::Bold),
            ("d", FontStyle::Bold),
            ("i", FontStyle::Italic),
            ("t", FontStyle::Italic),
            ("a", FontStyle::Italic),
            ("l", FontStyle::Italic),
            ("i", FontStyle::Italic),
            ("c", FontStyle::Italic),
            ("b", FontStyle::BoldItalic),
            ("-", FontStyle::BoldItalic),
            ("i", FontStyle::BoldItalic),
            ("t", FontStyle::BoldItalic),
            ("a", FontStyle::BoldItalic),
            ("l", FontStyle::BoldItalic),
            ("i", FontStyle::BoldItalic),
            ("c", FontStyle::BoldItalic),
            ("🤪", FontStyle::Normal),
            ("🤩", FontStyle::Normal),
        ]
        .into_iter()
        .map(|(symbol, style)| atlas.get_glyph_coord(symbol, style))
        .map(|g| g.unwrap_or(' ' as u16))
        .collect()
    }
}

fn resize_cell_grid(
    cells: &[CellDynamic],
    old_size: (i32, i32),
    new_size: (i32, i32),
) -> Vec<CellDynamic> {
    let new_len = new_size.0 * new_size.1;

    let mut new_cells = Vec::with_capacity(new_len as usize);
    for _ in 0..new_len {
        new_cells.push(CellDynamic::new(' ' as u16, 0xFFFFFF, 0x000000));
    }

    for y in 0..min(old_size.1, new_size.1) {
        for x in 0..min(old_size.0, new_size.0) {
            let new_idx = (y * new_size.0 + x) as usize;
            let old_idx = (y * old_size.0 + x) as usize;
            new_cells[new_idx] = cells[old_idx];
        }
    }

    new_cells
}

fn create_vao(gl: &WebGl2RenderingContext) -> Result<web_sys::WebGlVertexArrayObject, Error> {
    gl.create_vertex_array().ok_or(Error::vertex_array_creation_failed())
}

fn setup_buffers(
    gl: &WebGl2RenderingContext,
    vao: web_sys::WebGlVertexArrayObject,
    cell_pos: &[CellStatic],
    cell_data: &[CellDynamic],
    cell_size: (i32, i32),
) -> Result<TerminalBuffers, Error> {
    let (w, h) = (cell_size.0 as f32, cell_size.1 as f32);

    // let overlap = 0.5;
    let overlap = 0.0; // no overlap for now, can be adjusted later
    #[rustfmt::skip]
    let vertices = [
        //    x            y       u    v
        w + overlap,    -overlap, 1.0, 0.0, // top-right
           -overlap, h + overlap, 0.0, 1.0, // bottom-left
        w + overlap, h + overlap, 1.0, 1.0, // bottom-right
           -overlap,    -overlap, 0.0, 0.0  // top-left
    ];
    let indices = [0, 1, 2, 0, 3, 1];

    Ok(TerminalBuffers {
        vao,
        vertices: create_buffer_f32(gl, GL::ARRAY_BUFFER, &vertices, GL::STATIC_DRAW)?,
        instance_pos: create_static_instance_buffer(gl, cell_pos)?,
        instance_cell: create_dynamic_instance_buffer(gl, cell_data)?,
        indices: create_buffer_u8(gl, GL::ELEMENT_ARRAY_BUFFER, &indices, GL::STATIC_DRAW)?,
    })
}

fn create_buffer_u8(
    gl: &WebGl2RenderingContext,
    target: u32,
    data: &[u8],
    usage: u32,
) -> Result<web_sys::WebGlBuffer, Error> {
    let index_buf = gl.create_buffer().ok_or(Error::buffer_creation_failed("vbo-u8"))?;
    gl.bind_buffer(target, Some(&index_buf));

    gl.buffer_data_with_u8_array(target, data, usage);

    Ok(index_buf)
}

fn create_buffer_f32(
    gl: &WebGl2RenderingContext,
    target: u32,
    data: &[f32],
    usage: u32,
) -> Result<web_sys::WebGlBuffer, Error> {
    let buffer = gl.create_buffer().ok_or(Error::buffer_creation_failed("vbo-f32"))?;

    gl.bind_buffer(target, Some(&buffer));

    unsafe {
        let view = js_sys::Float32Array::view(data);
        gl.buffer_data_with_array_buffer_view(target, &view, usage);
    }

    // vertex attributes \\
    const STRIDE: i32 = (2 + 2) * 4; // 4 floats per vertex
    enable_vertex_attrib(gl, attrib::POS, 2, GL::FLOAT, 0, STRIDE);
    enable_vertex_attrib(gl, attrib::UV, 2, GL::FLOAT, 8, STRIDE);

    Ok(buffer)
}

fn create_static_instance_buffer(
    gl: &WebGl2RenderingContext,
    instance_data: &[CellStatic],
) -> Result<web_sys::WebGlBuffer, Error> {
    let instance_buf = gl
        .create_buffer()
        .ok_or(Error::buffer_creation_failed("static-instance-buffer"))?;

    gl.bind_buffer(GL::ARRAY_BUFFER, Some(&instance_buf));
    buffer_upload_array(gl, GL::ARRAY_BUFFER, instance_data, GL::STATIC_DRAW);

    let stride = size_of::<CellStatic>() as i32;
    enable_vertex_attrib_array(gl, attrib::GRID_XY, 2, GL::UNSIGNED_SHORT, 0, stride);

    Ok(instance_buf)
}

fn create_dynamic_instance_buffer(
    gl: &WebGl2RenderingContext,
    instance_data: &[CellDynamic],
) -> Result<web_sys::WebGlBuffer, Error> {
    let instance_buf = gl
        .create_buffer()
        .ok_or(Error::buffer_creation_failed("dynamic-instance-buffer"))?;

    gl.bind_buffer(GL::ARRAY_BUFFER, Some(&instance_buf));
    buffer_upload_array(gl, GL::ARRAY_BUFFER, instance_data, GL::DYNAMIC_DRAW);

    let stride = size_of::<CellDynamic>() as i32;

    // setup instance attributes (while VAO is bound)
    enable_vertex_attrib_array(gl, attrib::PACKED_DEPTH_FG_BG, 2, GL::UNSIGNED_INT, 0, stride);

    Ok(instance_buf)
}

fn enable_vertex_attrib_array(
    gl: &WebGl2RenderingContext,
    index: u32,
    size: i32,
    type_: u32,
    offset: i32,
    stride: i32,
) {
    enable_vertex_attrib(gl, index, size, type_, offset, stride);
    gl.vertex_attrib_divisor(index, 1);
}

fn enable_vertex_attrib(
    gl: &WebGl2RenderingContext,
    index: u32,
    size: i32,
    type_: u32,
    offset: i32,
    stride: i32,
) {
    gl.enable_vertex_attrib_array(index);
    if type_ == GL::FLOAT {
        gl.vertex_attrib_pointer_with_i32(index, size, type_, false, stride, offset);
    } else {
        gl.vertex_attrib_i_pointer_with_i32(index, size, type_, stride, offset);
    }
}

impl Drawable for TerminalGrid {
    fn prepare(&self, context: &mut RenderContext) {
        let gl = context.gl;

        self.shader.use_program(gl);

        gl.bind_vertex_array(Some(&self.buffers.vao));

        self.atlas.bind(gl, 0);
        self.ubo_vertex.bind(context.gl);
        self.ubo_fragment.bind(context.gl);
        gl.uniform1i(Some(&self.sampler_loc), 0);
    }

    fn draw(&self, context: &mut RenderContext) {
        let gl = context.gl;
        let cell_count = self.cells.len() as i32;
        gl.draw_elements_instanced_with_i32(GL::TRIANGLES, 6, GL::UNSIGNED_BYTE, 0, cell_count);
    }

    fn cleanup(&self, context: &mut RenderContext) {
        let gl = context.gl;
        gl.bind_vertex_array(None);
        gl.bind_texture(GL::TEXTURE_2D_ARRAY, None);

        self.ubo_vertex.unbind(gl);
        self.ubo_fragment.unbind(gl);
    }
}

/// Data for a single terminal cell including character and colors.
///
/// `CellData` represents the visual content of one terminal cell, including
/// the character to display and its foreground and background colors.
/// Colors are specified as RGB values packed into 32-bit integers.
///
/// # Color Format
/// Colors use the format 0xRRGGBB where:
/// - RR: Red component
/// - GG: Green component  
/// - BB: Blue component
#[derive(Debug)]
pub struct CellData<'a> {
    // todo: try to pre-pack the available glyph id bits
    symbol: &'a str,
    style_bits: u16,
    fg: u32,
    bg: u32,
}

impl<'a> CellData<'a> {
    /// Creates new cell data with the specified character and colors.
    ///
    /// # Parameters
    /// * `symbol` - Character to display (should be a single character)
    /// * `style` - Font style for the character (e.g. bold, italic)
    /// * `effect` - Optional glyph effect (e.g. underline, strikethrough)
    /// * `fg` - Foreground color as RGB value (0xRRGGBB)
    /// * `bg` - Background color as RGB value (0xRRGGBB)
    ///
    /// # Returns
    /// New `CellData` instance
    pub fn new(symbol: &'a str, style: FontStyle, effect: GlyphEffect, fg: u32, bg: u32) -> Self {
        let style_bits = style.style_mask() | effect as u16;
        Self { symbol, style_bits, fg, bg }
    }

    pub fn new_with_style_bits(symbol: &'a str, style_bits: u16, fg: u32, bg: u32) -> Self {
        Self { symbol, style_bits, fg, bg }
    }
}

/// Static instance data for terminal cell positioning.
///
/// `CellStatic` represents the unchanging positional data for each terminal cell
/// in the grid. This data is uploaded once during initialization and remains
/// constant throughout the lifetime of the terminal grid. Each instance
/// corresponds to one cell position in the terminal grid.
///
/// # Memory Layout
/// This struct uses `#[repr(C, align(4))]` to ensure:
/// - C-compatible memory layout for GPU buffer uploads
/// - 4-byte alignment for efficient GPU access
/// - Predictable field ordering (grid_xy at offset 0)
///
/// # GPU Usage
/// This data is used as per-instance vertex attributes in the vertex shader,
/// allowing the same cell geometry to be rendered at different grid positions
/// using instanced drawing.
///
/// # Buffer Upload
/// Uploaded to GPU using `GL::STATIC_DRAW` since positions don't change.
#[repr(C, align(4))]
struct CellStatic {
    /// Grid position as (x, y) coordinates in cell units.
    pub grid_xy: [u16; 2],
}

/// Dynamic instance data for terminal cell appearance.
///
/// `CellDynamic` contains the frequently-changing visual data for each terminal
/// cell, including the character glyph and colors. This data is updated whenever
/// cell content changes and is efficiently uploaded to the GPU using dynamic
/// buffer updates.
///
/// # Memory Layout
/// The 8-byte data array is packed as follows:
/// - Bytes 0-1: Glyph depth/layer index (u16, little-endian)
/// - Bytes 2-4: Foreground color RGB (3 bytes)
/// - Bytes 5-7: Background color RGB (3 bytes)
///
/// This compact layout minimizes GPU memory usage and allows efficient
/// instanced rendering of the entire terminal grid.
///
/// # Color Format
/// Colors are stored as RGB bytes (no alpha channel in the instance data).
/// The alpha channel is handled separately in the shader based on glyph
/// transparency from the texture atlas.
///
/// # GPU Usage
/// Uploaded as instance attributes and accessed in both vertex and fragment
/// shaders for character selection and color application.
///
/// # Buffer Upload
/// Uploaded to GPU using `GL::DYNAMIC_DRAW` for efficient updates.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(4))]
struct CellDynamic {
    /// Packed cell data:
    ///
    /// # Byte Layout
    /// - `data[0]`: Lower 8 bits of glyph depth/layer index
    /// - `data[1]`: Upper 8 bits of glyph depth/layer index  
    /// - `data[2]`: Foreground red component (0-255)
    /// - `data[3]`: Foreground green component (0-255)
    /// - `data[4]`: Foreground blue component (0-255)
    /// - `data[5]`: Background red component (0-255)
    /// - `data[6]`: Background green component (0-255)
    /// - `data[7]`: Background blue component (0-255)
    pub data: [u8; 8], // 2b layer, fg:rgb, bg:rgb
}

impl CellStatic {
    fn create_grid(cols: i32, rows: i32) -> Vec<Self> {
        debug_assert!(cols > 0 && cols < u16::MAX as i32, "cols: {cols}");
        debug_assert!(rows > 0 && rows < u16::MAX as i32, "rows: {rows}");

        (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (col, row)))
            .map(|(col, row)| Self { grid_xy: [col as u16, row as u16] })
            .collect()
    }
}

impl CellDynamic {

    #[rustfmt::skip]
    fn new(glyph_id: u16, fg: u32, bg: u32) -> Self {
        let mut data = [0; 8];

        data[0] = (glyph_id & 0xFF) as u8;
        data[1] = ((glyph_id >> 8) & 0xFF) as u8;

        data[2] = ((fg >> 16) & 0xFF) as u8; // R
        data[3] = ((fg >> 8) & 0xFF) as u8;  // G
        data[4] = ((fg) & 0xFF) as u8;       // B

        data[5] = ((bg >> 16) & 0xFF) as u8; // R
        data[6] = ((bg >> 8) & 0xFF) as u8;  // G
        data[7] = ((bg) & 0xFF) as u8;       // B

        Self { data }
    }
}

#[repr(C, align(16))] // std140 layout requires proper alignment
struct CellVertexUbo {
    pub projection: [f32; 16], // mat4
    pub cell_size: [f32; 2],   // vec2 - screen cell size
    pub _padding: [f32; 2],
}

#[repr(C, align(16))] // std140 layout requires proper alignment
struct CellFragmentUbo {
    pub padding_frac: [f32; 2], // padding as a fraction of cell size
    pub _padding: [f32; 2],
}

impl CellVertexUbo {
    pub const BINDING_POINT: u32 = 0;

    fn new(canvas_size: (i32, i32), cell_size: (i32, i32)) -> Self {
        let projection =
            Mat4::orthographic_from_size(canvas_size.0 as f32, canvas_size.1 as f32).data;
        Self {
            projection,
            cell_size: [cell_size.0 as f32, cell_size.1 as f32],
            _padding: [0.0; 2], // padding to ensure proper alignment
        }
    }
}

impl CellFragmentUbo {
    pub const BINDING_POINT: u32 = 1;

    fn new(cell_size: (i32, i32)) -> Self {
        Self {
            padding_frac: [
                FontAtlasData::PADDING as f32 / cell_size.0 as f32,
                FontAtlasData::PADDING as f32 / cell_size.1 as f32,
            ],
            _padding: [0.0; 2], // padding to ensure proper alignment
        }
    }
}

fn create_terminal_cell_data(cols: i32, rows: i32, fill_glyph: &[u16]) -> Vec<CellDynamic> {
    let glyph_len = fill_glyph.len();
    (0..cols * rows)
        .map(|i| {
            CellDynamic::new(
                fill_glyph[i as usize % glyph_len] | GlyphEffect::Underline as u16,
                0xffff_ff,
                0x0000_00,
            )
        })
        .collect()
}

mod attrib {
    pub const POS: u32 = 0;
    pub const UV: u32 = 1;

    pub const GRID_XY: u32 = 2;
    pub const PACKED_DEPTH_FG_BG: u32 = 3;
}
