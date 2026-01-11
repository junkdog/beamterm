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

interface FontSettings {
    family: string;
    size: number;
    mode: 'dynamic' | 'static';
}

class TerminalApp {
    private renderer: BeamtermRenderer;
    private cols: number;
    private rows: number;
    private fontSettings: FontSettings;

    constructor(renderer: BeamtermRenderer, fontSettings: FontSettings) {
        this.renderer = renderer;
        this.fontSettings = fontSettings;
        const size = renderer.terminalSize();
        this.cols = size.width;
        this.rows = size.height;
    }

    public updateFontSettings(settings: FontSettings): void {
        this.fontSettings = settings;
    }

    public replaceWithDynamicAtlas(fontFamily: string, fontSize: number): void {
        this.renderer.replaceWithDynamicAtlas([fontFamily, 'monospace'], fontSize);
        this.fontSettings = { family: fontFamily, size: fontSize, mode: 'dynamic' };

        // Update terminal size after atlas change (cell size may have changed)
        const size = this.renderer.terminalSize();
        this.cols = size.width;
        this.rows = size.height;
    }

    public replaceWithStaticAtlas(): void {
        this.renderer.replaceWithStaticAtlas(null); // null = use default embedded atlas
        this.fontSettings = { family: 'Hack (embedded)', size: 15, mode: 'static' };

        const size = this.renderer.terminalSize();
        this.cols = size.width;
        this.rows = size.height;
    }

    public render(): void {
        const batch = this.renderer.batch();

        this.clear(batch);
        this.drawHeader(batch);
        this.drawFontInfo(batch);
        this.drawContent(batch);
        this.drawStatus(batch);

        batch.flush();
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
        const title = "beamterm: Runtime Atlas Replacement";
        const x = Math.floor((this.cols - title.length) / 2);

        batch.text(x, 1, title, style().bold().fg(tokyoNight.primary).bg(tokyoNight.bg));
    }

    private drawFontInfo(batch: Batch): void {
        const y = 3;
        const cellSize = this.renderer.cellSize();
        const modeLabel = this.fontSettings.mode === 'static' ? 'Static' : 'Dynamic';

        const info = [
            { label: 'Mode:', value: modeLabel, color: tokyoNight.primary },
            { label: 'Font:', value: this.fontSettings.family, color: tokyoNight.secondary },
            { label: 'Size:', value: `${this.fontSettings.size}px`, color: tokyoNight.secondary },
            { label: 'Cell:', value: `${cellSize.width}x${cellSize.height}`, color: tokyoNight.warning },
        ];

        let x = 2;
        info.forEach(item => {
            batch.text(x, y, item.label, style().fg(tokyoNight.fg).bg(tokyoNight.bg));
            x += item.label.length + 1;
            batch.text(x, y, item.value, style().bold().fg(item.color).bg(tokyoNight.bg));
            x += item.value.length + 3;
        });
    }

    private drawContent(batch: Batch): void {
        const windowY = 6;
        const windowHeight = this.rows - 10;

        const demoLines = [
            { text: "# Dynamic atlas: change font/size above!", color: tokyoNight.warning },
            { text: "", color: tokyoNight.fg },
            { text: "  The terminal content is preserved when", color: tokyoNight.fg },
            { text: "  switching fonts - only the rendering", color: tokyoNight.fg },
            { text: "  atlas is replaced at runtime.", color: tokyoNight.fg },
            { text: "", color: tokyoNight.fg },
            { text: "  Emoji:  🚀🔥✨🎮🎯💻🦀📦", color: tokyoNight.fg },
            { text: "  CJK:    日本語 中文 한국어", color: tokyoNight.fg },
            { text: "  Arrows: ← → ↑ ↓ ⇐ ⇒ ⇑ ⇓", color: tokyoNight.fg },
            { text: "  Math:   ∑ ∏ ∫ √ ∞ ≈ ≠ ≤ ≥", color: tokyoNight.fg },
            { text: "  Box:    ┌─┬─┐ ╔═╦═╗", color: tokyoNight.fg },
            { text: "          │ │ │ ║ ║ ║", color: tokyoNight.fg },
            { text: "          └─┴─┘ ╚═╩═╝", color: tokyoNight.fg },
            { text: "", color: tokyoNight.fg },
            { text: "  ➜ Glyphs rasterized on-demand via Canvas API", color: tokyoNight.primary },
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

        const bar = '─'.repeat(this.cols);
        batch.text(0, y, bar, style().fg(tokyoNight.fg).bg(tokyoNight.bg));

        const x = this.cols - status.length - 2;
        batch.text(x, y, status, style().fg(tokyoNight.secondary).bg(tokyoNight.bg));
    }
}

class AnimationController {
    private app: TerminalApp;
    private lastTime: number = 0;
    private updateInterval: number = 16;

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

function showStatus(): void {
    const status = document.getElementById('status');
    if (status) {
        status.classList.add('visible');
        setTimeout(() => status.classList.remove('visible'), 1500);
    }
}

async function getMonospaceFonts(): Promise<string[]> {
    // Check if Local Font Access API is available
    if (!('queryLocalFonts' in window)) {
        console.log('Local Font Access API not available, using fallback list');
        return ['monospace'];
    }

    try {
        // @ts-ignore - queryLocalFonts is not in TypeScript's lib yet
        const fonts: FontData[] = await window.queryLocalFonts();

        // Filter for likely monospace fonts and get unique family names
        const monoKeywords = ['mono', 'code', 'consol', 'courier', 'terminal', 'fixed', 'hack', 'fira', 'jetbrains', 'source code', 'ibm plex'];
        const monoFamilies = new Set<string>();

        for (const font of fonts) {
            const familyLower = font.family.toLowerCase();
            if (monoKeywords.some(keyword => familyLower.includes(keyword))) {
                monoFamilies.add(font.family);
            }
        }

        const sorted = Array.from(monoFamilies).sort();
        return sorted.length > 0 ? sorted : ['monospace'];
    } catch (error) {
        console.log('Font access denied or failed:', error);
        return ['monospace'];
    }
}

interface FontData {
    family: string;
    fullName: string;
    postscriptName: string;
    style: string;
}

async function populateFontSelect(selectElement: HTMLSelectElement): Promise<string> {
    const fonts = await getMonospaceFonts();

    selectElement.innerHTML = '';
    fonts.forEach(font => {
        const option = document.createElement('option');
        option.value = font;
        option.textContent = font;
        selectElement.appendChild(option);
    });

    // Return the first available font as default
    return fonts[0];
}

async function main() {
    await init();

    const fontSelect = document.getElementById('font-select') as HTMLSelectElement;
    const loadFontsBtn = document.getElementById('load-fonts-btn') as HTMLButtonElement;
    const atlasModeSelect = document.getElementById('atlas-mode') as HTMLSelectElement;
    const fontControls = document.getElementById('font-controls') as HTMLDivElement;
    const sizeControls = document.getElementById('size-controls') as HTMLDivElement;
    const initialFont = 'monospace';
    const initialSize = 16;

    const renderer = BeamtermRenderer.withDynamicAtlas(
        '#terminal',
        [initialFont],
        initialSize
    );

    const fontSettings: FontSettings = { family: initialFont, size: initialSize, mode: 'dynamic' };
    const app = new TerminalApp(renderer, fontSettings);

    // Set initial canvas size
    const canvas = document.getElementById('terminal') as HTMLCanvasElement;
    const { width, height } = calculateCanvasSize();
    canvas.width = width;
    canvas.height = height;

    app.resizeTerminal(width, height);

    // Start animation loop
    const animationController = new AnimationController(app);
    animationController.start();

    // Helper to resize after atlas change
    const resizeAfterAtlasChange = () => {
        const { width, height } = calculateCanvasSize();
        canvas.width = width;
        canvas.height = height;
        app.resizeTerminal(width, height);
    };

    // Atlas mode toggle handler
    atlasModeSelect.addEventListener('change', () => {
        const mode = atlasModeSelect.value;
        if (mode === 'static') {
            app.replaceWithStaticAtlas();
            fontControls.style.display = 'none';
            sizeControls.style.display = 'none';
        } else {
            const currentFont = fontSelect.value;
            const currentSize = parseInt((document.getElementById('size-slider') as HTMLInputElement).value);
            app.replaceWithDynamicAtlas(currentFont, currentSize);
            fontControls.style.display = 'flex';
            sizeControls.style.display = 'flex';
        }
        resizeAfterAtlasChange();
        showStatus();
    });

    // Load fonts button - triggers permission request
    loadFontsBtn.addEventListener('click', async () => {
        const currentFont = fontSelect.value;
        await populateFontSelect(fontSelect);
        // Try to preserve current selection
        if (Array.from(fontSelect.options).some(opt => opt.value === currentFont)) {
            fontSelect.value = currentFont;
        }
        loadFontsBtn.style.display = 'none';
    });

    // Font selection handler
    fontSelect.addEventListener('change', () => {
        const newFont = fontSelect.value;
        const currentSize = parseInt((document.getElementById('size-slider') as HTMLInputElement).value);
        app.replaceWithDynamicAtlas(newFont, currentSize);
        showStatus();
    });

    // Size slider handler
    const sizeSlider = document.getElementById('size-slider') as HTMLInputElement;
    const sizeValue = document.getElementById('size-value') as HTMLSpanElement;

    sizeSlider.addEventListener('input', () => {
        sizeValue.textContent = `${sizeSlider.value}px`;
    });

    sizeSlider.addEventListener('change', () => {
        const newSize = parseInt(sizeSlider.value);
        const currentFont = fontSelect.value;
        app.replaceWithDynamicAtlas(currentFont, newSize);
        resizeAfterAtlasChange();
        showStatus();
    });

    // Handle window resize
    let resizeTimeout: number;
    window.addEventListener('resize', () => {
        clearTimeout(resizeTimeout);
        resizeTimeout = window.setTimeout(() => {
            resizeAfterAtlasChange();
        }, 100);
    });
}

function calculateCanvasSize(): { width: number; height: number } {
    const width = Math.min(window.innerWidth - 40, 1200);
    const height = Math.min(window.innerHeight - 160, 800);

    return { width, height };
}

main().catch(error => {
    console.error('Failed to initialize Beamterm:', error);
});
