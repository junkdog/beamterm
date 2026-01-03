use beamterm_data::{FontAtlasData, FontStyle, Glyph};

use super::canvas_rasterizer::RasterizedGlyph;
use crate::{error::Error, gl::GL};

/// Number of glyphs stored per texture layer (1x32 vertical grid)
const GLYPHS_PER_LAYER: i32 = 32;

#[derive(Debug)]
pub(super) struct Texture {
    gl_texture: web_sys::WebGlTexture,
    pub(super) format: u32,
    /// Texture dimensions (width, height, layers)
    dimensions: (i32, i32, i32),
}

impl Texture {
    pub(super) fn from_font_atlas_data(
        gl: &web_sys::WebGl2RenderingContext,
        format: u32,
        atlas: &FontAtlasData,
    ) -> Result<Self, Error> {
        let (width, height, layers) = atlas.texture_dimensions;

        // prepare texture
        let gl_texture = gl
            .create_texture()
            .ok_or(Error::texture_creation_failed())?;
        gl.bind_texture(GL::TEXTURE_2D_ARRAY, Some(&gl_texture));
        gl.tex_storage_3d(GL::TEXTURE_2D_ARRAY, 1, GL::RGBA8, width, height, layers);

        // upload the texture data; convert to u8 array
        #[rustfmt::skip]
        gl.tex_sub_image_3d_with_opt_u8_array_and_src_offset(
            GL::TEXTURE_2D_ARRAY,
            0, // level
            0, 0, 0, // offset
            width, height, layers, // texture size
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            Some(&atlas.texture_data),
            0 // src offset
        ).map_err(|_| Error::texture_creation_failed())?;

        Self::setup_mipmap(gl);

        let (width, height, layers) = atlas.texture_dimensions;
        Ok(Self {
            gl_texture,
            format,
            dimensions: (width, height, layers),
        })
    }

    /// Creates an empty texture array for dynamic glyph rasterization.
    ///
    /// Allocates a fixed-size 2D texture array and initializes all layers to transparent
    /// black (RGBA 0,0,0,0).
    ///
    /// **LRU eviction**: When the glyph cache evicts old entries, the texture slots
    /// are reused. The new glyph completely overwrites the slot, so no explicit
    /// clearing is needed on eviction.
    ///
    /// # Arguments
    /// * `gl` - WebGL2 context
    /// * `format` - Texture format
    /// * `cell_size` - (width, height) of each glyph cell in pixels
    /// * `initial_layers` - Number of texture layers to allocate initially
    pub(super) fn for_dynamic_font_atlas(
        gl: &web_sys::WebGl2RenderingContext,
        format: u32,
        cell_size: (i32, i32),
        initial_layers: i32,
    ) -> Result<Self, Error> {
        let (cell_w, cell_h) = cell_size;

        // Each layer holds 32 glyphs in a 1x32 vertical grid
        // Match static atlas layout: single cell width per layer
        // (double-width glyphs like emoji use two consecutive glyph slots)
        let width = cell_w;
        let height = cell_h * GLYPHS_PER_LAYER;

        let gl_texture = gl
            .create_texture()
            .ok_or(Error::texture_creation_failed())?;

        gl.bind_texture(GL::TEXTURE_2D_ARRAY, Some(&gl_texture));
        gl.tex_storage_3d(
            GL::TEXTURE_2D_ARRAY,
            1, // mip levels
            GL::RGBA8,
            width,
            height,
            initial_layers,
        );

        // Initialize all layers to transparent black to prevent undefined memory artifacts.
        // See doc comment above for rationale. We upload all layers in a single call to
        // minimize GPU state changes (1 call vs 128 per-layer calls).
        let empty_data = vec![0u8; (width * height * initial_layers * 4) as usize];
        gl.tex_sub_image_3d_with_opt_u8_array_and_src_offset(
            GL::TEXTURE_2D_ARRAY,
            0,     // mip level
            0,     // x offset
            0,     // y offset
            0,     // z offset (first layer)
            width,
            height,
            initial_layers, // all layers at once
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            Some(&empty_data),
            0,
        )
        .map_err(|_| Error::texture_creation_failed())?;

        Self::setup_mipmap(gl);

        Ok(Self {
            gl_texture,
            format,
            dimensions: (width, height, initial_layers),
        })
    }

    /// Uploads a rasterized glyph to the texture at the position determined by its ID.
    ///
    /// Glyph positions follow the layout: layer = id / 32, y = (id % 32) * cell_height
    pub(super) fn upload_glyph(
        &self,
        gl: &web_sys::WebGl2RenderingContext,
        glyph_id: u16,
        padded_cell_size: (i32, i32),
        rasterized: &RasterizedGlyph,
    ) -> Result<(), Error> {
        let (cell_w, cell_h) = padded_cell_size;

        // Calculate position in texture array
        let layer = (glyph_id as i32) / GLYPHS_PER_LAYER;
        let glyph_index = (glyph_id as i32) % GLYPHS_PER_LAYER;
        let y_offset = glyph_index * cell_h;

        if layer >= self.dimensions.2 {
            return Err(Error::texture_creation_failed());
        }

        gl.bind_texture(GL::TEXTURE_2D_ARRAY, Some(&self.gl_texture));

        gl.tex_sub_image_3d_with_opt_u8_array_and_src_offset(
            GL::TEXTURE_2D_ARRAY,
            0, // level
            0,
            y_offset,
            layer, // x, y, z offset
            rasterized.width as i32,
            rasterized.height as i32,
            1, // depth (single layer)
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            Some(&rasterized.pixels),
            0, // src offset
        )
        .map_err(|_| Error::texture_creation_failed())?;

        Ok(())
    }

    /// Returns the texture dimensions (width, height, layers)
    pub(super) fn dimensions(&self) -> (i32, i32, i32) {
        self.dimensions
    }

    pub fn bind(&self, gl: &web_sys::WebGl2RenderingContext, texture_unit: u32) {
        // bind texture and set uniform
        gl.active_texture(GL::TEXTURE0 + texture_unit);
        gl.bind_texture(GL::TEXTURE_2D_ARRAY, Some(&self.gl_texture));
    }

    pub fn delete(&self, gl: &web_sys::WebGl2RenderingContext) {
        gl.delete_texture(Some(&self.gl_texture));
    }

    pub fn gl_texture(&self) -> &web_sys::WebGlTexture {
        &self.gl_texture
    }

    fn setup_mipmap(gl: &web_sys::WebGl2RenderingContext) {
        gl.generate_mipmap(GL::TEXTURE_2D_ARRAY);
        gl.tex_parameteri(
            GL::TEXTURE_2D_ARRAY,
            GL::TEXTURE_MIN_FILTER,
            GL::NEAREST as i32,
        );
        gl.tex_parameteri(
            GL::TEXTURE_2D_ARRAY,
            GL::TEXTURE_MAG_FILTER,
            GL::NEAREST as i32,
        );
        gl.tex_parameteri(GL::TEXTURE_2D_ARRAY, GL::TEXTURE_BASE_LEVEL, 0);
        gl.tex_parameteri(
            GL::TEXTURE_2D_ARRAY,
            GL::TEXTURE_WRAP_S,
            GL::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameteri(
            GL::TEXTURE_2D_ARRAY,
            GL::TEXTURE_WRAP_T,
            GL::CLAMP_TO_EDGE as i32,
        );
    }
}
