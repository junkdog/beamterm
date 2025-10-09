# beamterm-atlas

A font atlas generator for WebGL terminal renderers, optimized for GPU texture memory and
rendering efficiency.

## Overview

`beamterm-atlas` generates tightly-packed 2D texture array atlases from TTF/OTF font files, producing a
binary format optimized for GPU upload. The system supports multiple font styles, full Unicode
including emoji, and automatic grapheme clustering.

## Architecture

The crate consists of:
- **Font rasterization engine** using cosmic-text for high-quality text rendering
- **2D texture array packer** organizing glyphs into 32×1 grids per texture layer
- **Binary serializer** with zlib compression for efficient storage
- **Atlas verification tool** for debugging and visualization

## Glyph ID Assignment System

### ID Structure

The system uses a 16-bit glyph ID that encodes both the base character and its style variations:


| Bit Range | Purpose       | Description                            |
|-----------|---------------|----------------------------------------|
| 0-9       | Base Glyph ID | 1024 possible base glyphs (0x000-0x3FF) |
| 10        | Bold Flag     | Selects bold variant (0x0400)          |
| 11        | Italic Flag   | Selects italic variant (0x0800)        |
| 12        | Emoji Flag    | Indicates emoji glyph (0x1000)         |
| 13        | Underline     | Underline effect (0x2000)              |
| 14        | Strikethrough | Strikethrough effect (0x4000)          |
| 15        | Reserved      | Reserved for future use                |

The atlas only encodes glyphs with the first 13 bits. Bits 13 and 14 are applied
at runtime for text decoration effects, while bit 15 is reserved for future extensions.

### Font Style Encoding

Each base glyph automatically generates four style variants by combining the bold and italic flags:

| Style       | Bit Pattern | ID Offset | Example ('A' = 0x41) |
|-------------|-------------|-----------|----------------------|
| Normal      | `0x0000`    | +0        | `0x0041`             |
| Bold        | `0x0400`    | +1024     | `0x0441`             |
| Italic      | `0x0800`    | +2048     | `0x0841`             |
| Bold+Italic | `0x0C00`    | +3072     | `0x0C41`             |

This encoding allows the shader to compute texture coordinates directly from the glyph ID without
lookup tables.

### Character Category Assignment

The generator assigns IDs based on three character categories:

**1. ASCII Characters (0x00-0x7F)**
- Direct mapping: character code = base glyph ID
- Guarantees fast lookup for common characters
- Occupies first 8 texture layers (128 chars ÷ 16 per layer)

**2. Unicode Characters**
- Fill unused slots in the 0x00-0x1FF range
- Sequential assignment starting from first available ID
- Constrained to 512 glyphs (0x000-0x1FF)

**3. Emoji Characters (Double-Width)**
- Start at ID 0x1000 (bit 12 set)
- Each emoji occupies TWO consecutive IDs (left half + right half)
- Sequential assignment: 0x1000/0x1001 (emoji 0), 0x1002/0x1003 (emoji 1), etc.
- No style variants (emoji are always rendered as-is)
- Maximum 2048 emoji (using 4096 glyph slots)

### Texture Layer Allocation

Each font style reserves exactly 32 layers (1024 glyph slots), regardless of actual usage:

| Style Range | Layers  | Glyph ID Range | Capacity      |
|-------------|---------|----------------|---------------|
| Normal      | 0-31    | 0x0000-0x03FF  | 1024 slots    |
| Bold        | 32-63   | 0x0400-0x07FF  | 1024 slots    |
| Italic      | 64-95   | 0x0800-0x0BFF  | 1024 slots    |
| BoldItalic  | 96-127  | 0x0C00-0x0FFF  | 1024 slots    |
| Emoji       | 128+    | 0x1000+        | Up to 4096    |

**Total: 128 layers for font styles (always allocated) + up to 128 layers for emoji**

**Example with 500 base glyphs + 100 emoji:**
- Font styles: 128 layers (with gaps - only ~500/1024 slots used per style)
- Emoji: 7 layers (100 emoji × 2 IDs = 200 slots)
- Total: 135 layers

**Maximum capacity (1024 base glyphs + 2048 emoji):**
- Font styles: 128 layers (fully utilized)
- Emoji: 128 layers (2048 emoji × 2 IDs = 4096 slots)
- Total: 256 layers

## 2D Texture Array Organization

### Layer Layout

Each texture layer contains a 32×1 grid of glyphs:

```
Position in layer = ID & 0x1F (modulo 32)
Grid X = Position (0-31)
Grid Y = 0 (always single row)
Layer = ID ÷ 32
```

### Memory Layout

The 2D texture array uses RGBA format with dimensions:

- Width: cell_width × 32
- Height: cell_height × 1
- Layers: max_glyph_id ÷ 32

The RGBA format is required for emoji support - while monochrome glyphs could use a single channel,
emoji glyphs need full color information.

This layout ensures:
- Efficient GPU memory alignment
- Cache-friendly access pattern (sequential glyphs in same row)
- Simple coordinate calculation using bit operations

## Rasterization Process

### Cell Dimension Calculation

The system determines cell size by measuring the full block character `█` to ensure all glyphs
fit within the cell boundaries. Additional padding of 1px on all sides prevents texture bleeding.

### Font Style Handling

Each glyph is rendered four times, one for each of the styles (normal, bold, italic, bold+italic).

### Emoji Special Handling (Double-Width)

Emoji glyphs require special processing as they are rendered double-width:
1. Rendered at 2× width for measurement
2. Split into left and right halves
3. Each half placed in consecutive glyph slots
4. Scaled to fit within cell boundaries
5. Color information preserved in texture

The presence of emoji is the primary reason the atlas uses RGBA format instead of a single-channel
texture. While monochrome glyphs only need an alpha channel, emoji require full color information
to render correctly. Each emoji occupies two consecutive IDs in the atlas.

## Binary Atlas Format

### File Structure

The atlas uses a versioned binary format with header validation:

```
Header (5 bytes)
├─ Magic: [0xBA, 0xB1, 0xF0, 0xA7]
└─ Version: 0x01

Metadata Section
├─ Font name (u8 length + UTF-8 string)
├─ Font size (f32)
├─ Texture width (i32)
├─ Texture height (i32)
├─ Texture layers (i32)
├─ Cell width (i32)
├─ Cell height (i32)
├─ Underline position (f32)
├─ Underline thickness (f32)
├─ Strikethrough position (f32)
├─ Strikethrough thickness (f32)
└─ Glyph count (u16)

Glyph Definitions
└─ Per glyph:
   ├─ ID (u16 - includes style bits)
   ├─ Style (u8) - ordinal: 0=Normal, 1=Bold, 2=Italic, 3=BoldItalic
   ├─ Is emoji (u8) - 0=false, 1=true
   ├─ Pixel X (i32)
   ├─ Pixel Y (i32)
   └─ Symbol (u8 length + UTF-8 string)

Compressed Texture Data
├─ Data length (u32)
└─ zlib-compressed RGBA data
```

### Serialization Properties

- **Endianness**: Little-endian for cross-platform compatibility
- **Compression**: zlib level 9 (typically 75% size reduction)
- **String encoding**: Length-prefixed UTF-8 (u8 for strings, max 255 bytes)
- **Texture data**: Length-prefixed compressed data (u32 length)
- **Alignment**: Natural alignment without padding

## Usage

### Installation

```bash
cargo install beamterm-atlas
```

### Command-Line Interface

```bash
beamterm-atlas [OPTIONS] <FONT>
```

#### Arguments

- `<FONT>` - Font selection by name (partial match) or 1-based index

#### Options

- `-s, --font-size <SIZE>` - Font size in points (default: 15.0)
- `-l, --line-height <MULTIPLIER>` - Line height multiplier (default: 1.0)
- `-o, --output <PATH>` - Output file path (default: "./bitmap_font.atlas")
- `--underline-position <FRACTION>` - Underline position from 0.0 (top) to 1.0 (bottom) of cell (default: 0.85)
- `--underline-thickness <PERCENT>` - Underline thickness as percentage of cell height (default: 5.0)
- `--strikethrough-position <FRACTION>` - Strikethrough position from 0.0 (top) to 1.0 (bottom) of cell (default: 0.5)
- `--strikethrough-thickness <PERCENT>` - Strikethrough thickness as percentage of cell height (default: 5.0)
- `-L, --list-fonts` - List available fonts and exit

### Examples

List all available monospace fonts with complete style variants:
```bash
beamterm-atlas --list-fonts
```

Generate an atlas using JetBrains Mono at 16pt:
```bash
beamterm-atlas "JetBrains Mono" -s 16 -o jetbrains-16.atlas
```

Generate with custom text decoration settings:
```bash
beamterm-atlas "Fira Code" \
  --underline-position 0.9 \
  --underline-thickness 7.5 \
  --strikethrough-position 0.45
```

Select font by index (useful for scripting):
```bash
# First, list fonts to see indices
beamterm-atlas -L

# Then select by number
beamterm-atlas 5 -s 14
```

### Character Set

The tool generates an atlas from a predefined character set including:
- Full ASCII and Latin-1 supplement
- Box drawing characters
- Mathematical symbols
- Arrows and geometric shapes
- Braille patterns
- Extensive emoji set

### Verification

The `verify-atlas` binary visualizes the texture layout, showing:
- Layer organization
- Character placement
- Grid boundaries
- Glyph distribution

```bash
verify-atlas
```

## Font Requirements

The generator requires monospace fonts with all four style variants:
- Regular
- Bold
- Italic
- Bold+Italic

Fonts missing any variant will not appear in the font list. The system automatically discovers
all installed system fonts that meet these requirements.