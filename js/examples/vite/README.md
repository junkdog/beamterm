# js/examples/vite/README.md

# Beamterm Vite + TypeScript Example

This example demonstrates how to use Beamterm with Vite and TypeScript for a modern development experience.

## Prerequisites

Before running this example, you must build the WASM packages:

```bash
# From the project root
./build.zsh build-wasm

# Or if you haven't run setup yet
./build.zsh setup
./build.zsh build-wasm
```

## Setup

1. Install dependencies:
   ```bash
   npm ci
   ```

2. Start the development server:
   ```bash
   npm run dev
   ```

3. Open http://localhost:5173 in your browser

## Important Notes

- This example uses a local file reference to the `@beamterm/renderer` package
- The WASM must be built before running the example
- If you get import errors, see [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)

## Features Demonstrated

- **TypeScript Integration**: Full type safety with Beamterm's WASM bindings
- **Tokyo Night Theme**: Beautiful color scheme implementation
- **Responsive Design**: Automatic terminal resizing
- **Component Architecture**: Clean separation of concerns with `TerminalApp` class
- **Modern Build Tool**: Fast HMR (Hot Module Replacement) with Vite

## Project Structure

```
vite/
├── index.html          # Main HTML file
├── package.json        # Dependencies and scripts
├── tsconfig.json       # TypeScript configuration
├── vite.config.ts      # Vite configuration with WASM plugin
├── TROUBLESHOOTING.md  # Common issues and solutions
└── src/
    └── main.ts         # Application entry point
```

## Key Files

### vite.config.ts
Configures Vite to handle WASM modules:
```typescript
import wasm from 'vite-plugin-wasm';

export default defineConfig({
    plugins: [wasm()],
    build: {
        target: 'esnext',
    },
    optimizeDeps: {
        exclude: ['@beamterm/renderer']
    }
});
```

### src/main.ts
Shows how to:
- Initialize the WASM module
- Create a renderer instance
- Build a simple UI with menus and status bar
- Handle window resizing
- Use TypeScript for better code organization
