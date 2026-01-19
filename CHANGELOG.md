# Changelog

All notable changes to this project will be documented in this file.

## [unreleased]

### 🚀 Features

- *(atlas)* Runtime font atlas replacement (#73)
- *(mouse)* Modifier key requirements for text selection (#74)

### 💼 Other

- *(deps)* Bump serde_json from 1.0.148 to 1.0.149 (#77)
- *(deps)* Bump lru from 0.16.2 to 0.16.3 (#76)
- *(deps)* Bump miniz_oxide from 0.8.9 to 0.9.0 (#75)

### 🐛 Bug Fixes

- *(dynamic-atlas)* Use atlas glyph ID for space instead of ASCII code

## [beamterm-v0.12.0] - 2026-01-08

### 🚀 Features

- *(atlas)* Detect and report fallback font usage during atlas generation
- *(selection)* Auto-clear mouse selection when content changes (#68)
- *(examples)* Add performance metrics display to canvas_waves
- *(static-atlas)* Add `--debug-space-pattern` option for pixel-perfect validation
- *(dynamic-atlas)* Add `Terminal::builder().debug_dynamic_font_atlas()` to validate pixel-perfect rendering

### 💼 Other

- *(deps)* Bump clap from 4.5.53 to 4.5.54 (#65)
- *(deps)* Bump cosmic-text from 0.14.2 to 0.16.0 (#64)

### 🐛 Bug Fixes

- *(verify-atlas)* Update for vertical layout and double-width glyphs
- *(dynamic-atlas)* Clip glyph rasterization to prevent pixel bleed
- *(dynamic-atlas)* Account for underline/strikethrough flags
- *(dynamic-atlas)* Handle ASCII characters in `get_symbol()` (#70)

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.12.0

## [beamterm-v0.11.0] - 2026-01-05

### 🚀 Features

- *(atlas)* Add `DynamicFontAtlas` for on-demand glyph rasterization with LRU cache (#63)

### 💼 Other

- *(deps-dev)* Bump the minor-and-patch group across 1 directory with 2 updates (#62)
- *(deps)* Bump serde_json from 1.0.146 to 1.0.148 (#61)

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.11.0

## [beamterm-v0.10.0] - 2025-12-25

### 🚀 Features

- *(verify-atlas)* Atlas path is now a required argument
- *(renderer)* Automatic recovery from opengl context loss

### 💼 Other

- *(deps)* Bump actions/upload-artifact from 5 to 6 (#56)
- *(deps-dev)* Bump jsdom in /js in the minor-and-patch group (#55)
- *(deps)* Bump tracing from 0.1.43 to 0.1.44 (#58)
- *(deps)* Bump serde_json from 1.0.145 to 1.0.146 (#59)

### 🐛 Bug Fixes

- *(renderer)* Fix green tint in chrome-based browsers due to ANGLE uint bit operation bugs (AMD/Qualcomm)
- *(renderer)* Fix vertical banding artifacts in chrome-based browsers due to ANGLE mediump precision issues

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.10.0

## [beamterm-v0.9.0] - 2025-12-07

### 🚀 Features

- *(atlas)* Fullwidth glyph support (#49)
- *(atlas)* Change texture layout from 32x1 horizontal to 1x32 vertical

### 🛡️ Security

- *(examples)* Npm security hardening

### 💼 Other

- *(deps)* Bump tracing-appender from 0.2.3 to 0.2.4 (#50)
- *(deps)* Bump emojis from 0.7.2 to 0.8.0 (#39)
- *(deps)* Bump clap from 4.5.48 to 4.5.53 (#47)
- *(deps)* Bump actions/checkout from 5 to 6 (#48)
- *(deps)* Bump tracing from 0.1.41 to 0.1.43 (#51)
- *(deps)* Bump actions/setup-node from 5 to 6 (#41)
- *(deps)* Bump actions/upload-artifact from 4 to 5 (#42)
- *(deps)* Bump tracing-subscriber from 0.3.20 to 0.3.22 (#52)
- *(deps-dev)* Bump the minor-and-patch group in /js with 2 updates (#53)
- *(deps-dev)* Bump jsdom from 23.2.0 to 27.2.0 in /js (#54)
- *(canvas_waves)* Add profiling demo from from ratzilla

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.9.0

## [beamterm-v0.8.0] - 2025-10-09

### 🚀 Features

- *(renderer)* Double-width emoji support (#37)
- *(atlas)* Add emoji font selection via `--emoji-font`

### 💼 Other

- *(deps)* Bump serde from 1.0.219 to 1.0.226 (#34)
- *(deps)* Bump clap from 4.5.46 to 4.5.48 (#33)
- *(deps)* Bump serde_json from 1.0.143 to 1.0.145 (#32)
- *(deps)* Bump actions/github-script from 7 to 8 (#30)
- *(deps)* Bump actions/setup-node from 4 to 5 (#29)

### 🐛 Bug Fixes

- *(atlas)* Glyph mismatch from truncated conversion

### ⚙️ Miscellaneous Tasks

- *(font)* Hack 14.94pt 11x18 with Noto Color Emoji
- Release beamterm 0.8.0

## [beamterm-v0.7.0] - 2025-09-07

### 🚀 Features

- *(atlas)* Double glyph capacity from 512 to 1024 glyphs per font style (#27)
- *(atlas)* Add `--check-missing` option to CLI

### ⚙️ Miscellaneous Tasks

- *(font)* Hack 14.94pt, 11x18px
- Release beamterm 0.7.0

## [beamterm-v0.6.0] - 2025-08-14

### 🚀 Features

- *(renderer)* Add `TerminalBuilder::enable_debug_api`. When enabled, a debug API will be available at `window.__beamterm_debug`.
- *(atlas)* Font size automatically resized to better fill the cell
- *(atlas)* Nudge line decoration positions to half-pixel boundaries

### 🐛 Bug Fixes

- *(mouse)* Handle cursor leaving terminal during selection
- *(atlas)* Fix font face using system defaults instead of user-selected fonts

### ⚙️ Miscellaneous Tasks

- *(atlas)* More glyphs
- *(font)* Hack 16.6pt (12x20)
- Release beamterm 0.6.0

## [beamterm-v0.5.0] - 2025-06-29

### 🚀 Features

- *(renderer)* Add `TerminalGrid::cell_data_mut` and `CellDynamic` mutators
- *(renderer)* Expose `TerminalMouseHandler` public API for external mouse handling

### 💼 Other

- *(renderer)* The `mouse` module is now pub and not re-exported from the root

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.5.0

## [beamterm-0.4.0] - 2025-06-28

### 🚀 Features

- *(renderer)* Add Terminal::update_cells_by_position

### 🗑️ Deprecations

- *(js)* Batch::flush, as it now automatic

### ⚡ Performance

- *(fragment shader)* Remove all division ops and change to multiplication

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.4.0

## [beamterm-0.3.0] - 2025-06-26

### 🚀 Features

- *(renderer)* Add Terminal API with builder pattern (#11)
- *(renderer)* Add linear and block-based copy selection (#12)

### 🐛 Bug Fixes

- *(renderer)* Remove faulty debug_assert from CellDynamic::new

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.3.0

## [beamterm-0.2.0] - 2025-06-15

### 🚀 Features

- *(js-api)* Basic JS API locked behind "js-api" feature

### 💼 Other

- *(renderer)* Add TerminalGrid::base_glyph_id(&str)
- *(renderer)* Add experimental JS support
- *(js)* Add webpack example
- *(js)* Add vite+typescript example
- *(github-pages)* Deploy webpack and vite examples
- *(api-demo)* Add JS API demo

### 🐛 Bug Fixes

- *(shader)* Propagate LineEffects from `FontAtlasData` to fragment shader
- *(atlas)* Skip control characters during generation

### 📚 Documentation

- *(README)* Add link to live demos

### ⚡ Performance

- *(renderer)* Replace bit ops in `CellDynamic` with `to_le_bytes()`

### ⚙️ Miscellaneous Tasks

- Start using git-cliff
- Add developer-facing build.zsh and supporting scripts
- *(renderer)* Omit local main.rs/index.html from published files
- *(emoji)* Embed ~200 more emoji into the atlas
- Release beamterm 0.2.0

## [beamterm-0.1.1] - 2025-06-06

### ⚙️ Miscellaneous Tasks

- Release beamterm 0.1.1


*generated by [git-cliff](https://git-cliff.org/docs/)*
