//! Ligature-aware text shaping via [`rustybuzz`] (a pure-Rust HarfBuzz port).
//!
//! This module is used purely to *detect* ligature clusters so the renderer can
//! treat a multi-character ligature (e.g. `=>`, `===`, `<==>`) as a single glyph
//! spanning multiple terminal cells. The actual pixel rasterization is still
//! performed by the platform rasterizer (the browser canvas in the WASM path),
//! which re-shapes the same byte sequence using the same font and therefore
//! produces the matching ligature glyph.
//!
//! Programming ligatures in fonts such as Fira Code, JetBrains Mono, Cascadia
//! Code and Monaspace Neon are implemented mostly through the OpenType `calt`
//! (contextual alternates) feature rather than plain `liga`, so a static table
//! read is insufficient — full shaping is required to find them.

use std::{cell::RefCell, num::NonZeroUsize};

use lru::LruCache;
use rustybuzz::{Face, Feature, UnicodeBuffer, ttf_parser::Tag};

/// Maximum number of cells a single ligature may span.
///
/// Runs that the font would ligate into a wider cluster are left un-ligated and
/// rendered per cell. This bounds the texture-slot span allocated per glyph.
pub const MAX_LIGATURE_CELLS: u8 = 8;

const LIGA: Tag = Tag::from_bytes(b"liga");
const CALT: Tag = Tag::from_bytes(b"calt");

/// A contiguous shaped segment of a text run.
///
/// `start`/`len` are byte offsets into the run that was passed to
/// [`Shaper::segment`]. A segment with `cells > 1` and `ligated == true`
/// represents a ligature that should be rasterized as one multi-cell glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Byte offset into the shaped run where this segment starts.
    pub start: usize,
    /// Byte length of this segment.
    pub len: usize,
    /// Number of source cells (= source characters) this segment covers.
    pub cells: u8,
    /// True when several source characters collapse into a single ligature glyph.
    pub ligated: bool,
}

/// Errors produced while constructing a [`Shaper`].
#[derive(Debug, thiserror::Error)]
pub enum ShaperError {
    /// The supplied bytes are a WOFF/WOFF2 container and must be decompressed
    /// to raw TrueType/OpenType (sfnt) before being passed to the shaper.
    #[error("compressed font (WOFF/WOFF2) is not supported; decompress to sfnt first")]
    CompressedFont,
    /// The supplied bytes could not be parsed as a font face.
    #[error("failed to parse font face from the supplied bytes")]
    ParseFailed,
}

/// Upper bound on distinct text runs whose segmentation is memoized.
///
/// Generously covers a screenful of distinct runs (rows × per-row style spans)
/// plus churn from a moving cursor line; entries are tiny (`Vec<Segment>`).
const SEGMENT_CACHE_CAP: usize = 1024;

/// Detects ligature clusters for a single font using rustybuzz.
///
/// Owns the raw font bytes; the borrowing [`Face`] is constructed transiently
/// for each *uncached* shaping call. Results are memoized per run text (see
/// [`Shaper::segment`]) because the renderer re-shapes the whole screen every
/// frame and the vast majority of runs are unchanged frame-to-frame.
pub struct Shaper {
    font_data: Box<[u8]>,
    face_index: u32,
    has_ligatures: bool,
    /// run text → segmentation. Keyed on text alone: segmentation depends only
    /// on the characters and the font, and a font change builds a new `Shaper`
    /// (hence a fresh cache), so no explicit invalidation is needed.
    cache: RefCell<LruCache<String, Vec<Segment>>>,
}

impl Shaper {
    /// Builds a shaper from raw sfnt (TrueType/OpenType) font bytes.
    ///
    /// # Errors
    /// Returns [`ShaperError::CompressedFont`] for WOFF/WOFF2 input and
    /// [`ShaperError::ParseFailed`] if the bytes are not a valid font face.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ShaperError> {
        if data.len() >= 4 && (data[0..4] == *b"wOFF" || data[0..4] == *b"wOF2") {
            return Err(ShaperError::CompressedFont);
        }

        let font_data: Box<[u8]> = Box::from(data);
        let has_ligatures = {
            let face = Face::from_slice(&font_data, 0).ok_or(ShaperError::ParseFailed)?;
            face_has_ligature_features(&face)
        };

        let cache = RefCell::new(LruCache::new(
            NonZeroUsize::new(SEGMENT_CACHE_CAP).expect("cache cap is non-zero"),
        ));

        Ok(Self { font_data, face_index: 0, has_ligatures, cache })
    }

    /// Returns true if the font advertises `liga` or `calt` substitutions.
    ///
    /// When false, callers can skip shaping entirely since no ligatures form.
    #[must_use]
    pub fn has_ligatures(&self) -> bool {
        self.has_ligatures
    }

    /// Segments a text run into ligated and non-ligated spans.
    ///
    /// The returned segments cover `text` left-to-right with no gaps. Single-cell
    /// segments (`cells == 1`) should be rendered glyph-by-glyph as before; multi-
    /// cell ligated segments should be rasterized as one wide glyph.
    ///
    /// Detection works for both ligature implementations used by programming
    /// fonts: classic GSUB ligature substitution (which reduces the glyph count,
    /// merging clusters) and the `calt` "spacer" approach used by Fira Code /
    /// JetBrains Mono / Cascadia (which keeps the glyph count equal to the
    /// character count but swaps each glyph for a ligature piece). A ligature is
    /// a maximal run of two or more consecutive characters whose glyphs were
    /// altered from their nominal `cmap` mapping (or merged).
    ///
    /// Ligatures wider than [`MAX_LIGATURE_CELLS`] are decomposed into single-cell
    /// segments.
    ///
    /// Results are memoized per run text. Building a [`Face`] and running the
    /// shaper for every run on every frame dominates render time on a static
    /// screen; the cache turns repeated runs into an `O(len)` map lookup.
    #[must_use]
    pub fn segment(&self, text: &str) -> Vec<Segment> {
        if text.is_empty() {
            return Vec::new();
        }

        if let Some(cached) = self.cache.borrow_mut().get(text) {
            return cached.clone();
        }

        let segments = self.segment_uncached(text);
        self.cache
            .borrow_mut()
            .put(text.to_string(), segments.clone());
        segments
    }

    /// Performs the actual rustybuzz shaping for a run (the cache miss path).
    fn segment_uncached(&self, text: &str) -> Vec<Segment> {
        let Some(face) = Face::from_slice(&self.font_data, self.face_index) else {
            return per_char_segments(text);
        };

        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let n = chars.len();

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();

        let features = [Feature::new(LIGA, 1, ..), Feature::new(CALT, 1, ..)];
        let glyphs = rustybuzz::shape(&face, &features, buffer);
        let infos = glyphs.glyph_infos();

        // Non-LTR / reordered runs break the monotonic-cluster assumption below;
        // programming-ligature runs are always LTR, but guard anyway.
        let monotonic = infos
            .windows(2)
            .all(|w| w[0].cluster <= w[1].cluster);
        if !monotonic {
            return per_char_segments(text);
        }

        // Mark which source characters were altered or merged by shaping.
        let mut altered = vec![false; n];
        for (i, info) in infos.iter().enumerate() {
            let start_byte = info.cluster as usize;
            let end_byte = infos
                .get(i + 1)
                .map_or(text.len(), |g| g.cluster as usize);

            let Some(ci) = chars.iter().position(|&(b, _)| b == start_byte) else {
                continue;
            };
            let covered = chars[ci..]
                .iter()
                .take_while(|&&(b, _)| b < end_byte)
                .count()
                .max(1);

            if covered > 1 {
                // classic ligature merge: every covered character participates
                for slot in altered.iter_mut().skip(ci).take(covered) {
                    *slot = true;
                }
            } else {
                // 1:1 glyph — altered if it differs from the nominal cmap glyph
                let nominal = face
                    .glyph_index(chars[ci].1)
                    .map(|g| u32::from(g.0));
                if Some(info.glyph_id) != nominal {
                    altered[ci] = true;
                }
            }
        }

        build_segments(&chars, &altered, text.len())
    }
}

/// Groups maximal runs of altered characters into ligature segments.
///
/// Runs of length 2..=[`MAX_LIGATURE_CELLS`] become a single ligature segment;
/// everything else is emitted as one single-cell segment per character.
fn build_segments(chars: &[(usize, char)], altered: &[bool], text_len: usize) -> Vec<Segment> {
    let n = chars.len();
    let mut segments = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if altered[i] {
            let mut j = i + 1;
            while j < n && altered[j] {
                j += 1;
            }
            let run = j - i;
            if (2..=MAX_LIGATURE_CELLS as usize).contains(&run) {
                let start = chars[i].0;
                let end = if j < n { chars[j].0 } else { text_len };
                segments.push(Segment {
                    start,
                    len: end - start,
                    cells: run as u8,
                    ligated: true,
                });
                i = j;
                continue;
            }
        }

        let start = chars[i].0;
        let end = if i + 1 < n { chars[i + 1].0 } else { text_len };
        segments.push(Segment { start, len: end - start, cells: 1, ligated: false });
        i += 1;
    }
    segments
}

/// Returns true if the face's GSUB table exposes `liga` or `calt` features.
fn face_has_ligature_features(face: &Face<'_>) -> bool {
    let Some(gsub) = face.tables().gsub else {
        return false;
    };
    gsub.features
        .into_iter()
        .any(|f| f.tag == LIGA || f.tag == CALT)
}

/// One single-cell segment per character of `text`.
fn per_char_segments(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    push_per_char(&mut segments, text, 0, text.len());
    segments
}

/// Appends one single-cell segment per character of `text[start..end]`.
fn push_per_char(segments: &mut Vec<Segment>, text: &str, start: usize, end: usize) {
    let mut offset = start;
    for ch in text[start..end].chars() {
        let len = ch.len_utf8();
        segments.push(Segment { start: offset, len, cells: 1, ligated: false });
        offset += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_woff2() {
        let bytes = b"wOF2\x00\x00\x00\x00";
        assert!(matches!(
            Shaper::from_bytes(bytes),
            Err(ShaperError::CompressedFont)
        ));
    }

    #[test]
    fn rejects_garbage() {
        let bytes = b"not a font at all";
        assert!(matches!(
            Shaper::from_bytes(bytes),
            Err(ShaperError::ParseFailed)
        ));
    }

    #[test]
    fn per_char_segments_cover_text() {
        let segs = per_char_segments("a=>b");
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| s.cells == 1 && !s.ligated));
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[3].start, 3);
    }

    /// The memoized `segment()` returns results identical to the uncached path,
    /// on both the first (miss) and second (hit) call. Requires a ligature font.
    #[test]
    fn cache_returns_identical_segments() {
        let Ok(path) = std::env::var("BEAMTERM_LIGATURE_TEST_FONT") else {
            return;
        };
        let bytes = std::fs::read(path).expect("read test font");
        let shaper = Shaper::from_bytes(&bytes).expect("parse test font");

        for text in ["a => b", "x != y", "plain text", "let v = vec![];"] {
            let uncached = shaper.segment_uncached(text);
            let first = shaper.segment(text); // miss → populates cache
            let second = shaper.segment(text); // hit → from cache
            assert_eq!(first, uncached, "miss diverges from uncached for {text:?}");
            assert_eq!(second, uncached, "hit diverges from uncached for {text:?}");
        }

        // A run that ligates must still report the ligature on the cached call.
        let _ = shaper.segment("a => b");
        assert!(
            shaper.segment("a => b").iter().any(|s| s.ligated),
            "cached call lost the ligature"
        );
    }

    /// Exercises real shaping when a ligature font is available on disk.
    /// Set `BEAMTERM_LIGATURE_TEST_FONT` to an sfnt (.ttf/.otf) path to run.
    #[test]
    fn shapes_ligatures_when_font_provided() {
        let Ok(path) = std::env::var("BEAMTERM_LIGATURE_TEST_FONT") else {
            return;
        };
        let bytes = std::fs::read(path).expect("read test font");
        let shaper = Shaper::from_bytes(&bytes).expect("parse test font");
        assert!(
            shaper.has_ligatures(),
            "test font should advertise liga/calt"
        );

        let ligature_of = |text: &str| -> Option<(String, u8)> {
            let segs = shaper.segment(text);
            // total cells must always equal the source character count
            let total: usize = segs.iter().map(|s| s.cells as usize).sum();
            assert_eq!(
                total,
                text.chars().count(),
                "cell total mismatch for {text:?}: {segs:?}"
            );
            segs.iter()
                .find(|s| s.ligated)
                .map(|s| (text[s.start..s.start + s.len].to_string(), s.cells))
        };

        for (input, want_text, want_cells) in
            [("a => b", "=>", 2u8), ("a -> b", "->", 2), ("x != y", "!=", 2), ("x === y", "===", 3)]
        {
            let got = ligature_of(input);
            eprintln!("{input:?} -> ligature {got:?}");
            assert_eq!(
                got,
                Some((want_text.to_string(), want_cells)),
                "unexpected ligature for {input:?}"
            );
        }

        // A lone '=' between spaces must NOT ligate.
        assert!(
            shaper.segment("a = b").iter().all(|s| !s.ligated),
            "lone = should not ligate"
        );
    }
}
