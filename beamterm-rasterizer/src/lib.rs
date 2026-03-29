//! Native font rasterization for beamterm.

mod error;
/// System font discovery and enumeration.
pub mod font_discovery;
mod font_fallback;
mod metrics;
mod rasterizer;

pub use error::Error;
pub use font_discovery::{FontDiscovery, FontFamily, FontVariants};
pub use metrics::CellMetrics;
pub use rasterizer::{NativeRasterizer, RasterizedGlyph};
