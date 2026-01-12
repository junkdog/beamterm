// Beamterm Selection Demo
import {
    main as init,
    style,
    cell,
    BeamtermRenderer,
    SelectionMode,
    ModifierKeys
} from '@beamterm/renderer';

class SelectionDemo {
    constructor() {
        this.renderer = null;
        this.size = null;
        this.selectionEnabled = false;

        // Get UI elements
        this.statusEl = document.getElementById('status');
        this.selectionModeEl = document.getElementById('selectionMode');
        this.enableBtn = document.getElementById('enableSelection');
        this.disableBtn = document.getElementById('disableSelection');

        // Modifier key checkboxes
        this.modifierCtrl = document.getElementById('modifierCtrl');
        this.modifierShift = document.getElementById('modifierShift');
        this.modifierAlt = document.getElementById('modifierAlt');
        this.modifierMeta = document.getElementById('modifierMeta');

        this.setupEventListeners();
    }

    setupEventListeners() {
        this.enableBtn.addEventListener('click', () => this.enableSelection());
        this.disableBtn.addEventListener('click', () => this.disableSelection());

        // Re-enable selection when mode changes
        this.selectionModeEl.addEventListener('change', () => {
            if (this.selectionEnabled) {
                console.log('🔄 Selection mode changed, re-enabling with new mode');
                this.enableSelection();
            }
        });

        // Re-enable selection when modifier key requirements change
        const modifierCheckboxes = [this.modifierCtrl, this.modifierShift, this.modifierAlt, this.modifierMeta];
        modifierCheckboxes.forEach(checkbox => {
            checkbox.addEventListener('change', () => {
                if (this.selectionEnabled) {
                    console.log('🔄 Modifier keys changed, re-enabling with new modifiers');
                    this.enableSelection();
                }
            });
        });
    }

    getRequiredModifiers() {
        let modifiers = ModifierKeys.NONE;
        if (this.modifierCtrl.checked) {
            modifiers = modifiers.or(ModifierKeys.CONTROL);
        }
        if (this.modifierShift.checked) {
            modifiers = modifiers.or(ModifierKeys.SHIFT);
        }
        if (this.modifierAlt.checked) {
            modifiers = modifiers.or(ModifierKeys.ALT);
        }
        if (this.modifierMeta.checked) {
            modifiers = modifiers.or(ModifierKeys.META);
        }
        return modifiers;
    }

    getModifierDescription() {
        const mods = [];
        if (this.modifierCtrl.checked) mods.push('Ctrl');
        if (this.modifierShift.checked) mods.push('Shift');
        if (this.modifierAlt.checked) mods.push('Alt');
        if (this.modifierMeta.checked) mods.push('Meta');
        return mods.length > 0 ? mods.join('+') + '+Click' : 'Click';
    }

    updateStatus(message, isError = false) {
        this.statusEl.textContent = message;
        this.statusEl.style.borderLeftColor = isError ? '#f7768e' : '#7aa2f7';
        this.statusEl.style.color = isError ? '#f7768e' : '#c0caf5';
    }

    async initialize() {
        try {
            // Initialize WASM module
            await init();
            this.updateStatus('WASM initialized successfully');

            // Create renderer
            this.renderer = new BeamtermRenderer('#terminal');
            this.size = this.renderer.terminalSize();
            
            console.log('✅ Terminal initialized:', `${this.size.width}×${this.size.height} cells`);

            // Set up canvas size
            const canvas = document.getElementById('terminal');
            this.renderer.resize(canvas.width, canvas.height);
            this.size = this.renderer.terminalSize();

            // Render initial content
            this.renderSampleContent();

            // Enable selection by default
            this.enableSelection();

        } catch (error) {
            console.error('❌ Initialization failed:', error);
            this.updateStatus(`Initialization failed: ${error.message}`, true);
        }
    }

    setupMouseEventLogging() {
        // Set up a mouse handler that just logs events when selection is disabled
        this.renderer.setMouseHandler((mouseEvent) => {
            const eventTypeStr = this.getEventTypeString(mouseEvent.event_type);
            console.log(`🖱️ Mouse ${eventTypeStr} at (${mouseEvent.col}, ${mouseEvent.row}) button=${mouseEvent.button}`);
            
            if (!this.selectionEnabled) {
                this.updateStatus(`Mouse ${eventTypeStr} at (${mouseEvent.col}, ${mouseEvent.row}) - Enable selection to select text`);
            }
        });
    }

    getEventTypeString(eventType) {
        // MouseEventType enum values
        switch(eventType) {
            case 0: return 'DOWN';
            case 1: return 'UP';  
            case 2: return 'MOVE';
            default: return 'UNKNOWN';
        }
    }

    enableSelection() {
        if (!this.renderer) return;

        try {
            const mode = this.selectionModeEl.value === 'linear' ?
                SelectionMode.Linear : SelectionMode.Block;
            const modifiers = this.getRequiredModifiers();
            const modifierDesc = this.getModifierDescription();

            console.log('🚀 Enabling selection with mode:', this.selectionModeEl.value, 'modifiers:', modifierDesc);

            // Enable built-in selection with auto-copy and modifier key requirements
            this.renderer.enableSelectionWithOptions(mode, true, modifiers);

            this.selectionEnabled = true;
            this.enableBtn.disabled = true;
            this.disableBtn.disabled = false;

            const modeStr = this.selectionModeEl.value;
            this.updateStatus(`Selection enabled (${modeStr} mode) - ${modifierDesc} and drag to select text`);

            console.log('✅ Selection enabled with mode:', modeStr, 'modifiers:', modifierDesc);
        } catch (error) {
            console.error('❌ Failed to enable selection:', error);
            this.updateStatus(`Failed to enable selection: ${error.message}`, true);
        }
    }

    disableSelection() {
        if (!this.renderer) return;

        // Clear any active selection
        this.renderer.clearSelection();
        
        // Re-setup mouse event logging
        this.setupMouseEventLogging();
        
        this.selectionEnabled = false;
        this.enableBtn.disabled = false;
        this.disableBtn.disabled = true;
        
        this.updateStatus('Selection disabled - Mouse events logged to console');
        console.log('✅ Selection disabled');
    }

    renderSampleContent() {
        const batch = this.renderer.batch();
        
        // Clear terminal
        batch.clear(0x000000);

        let y = 0;

        // Header
        batch.text(2, y++, "🖱️ Beamterm Selection Demo", style().bold().underline().fg(0x7aa2f7));
        y++;

        // Sample content for selection
        const lines = [
            "This is a sample terminal with selectable text content.",
            "Enable selection above, then click and drag to select text.",
            "",
            "Try both selection modes:",
            "• Linear mode - follows text flow like normal terminals",
            "• Block mode - selects rectangular areas like editors",
            "",
            "Try modifier key requirements:",
            "• Check Shift to require Shift+Click for selection",
            "• Combine modifiers like Ctrl+Shift for custom workflows",
            "",
            "Sample content for testing:",
            "┌────────────────────────────────────────────┐",
            "│  Column 1    │  Column 2    │  Column 3    │",
            "├────────────────────────────────────────────┤",
            "│  Data A      │  Data B      │  Data C      │",
            "│  Info X      │  Info Y      │  Info Z      │",
            "│  Value 1     │  Value 2     │  Value 3     │",
            "└────────────────────────────────────────────┘",
            "",
            "Programming code example:",
            "function selectText(mode) {",
            "    renderer.enableSelection(mode, true);",
            "    console.log('Selection enabled');",
            "}",
            "",
            "selectText(SelectionMode.Linear);",
            ""
        ];

        lines.forEach((line, index) => {
            if (line === "") {
                y++;
                return;
            }

            let lineStyle = style().fg(0xc0caf5);
            
            // Color code different types of content
            if (line.startsWith("🖱️") || line.startsWith("Try ") || line.startsWith("Sample content")) {
                lineStyle = style().bold().fg(0x7aa2f7);
            } else if (line.startsWith("•") || line.startsWith("┌") || line.startsWith("│") || line.startsWith("├") || line.startsWith("└")) {
                lineStyle = style().fg(0x9ece6a);
            } else if (line.includes("function") || line.includes("renderer.") || line.includes("console.")) {
                lineStyle = style().fg(0x7dcfff);
            } else if (line.startsWith("    ")) {
                lineStyle = style().fg(0xbb9af7);
            } else if (line.startsWith("🔍") || line.startsWith("📋") || line.startsWith("🔄")) {
                lineStyle = style().italic().fg(0xe0af68);
            } else if (line.startsWith("Line ") || line.startsWith("Test ")) {
                lineStyle = style().fg(0xf7768e);
            }

            batch.text(2, y++, line, lineStyle);
        });

        // Status line at bottom
        y = this.size.height - 2;
        batch.text(2, y, "Status: Ready - Enable selection to start testing", style().bold().fg(0x565f89));

        this.renderer.render();
    }

    startAnimation() {
        let frame = 0;
        const animate = () => {
            // Simple status animation
            const batch = this.renderer.batch();

            // Spinner for status indication
            const spinnerChars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            const spinnerChar = spinnerChars[Math.floor(frame / 8) % spinnerChars.length];

            // Show selection status indicator
            const hasSelection = this.renderer && this.renderer.hasSelection();
            const statusChar = hasSelection ? '📋' : (this.selectionEnabled ? '🖱️' : spinnerChar);
            const statusColor = hasSelection ? 0x9ece6a : (this.selectionEnabled ? 0x7aa2f7 : 0x565f89);

            batch.cell(this.size.width - 3, 1, cell(statusChar, style().fg(statusColor)));

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
        const demo = new SelectionDemo();
        await demo.initialize();

        console.log('✅ Selection demo ready!');
        console.log('💡 Click "Enable Selection" and try both Linear and Block modes');
        
        demo.startAnimation();

    } catch (error) {
        console.error('❌ Demo failed:', error);
        const container = document.querySelector('.container');
        container.innerHTML = `
            <div class="error">
                <h2>Failed to load Beamterm Selection Demo</h2>
                <p><strong>Error:</strong> ${error.message}</p>
                <p>Make sure WASM packages are built: <code>./scripts/build-wasm.zsh</code></p>
                <p>Then run: <code>cd js/examples/selection-demo && npm run dev</code></p>
            </div>
        `;
    }
}

// Handle window resize
window.addEventListener('resize', () => {
    console.log('💡 Window resized - you could update canvas size and re-render');
});

// Start the demo
main(); 