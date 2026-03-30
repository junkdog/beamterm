# @beamterm/renderer

[![npm version](https://img.shields.io/npm/v/@beamterm/renderer.svg)](https://www.npmjs.com/package/@beamterm/renderer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

High-performance WebGL2 terminal renderer achieving sub-millisecond render times
through GPU-accelerated instanced rendering. Pure WASM + WebGL2 with zero runtime
dependencies.

## Features

- **Rich text styling** — bold, italic, underline, strikethrough with 24-bit color
- **Batch updates** — all cell changes are collected and uploaded to the GPU in a single pass
- **Two font atlas modes** — pre-rasterized static atlas or on-demand dynamic atlas with any browser font
- **Built-in text selection** — linear and block modes with clipboard integration and URL detection
- **Responsive** — automatic terminal resizing with HiDPI support
- **TypeScript definitions included**

Requires a browser with **WebGL2** and **WASM** support (any modern browser).

## Installation

```bash
npm install @beamterm/renderer
```

Or via CDN:

```html
<script src="https://unpkg.com/@beamterm/renderer@latest/dist/cdn/beamterm.min.js"></script>
<script>
  await Beamterm.init();
  const renderer = new Beamterm.BeamtermRenderer('#terminal');
</script>
```

## Quick Start

```javascript
import {
  main as init,
  style,
  cell,
  BeamtermRenderer,
} from "@beamterm/renderer";

await init();

// Create renderer — uses the embedded static font atlas by default
const renderer = new BeamtermRenderer("#terminal");

// Or use a dynamic atlas that rasterizes any browser font on demand
// const renderer = BeamtermRenderer.withDynamicAtlas('#terminal', ['JetBrains Mono', 'Fira Code'], 16.0);

const { cols, rows } = renderer.terminalSize();
console.log(`Terminal: ${cols}x${rows} cells`);

const batch = renderer.batch();

batch.clear(0x1a1b26);
batch.text(
  2,
  1,
  "Hello, Beamterm!",
  style().bold().underline().fg(0x7aa2f7).bg(0x1a1b26),
);
batch.cell(0, 0, cell("🚀", style().fg(0xffffff)));
batch.fill(1, 3, 18, 1, cell("─", style().fg(0x565f89).bg(0x1a1b26)));

renderer.render();
```

## API Reference

### BeamtermRenderer

The main renderer class. Manages the WebGL2 context and rendering pipeline.

#### Constructors

| Signature                                                                                   | Description                                              |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `new BeamtermRenderer(canvasSelector)`                                                      | Create with the embedded static font atlas               |
| `BeamtermRenderer.withDynamicAtlas(canvasSelector, fontFamilies, fontSize, autoResizeCss?)` | Create with a dynamic atlas using browser fonts          |
| `BeamtermRenderer.withStaticAtlas(canvasSelector, atlasData?, autoResizeCss?)`              | Create with custom `.atlas` data (or `null` for default) |

#### Rendering

| Method                  | Description                                             |
| ----------------------- | ------------------------------------------------------- |
| `batch()`               | Create a new `Batch` for buffering cell updates         |
| `render()`              | Render the current frame to the canvas                  |
| `resize(width, height)` | Resize the canvas and recalculate terminal dimensions   |
| `terminalSize()`        | Returns `{ cols, rows }` — terminal dimensions in cells |
| `cellSize()`            | Returns `{ width, height }` — cell dimensions in pixels |

#### Font Switching

Swap the font atlas at runtime. Existing cell content is preserved and translated to the new atlas.

| Method                                            | Description                                                   |
| ------------------------------------------------- | ------------------------------------------------------------- |
| `replaceWithDynamicAtlas(fontFamilies, fontSize)` | Switch to a dynamic atlas with the given fonts                |
| `replaceWithStaticAtlas(atlasData?)`              | Switch to a static atlas (`Uint8Array` or `null` for default) |

#### Selection & Mouse

| Method                                                        | Description                                                                 |
| ------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `enableSelection(mode, trimWhitespace)`                       | Enable built-in text selection                                              |
| `enableSelectionWithOptions(mode, trimWhitespace, modifiers)` | Selection that requires modifier keys (e.g. Shift+click)                    |
| `setMouseHandler(callback)`                                   | Set a custom mouse event handler (receives `MouseEvent`)                    |
| `getText(query)`                                              | Extract text for a `CellQuery` region                                       |
| `findUrlAt(col, row)`                                         | Detect a URL at the given cell position — returns `UrlMatch` or `undefined` |
| `copyToClipboard(text)`                                       | Copy text to the system clipboard                                           |
| `clearSelection()`                                            | Clear any active selection                                                  |
| `hasSelection()`                                              | Check if there is an active selection                                       |

### Batch

Buffers cell updates for efficient GPU upload. Create one per frame via `renderer.batch()`.

| Method                                | Description                                                             |
| ------------------------------------- | ----------------------------------------------------------------------- |
| `clear(bgColor)`                      | Clear the entire terminal with a background color                       |
| `text(x, y, text, style)`             | Write a string at `(x, y)` with uniform styling — fastest for text runs |
| `cell(x, y, cellData)`                | Update a single cell                                                    |
| `cells(array)`                        | Update multiple cells — each element is `[x, y, cellData]`              |
| `fill(x, y, width, height, cellData)` | Fill a rectangular region                                               |

All changes are automatically uploaded when `renderer.render()` is called.

### CellStyle

Fluent builder for text styling. Create one with `style()`.

```javascript
const heading = style().bold().fg(0x7aa2f7).bg(0x1a1b26);
const warning = style().bold().italic().fg(0xe0af68);
```

| Method            | Description                                |
| ----------------- | ------------------------------------------ |
| `fg(color)`       | Foreground color (`0xRRGGBB`)              |
| `bg(color)`       | Background color (`0xRRGGBB`)              |
| `bold()`          | Bold                                       |
| `italic()`        | Italic                                     |
| `underline()`     | Underline                                  |
| `strikethrough()` | Strikethrough                              |
| `bits`            | (property) Combined style bits as a number |

### CellQuery

Describes a rectangular or linear region of cells for text extraction.

```javascript
const query = new CellQuery(SelectionMode.Linear)
  .start(0, 2)
  .end(40, 5)
  .trimTrailingWhitespace(true);

const text = renderer.getText(query);
```

### ModifierKeys

Modifier key flags for `enableSelectionWithOptions()`.

```javascript
// Require Shift+Click to start selection
renderer.enableSelectionWithOptions(
  SelectionMode.Linear,
  true,
  ModifierKeys.SHIFT,
);

// Require Ctrl+Shift+Click
renderer.enableSelectionWithOptions(
  SelectionMode.Block,
  true,
  ModifierKeys.CONTROL.or(ModifierKeys.SHIFT),
);
```

| Static property        | Description               |
| ---------------------- | ------------------------- |
| `ModifierKeys.NONE`    | No modifier keys required |
| `ModifierKeys.SHIFT`   | Shift key                 |
| `ModifierKeys.CONTROL` | Ctrl key                  |
| `ModifierKeys.ALT`     | Alt / Option key          |
| `ModifierKeys.META`    | Cmd / Windows key         |

Combine with `.or()`: `ModifierKeys.SHIFT.or(ModifierKeys.ALT)`

### Enums

**SelectionMode** — `Linear` (text flow, like a terminal) or `Block` (rectangular, like a text editor).

**MouseEventType** — `MouseDown`, `MouseUp`, `MouseMove`, `Click`, `MouseEnter`, `MouseLeave`.

### Helper Functions

| Function              | Description                                               |
| --------------------- | --------------------------------------------------------- |
| `main()`              | Initialize the WASM module (typically imported as `init`) |
| `style()`             | Create a new `CellStyle`                                  |
| `cell(symbol, style)` | Create a `Cell`                                           |

### Color Format

Colors are 24-bit RGB values: `0xRRGGBB`.

```javascript
const white = 0xffffff;
const red = 0xff0000;
const tokyoBg = 0x1a1b26;
```

## Common Patterns

### Animation Loop

```javascript
function animate() {
  const batch = renderer.batch();
  batch.clear(0x1a1b26);
  batch.text(0, 0, `Frame: ${Date.now()}`, style().fg(0xc0caf5));
  renderer.render();
  requestAnimationFrame(animate);
}
```

### Responsive Terminal

```javascript
window.addEventListener("resize", () => {
  renderer.resize(window.innerWidth, window.innerHeight);
  redrawTerminal();
});
```

### URL Detection

```javascript
renderer.setMouseHandler((event) => {
  if (event.event_type === MouseEventType.Click) {
    const match = renderer.findUrlAt(event.col, event.row);
    if (match) window.open(match.url, "_blank");
  }
});
```

## Examples

See the [`examples/`](https://github.com/junkdog/beamterm/tree/main/js/examples) directory:

- **[Batch API Demo](https://junkdog.github.io/beamterm/api-demo/)** — interactive demonstration of all API methods
- **[Selection Demo](https://junkdog.github.io/beamterm/selection-demo/)** — text selection, URL detection, modifier keys
- **[Vite + TypeScript](https://junkdog.github.io/beamterm/vite/)** — modern dev setup with HMR and runtime font switching
- **[Webpack](https://junkdog.github.io/beamterm/webpack/)** — classic bundler setup

## Performance Tips

- Prefer `batch.text()` over multiple `batch.cell()` calls for strings with uniform styling
- Use `batch.fill()` for large rectangular regions
- Batch all updates within a single `renderer.batch()` / `renderer.render()` cycle
- Reuse `CellStyle` objects when possible

## License

MIT — see [LICENSE](https://github.com/junkdog/beamterm/blob/main/LICENSE) for details.

## Links

- [GitHub Repository](https://github.com/junkdog/beamterm)
- [Live Examples](https://junkdog.github.io/beamterm/)
- [Issues](https://github.com/junkdog/beamterm/issues)
