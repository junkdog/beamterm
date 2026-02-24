mod canvas_rasterizer;
mod context_loss;
mod dynamic_atlas;
mod renderer;
mod selection;

// Re-export platform-agnostic types from beamterm-core
pub use beamterm_core::gl::{
    Atlas, CellData, CellIterator, CellQuery, Drawable, FontAtlas, GlyphSlot, GlyphTracker,
    RenderContext, SelectionMode, SelectionTracker, StaticFontAtlas, TerminalGrid, select,
};
// Web-specific exports
pub(crate) use context_loss::ContextLossHandler;
pub(crate) use dynamic_atlas::DynamicFontAtlas;
pub use renderer::Renderer;
pub(crate) use selection::TerminalMetrics;
