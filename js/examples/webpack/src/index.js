import {BeamtermRenderer, SelectionMode, cell, main as init, style} from '@beamterm/renderer';

// Initialize and run the terminal demo
async function main() {
    try {
        // Initialize WASM module
        await init();
        console.log('✅ WASM module initialized');

        const fps = fpsCounter();

        // Create renderer instance
        const renderer = new BeamtermRenderer('#terminal');
        renderer.enableSelection(SelectionMode.Block, true);
        const size = renderer.terminalSize();
        console.log(`✅ Terminal created: ${size.width}x${size.height}`);

        // Create demo app
        const app = new TerminalDemo(renderer);

        // Initial render
        app.fullRender(fps);

        // Start animation loop
        app.startAnimation(fps);

        // Handle window resize
        window.addEventListener('resize', () => {
            const canvas = document.getElementById('terminal');
            canvas.width = window.innerWidth - 40;
            canvas.height = window.innerHeight - 40;

            renderer.resize(canvas.width, canvas.height);
            app.updateSize();
            app.fullRender(fps);
        });

    } catch (error) {
        console.error('❌ Failed to initialize:', error);
        document.body.innerHTML = `
            <div style="color: red; padding: 20px;">
                <h2>Failed to initialize Beamterm</h2>
                <pre>${error.message}</pre>
                <p>Make sure WASM is built: <code>./build.zsh build-wasm</code></p>
            </div>
        `;
    }
}

// Terminal demo application
class TerminalDemo {
    constructor(renderer) {
        this.renderer = renderer;
        this.frame = 0;
        this.size = renderer.terminalSize();
    }

    updateSize() {
        this.size = this.renderer.terminalSize();
    }

    fullRender(fps) {
        // Create a new batch for this frame
        const batch = this.renderer.batch();

        // Clear the terminal
        batch.clear(0x1a1b26);

        // Draw all UI elements
        this.drawBorder(batch);
        this.drawHeader(batch);
        this.drawContent(batch);
        this.drawColorPalette(batch);
        this.drawStatusBar(batch, fps);
        this.drawWaveAnimation(batch)

        // Render the frame
        this.renderer.render();
    }

    render(fps) {
        // Create a new batch for this frame
        const batch = this.renderer.batch();

        // Draw animated UI elements
        this.drawStatusBar(batch, fps);
        this.drawWaveAnimation(batch)

        const spinnerChars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        const spinnerChar = spinnerChars[(this.frame >> 3) % spinnerChars.length];
        let spinnerStyle = style().fg(0x7aa2f7).bg(0x24283b);
        batch.cell(this.size.width - 2, this.size.height - 2, cell(spinnerChar, spinnerStyle));

        // Render the frame
        this.renderer.render();
    }

    drawBorder(batch) {
        const borderStyle = style();
        const borderColor = 0x414868;
        const bgColor = 0x1a1b26;

        // Collect all border cells for efficient batch update
        const borderCells = [];

        // Top and bottom borders
        for (let x = 0; x < this.size.width; x++) {
            borderCells.push([x, 0, { symbol: '─', style: borderStyle.bits, fg: borderColor, bg: bgColor }]);
            borderCells.push([x, this.size.height - 1, { symbol: '─', style: borderStyle.bits, fg: borderColor, bg: bgColor }]);
        }

        // Left and right borders
        for (let y = 0; y < this.size.height; y++) {
            borderCells.push([0, y, { symbol: '│', style: borderStyle.bits, fg: borderColor, bg: bgColor }]);
            borderCells.push([this.size.width - 1, y, { symbol: '│', style: borderStyle.bits, fg: borderColor, bg: bgColor }]);
        }

        // Corners
        const corners = [
            { pos: [0, 0], char: '┌' },
            { pos: [this.size.width - 1, 0], char: '┐' },
            { pos: [0, this.size.height - 1], char: '└' },
            { pos: [this.size.width - 1, this.size.height - 1], char: '┘' }
        ];

        corners.forEach(({ pos: [x, y], char }) => {
            borderCells.push([x, y, { symbol: char, style: borderStyle.bits, fg: borderColor, bg: bgColor }]);
        });

        // Update all border cells in one efficient call
        batch.cells(borderCells);
    }

    drawHeader(batch) {
        const title = " 🚀 beamterm Webpack Example ";
        const startX = Math.floor((this.size.width - title.length) / 2);

        batch.text(startX, 0, title, style().bold().fg(0x7aa2f7).bg(0x1a1b26));

        // Version info
        const version = "v0.1.0";
        batch.text(this.size.width - version.length - 2, 0, version, style().bold().fg(0x7aa2f7).bg(0x1a1b26));
    }

    drawContent(batch) {
        // Welcome message with different styles
        const content = [
            { text: "Welcome to beamterm!", y: 3, style: style().bold().fg(0x9ece6a) },
            { text: "High-performance WebGL2 terminal renderer", y: 5, style: style().fg(0xc0caf5) },
        ];

        content.forEach(({ text, y, style }) => {
            const x = Math.floor((this.size.width - text.length) / 2);
            batch.text(x, y, text, style.bg(0x1a1b26));
        });

        // Feature list
        const features = [
            "• GPU-accelerated text rendering",
            "• Multiple text styles",
            "• Emoji support 🫡🤔🐥",
            "• Zero allocations in render loop",
            "• Experimental TS/JS API",
        ];

        features.forEach((feature, i) => {
            batch.text(4, 7 + i, feature, style().fg(0xa9b1d6).bg(0x1a1b26));
        });

        // Style demonstrations
        this.drawStyleDemo(batch, 14);
    }

    drawStyleDemo(batch, startY) {
        batch.text(4, startY, "Text Styles:", style().bold().fg(0xc0caf5).bg(0x1a1b26));

        const styleDemo = [
            { text: "Normal", style: style(), x: 4 },
            { text: "Bold", style: style().bold(), x: 14 },
            { text: "Italic", style: style().italic(), x: 22 },
            { text: "Underline", style: style().underline(), x: 32 },
            { text: "Strikethrough", style: style().strikethrough(), x: 45 },
        ];

        styleDemo.forEach(({ text, style, x }) => {
            batch.text(x, startY + 2, text, style.fg(0x7aa2f7).bg(0x1a1b26));
        });

        // Combined styles
        const combined = style().bold().italic().underline().fg(0xbb9af7).bg(0x1a1b26)
        batch.text(4, startY + 4, "Combined: Bold + Italic + Underline", combined);
    }

    drawColorPalette(batch) {
        const startY = 20;
        const colors = [
            { name: "Red", value: 0xf7768e },
            { name: "Green", value: 0x9ece6a },
            { name: "Blue", value: 0x7aa2f7 },
            { name: "Purple", value: 0xbb9af7 },
            { name: "Orange", value: 0xe0af68 },
            { name: "Cyan", value: 0x7dcfff },
        ];

        batch.text(4, startY, "Color Palette:", style().bold().fg(0xc0caf5).bg(0x1a1b26));

        // Collect all color block cells for batch update
        const colorCells = [];

        colors.forEach((color, i) => {
            const x = 4 + (i % 3) * 20;
            const y = startY + 2 + Math.floor(i / 3) * 2;

            // Color blocks (5 cells wide for better visibility)
            for (let j = 0; j < 5; j++) {
                colorCells.push([x + j, y, {
                    symbol: '█',
                    style: 0,
                    fg: color.value,
                    bg: 0x1a1b26
                }]);
            }

            // Color name
            batch.text(x + 6, y, color.name, style().fg(0xa9b1d6).bg(0x1a1b26));
        });

        // Update all color cells at once for efficiency
        batch.cells(colorCells);
    }

    drawStatusBar(batch, fps) {
        const y = this.size.height - 2;

        // Status bar background
        const statusBg = [];
        for (let x = 1; x < this.size.width - 1; x++) {
            statusBg.push([x, y, { symbol: ' ', style: 0, fg: 0xc0caf5, bg: 0x24283b }]);
        }
        batch.cells(statusBg);

        // Status text
        const status = ` FPS: ${fps.tick().toFixed(1)} | Cells: ${this.size.width * this.size.height} | Frame: ${this.frame} `;
        batch.text(2, y, status, style().fg(0xc0caf5).bg(0x24283b));

        // Right-aligned info
        const info = "Press F11 for fullscreen ";
        batch.text(this.size.width - info.length - 3, y, info, style().fg(0xa9b1d6).bg(0x24283b));
    }

    startAnimation(fps) {
        const animate = () => {
            this.render(fps);
            this.frame++;
            requestAnimationFrame(animate);
        };

        animate();
    }

    drawWaveAnimation(batch) {
        const waveY = 26;
        const waveChars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '▇', '▆', '▅', '▄', '▃', '▂'];
        const waveColors = [0x7aa2f7, 0x7dcfff, 0x89ddff, 0x7dcfff, 0x7aa2f7];

        const waveCells = [];

        for (let x = 1; x < this.size.width - 1; x++) {
            const offset = (this.frame / 2 + x * 2) % waveChars.length;
            const charIndex = Math.floor(offset) % waveChars.length;
            const colorIndex = Math.floor((this.frame / 10 + x) % waveColors.length);

            waveCells.push([x , waveY, {
                symbol: waveChars[charIndex],
                style: style().bits,
                fg: waveColors[colorIndex],
                bg: 0x1a1b26
            }]);
        }

        batch.cells(waveCells);
    }
}

function fpsCounter() {
    let samples = [];
    let lastTime = performance.now();
    let remaining_repeat = 0;
    let reported = 0.0;

    return {
        tick() {
            const now = performance.now();
            const delta = now - lastTime;
            lastTime = now;

            // Keep last 3 samples
            samples.push(1000 / delta);
            if (samples.length > 3) samples.shift();

            if (remaining_repeat > 0) {
                remaining_repeat--;
            } else {
                remaining_repeat = 30;
                if (samples.length === 3) {
                    const sorted = [...samples].sort((a, b) => a - b);
                    reported = sorted[1];
                } else {
                    reported = samples[samples.length - 1];
                }
            }

            return reported;
        }
    };
}

// Start the application when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', main);
} else {
    main();
}