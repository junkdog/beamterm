pub(crate) mod atlas;
mod buffer;
pub(crate) mod cell_query;
pub(crate) mod context;
mod dirty_regions;
pub(crate) mod dynamic_atlas;
pub(crate) mod glyph_cache;
pub(crate) mod glyph_rasterizer;
#[cfg(feature = "native-dynamic-atlas")]
mod native_dynamic_atlas;
mod program;
pub(crate) mod renderer;
pub(crate) mod selection;
pub(crate) mod static_atlas;
pub(crate) mod terminal_grid;
pub(crate) mod texture;
mod ubo;

// Primary API re-exports
// Re-exports for sibling crates (beamterm-renderer)
pub use atlas::{Atlas, FontAtlas, GlyphSlot, GlyphTracker, sealed};
// Crate-internal re-exports
use buffer::*;
pub use cell_query::{CellIterator, CellQuery, SelectionMode, select};
pub use context::GlState;
/// internal type; not covered by semver guarantees.
/// Use [`Terminal::builder()`](beamterm_renderer::Terminal::builder) or the
/// [`NativeDynamicAtlas`] type alias instead.
#[doc(hidden)]
pub use dynamic_atlas::DynamicFontAtlas;
/// Internal trait — not covered by semver guarantees.
/// Use pre-built implementations ([`NativeGlyphRasterizer`] or `CanvasRasterizer`) instead.
#[doc(hidden)]
pub use glyph_rasterizer::GlyphRasterizer;
#[cfg(feature = "native-dynamic-atlas")]
pub use native_dynamic_atlas::{NativeDynamicAtlas, NativeGlyphRasterizer};
pub(crate) use program::*;
pub use renderer::{Drawable, RenderContext};
pub use selection::SelectionTracker;
pub use static_atlas::StaticFontAtlas;
pub use terminal_grid::{CellData, CellDynamic, TerminalGrid};
/// internal type; not covered by semver guarantees.
#[doc(hidden)]
pub use texture::RasterizedGlyph;
