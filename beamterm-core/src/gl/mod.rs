pub mod atlas;
mod buffer;
pub mod cell_query;
pub mod context;
pub mod glyph_cache;
mod program;
pub mod renderer;
pub mod selection;
pub mod static_atlas;
pub mod terminal_grid;
pub mod texture;
mod ubo;

// Primary API re-exports
pub use atlas::{Atlas, FontAtlas, GlyphSlot, GlyphTracker};
// Crate-internal re-exports
use buffer::*;
pub use cell_query::{CellIterator, CellQuery, SelectionMode, select};
pub use context::GlState;
pub(crate) use program::*;
pub use renderer::{Drawable, RenderContext};
pub use selection::SelectionTracker;
pub use static_atlas::StaticFontAtlas;
pub use terminal_grid::{CellData, CellDynamic, TerminalGrid};
