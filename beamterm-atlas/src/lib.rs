//! Font atlas generation library for beamterm.

/// Font atlas generation and glyph rasterization orchestration.
pub mod atlas_generator;
/// Bitmap font output and serialization.
pub mod bitmap_font;
mod coordinate;
/// Pixel-accurate glyph bounds measurement.
pub mod glyph_bounds;
/// Glyph categorization into ASCII, Unicode, and emoji sets.
pub mod grapheme;
pub(crate) mod logging;
mod raster_config;
