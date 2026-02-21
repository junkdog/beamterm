# beamterm-data

[![Crates.io](https://img.shields.io/crates/v/beamterm-data.svg)](https://crates.io/crates/beamterm-data)
[![Documentation](https://docs.rs/beamterm-data/badge.svg)](https://docs.rs/beamterm-data)
[![License](https://img.shields.io/crates/l/beamterm-data.svg)](https://github.com/junkdog/beamterm#license)

Core data structures and binary serialization for the beamterm GPU terminal renderer.

> ⚠️ **Note**: This is an internal crate for the beamterm project. You probably want to use [`beamterm-renderer`](https://crates.io/crates/beamterm-renderer) or [`beamterm-atlas`](https://crates.io/crates/beamterm-atlas) instead.

## Overview

This crate provides the shared data structures and serialization functionality used by the beamterm terminal
rendering system. It includes:

- **Font atlas data structures** - Efficient representation of font glyph metadata
- **Glyph encoding system** - 16-bit glyph IDs with embedded style information
- **Binary serialization** - Compact, versioned format with zlib compression
- **Cross-platform compatibility** - Works in both native and WASM environments

