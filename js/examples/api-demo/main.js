// Comprehensive Beamterm Batch API Example
// Two-screen demo: Press SPACE to switch between screens

import { main as init, style, cell, BeamtermRenderer, SelectionMode } from '@beamterm/renderer';

class BatchAPIDemo {
    constructor() {
        this.renderer = null;
        this.size = null;
        this.currentScreen = 1;
        this.setupKeyHandler();
    }

    setupKeyHandler() {
        document.addEventListener('keydown', (e) => {
            if (e.code === 'Space') {
                e.preventDefault();
                this.currentScreen = this.currentScreen === 1 ? 2 : 1;
                this.render();
            }
        });
    }

    async initialize() {
        // Initialize WASM module
        await init();
        console.log('✅ WASM initialized');

        // Create renderer
        this.renderer = new BeamtermRenderer('#terminal');
        this.renderer.enableSelection(SelectionMode.Block, true);
        this.size = this.renderer.terminalSize();
        console.log(`✅ Terminal: ${this.size.width}×${this.size.height} cells`);

        // Set up canvas size
        const canvas = document.getElementById('terminal');
        canvas.width = Math.min(window.innerWidth - 40, 1000);
        canvas.height = Math.min(window.innerHeight - 100, 600);
        this.renderer.resize(canvas.width, canvas.height);
        this.size = this.renderer.terminalSize();
    }

    renderScreen1() {
        const batch = this.renderer.batch();

        // Clear terminal
        // batch.clear(0x1a1b26);
        batch.clear(0x000000);

        let y = 1;

        // Header
        batch.text(1, y++, "🚀 Beamterm Batch API - Screen 1/2", style().bold().underline().fg(0x7aa2f7));
        batch.text(1, y++, "Press SPACE for Screen 2", style().italic().fg(0x565f89));
        y++;

        // Method 1: clear()
        batch.text(1, y++, "1. batch.clear(backgroundColor)", style().bold().fg(0x9ece6a));
        batch.text(3, y++, "Clears entire terminal with background color", style().fg(0xa9b1d6));
        batch.text(3, y++, "batch.clear(0x1a1b26); // Tokyo Night background", style().fg(0x7dcfff));
        y++;

        // Method 2: text()
        batch.text(1, y++, "2. batch.text(x, y, text, style)", style().bold().fg(0x9ece6a));
        batch.text(3, y++, "Most efficient way to render styled text", style().fg(0xa9b1d6));
        batch.text(3, y++, "Examples:", style().fg(0xa9b1d6));

        // Text examples
        let x = 5;
        batch.text(x, y, "Normal", style().fg(0xc0caf5));
        x += 8;
        batch.text(x, y, "Bold", style().bold().fg(0x7aa2f7));
        x += 6;
        batch.text(x, y, "Italic", style().italic().fg(0xbb9af7));
        x += 8;
        batch.text(x, y, "Underline", style().underline().fg(0x9ece6a));
        y += 2;

        // Method 3: cell()
        batch.text(1, y++, "3. batch.cell(x, y, cellData)", style().bold().fg(0x9ece6a));
        batch.text(3, y++, "Update individual cells with precise control", style().fg(0xa9b1d6));
        batch.text(3, y++, "Examples:", style().fg(0xa9b1d6));

        // Individual cell examples
        x = 5;
        batch.cell(x++, y, cell("🚀", style())); x++; // double-width emoji
        batch.cell(x++, y, cell("A", style().bold().fg(0xf7768e)));
        batch.cell(x++, y, cell("B", style().italic().fg(0x9ece6a)));
        batch.cell(x++, y, cell("C", style().underline().fg(0x7aa2f7)));
        batch.cell(x++, y, cell("D", style().strikethrough().fg(0xbb9af7)));
        y += 2;

        // Style builder explanation
        batch.text(1, y++, "Style Builder Pattern:", style().bold().fg(0xe0af68));
        batch.text(3, y++, "const myStyle = style().bold().italic().fg(0x7aa2f7);", style().fg(0x7dcfff));
        batch.text(3, y++, "batch.text(0, 0, \"Hello!\", myStyle);", style().fg(0x7dcfff));
        y++;

        // Color format
        batch.text(1, y++, "Color Format: 0xRRGGBB (24-bit RGB)", style().bold().fg(0xe0af68));
        batch.text(3, y, "0xffffff = white", style().fg(0xffffff));
        batch.text(20, y, "0x000000 = black", style().fg(0x000000).bg(0xffffff));
        batch.text(37, y, "0xff0000 = red", style().fg(0xff0000));
        y++;

        // Navigation hint
        batch.text(1, this.size.height - 3, "Press SPACE to switch screens",
            style().bold().fg(0x7aa2f7));
    }

    renderScreen2() {
        const batch = this.renderer.batch();

        // Clear terminal
        batch.clear(0x000000);
        // batch.clear(0x1a1b26);

        let y = 1;

        // Header
        batch.text(1, y++, "🚀 Beamterm Batch API - Screen 2/2", style().bold().underline().fg(0x7aa2f7));
        batch.text(1, y++, "Press SPACE for Screen 1", style().italic().fg(0x565f89));
        y++;

        // Method 4: cells()
        batch.text(1, y++, "4. batch.cells(array) - For mixed styling only!", style().bold().fg(0x9ece6a));
        batch.text(3, y++, "Use when each cell needs different styles/colors", style().fg(0xa9b1d6));

        // Helper functions for reusable styles (to avoid consumption)
        const normalText = () => style().fg(0xc0caf5);
        const boldRed = () => style().bold().fg(0xf7768e);

        const mixedCells = [
            [3, y, cell("M", normalText())],
            [4, y, cell("i", normalText())],
            [5, y, cell("x", normalText())],
            [6, y, cell("e", normalText())],
            [7, y, cell("d", normalText())],
            [9, y, cell("S", boldRed())],
            [10, y, cell("t", style().italic().fg(0xbb9af7))],
            [11, y, cell("y", style().underline().fg(0x9ece6a))],
            [12, y, cell("l", style().strikethrough().fg(0xe0af68))],
            [13, y, cell("e", boldRed())],
            [14, y, cell("s", normalText())],
            [16, y, cell("🎨", style())],
        ];
        batch.cells(mixedCells);
        y += 2;

        // Method 5: fill()
        batch.text(1, y++, "5. batch.fill(x, y, width, height, cellData)", style().bold().fg(0x9ece6a));
        batch.text(3, y++, "Fill rectangular regions efficiently", style().fg(0xa9b1d6));

        // Fill example
        batch.fill(3, y, 25, 1, cell("█", style().fg(0x414868)));
        batch.text(4, y, "Filled rectangle example", style().fg(0x1a1b26).bg(0x414868));
        y += 2;

        // Performance tips
        batch.text(1, y++, "⚡ Performance Tips:", style().bold().fg(0xe0af68));
        batch.text(3, y++, "• Use batch.text() for uniform styling (fastest)", style().fg(0xa9b1d6));
        batch.text(3, y++, "• Use batch.cells() only for mixed styles", style().fg(0xa9b1d6));
        batch.text(3, y++, "• Use batch.fill() for large rectangular areas", style().fg(0xa9b1d6));
        y++;

        // Style gotcha
        batch.text(1, y++, "💡 Style Objects:", style().bold().fg(0xe0af68));
        batch.text(3, y++, "• Create fresh: style().bold() (not reusable)", style().fg(0xa9b1d6));
        batch.text(3, y++, "• Helper functions: const bold = () => style().bold()", style().fg(0xa9b1d6));

        // Essential pattern
        const patternY = this.size.height - 6;
        batch.text(1, patternY, "Essential Pattern:", style().bold().fg(0x7dcfff));
        batch.text(1, patternY + 1, "1. const batch = renderer.batch()", style().fg(0xa9b1d6));
        batch.text(1, patternY + 2, "2. batch.clear/text/cell/cells/fill...", style().fg(0xa9b1d6));
        batch.text(1, patternY + 3, "3. batch.flush()  // Upload to GPU", style().fg(0xa9b1d6));
        batch.text(1, patternY + 4, "4. renderer.render()  // Draw frame", style().fg(0xa9b1d6));
    }

    render() {
        if (this.currentScreen === 1) {
            this.renderScreen1();
        } else {
            this.renderScreen2();
        }
        this.renderer.render();
    }

    startAnimation() {
        let frame = 0;
        const animate = () => {
            // Simple status update
            const batch = this.renderer.batch();

            const spinnerChars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            const spinnerChar = spinnerChars[Math.floor(frame / 6) % spinnerChars.length];

            batch.cell(this.size.width - 2, 1, cell(spinnerChar, style().fg(0x7aa2f7)));

            batch.flush();
            this.renderer.render();

            frame++;
            requestAnimationFrame(animate);
        };

        animate();
    }
}

// Initialize and run the demo
async function main() {
    try {
        const demo = new BatchAPIDemo();
        await demo.initialize();

        demo.render();

        console.log('✅ Batch API demo ready! Press SPACE to switch screens');
        demo.startAnimation();

    } catch (error) {
        console.error('❌ Demo failed:', error);
        document.body.innerHTML = `
            <div style="color: #f7768e; padding: 20px; text-align: center; font-family: monospace;">
                <h2>Failed to load Beamterm</h2>
                <p>${error.message}</p>
                <p>Make sure WASM packages are built: <code>./build.zsh build-wasm</code></p>
            </div>
        `;
    }
}

// Handle window resize
window.addEventListener('resize', () => {
    console.log('Window resized - you could update canvas size and re-render');
});

// Start the demo
main();