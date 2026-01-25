use std::{borrow::Cow, cmp::min, fmt::Debug, ops::Index};

use beamterm_data::{FontAtlasData, FontStyle, Glyph, GlyphEffect};
use compact_str::{CompactString, CompactStringExt};
use web_sys::{WebGl2RenderingContext, console};

use crate::{
    error::Error,
    gl::{
        CellIterator, CellQuery, Drawable, GL, RenderContext, ShaderProgram, StaticFontAtlas,
        atlas::{FontAtlas, GlyphSlot},
        buffer_upload_array,
        selection::SelectionTracker,
        ubo::UniformBufferObject,
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
    /// GPU resources (shader, buffers, UBOs) - recreated on context loss
    gpu: GpuResources,
    /// Terminal cell instance data
    cells: Vec<CellDynamic>,
    /// Terminal size in cells
    terminal_size: (u16, u16),
    /// Size of the canvas in pixels (physical)
    canvas_size_px: (i32, i32),
    /// Current device pixel ratio
    pixel_ratio: f32,
    /// Font atlas for rendering text.
    atlas: FontAtlas,
    /// Fallback glyph for missing symbols.
    fallback_glyph: u16,
    /// Selection tracker for managing cell selections.
    selection: SelectionTracker,
    /// Indicates whether there are cells pending flush to the GPU.
    cells_pending_flush: bool,
}

/// GPU resources that need to be recreated after a WebGL context loss.
///
/// This struct encapsulates all WebGL-dependent resources: shader program,
/// vertex buffers, uniform buffer objects, and uniform locations. These
/// resources become invalid after a context loss and must be recreated
/// with a fresh WebGL context.
#[derive(Debug)]
struct GpuResources {
    /// Shader program for rendering the terminal cells.
    shader: ShaderProgram,
    /// Buffers for the terminal grid (VAO, VBO, instance buffers)
    buffers: TerminalBuffers,
    /// Shared state for the vertex shader
    ubo_vertex: UniformBufferObject,
    /// Shared state for the fragment shader
    ubo_fragment: UniformBufferObject,
    /// Uniform location for the texture sampler.
    sampler_loc: web_sys::WebGlUniformLocation,
}

impl GpuResources {
    const FRAGMENT_GLSL: &'static str = include_str!("../shaders/cell.frag");
    const VERTEX_GLSL: &'static str = include_str!("../shaders/cell.vert");

    /// Creates all GPU resources for the terminal grid.
    ///
    /// This method creates and initializes:
    /// - Vertex Array Object (VAO)
    /// - Vertex and index buffers
    /// - Instance buffers for cell positions and data
    /// - Shader program
    /// - Uniform Buffer Objects (UBOs)
    fn new(
        gl: &WebGl2RenderingContext,
        cell_pos: &[CellStatic],
        cell_data: &[CellDynamic],
        cell_size: (i32, i32),
    ) -> Result<Self, Error> {
        // Create and setup the Vertex Array Object
        let vao = create_vao(gl)?;
        gl.bind_vertex_array(Some(&vao));

        // Create all buffers
        let buffers = setup_buffers(gl, vao, cell_pos, cell_data, cell_size)?;

        // Unbind VAO to prevent accidental modification
        gl.bind_vertex_array(None);

        // Setup shader and uniform data
        let shader = ShaderProgram::create(gl, Self::VERTEX_GLSL, Self::FRAGMENT_GLSL)?;
        shader.use_program(gl);

        let ubo_vertex = UniformBufferObject::new(gl, CellVertexUbo::BINDING_POINT)?;
        ubo_vertex.bind_to_shader(gl, &shader, "VertUbo")?;
        let ubo_fragment = UniformBufferObject::new(gl, CellFragmentUbo::BINDING_POINT)?;
        ubo_fragment.bind_to_shader(gl, &shader, "FragUbo")?;

        let sampler_loc = gl
            .get_uniform_location(&shader.program, "u_sampler")
            .ok_or(Error::uniform_location_failed("u_sampler"))?;

        Ok(Self {
            shader,
            buffers,
            ubo_vertex,
            ubo_fragment,
            sampler_loc,
        })
    }
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

    /// Updates the vertex buffer with new cell dimensions.
    fn update_vertex_buffer(&self, gl: &WebGl2RenderingContext, cell_size: (i32, i32)) {
        let (w, h) = (cell_size.0 as f32, cell_size.1 as f32);

        #[rustfmt::skip]
        let vertices: [f32; 16] = [
            //x    y    u    v
              w, 0.0, 1.0, 0.0, // top-right
            0.0,   h, 0.0, 1.0, // bottom-left
              w,   h, 1.0, 1.0, // bottom-right
            0.0, 0.0, 0.0, 0.0  // top-left
        ];

        gl.bind_vertex_array(Some(&self.vao));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.vertices));

        unsafe {
            let view = js_sys::Float32Array::view(&vertices);
            gl.buffer_sub_data_with_i32_and_array_buffer_view(GL::ARRAY_BUFFER, 0, &view);
        }

        gl.bind_vertex_array(None);
    }
}

impl TerminalGrid {
    pub(crate) fn new(
        gl: &WebGl2RenderingContext,
        atlas: FontAtlas,
        screen_size: (i32, i32),
        pixel_ratio: f32,
    ) -> Result<Self, Error> {
        let cell_scale = atlas.cell_scale_for_dpr(pixel_ratio);
        let base_cell_size = atlas.cell_size();
        let cell_size = (
            (base_cell_size.0 as f32 * cell_scale).round() as i32,
            (base_cell_size.1 as f32 * cell_scale).round() as i32,
        );
        let (cols, rows) = (screen_size.0 / cell_size.0, screen_size.1 / cell_size.1);

        let space_glyph = atlas.space_glyph_id();
        let cell_data = create_terminal_cell_data(cols, rows, space_glyph);
        let cell_pos = CellStatic::create_grid(cols, rows);

        let grid = Self {
            gpu: GpuResources::new(gl, &cell_pos, &cell_data, cell_size)?,
            terminal_size: (cols as u16, rows as u16),
            canvas_size_px: screen_size,
            pixel_ratio,
            cells: cell_data,
            atlas,
            fallback_glyph: space_glyph,
            selection: SelectionTracker::new(),
            cells_pending_flush: false,
        };

        grid.upload_ubo_data(gl);

        Ok(grid)
    }

    /// Returns the effective cell size for layout (base cell size * cell scale).
    fn effective_cell_size(&self) -> (i32, i32) {
        let cell_scale = self.atlas.cell_scale_for_dpr(self.pixel_ratio);
        let base = self.atlas.cell_size();
        (
            (base.0 as f32 * cell_scale).round() as i32,
            (base.1 as f32 * cell_scale).round() as i32,
        )
    }

    /// Sets the fallback glyph for missing characters.
    pub fn set_fallback_glyph(&mut self, fallback: &str) {
        self.fallback_glyph = self
            .atlas
            .get_glyph_id(fallback, FontStyle::Normal as u16)
            .unwrap_or(' ' as u16);
    }

    /// Replaces the current font atlas with a new one, translating all existing
    /// glyph IDs to the new atlas.
    ///
    /// This method handles the transition between atlases by:
    /// 1. Looking up the symbol for each existing glyph ID in the old atlas
    /// 2. Resolving the corresponding glyph slot in the new atlas
    /// 3. Updating double-width glyphs (emoji, wide chars) across both cells
    /// 4. Resizing the grid if cell dimensions changed
    ///
    /// # Parameters
    /// * `gl` - WebGL2 rendering context
    /// * `atlas` - The new font atlas to use
    pub(crate) fn replace_atlas(&mut self, gl: &WebGl2RenderingContext, atlas: FontAtlas) {
        let glyph_mask = self.atlas.base_lookup_mask() as u16;
        let style_mask = !glyph_mask;

        // update fallback glyph to new atlas, before translating existing cells
        self.fallback_glyph = self
            .atlas
            .get_symbol(self.fallback_glyph & glyph_mask)
            .and_then(|symbol| {
                let style_bits = self.fallback_glyph & style_mask;
                atlas.resolve_glyph_slot(symbol.as_str(), style_bits)
            })
            .map(|slot| slot.slot_id())
            .unwrap_or(atlas.space_glyph_id());

        // translate existing glyph ids to new atlas
        let mut skip_next = false;
        for idx in 0..self.cells.len() {
            if skip_next {
                skip_next = false;
                continue;
            }

            let old_glyph_id = self.cells[idx].glyph_id();
            let style_bits = old_glyph_id & style_mask;

            let slot = self
                .atlas
                .get_symbol(old_glyph_id & glyph_mask)
                .and_then(|symbol| atlas.resolve_glyph_slot(symbol.as_str(), style_bits));

            match slot {
                Some(GlyphSlot::Normal(id)) => {
                    self.cells[idx].set_glyph_id(id);
                },
                Some(GlyphSlot::Wide(id)) | Some(GlyphSlot::Emoji(id)) => {
                    self.cells[idx].set_glyph_id(id);
                    // update right-half in next cell if within bounds
                    if let Some(next_cell) = self.cells.get_mut(idx + 1) {
                        next_cell.set_glyph_id(id + 1);
                        skip_next = true;
                    }
                },
                None => {
                    self.cells[idx].set_glyph_id(self.fallback_glyph);
                },
            }
        }

        // clear any active selection, just to keep it simple
        self.selection.clear();

        // replace atlas and resize grid accordingly
        let old_atlas = std::mem::replace(&mut self.atlas, atlas);
        old_atlas.delete(gl);
        self.cells_pending_flush = true;

        // update vertex buffer with new cell dimensions
        self.gpu
            .buffers
            .update_vertex_buffer(gl, self.effective_cell_size());

        let _ = self.resize(gl, self.canvas_size_px, self.pixel_ratio);
    }

    /// Returns the [`FontAtlas`] used by this terminal grid.
    pub(crate) fn atlas(&self) -> &FontAtlas {
        &self.atlas
    }

    /// Returns a mutable reference to the font atlas.
    pub(crate) fn atlas_mut(&mut self) -> &mut FontAtlas {
        &mut self.atlas
    }

    /// Returns the canvas size in pixels.
    pub(crate) fn canvas_size(&self) -> (i32, i32) {
        self.canvas_size_px
    }

    /// Returns the effective cell dimensions in pixels (base size * cell scale).
    pub fn cell_size(&self) -> (i32, i32) {
        self.effective_cell_size()
    }

    /// Returns the size of the terminal grid in cells.
    pub fn terminal_size(&self) -> (u16, u16) {
        self.terminal_size
    }

    /// Returns a mutable reference to the cell data at the specified cell coordinates.
    pub fn cell_data_mut(&mut self, x: u16, y: u16) -> Option<&mut CellDynamic> {
        let (cols, _) = self.terminal_size;
        let idx = y as usize * cols as usize + x as usize;
        self.cells.get_mut(idx)
    }

    /// Returns the active selection state of the terminal grid.
    pub(crate) fn selection_tracker(&self) -> SelectionTracker {
        self.selection.clone()
    }

    /// Returns the symbols in the specified block range as a `CompactString`.
    pub(super) fn get_symbols(&self, selection: CellIterator) -> CompactString {
        let (cols, rows) = self.terminal_size;
        let mut text = CompactString::new("");

        for (idx, require_newline_after) in selection {
            let cell_symbol = self.get_cell_symbol(idx);
            if cell_symbol.is_some() {
                text.push_str(&cell_symbol.unwrap_or_default());
            }

            if require_newline_after {
                text.push('\n'); // add newline after each row
            }
        }

        text
    }

    pub(crate) fn hash_cells(&self, selection: CellQuery) -> u64 {
        use std::hash::{Hash, Hasher};

        use rustc_hash::FxHasher;

        let mut hasher = FxHasher::default();
        for (idx, _) in self.cell_iter(selection) {
            self.cells[idx].hash(&mut hasher);
        }

        hasher.finish()
    }

    fn get_cell_symbol(&self, idx: usize) -> Option<CompactString> {
        if idx < self.cells.len() {
            let glyph_id = self.cells[idx].glyph_id();
            let cell_symbol = self.atlas.get_symbol(glyph_id);
            if cell_symbol.is_some() {
                return cell_symbol;
            }
        }

        self.fallback_symbol()
    }

    /// Uploads uniform buffer data for screen and cell dimensions.
    ///
    /// This method updates the shader uniform buffers with the current screen
    /// size and cell dimensions. Must be called when the screen size changes
    /// or when initializing the grid.
    ///
    /// # Parameters
    /// * `gl` - WebGL2 rendering context
    fn upload_ubo_data(&self, gl: &WebGl2RenderingContext) {
        let vertex_ubo = CellVertexUbo::new(self.canvas_size_px, self.effective_cell_size());
        self.gpu.ubo_vertex.upload_data(gl, &vertex_ubo);

        let fragment_ubo = CellFragmentUbo::new(&self.atlas);
        self.gpu
            .ubo_fragment
            .upload_data(gl, &fragment_ubo);
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

        let fallback_glyph = GlyphSlot::Normal(self.fallback_glyph);

        // handle double-width emoji that span two cells
        let mut pending_cell: Option<CellDynamic> = None;
        self.cells
            .iter_mut()
            .zip(cells)
            .for_each(|(cell, data)| {
                let glyph = atlas
                    .resolve_glyph_slot(data.symbol, data.style_bits)
                    .unwrap_or(fallback_glyph);

                *cell = if let Some(second_cell) = pending_cell.take() {
                    second_cell
                } else {
                    match glyph {
                        GlyphSlot::Normal(id) => CellDynamic::new(id, data.fg, data.bg),

                        GlyphSlot::Wide(id) | GlyphSlot::Emoji(id) => {
                            // storing a double-width glyph, reserve next cell with right-half id
                            pending_cell = Some(CellDynamic::new(id + 1, data.fg, data.bg));
                            CellDynamic::new(id, data.fg, data.bg)
                        },
                    }
                }
            });

        self.cells_pending_flush = true;
        Ok(())
    }

    pub(crate) fn update_cells_by_position<'a>(
        &mut self,
        cells: impl Iterator<Item = (u16, u16, CellData<'a>)>,
    ) -> Result<(), Error> {
        let cols = self.terminal_size.0 as usize;
        let cells_by_index = cells.map(|(x, y, data)| (y as usize * cols + x as usize, data));

        self.update_cells_by_index(cells_by_index)
    }

    pub(crate) fn update_cells_by_index<'a>(
        &mut self,
        cells: impl Iterator<Item = (usize, CellData<'a>)>,
    ) -> Result<(), Error> {
        // update instance buffer with new cell data by position
        let atlas = &self.atlas;

        let cell_count = self.cells.len();
        let fallback_glyph = GlyphSlot::Normal(self.fallback_glyph);

        // ratatui and beamterm can disagree on which emoji
        // are double-width (beamterm assumes double-width for all emoji),
        // so for ratatui and similar clients we need to skip the next cell
        // if we just wrote a double-width emoji in the current cell.
        let mut skip_idx = None;

        cells
            .filter(|(idx, _)| *idx < cell_count)
            .for_each(|(idx, cell)| {
                if skip_idx.take() == Some(idx) {
                    // skip this cell, already handled as part of previous double-width emoji
                    return;
                }

                let glyph = atlas
                    .resolve_glyph_slot(cell.symbol, cell.style_bits)
                    .unwrap_or(fallback_glyph);

                match glyph {
                    GlyphSlot::Normal(id) => {
                        self.cells[idx] = CellDynamic::new(id, cell.fg, cell.bg);
                    },

                    GlyphSlot::Wide(id) | GlyphSlot::Emoji(id) => {
                        // render left half in current cell
                        self.cells[idx] = CellDynamic::new(id, cell.fg, cell.bg);

                        // render right half in next cell, if within bounds
                        if let Some(c) = self.cells.get_mut(idx + 1) {
                            *c = CellDynamic::new(id + 1, cell.fg, cell.bg);
                            skip_idx = Some(idx + 1);
                        }
                    },
                }
            });

        self.cells_pending_flush = true;

        Ok(())
    }

    pub(crate) fn update_cell(&mut self, x: u16, y: u16, cell_data: CellData) -> Result<(), Error> {
        let (cols, _) = self.terminal_size;
        let idx = y as usize * cols as usize + x as usize;
        self.update_cell_by_index(idx, cell_data)
    }

    pub(crate) fn update_cell_by_index(
        &mut self,
        idx: usize,
        cell_data: CellData,
    ) -> Result<(), Error> {
        self.update_cells_by_index(std::iter::once((idx, cell_data)))
    }

    /// Flushes pending cell updates to the GPU.
    pub(crate) fn flush_cells(&mut self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        if !self.cells_pending_flush {
            return Ok(()); // no pending updates to flush
        }

        // if there is an active selected region with a content hash,
        // check if the underlying content has changed; if so, clear the selection
        self.clear_stale_selection();

        // If there's an active selection, flip the colors of the selected cells.
        // This ensures that the selected cells are rendered with inverted colors
        // during the GPU upload process.
        self.flip_selected_cell_colors();

        self.gpu
            .buffers
            .upload_instance_data(gl, &self.cells);

        // Restore the original colors of the selected cells after the upload.
        // This ensures that the internal state of the cells remains consistent.
        self.flip_selected_cell_colors();

        self.cells_pending_flush = false;
        Ok(())
    }

    fn flip_selected_cell_colors(&mut self) {
        if let Some(iter) = self.selected_cells_iter() {
            iter.for_each(|(idx, _)| self.cells[idx].flip_colors());
        }
    }

    fn selected_cells_iter(&self) -> Option<CellIterator> {
        self.selection
            .get_query()
            .map(|query| self.cell_iter(query))
    }

    fn flip_cell_colors(&mut self, x: u16, y: u16) {
        let (cols, _) = self.terminal_size;
        let idx = y as usize * cols as usize + x as usize;
        if idx < self.cells.len() {
            self.cells[idx].flip_colors();
        }
    }

    /// Resizes the terminal grid to fit the new canvas dimensions.
    ///
    /// This method recalculates the terminal dimensions based on the canvas size and cell
    /// dimensions, then recreates the necessary GPU buffers if the grid size changed.
    /// Existing cell content is preserved where possible during resizing.
    ///
    /// # Parameters
    ///
    /// * `gl` - WebGL2 rendering context
    /// * `canvas_size` - New canvas dimensions in pixels as `(width, height)`
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully resized the terminal
    /// * `Err(Error)` - Failed to recreate buffers or other WebGL error
    pub fn resize(
        &mut self,
        gl: &WebGl2RenderingContext,
        canvas_size: (i32, i32),
        pixel_ratio: f32,
    ) -> Result<(), Error> {
        self.canvas_size_px = canvas_size;
        self.pixel_ratio = pixel_ratio;

        let cell_size = self.effective_cell_size();

        // Update vertex buffer with new cell dimensions
        self.gpu
            .buffers
            .update_vertex_buffer(gl, cell_size);

        // Update the UBO with new screen size
        self.upload_ubo_data(gl);

        let cols = canvas_size.0 / cell_size.0;
        let rows = canvas_size.1 / cell_size.1;
        if self.terminal_size == (cols as u16, rows as u16) {
            return Ok(()); // no change in terminal size
        }

        // update buffers; bind VAO to ensure correct state
        gl.bind_vertex_array(Some(&self.gpu.buffers.vao));

        // delete old cell instance buffers
        gl.delete_buffer(Some(&self.gpu.buffers.instance_cell));
        gl.delete_buffer(Some(&self.gpu.buffers.instance_pos));

        // resize cell data vector
        let current_size = (self.terminal_size.0 as i32, self.terminal_size.1 as i32);
        let cell_data = self.resize_cell_grid(current_size, (cols, rows));
        self.cells = cell_data;

        let cell_pos = CellStatic::create_grid(cols, rows);

        // re-create buffers with new data
        self.gpu.buffers.instance_cell = create_dynamic_instance_buffer(gl, &self.cells)?;
        self.gpu.buffers.instance_pos = create_static_instance_buffer(gl, &cell_pos)?;

        // unbind VAO
        gl.bind_vertex_array(None);

        self.terminal_size = (cols as u16, rows as u16);

        Ok(())
    }

    /// Recreates all GPU resources after a WebGL context loss.
    ///
    /// This method rebuilds all GPU-side resources (VAO, buffers, shaders, UBOs)
    /// while preserving the current cell data and terminal state. Call this after
    /// obtaining a new WebGL context following a context loss event.
    ///
    /// # Parameters
    /// * `gl` - The new WebGL2 rendering context
    ///
    /// # Returns
    /// * `Ok(())` - All resources successfully recreated
    /// * `Err(Error)` - Failed to recreate one or more resources
    ///
    /// # Note
    /// The font atlas texture must be recreated separately via
    /// [`StaticFontAtlas::recreate_texture`] before calling this method.
    pub fn recreate_resources(&mut self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        let cell_size = self.effective_cell_size();
        let (cols, rows) = (self.terminal_size.0 as i32, self.terminal_size.1 as i32);
        let cell_pos = CellStatic::create_grid(cols, rows);

        // Recreate all GPU resources
        self.gpu = GpuResources::new(gl, &cell_pos, &self.cells, cell_size)?;

        // Upload UBO data
        self.upload_ubo_data(gl);

        // Mark cells as needing flush to upload to new buffers
        self.cells_pending_flush = true;

        Ok(())
    }

    /// Recreates the font atlas texture after a WebGL context loss.
    ///
    /// This is a convenience method that delegates to [`StaticFontAtlas::recreate_texture`].
    /// Call this before [`recreate_resources`] when recovering from context loss.
    pub fn recreate_atlas_texture(&mut self, gl: &WebGl2RenderingContext) -> Result<(), Error> {
        self.atlas.recreate_texture(gl)
    }

    /// Returns the base glyph identifier for a given symbol.
    pub fn base_glyph_id(&self, symbol: &str) -> Option<u16> {
        self.atlas.get_base_glyph_id(symbol)
    }

    fn fallback_symbol(&self) -> Option<CompactString> {
        self.atlas.get_symbol(self.fallback_glyph)
    }

    fn clear_stale_selection(&self) {
        if let Some(query) = self.selection_tracker().get_query()
            && let Some(hash) = query.content_hash
            && hash != self.hash_cells(query)
        {
            self.selection.clear();
        }
    }

    fn resize_cell_grid(&self, old_size: (i32, i32), new_size: (i32, i32)) -> Vec<CellDynamic> {
        let empty_cell = CellDynamic::new(self.atlas.space_glyph_id(), 0xFFFFFF, 0x000000);

        let new_len = new_size.0 * new_size.1;
        let mut new_cells = Vec::with_capacity(new_len as usize);
        for _ in 0..new_len {
            new_cells.push(empty_cell);
        }

        let cells = &self.cells;
        for y in 0..min(old_size.1, new_size.1) {
            for x in 0..min(old_size.0, new_size.0) {
                let new_idx = (y * new_size.0 + x) as usize;
                let old_idx = (y * old_size.0 + x) as usize;
                new_cells[new_idx] = cells[old_idx];
            }
        }

        new_cells
    }
}

fn create_vao(gl: &WebGl2RenderingContext) -> Result<web_sys::WebGlVertexArrayObject, Error> {
    gl.create_vertex_array()
        .ok_or(Error::vertex_array_creation_failed())
}

fn setup_buffers(
    gl: &WebGl2RenderingContext,
    vao: web_sys::WebGlVertexArrayObject,
    cell_pos: &[CellStatic],
    cell_data: &[CellDynamic],
    cell_size: (i32, i32),
) -> Result<TerminalBuffers, Error> {
    let (w, h) = (cell_size.0 as f32, cell_size.1 as f32);

    #[rustfmt::skip]
    let vertices = [
        //x    y    u    v
          w, 0.0, 1.0, 0.0, // top-right
        0.0,   h, 0.0, 1.0, // bottom-left
          w,   h, 1.0, 1.0, // bottom-right
        0.0, 0.0, 0.0, 0.0  // top-left
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
    let index_buf = gl
        .create_buffer()
        .ok_or(Error::buffer_creation_failed("vbo-u8"))?;
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
    let buffer = gl
        .create_buffer()
        .ok_or(Error::buffer_creation_failed("vbo-f32"))?;

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
    enable_vertex_attrib_array(
        gl,
        attrib::PACKED_DEPTH_FG_BG,
        2,
        GL::UNSIGNED_INT,
        0,
        stride,
    );

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

        self.gpu.shader.use_program(gl);

        gl.bind_vertex_array(Some(&self.gpu.buffers.vao));

        self.atlas.bind(gl, 0);
        self.atlas.flush(gl).unwrap(); // fixme: handle error
        self.gpu.ubo_vertex.bind(context.gl);
        self.gpu.ubo_fragment.bind(context.gl);
        gl.uniform1i(Some(&self.gpu.sampler_loc), 0);
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

        self.gpu.ubo_vertex.unbind(gl);
        self.gpu.ubo_fragment.unbind(gl);
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
#[derive(Debug, Copy, Clone)]
pub struct CellData<'a> {
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
        Self::new_with_style_bits(symbol, style.style_mask() | effect as u16, fg, bg)
    }

    /// Creates new cell data with pre-encoded style bits.
    ///
    /// This is a lower-level constructor that accepts pre-encoded style bits rather than
    /// separate `FontStyle` and `GlyphEffect` parameters. Use this when you have already
    /// combined the style flags or when working directly with the bit representation.
    ///
    /// # Parameters
    /// * `symbol` - Character to display
    /// * `style_bits` - Pre-encoded style flags. Must not overlap with base glyph ID bits (0x01FF).
    ///   Valid bits include:
    ///   - `0x0200` - Bold
    ///   - `0x0400` - Italic
    ///   - `0x0800` - Emoji (set automatically by the renderer for emoji glyphs)
    ///   - `0x1000` - Underline
    ///   - `0x2000` - Strikethrough
    /// * `fg` - Foreground color as RGB value (0xRRGGBB)
    /// * `bg` - Background color as RGB value (0xRRGGBB)
    ///
    /// # Returns
    /// New `CellData` instance
    ///
    /// # Panics
    /// Debug builds will panic if `style_bits` contains any invalid bits.
    pub fn new_with_style_bits(symbol: &'a str, style_bits: u16, fg: u32, bg: u32) -> Self {
        // emoji and glyph base mask should not intersect with style bits
        debug_assert!(
            0x81FF & style_bits == 0,
            "Invalid style bits: {style_bits:#04x}"
        );
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
#[derive(Debug, Clone, Copy, Hash)]
#[repr(C, align(4))]
pub struct CellDynamic {
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
    data: [u8; 8], // 2b layer, fg:rgb, bg:rgb
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
    const GLYPH_STYLE_MASK: u16 =
        Glyph::BOLD_FLAG | Glyph::ITALIC_FLAG | Glyph::UNDERLINE_FLAG | Glyph::STRIKETHROUGH_FLAG;

    #[inline]
    pub fn new(glyph_id: u16, fg: u32, bg: u32) -> Self {
        let mut data = [0; 8];

        // pack glyph ID into the first two bytes
        let glyph_id = glyph_id.to_le_bytes();
        data[0] = glyph_id[0];
        data[1] = glyph_id[1];

        let fg = fg.to_le_bytes();
        data[2] = fg[2]; // R
        data[3] = fg[1]; // G
        data[4] = fg[0]; // B

        let bg = bg.to_le_bytes();
        data[5] = bg[2]; // R
        data[6] = bg[1]; // G
        data[7] = bg[0]; // B

        Self { data }
    }

    /// Overwrites the current cell style bits with the provided style bits.
    pub fn style(&mut self, style_bits: u16) {
        let glyph_id = (self.glyph_id() & !Self::GLYPH_STYLE_MASK) | style_bits;
        self.data[..2].copy_from_slice(&glyph_id.to_le_bytes());
    }

    /// Sets the foreground color of the cell.
    pub fn flip_colors(&mut self) {
        // swap foreground and background colors
        let fg = [self.data[2], self.data[3], self.data[4]];
        self.data[2] = self.data[5]; // R
        self.data[3] = self.data[6]; // G
        self.data[4] = self.data[7]; // B
        self.data[5] = fg[0]; // R
        self.data[6] = fg[1]; // G
        self.data[7] = fg[2]; // B
    }

    /// Sets the foreground color of the cell.
    pub fn fg_color(&mut self, fg: u32) {
        let fg = fg.to_le_bytes();
        self.data[2] = fg[2]; // R
        self.data[3] = fg[1]; // G
        self.data[4] = fg[0]; // B
    }

    /// Sets the background color of the cell.
    pub fn bg_color(&mut self, bg: u32) {
        let bg = bg.to_le_bytes();
        self.data[5] = bg[2]; // R
        self.data[6] = bg[1]; // G
        self.data[7] = bg[0]; // B
    }

    /// Returns foreground color as a packed RGB value.
    pub fn get_fg_color(&self) -> u32 {
        // unpack foreground color from data
        ((self.data[2] as u32) << 16) | ((self.data[3] as u32) << 8) | (self.data[4] as u32)
    }

    /// Returns background color as a packed RGB value.
    pub fn get_bg_color(&self) -> u32 {
        // unpack background color from data
        ((self.data[5] as u32) << 16) | ((self.data[6] as u32) << 8) | (self.data[7] as u32)
    }

    /// Returns the style bits for this cell, excluding id and emoji bits.
    pub fn get_style(&self) -> u16 {
        self.glyph_id() & Self::GLYPH_STYLE_MASK
    }

    /// Returns true if the glyph is an emoji.
    pub fn is_emoji(&self) -> bool {
        self.glyph_id() & Glyph::EMOJI_FLAG != 0
    }

    #[inline]
    fn glyph_id(&self) -> u16 {
        u16::from_le_bytes([self.data[0], self.data[1]])
    }

    fn set_glyph_id(&mut self, glyph_id: u16) {
        let bytes = glyph_id.to_le_bytes();
        self.data[0] = bytes[0];
        self.data[1] = bytes[1];
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
    pub padding_frac: [f32; 2],       // padding as a fraction of cell size
    pub underline_pos: f32,           // underline position (0.0 = top, 1.0 = bottom)
    pub underline_thickness: f32,     // underline thickness as fraction of cell height
    pub strikethrough_pos: f32,       // strikethrough position (0.0 = top, 1.0 = bottom)
    pub strikethrough_thickness: f32, // strikethrough thickness as fraction of cell height
    pub texture_lookup_mask: u32,     // static atlas: 0x1FFF, dynamic atlas: 0x0FFF
    pub _padding: f32,
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

    fn new(atlas: &FontAtlas) -> Self {
        // Use texture cell size for padding calculation (physical pixels in texture)
        let texture_cell_size = atlas.texture_cell_size();
        let underline = atlas.underline();
        let strikethrough = atlas.strikethrough();

        Self {
            padding_frac: [
                FontAtlasData::PADDING as f32 / texture_cell_size.0 as f32,
                FontAtlasData::PADDING as f32 / texture_cell_size.1 as f32,
            ],
            underline_pos: underline.position,
            underline_thickness: underline.thickness,
            strikethrough_pos: strikethrough.position,
            strikethrough_thickness: strikethrough.thickness,
            texture_lookup_mask: atlas.base_lookup_mask(),
            _padding: 0.0, // padding to ensure proper alignment
        }
    }
}

fn create_terminal_cell_data(cols: i32, rows: i32, fill_glyph: u16) -> Vec<CellDynamic> {
    (0..cols * rows)
        .map(|i| CellDynamic::new(fill_glyph, 0x00ff_ffff, 0x0000_0000))
        .collect()
}

mod attrib {
    pub const POS: u32 = 0;
    pub const UV: u32 = 1;

    pub const GRID_XY: u32 = 2;
    pub const PACKED_DEPTH_FG_BG: u32 = 3;
}
