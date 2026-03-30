# Beamterm Batch API Example

A comprehensive, minimal example demonstrating **every method** available on the Beamterm Batch API.

## 🎯 What This Example Shows

This example demonstrates all 5 core Batch API methods:

1. **`batch.clear(backgroundColor)`** - Clear terminal with background color
2. **`batch.text(x, y, text, style)`** - Write styled text at position
3. **`batch.cell(x, y, cell)`** - Update individual cells with full control
4. **`batch.cells(arrayOfCells)`** - Update multiple cells efficiently
5. **`batch.fill(x, y, width, height, cell)`** - Fill rectangular regions

## 🚀 Quick Start

### Prerequisites

Build the WASM packages first:
```bash
# From project root
./build.zsh build-wasm
```

### Run the Example

```bash
npm install
npm run dev
```

Then open http://localhost:3000


### The Essential Pattern
```javascript
// 1. Create batch
const batch = renderer.batch();

// 2. Make all your updates (order doesn't matter)
batch.clear(0x1a1b26);
batch.text(0, 0, "Hello!", style().bold().fg(0x7aa2f7));
batch.cell(0, 2, cell("🚀", style()));

// 3. Render the frame
renderer.render();
```

### Style Builder Pattern
```javascript
const myStyle = style()
    .bold()
    .italic()
    .underline()
    .fg(0x7aa2f7)  // Foreground color
    .bg(0x1a1b26); // Background color
```

### Cell Data Structure
```javascript
const cellData = {
    symbol: "A",                    // Character or emoji
    style: style().bold().bits,     // Style bits
    fg: 0x7aa2f7,                  // Foreground color (0xRRGGBB)
    bg: 0x1a1b26                   // Background color (0xRRGGBB)
};

// Or use the helper function
const cellData = cell("A", style().bold().fg(0x7aa2f7));
```

### Efficient Multiple Updates
```javascript
// GOOD: Use batch.text() for uniform styling (much more efficient)
batch.text(0, 0, "Hello World", style().bold().fg(0x7aa2f7));

// GOOD: Use batch.cells() only when you need different styles per cell
const mixedCells = [
    [0, 1, cell("R", style().bold().fg(0xf7768e))],    // Red bold
    [1, 1, cell("G", style().italic().fg(0x9ece6a))],  // Green italic  
    [2, 1, cell("B", style().underline().fg(0x7aa2f7))], // Blue underline
];
batch.cells(mixedCells);

// AVOID: Don't convert uniform text to individual cells
// This is inefficient compared to batch.text()
const inefficient = text.split('').map((char, i) => [x + i, y, cell(char, style)]);
```

### Clean Code Patterns
The example demonstrates several best practices:
- **Use the right method for the job**: `batch.text()` for uniform styling, `batch.cells()` for mixed styling
- **Extract styles to variables** to avoid repetition
- **Use the `cell()` helper** instead of raw object literals
- **Group related updates** in the same batch
- **Meaningful variable names** that explain the purpose

### Performance Guidelines
- ⚡ **`batch.text()`** - Use for strings with uniform styling (fastest)
- 🎨 **`batch.cells()`** - Use when cells need different styles/colors
- 📦 **`batch.fill()`** - Use for large rectangular regions
- 🚫 **Avoid** converting uniform text to individual cells

## 📊 What You'll See

The running example shows:

- ✨ All text styles (normal, bold, italic, underline, strikethrough)
- 🎨 Color palette demonstrations
- 🚀 Emoji rendering
- 📦 Filled rectangles and patterns
- ⚡ Simple animation loop
- 📝 Real-time frame counter

## 🎮 Interaction

- The terminal displays static content demonstrating all API methods
- A spinning animation shows dynamic updates
- Check the browser console for detailed logs
- Resize the window to see responsive behavior

