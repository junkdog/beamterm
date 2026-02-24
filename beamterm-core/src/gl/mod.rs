pub(crate) mod atlas;
mod buffer;
pub(crate) mod cell_query;
pub(crate) mod context;
pub(crate) mod glyph_cache;
mod program;
pub(crate) mod renderer;
pub(crate) mod selection;
pub(crate) mod static_atlas;
pub(crate) mod terminal_grid;
pub(crate) mod texture;
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

// Re-exports for sibling crates (beamterm-renderer)
pub use atlas::DYNAMIC_ATLAS_LOOKUP_MASK;
pub use glyph_cache::{ASCII_SLOTS, GlyphCache};
pub use texture::{RasterizedGlyph, Texture};
