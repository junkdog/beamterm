# beamterm-atlas

A static font atlas generator for GPU terminal renderers, optimized for texture memory and
rendering efficiency.

## Overview

`beamterm-atlas` generates tightly-packed 2D texture array atlases from TTF/OTF font files, producing a
binary format optimized for GPU upload. The system supports multiple font styles, full Unicode
including emoji, and automatic grapheme clustering.

This tool is used for generating **static font atlases** - pre-built atlas files that are loaded
at runtime. For applications where the required character set isn't known at build time, consider
using `beamterm-renderer`'s **dynamic font atlas** which rasterizes glyphs on-demand using the
browser's Canvas API (see [main README](../README.md#dynamic-font-atlas)).

## Usage

The CLI has two subcommands: `generate` and `inspect`.

### Generating Atlases

```bash
# List available monospace fonts (requires Regular, Bold, Italic, Bold+Italic)
beamterm-atlas generate --list-fonts

# Generate with default Unicode ranges
beamterm-atlas generate "JetBrains Mono" -s 16 -o jetbrains-16.atlas

# Generate with custom symbols file (including emoji) and Unicode ranges
beamterm-atlas generate "Hack" \
  --symbols-file symbols.txt \
  --range 0x2500..0x257F \
  --range 0x2580..0x259F

# Check glyph coverage
beamterm-atlas generate "Cascadia Code" --check-missing

# Dump atlas texture as PNG
beamterm-atlas generate "Fira Code" --dump-png atlas.png
```

**Key options:** `--emoji-font` (default: "Noto Color Emoji"), `-s/--font-size` (default: 15.0),
`-l/--line-height` (default: 1.0), `-o/--output` (default: ./bitmap_font.atlas),
`--underline-position`, `--underline-thickness`, `--strikethrough-position`,
`--strikethrough-thickness`, `--check-missing`, `--dump-png`, `-r/--range`.

### Inspecting Atlases

```bash
# Print metadata summary
beamterm-atlas inspect bitmap_font.atlas

# Inspect and export texture as PNG
beamterm-atlas inspect bitmap_font.atlas --dump-png atlas.png
```

## Glyph ID Encoding

16-bit glyph IDs encode base character and style:

| Bit Range | Purpose       | Description                             |
|-----------|---------------|-----------------------------------------|
| 0-9       | Base Glyph ID | 1024 possible base glyphs (0x000-0x3FF) |
| 10        | Bold Flag     | Selects bold variant (0x0400)           |
| 11        | Italic Flag   | Selects italic variant (0x0800)         |
| 12        | Emoji Flag    | Indicates emoji glyph (0x1000)          |
| 13        | Underline     | Underline effect (0x2000, runtime only) |
| 14        | Strikethrough | Strikethrough effect (0x4000, runtime only) |
| 15        | Reserved      | Reserved for future use                 |

The atlas encodes glyphs using bits 0-12. Bits 13-14 are applied at runtime for text decorations.

### Character Categories

- **ASCII (0x20-0x7E):** Direct mapping (char code = base glyph ID), 4 style variants each
- **Halfwidth Unicode:** Sequential IDs filling unused slots in 0x00-0x1FF, 4 style variants
- **Fullwidth Unicode:** Two consecutive IDs per glyph (left/right halves), 4 style variants
- **Emoji (0x1000+):** Two consecutive IDs per emoji, no style variants, max 2048

## Texture Layout

Each texture layer contains a 1×32 vertical grid of glyphs. Layer allocation:

| Style       | Layers  | Capacity   |
|-------------|---------|------------|
| Normal      | 0-31    | 1024 slots |
| Bold        | 32-63   | 1024 slots |
| Italic      | 64-95   | 1024 slots |
| BoldItalic  | 96-127  | 1024 slots |
| Emoji       | 128+    | Up to 4096 |

Coordinate calculation: `layer = ID >> 5`, `position = ID & 0x1F`.

## Binary Atlas Format

Versioned binary format with zlib-compressed texture data:

```
Header: Magic [0xBA, 0xB1, 0xF0, 0xA7] + Version 0x03
Metadata: font name, size, texture dims, cell size, line decorations, glyph count
Glyph Definitions: per glyph (ID, style, is_emoji, pixel coords, symbol)
Texture Data: u32 length + zlib-compressed RGBA data
```

Little-endian, length-prefixed UTF-8 strings, zlib level 9 compression (~75% size reduction).

## Font Requirements

Requires monospace fonts with all four style variants (Regular, Bold, Italic, Bold+Italic).
Fonts missing any variant won't appear in `--list-fonts`.
