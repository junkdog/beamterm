import { main as init, style, BeamtermRenderer, SelectionMode, Batch } from '@beamterm/renderer';

interface Theme {
    bg: number;
    fg: number;
    primary: number;
    secondary: number;
    success: number;
    error: number;
    warning: number;
}

const tokyoNight: Theme = {
    bg: 0x1a1b26,
    fg: 0xc0caf5,
    primary: 0x7aa2f7,
    secondary: 0xbb9af7,
    success: 0x9ece6a,
    error: 0xf7768e,
    warning: 0xe0af68,
};

class TerminalApp {
    private renderer: BeamtermRenderer;
    private cols: number;
    private rows: number;

    constructor(renderer: BeamtermRenderer) {
        this.renderer = renderer;
        const size = renderer.terminalSize();
        this.cols = size.width;
        this.rows = size.height;
    }

    public render(): void {
        // Create a batch for all updates
        const batch = this.renderer.batch();

        this.clear(batch);
        this.drawHeader(batch);
        this.drawMenu(batch);
        this.drawContent(batch);
        this.drawStatus(batch);

        // Synchronize all updates to GPU
        batch.flush();

        // Render the frame
        this.renderer.render();
    }

    public resizeTerminal(width_px: number, height_px: number): void {
        this.renderer.resize(width_px, height_px);
        this.renderer.enableSelection(SelectionMode.Block, true);
        const size = this.renderer.terminalSize();

        this.cols = size.width;
        this.rows = size.height;

        this.render();
    }

    private clear(batch: Batch): void {
        batch.clear(tokyoNight.bg);
    }

    private drawHeader(batch: Batch): void {
        const title = "🚀 beamterm + Vite + TypeScript";
        const x = Math.floor((this.cols - title.length) / 2);

        batch.text(x, 1, title, style().bold().fg(tokyoNight.primary).bg(tokyoNight.bg));
    }

    private drawMenu(batch: Batch): void {
        const menuItems = [
            { key: 'N', label: 'New', color: tokyoNight.success },
            { key: 'O', label: 'Open', color: tokyoNight.primary },
            { key: 'S', label: 'Save', color: tokyoNight.secondary },
            { key: 'Q', label: 'Quit', color: tokyoNight.error },
        ];

        let x = 2;
        const y = 3;

        menuItems.forEach(item => {
            const keyStyle = style().bold().underline().bg(tokyoNight.bg);
            const labelStyle = style().bg(tokyoNight.bg);

            batch.text(x, y, `[${item.key}]`, keyStyle.fg(item.color));
            x += 3;
            batch.text(x, y, ` ${item.label}  `, labelStyle.fg(tokyoNight.fg));
            x += item.label.length + 3;
        });
    }

    private drawContent(batch: Batch): void {
        // Draw a demo terminal window
        const windowY = 6;
        const windowHeight = this.rows - 10;

        // Demo content showcasing dynamic atlas capabilities
        const demoLines = [
            { text: "$ npm create beamterm-app my-terminal", color: tokyoNight.fg },
            { text: "✓ Created project structure", color: tokyoNight.success },
            { text: "✓ Installed dependencies", color: tokyoNight.success },
            { text: "✓ Generated WebGL shaders", color: tokyoNight.success },
            { text: "", color: tokyoNight.fg },
            { text: "# Dynamic atlas: emoji, CJK, and more! 🎉", color: tokyoNight.warning },
            { text: "  Emoji:  🚀🔥✨🎮🎯💻🦀📦", color: tokyoNight.fg },
            { text: "  CJK:    日本語中文한국어", color: tokyoNight.fg },
            { text: "  Arrows: ←→ ↑↓ ⇐⇒  ⇑⇓", color: tokyoNight.fg },
            { text: "  Math:   ∑ ∏ ∫ √ ∞ ≈  ≠ ≤ ≥", color: tokyoNight.fg },
            { text: "", color: tokyoNight.fg },
            { text: "  ➜  Fonts rasterized on-demand via Canvas API", color: tokyoNight.primary },
        ];

        demoLines.forEach((line, i) => {
            if (i < windowHeight - 2) {
                batch.text(4, windowY + 1 + i, line.text, style().fg(line.color).bg(tokyoNight.bg));
            }
        });
    }

    private drawStatus(batch: Batch): void {
        const status = `${this.cols}x${this.rows} | Dynamic Atlas | Ready`;
        const y = this.rows - 2;

        // Draw status bar background
        const bar = '─'.repeat(this.cols);
        batch.text(0, y, bar, style().fg(tokyoNight.fg).bg(tokyoNight.bg));

        // Draw status text
        const x = this.cols - status.length - 2;
        batch.text(x, y, status, style().fg(tokyoNight.secondary).bg(tokyoNight.bg));
    }
}

// Animation controller for smooth updates
class AnimationController {
    private app: TerminalApp;
    private lastTime: number = 0;
    private updateInterval: number = 16; // ~60fps

    constructor(app: TerminalApp) {
        this.app = app;
    }

    start(): void {
        this.animate(0);
    }

    private animate = (currentTime: number): void => {
        if (currentTime - this.lastTime >= this.updateInterval) {
            this.app.render();
            this.lastTime = currentTime;
        }
        requestAnimationFrame(this.animate);
    };
}

async function main() {
    // Initialize WASM module
    await init();

    // Create renderer with dynamic font atlas using browser fonts
    const renderer = BeamtermRenderer.withDynamicAtlas(
        '#terminal',
        ['JetBrains Mono', 'Fira Code', 'Cascadia Code'],
        16.0
    );
    const app = new TerminalApp(renderer);

    // Set initial canvas size
    const { width, height } = calculateCanvasSize();
    const canvas = document.getElementById('terminal') as HTMLCanvasElement;
    canvas.width = width;
    canvas.height = height;

    app.resizeTerminal(width, height);

    // Start animation loop
    const animationController = new AnimationController(app);
    animationController.start();

    // Handle window resize
    let resizeTimeout: number;
    window.addEventListener('resize', () => {
        clearTimeout(resizeTimeout);
        resizeTimeout = window.setTimeout(() => {
            const { width, height } = calculateCanvasSize();
            canvas.width = width;
            canvas.height = height;
            app.resizeTerminal(width, height);
        }, 100);
    });
}

function calculateCanvasSize(): { width: number; height: number } {
    const width = Math.min(window.innerWidth - 40, 1200);
    const height = Math.min(window.innerHeight - 100, 800);

    return { width, height };
}

// Start the application
main().catch(error => {
    console.error('Failed to initialize Beamterm:', error);
});