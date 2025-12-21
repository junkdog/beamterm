#![allow(unused)]

mod atlas;
mod buffer;
mod cell_query;
mod context;
mod context_loss;
mod program;
mod renderer;
mod selection;
mod terminal_grid;
mod texture;
mod ubo;

pub use atlas::FontAtlas;
use buffer::*;
pub use cell_query::*;
pub(crate) use context_loss::ContextLossHandler;
pub(crate) use program::*;
pub use renderer::*;
pub use selection::*;
pub use terminal_grid::*;

pub(crate) type GL = web_sys::WebGl2RenderingContext;
