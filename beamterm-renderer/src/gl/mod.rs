#![allow(unused)]

mod atlas;
mod buffer;
pub(crate) mod canvas_rasterizer;
mod cell_query;
mod context;
mod context_loss;
mod dynamic_atlas;
mod glyph_cache;
mod program;
mod renderer;
mod selection;
mod static_atlas;
mod terminal_grid;
mod texture;
mod ubo;

pub use atlas::{FontAtlas, GlyphTracker};
use buffer::*;
pub use canvas_rasterizer::{CanvasRasterizer, RasterizedGlyph};
pub use cell_query::*;
pub(crate) use context_loss::ContextLossHandler;
pub use dynamic_atlas::DynamicFontAtlas;
pub(crate) use program::*;
pub use renderer::*;
pub use selection::*;
pub use static_atlas::StaticFontAtlas;
pub use terminal_grid::*;

pub(crate) type GL = web_sys::WebGl2RenderingContext;
