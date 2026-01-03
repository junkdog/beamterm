# @beamterm/renderer

[![npm version](https://img.shields.io/npm/v/@beamterm/renderer.svg)](https://www.npmjs.com/package/@beamterm/renderer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

High-performance WebGL2 terminal renderer achieving sub-millisecond render times through GPU-accelerated instanced rendering.

## ✨ Features

- **📦 Zero Dependencies**: Pure WASM + WebGL2, no external runtime dependencies
- **🎨 Rich Text Styling**: Bold, italic, underline, strikethrough with full color support
- **⚡ Efficient Updates**: Batch cell updates with single GPU buffer upload
- **📐 Responsive**: Automatic terminal resizing with proper aspect ratio maintenance
- **🎯 TypeScript Ready**: Full TypeScript definitions included
- **🖱️ Mouse Selection**: Built-in text selection with clipboard integration

## 📋 Requirements

- **WebGL2** capable browser
- **WASM** support

Should work with any modern browser.

## 📦 Installation

### NPM/Yarn

```bash
npm install @beamterm/renderer
# or
yarn add @beamterm/renderer
```

### CDN

```html
<script src="https://unpkg.com/@beamterm/renderer@latest/dist/cdn/beamterm.min.js"></script>
<script>
    await Beamterm.init();
    const renderer = new Beamterm.BeamtermRenderer('#terminal');
    // SelectionMode available as Beamterm.SelectionMode
    renderer.enableSelection(Beamterm.SelectionMode.Linear, true);
</script>
```

## 🚀 Quick Start

### ES Modules (Recommended)

```javascript
import { main as init, style, cell, BeamtermRenderer, SelectionMode } from '@beamterm/renderer';

// Initialize WASM module
await init();

// Create renderer with embedded static atlas (default)
const renderer = new BeamtermRenderer('#terminal');

// Or use a dynamic font atlas with any system font
const dynamicRenderer = BeamtermRenderer.withDynamicAtlas(
    '#terminal',
    ['JetBrains Mono', 'Fira Code'],  // font fallback chain
    16.0                               // font size in pixels
);

// Get terminal dimensions
const size = renderer.terminalSize();
console.log(`Terminal: ${size.width}×${size.height} cells`);

// Create a batch for efficient updates
const batch = renderer.batch();

// Clear terminal with background color
batch.clear(0x1a1b26);

// Write styled text
const textStyle = style().bold().underline().fg(0x7aa2f7).bg(0x1a1b26);
batch.text(2, 1, "Hello, Beamterm!", textStyle);

// Draw individual cells
batch.cell(0, 0, cell("🚀", style().fg(0xffffff)));

// Fill a rectangular region
const boxStyle = style().fg(0x565f89).bg(0x1a1b26);
batch.fill(1, 0, 18, 3, cell("█", boxStyle));

// Render frame
renderer.render();
```

### TypeScript

```typescript
import { main as init, style, BeamtermRenderer, Batch, Size, SelectionMode } from '@beamterm/renderer';

async function createTerminal(): Promise<void> {
  await init();
  
  const renderer = new BeamtermRenderer('#terminal');
  const batch: Batch = renderer.batch();
  
  // TypeScript provides full type safety
  const labelStyle = style()
    .bold()
    .italic()
    .underline()
    .fg(0x9ece6a)
    .bg(0x1a1b26);
    
  batch.text(0, 0, "TypeScript Ready! ✨", labelStyle);
  batch.flush();
  renderer.render();
}
```

## 📖 API Reference

### BeamtermRenderer

The main renderer class that manages the WebGL2 context and rendering pipeline.

```javascript
// Using embedded static font atlas (default)
const renderer = new BeamtermRenderer(canvasSelector);

// Using dynamic font atlas with browser fonts
const renderer = BeamtermRenderer.withDynamicAtlas(canvasSelector, fontFamilies, fontSize);
```

#### Static Methods

- **`withDynamicAtlas(canvasSelector, fontFamilies, fontSize)`**: Create a renderer with a dynamic
  font atlas that rasterizes glyphs on-demand using browser fonts. Best when character set isn't
  known at build time.
  - `canvasSelector`: CSS selector for the canvas element
  - `fontFamilies`: Array of font family names (e.g., `['JetBrains Mono', 'Fira Code']`)
  - `fontSize`: Font size in pixels

#### Methods

- **`batch()`**: Create a new batch for efficient cell updates
- **`render()`**: Render the current frame to the canvas
- **`resize(width, height)`**: Resize the canvas and recalculate terminal dimensions
- **`terminalSize()`**: Get terminal dimensions as `{ width, height }` in cells
- **`cellSize()`**: Get cell dimensions as `{ width, height }` in pixels

#### Selection Methods

- **`enableSelection(mode, trimWhitespace)`**: Enable built-in text selection
- **`setMouseHandler(callback)`**: Set custom mouse event handler
- **`getText(query)`**: Get selected text based on cell query
- **`copyToClipboard(text)`**: Copy text to system clipboard
- **`clearSelection()`**: Clear any active selection
- **`hasSelection()`**: Check if there is an active selection

### Batch

Batch operations for efficient GPU updates. All cell modifications should go through a batch.

```javascript
const batch = renderer.batch();
```

#### Methods

- **`clear(backgroundColor)`**: Clear entire terminal with specified color
- **`cell(x, y, cellData)`**: Update a single cell
- **`cells(cellArray)`**: Update multiple cells (array of `[x, y, cellData]`)
- **`text(x, y, text, style)`**: Write text starting at position
- **`fill(x, y, width, height, cellData)`**: Fill rectangular region
- **`flush()`**: Upload all changes to GPU (required before render)

### CellStyle

Fluent API for text styling.

```javascript
const myStyle = style()
  .bold()
  .italic()
  .underline()
  .strikethrough()
  .fg(0x7aa2f7)
  .bg(0x204060);
```

#### Methods

- **`fg(color)`**: Set foreground color
- **`bg(color)`**: Set background color
- **`bold()`**: Add bold style
- **`italic()`**: Add italic style
- **`underline()`**: Add underline effect
- **`strikethrough()`**: Add strikethrough effect

#### Properties

- **`bits`**: Get the combined style bits as a number

### Helper Functions

- **`style()`**: Create a new CellStyle instance
- **`cell(symbol, style)`**: Create a cell data object

### Enums

#### SelectionMode

- **`SelectionMode.Linear`**: Linear text flow selection (like normal terminals)
- **`SelectionMode.Block`**: Rectangular block selection (like text editors)

### Cell Data Structure

```javascript
{
  symbol: string,    // Single character or emoji
  style: number,     // Style bits or CellStyle.bits
  fg: number,        // Foreground color (0xRRGGBB)
  bg: number         // Background color (0xRRGGBB)
}
```

### Color Format

Colors are 24-bit RGB values in hex format:

```javascript
const white = 0xffffff;
const black = 0x000000;
const red = 0xff0000;
const tokyoNightBg = 0x1a1b26;
```

## 🎯 Common Patterns

### Animation Loop

```javascript
function animate() {
  const batch = renderer.batch();
  
  // Update terminal content
  batch.clear(0x1a1b26);
  batch.text(0, 0, `Frame: ${Date.now()}`, style().fg(0xc0caf5));
  
  // Flush and render
  batch.flush();
  renderer.render();
  
  requestAnimationFrame(animate);
}
```

### Responsive Terminal

```javascript
window.addEventListener('resize', () => {
  const canvas = document.getElementById('terminal');
  canvas.width = window.innerWidth;
  canvas.height = window.innerHeight;
  
  renderer.resize(canvas.width, canvas.height);
  redrawTerminal();
});
```

### Efficient Mass Updates

```javascript
// Use batch.text() for uniform styling (fastest)
batch.text(0, 0, "Hello World", style().bold().fg(0x7aa2f7));

// Use batch.cells() for mixed styling
const mixedCells = [
  [0, 1, cell("R", style().bold().fg(0xf7768e))],    // Red bold
  [1, 1, cell("G", style().italic().fg(0x9ece6a))],  // Green italic  
  [2, 1, cell("B", style().underline().fg(0x7aa2f7))], // Blue underline
];
batch.cells(mixedCells);
```

### Text Selection

```javascript
// Enable built-in selection with linear mode
renderer.enableSelection(SelectionMode.Linear, true);

// Or use custom mouse handling
renderer.setMouseHandler((event) => {
  console.log(`Mouse ${event.event_type} at ${event.col},${event.row}`);
});
```

## 🎮 Examples

Check out the [`examples/`](https://github.com/junkdog/beamterm/tree/main/js/examples) directory for complete examples:

- **[Batch API Demo](https://junkdog.github.io/beamterm/api-demo/)** - Interactive demonstration of all API methods
- **[Webpack Example](https://junkdog.github.io/beamterm/webpack/)** - Classic bundler setup
- **[Vite + TypeScript Example](https://junkdog.github.io/beamterm/vite/)** - Modern development with HMR

## 📊 Performance Guidelines

### Optimal Usage Patterns

- ⚡ **`batch.text()`** - Use for strings with uniform styling (fastest)
- 🎨 **`batch.cells()`** - Use when cells need different styles/colors
- 📦 **`batch.fill()`** - Use for large rectangular regions
- 🚫 **Avoid** converting uniform text to individual cells

### Performance Tips

- Batch all updates in a single render cycle
- Call `batch.flush()` only once per frame
- Prefer `batch.text()` over multiple `batch.cell()` calls
- Reuse style objects when possible


## 📄 License

MIT License - see [LICENSE](https://github.com/junkdog/beamterm/blob/main/LICENSE) for details.

## 🔗 Links

- [GitHub Repository](https://github.com/junkdog/beamterm)
- [Live Examples](https://junkdog.github.io/beamterm/)
- [Documentation](https://docs.rs/beamterm-renderer)
- [Issues](https://github.com/junkdog/beamterm/issues)