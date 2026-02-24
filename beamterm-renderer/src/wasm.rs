use std::{cell::RefCell, rc::Rc};

use beamterm_data::{FontAtlasData, Glyph};
use compact_str::CompactString;
use serde_wasm_bindgen::from_value;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use wasm_bindgen::prelude::*;
use web_sys::console;

use crate::{
    CursorPosition, Terminal,
    gl::{
        CellData, CellQuery as RustCellQuery, SelectionMode as RustSelectionMode, TerminalGrid,
        select,
    },
    mouse::{ModifierKeys as RustModifierKeys, MouseSelectOptions, TerminalMouseEvent},
};

/// JavaScript wrapper for the terminal renderer.
///
/// Thin `#[wasm_bindgen]` wrapper that delegates to [`Terminal`].
#[wasm_bindgen]
#[derive(Debug)]
pub struct BeamtermRenderer {
    terminal: Terminal,
}

/// JavaScript wrapper for cell data
#[wasm_bindgen]
#[derive(Debug, Default, serde::Deserialize)]
pub struct Cell {
    symbol: CompactString,
    style: u16,
    fg: u32,
    bg: u32,
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct CellStyle {
    fg: u32,
    bg: u32,
    style_bits: u16,
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct Batch {
    terminal_grid: Rc<RefCell<TerminalGrid>>,
}

/// Selection mode for text selection in the terminal
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub enum SelectionMode {
    /// Rectangular block selection
    Block,
    /// Linear text flow selection
    Linear,
}

/// Type of mouse event
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub enum MouseEventType {
    /// Mouse button pressed
    MouseDown,
    /// Mouse button released
    MouseUp,
    /// Mouse moved
    MouseMove,
    /// Mouse button clicked (pressed and released)
    Click,
    /// Mouse cursor entered the terminal area
    MouseEnter,
    /// Mouse cursor left the terminal area
    MouseLeave,
}

/// Mouse event data with terminal coordinates
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// Type of mouse event
    pub event_type: MouseEventType,
    /// Column in terminal grid (0-based)
    pub col: u16,
    /// Row in terminal grid (0-based)
    pub row: u16,
    /// Mouse button (0=left, 1=middle, 2=right)
    pub button: i16,
    /// Whether Ctrl key was pressed
    pub ctrl_key: bool,
    /// Whether Shift key was pressed
    pub shift_key: bool,
    /// Whether Alt key was pressed
    pub alt_key: bool,
    /// Whether Meta key was pressed (Command on macOS, Windows key on Windows)
    pub meta_key: bool,
}

/// Modifier key flags for mouse selection.
///
/// Use bitwise OR to combine multiple modifiers:
/// ```javascript
/// const modifiers = ModifierKeys.SHIFT | ModifierKeys.CONTROL;
/// renderer.enableSelectionWithOptions(SelectionMode.Block, true, modifiers);
/// ```
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierKeys(u8);

#[wasm_bindgen]
#[allow(non_snake_case)]
impl ModifierKeys {
    /// No modifier keys required
    #[wasm_bindgen(getter)]
    pub fn NONE() -> ModifierKeys {
        ModifierKeys(0)
    }

    /// Control key (Ctrl)
    #[wasm_bindgen(getter)]
    pub fn CONTROL() -> ModifierKeys {
        ModifierKeys(RustModifierKeys::CONTROL.bits())
    }

    /// Shift key
    #[wasm_bindgen(getter)]
    pub fn SHIFT() -> ModifierKeys {
        ModifierKeys(RustModifierKeys::SHIFT.bits())
    }

    /// Alt key (Option on macOS)
    #[wasm_bindgen(getter)]
    pub fn ALT() -> ModifierKeys {
        ModifierKeys(RustModifierKeys::ALT.bits())
    }

    /// Meta key (Command on macOS, Windows key on Windows)
    #[wasm_bindgen(getter)]
    pub fn META() -> ModifierKeys {
        ModifierKeys(RustModifierKeys::META.bits())
    }

    /// Combines two modifier key sets using bitwise OR
    #[wasm_bindgen(js_name = "or")]
    pub fn or(&self, other: &ModifierKeys) -> ModifierKeys {
        ModifierKeys(self.0 | other.0)
    }
}

/// Query for selecting cells in the terminal
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct CellQuery {
    inner: RustCellQuery,
}

/// Result of URL detection at a terminal position.
///
/// Contains the detected URL string and a `CellQuery` for highlighting
/// or extracting the URL region.
#[wasm_bindgen]
#[derive(Debug)]
pub struct UrlMatch {
    /// The detected URL string
    url: String,
    /// Query for the URL's cell range
    query: CellQuery,
}

#[wasm_bindgen]
impl UrlMatch {
    /// Returns the detected URL string.
    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Returns a `CellQuery` for the URL's position in the terminal grid.
    ///
    /// This can be used for highlighting or extracting text.
    #[wasm_bindgen(getter)]
    pub fn query(&self) -> CellQuery {
        self.query.clone()
    }
}

#[wasm_bindgen]
impl CellQuery {
    /// Create a new cell query with the specified selection mode
    #[wasm_bindgen(constructor)]
    pub fn new(mode: SelectionMode) -> CellQuery {
        CellQuery { inner: select(mode.into()) }
    }

    /// Set the starting position for the selection
    pub fn start(mut self, col: u16, row: u16) -> CellQuery {
        self.inner = self.inner.start((col, row));
        self
    }

    /// Set the ending position for the selection
    pub fn end(mut self, col: u16, row: u16) -> CellQuery {
        self.inner = self.inner.end((col, row));
        self
    }

    /// Configure whether to trim trailing whitespace from lines
    #[wasm_bindgen(js_name = "trimTrailingWhitespace")]
    pub fn trim_trailing_whitespace(mut self, enabled: bool) -> CellQuery {
        self.inner = self.inner.trim_trailing_whitespace(enabled);
        self
    }

    /// Check if the query is empty (no selection range)
    #[wasm_bindgen(js_name = "isEmpty")]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[wasm_bindgen]
pub fn style() -> CellStyle {
    CellStyle::new()
}

#[wasm_bindgen]
pub fn cell(symbol: &str, style: CellStyle) -> Cell {
    Cell {
        symbol: symbol.into(),
        style: style.style_bits,
        fg: style.fg,
        bg: style.bg,
    }
}

#[wasm_bindgen]
impl CellStyle {
    /// Create a new TextStyle with default (normal) style
    #[wasm_bindgen(constructor)]
    pub fn new() -> CellStyle {
        Default::default()
    }

    /// Sets the foreground color
    #[wasm_bindgen]
    pub fn fg(mut self, color: u32) -> CellStyle {
        self.fg = color;
        self
    }

    /// Sets the background color
    #[wasm_bindgen]
    pub fn bg(mut self, color: u32) -> CellStyle {
        self.bg = color;
        self
    }

    /// Add bold style
    #[wasm_bindgen]
    pub fn bold(mut self) -> CellStyle {
        self.style_bits |= Glyph::BOLD_FLAG;
        self
    }

    /// Add italic style
    #[wasm_bindgen]
    pub fn italic(mut self) -> CellStyle {
        self.style_bits |= Glyph::ITALIC_FLAG;
        self
    }

    /// Add underline effect
    #[wasm_bindgen]
    pub fn underline(mut self) -> CellStyle {
        self.style_bits |= Glyph::UNDERLINE_FLAG;
        self
    }

    /// Add strikethrough effect
    #[wasm_bindgen]
    pub fn strikethrough(mut self) -> CellStyle {
        self.style_bits |= Glyph::STRIKETHROUGH_FLAG;
        self
    }

    /// Get the combined style bits
    #[wasm_bindgen(getter)]
    pub fn bits(&self) -> u16 {
        self.style_bits
    }
}

impl Default for CellStyle {
    fn default() -> Self {
        CellStyle {
            fg: 0xFFFFFF,  // Default foreground color (white)
            bg: 0x000000,  // Default background color (black)
            style_bits: 0, // No styles applied
        }
    }
}

#[wasm_bindgen]
impl Batch {
    /// Updates a single cell at the given position.
    #[wasm_bindgen(js_name = "cell")]
    pub fn cell(&mut self, x: u16, y: u16, cell_data: &Cell) {
        let _ = self
            .terminal_grid
            .borrow_mut()
            .update_cell(x, y, cell_data.as_cell_data());
    }

    /// Updates a cell by its buffer index.
    #[wasm_bindgen(js_name = "cellByIndex")]
    pub fn cell_by_index(&mut self, idx: usize, cell_data: &Cell) {
        let _ = self
            .terminal_grid
            .borrow_mut()
            .update_cell_by_index(idx, cell_data.as_cell_data());
    }

    /// Updates multiple cells from an array.
    /// Each element should be [x, y, cellData].
    #[wasm_bindgen(js_name = "cells")]
    pub fn cells(&mut self, cells_json: JsValue) -> Result<(), JsValue> {
        let updates = from_value::<Vec<(u16, u16, Cell)>>(cells_json)
            .map_err(|e| JsValue::from_str(&e.to_string()));

        match updates {
            Ok(cells) => {
                let cell_data = cells
                    .iter()
                    .map(|(x, y, data)| (*x, *y, data.as_cell_data()));

                let mut terminal_grid = self.terminal_grid.borrow_mut();
                terminal_grid
                    .update_cells_by_position(cell_data)
                    .map_err(|e| JsValue::from_str(&e.to_string()))
            },
            e => e.map(|_| ()),
        }
    }

    /// Write text to the terminal
    #[wasm_bindgen(js_name = "text")]
    pub fn text(&mut self, x: u16, y: u16, text: &str, style: &CellStyle) -> Result<(), JsValue> {
        let mut terminal_grid = self.terminal_grid.borrow_mut();
        let (cols, rows) = terminal_grid.terminal_size();

        if y >= rows {
            return Ok(()); // oob, ignore
        }

        let mut col_offset: u16 = 0;
        for ch in text.graphemes(true) {
            let char_width = if ch.len() == 1 { 1 } else { ch.width() };

            // Skip zero-width characters (they don't occupy terminal cells)
            if char_width == 0 {
                continue;
            }

            let current_col = x + col_offset;
            if current_col >= cols {
                break;
            }

            let cell = CellData::new_with_style_bits(ch, style.style_bits, style.fg, style.bg);
            terminal_grid
                .update_cell(current_col, y, cell)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;

            col_offset += char_width as u16;
        }

        Ok(())
    }

    /// Fill a rectangular region
    #[wasm_bindgen(js_name = "fill")]
    pub fn fill(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        cell_data: &Cell,
    ) -> Result<(), JsValue> {
        let mut terminal_grid = self.terminal_grid.borrow_mut();
        let (cols, rows) = terminal_grid.terminal_size();

        let width = (x + width).min(cols).saturating_sub(x);
        let height = (y + height).min(rows).saturating_sub(y);

        let fill_cell = cell_data.as_cell_data();
        for y in y..y + height {
            for x in x..x + width {
                terminal_grid
                    .update_cell(x, y, fill_cell)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Clear the terminal with specified background color
    #[wasm_bindgen]
    pub fn clear(&mut self, bg: u32) -> Result<(), JsValue> {
        let mut terminal_grid = self.terminal_grid.borrow_mut();
        let (cols, rows) = terminal_grid.terminal_size();

        let clear_cell = CellData::new_with_style_bits(" ", 0, 0xFFFFFF, bg);
        for y in 0..rows {
            for x in 0..cols {
                terminal_grid
                    .update_cell(x, y, clear_cell)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Synchronize all pending updates to the GPU
    #[wasm_bindgen]
    #[deprecated(since = "0.4.0", note = "no-op, flush is now automatic")]
    #[allow(deprecated)]
    pub fn flush(&mut self) -> Result<(), JsValue> {
        Ok(())
    }
}

#[wasm_bindgen]
impl Cell {
    #[wasm_bindgen(constructor)]
    pub fn new(symbol: String, style: &CellStyle) -> Cell {
        Cell {
            symbol: symbol.into(),
            style: style.style_bits,
            fg: style.fg,
            bg: style.bg,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn symbol(&self) -> String {
        self.symbol.to_string()
    }

    #[wasm_bindgen(setter)]
    pub fn set_symbol(&mut self, symbol: String) {
        self.symbol = symbol.into();
    }

    #[wasm_bindgen(getter)]
    pub fn fg(&self) -> u32 {
        self.fg
    }

    #[wasm_bindgen(setter)]
    pub fn set_fg(&mut self, color: u32) {
        self.fg = color;
    }

    #[wasm_bindgen(getter)]
    pub fn bg(&self) -> u32 {
        self.bg
    }

    #[wasm_bindgen(setter)]
    pub fn set_bg(&mut self, color: u32) {
        self.bg = color;
    }

    #[wasm_bindgen(getter)]
    pub fn style(&self) -> u16 {
        self.style
    }

    #[wasm_bindgen(setter)]
    pub fn set_style(&mut self, style: u16) {
        self.style = style;
    }
}

impl Cell {
    pub fn as_cell_data(&self) -> CellData<'_> {
        CellData::new_with_style_bits(&self.symbol, self.style, self.fg, self.bg)
    }
}

#[wasm_bindgen]
impl BeamtermRenderer {
    /// Create a new terminal renderer with the default embedded font atlas.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<BeamtermRenderer, JsValue> {
        Self::with_static_atlas(canvas_id, None, None)
    }

    /// Create a terminal renderer with custom static font atlas data.
    ///
    /// # Arguments
    /// * `canvas_id` - CSS selector for the canvas element
    /// * `atlas_data` - Binary atlas data (from .atlas file), or null for default
    /// * `auto_resize_canvas_css` - Whether to automatically set canvas CSS dimensions
    ///   on resize. Set to `false` when external CSS (flexbox, grid) controls sizing.
    ///   Defaults to `true` if not specified.
    #[wasm_bindgen(js_name = "withStaticAtlas")]
    pub fn with_static_atlas(
        canvas_id: &str,
        atlas_data: Option<js_sys::Uint8Array>,
        auto_resize_canvas_css: Option<bool>,
    ) -> Result<BeamtermRenderer, JsValue> {
        console_error_panic_hook::set_once();

        let atlas =
            match atlas_data {
                Some(data) => {
                    let bytes = data.to_vec();
                    Some(FontAtlasData::from_binary(&bytes).map_err(|e| {
                        JsValue::from_str(&format!("Failed to parse atlas data: {e}"))
                    })?)
                },
                None => None,
            };

        let mut builder = Terminal::builder(canvas_id)
            .auto_resize_canvas_css(auto_resize_canvas_css.unwrap_or(true));

        if let Some(atlas) = atlas {
            builder = builder.font_atlas(atlas);
        }

        let terminal = builder.build()?;

        Ok(BeamtermRenderer { terminal })
    }

    /// Create a terminal renderer with a dynamic font atlas using browser fonts.
    ///
    /// The dynamic atlas rasterizes glyphs on-demand using the browser's canvas API,
    /// enabling support for any system font, emoji, and complex scripts.
    ///
    /// # Arguments
    /// * `canvas_id` - CSS selector for the canvas element
    /// * `font_family` - Array of font family names (e.g., `["Hack", "JetBrains Mono"]`)
    /// * `font_size` - Font size in pixels
    /// * `auto_resize_canvas_css` - Whether to automatically set canvas CSS dimensions
    ///   on resize. Set to `false` when external CSS (flexbox, grid) controls sizing.
    ///   Defaults to `true` if not specified.
    ///
    /// # Example
    /// ```javascript
    /// const renderer = BeamtermRenderer.withDynamicAtlas(
    ///     "#terminal",
    ///     ["JetBrains Mono", "Fira Code"],
    ///     16.0
    /// );
    /// ```
    #[wasm_bindgen(js_name = "withDynamicAtlas")]
    pub fn with_dynamic_atlas(
        canvas_id: &str,
        font_family: js_sys::Array,
        font_size: f32,
        auto_resize_canvas_css: Option<bool>,
    ) -> Result<BeamtermRenderer, JsValue> {
        console_error_panic_hook::set_once();

        let font_families: Vec<String> = font_family
            .iter()
            .filter_map(|v| v.as_string())
            .collect();

        if font_families.is_empty() {
            return Err(JsValue::from_str("font_family array cannot be empty"));
        }

        let refs: Vec<&str> = font_families.iter().map(String::as_str).collect();

        let terminal = Terminal::builder(canvas_id)
            .auto_resize_canvas_css(auto_resize_canvas_css.unwrap_or(true))
            .dynamic_font_atlas(&refs, font_size)
            .build()?;

        Ok(BeamtermRenderer { terminal })
    }

    /// Enable default mouse selection behavior with built-in copy to clipboard
    #[wasm_bindgen(js_name = "enableSelection")]
    pub fn enable_selection(
        &mut self,
        mode: SelectionMode,
        trim_whitespace: bool,
    ) -> Result<(), JsValue> {
        self.enable_selection_with_options(mode, trim_whitespace, &ModifierKeys::default())
    }

    /// Enable mouse selection with full configuration options.
    ///
    /// This method allows specifying modifier keys that must be held for selection
    /// to activate, in addition to the selection mode and whitespace trimming.
    ///
    /// # Arguments
    /// * `mode` - Selection mode (Block or Linear)
    /// * `trim_whitespace` - Whether to trim trailing whitespace from selected text
    /// * `require_modifiers` - Modifier keys that must be held to start selection
    ///
    /// # Example
    /// ```javascript
    /// // Require Shift+Click to start selection
    /// renderer.enableSelectionWithOptions(
    ///     SelectionMode.Linear,
    ///     true,
    ///     ModifierKeys.SHIFT
    /// );
    ///
    /// // Require Ctrl+Shift+Click
    /// renderer.enableSelectionWithOptions(
    ///     SelectionMode.Block,
    ///     false,
    ///     ModifierKeys.CONTROL.or(ModifierKeys.SHIFT)
    /// );
    /// ```
    #[wasm_bindgen(js_name = "enableSelectionWithOptions")]
    pub fn enable_selection_with_options(
        &mut self,
        mode: SelectionMode,
        trim_whitespace: bool,
        require_modifiers: &ModifierKeys,
    ) -> Result<(), JsValue> {
        let options = MouseSelectOptions::new()
            .selection_mode(mode.into())
            .trim_trailing_whitespace(trim_whitespace)
            .require_modifier_keys((*require_modifiers).into());

        Ok(self.terminal.enable_mouse_selection(options)?)
    }

    /// Set a custom mouse event handler
    #[wasm_bindgen(js_name = "setMouseHandler")]
    pub fn set_mouse_handler(&mut self, handler: js_sys::Function) -> Result<(), JsValue> {
        let handler_closure = {
            let handler = handler.clone();
            move |event: TerminalMouseEvent, _grid: &TerminalGrid| {
                let js_event = MouseEvent::from(event);
                let this = JsValue::null();
                let args = js_sys::Array::new();
                args.push(&JsValue::from(js_event));

                if let Err(e) = handler.apply(&this, &args) {
                    console::error_1(&format!("Mouse handler error: {e:?}").into());
                }
            }
        };

        Ok(self
            .terminal
            .set_mouse_callback(handler_closure)?)
    }

    /// Get selected text based on a cell query
    #[wasm_bindgen(js_name = "getText")]
    pub fn get_text(&self, query: &CellQuery) -> String {
        self.terminal.get_text(query.inner).to_string()
    }

    /// Detects an HTTP/HTTPS URL at or around the given cell position.
    ///
    /// Scans left from the position to find a URL scheme (`http://` or `https://`),
    /// then scans right to find the URL end. Handles trailing punctuation and
    /// unbalanced parentheses (e.g., Wikipedia URLs).
    ///
    /// Returns `undefined` if no URL is found at the position.
    ///
    /// **Note:** Only detects URLs within a single row. URLs that wrap across
    /// multiple lines are not supported.
    ///
    /// # Example
    /// ```javascript
    /// // In a mouse handler:
    /// renderer.setMouseHandler((event) => {
    ///     const match = renderer.findUrlAt(event.col, event.row);
    ///     if (match) {
    ///         console.log("URL found:", match.url);
    ///         // match.query can be used for highlighting
    ///     }
    /// });
    /// ```
    #[wasm_bindgen(js_name = "findUrlAt")]
    pub fn find_url_at(&self, col: u16, row: u16) -> Option<UrlMatch> {
        let cursor = CursorPosition::new(col, row);
        self.terminal
            .find_url_at(cursor)
            .map(|m| UrlMatch {
                url: m.url.to_string(),
                query: CellQuery { inner: m.query },
            })
    }

    /// Copy text to the system clipboard
    #[wasm_bindgen(js_name = "copyToClipboard")]
    pub fn copy_to_clipboard(&self, text: &str) {
        crate::js::copy_to_clipboard(text);
    }

    /// Clear any active selection
    #[wasm_bindgen(js_name = "clearSelection")]
    pub fn clear_selection(&self) {
        self.terminal.clear_selection();
    }

    /// Check if there is an active selection
    #[wasm_bindgen(js_name = "hasSelection")]
    pub fn has_selection(&self) -> bool {
        self.terminal.has_selection()
    }

    /// Create a new render batch
    #[wasm_bindgen(js_name = "batch")]
    pub fn new_render_batch(&mut self) -> Batch {
        Batch { terminal_grid: self.terminal.grid() }
    }

    /// Get the terminal dimensions in cells
    #[wasm_bindgen(js_name = "terminalSize")]
    pub fn terminal_size(&self) -> Size {
        let (cols, rows) = self.terminal.terminal_size();
        Size { width: cols, height: rows }
    }

    /// Get the cell size in pixels
    #[wasm_bindgen(js_name = "cellSize")]
    pub fn cell_size(&self) -> Size {
        let (width, height) = self.terminal.cell_size();
        Size { width: width as u16, height: height as u16 }
    }

    /// Render the terminal to the canvas
    #[wasm_bindgen]
    pub fn render(&mut self) {
        if let Err(e) = self.terminal.render_frame() {
            console::error_1(&format!("Render error: {e:?}").into());
        }
    }

    /// Resize the terminal to fit new canvas dimensions
    #[wasm_bindgen]
    pub fn resize(&mut self, width: i32, height: i32) -> Result<(), JsValue> {
        Ok(self.terminal.resize(width, height)?)
    }

    /// Replace the current font atlas with a new static atlas.
    ///
    /// This method enables runtime font switching by loading a new `.atlas` file.
    /// All existing cell content is preserved and translated to the new atlas.
    ///
    /// # Arguments
    /// * `atlas_data` - Binary atlas data (from .atlas file), or null for default
    ///
    /// # Example
    /// ```javascript
    /// const atlasData = await fetch('new-font.atlas').then(r => r.arrayBuffer());
    /// renderer.replaceWithStaticAtlas(new Uint8Array(atlasData));
    /// ```
    #[wasm_bindgen(js_name = "replaceWithStaticAtlas")]
    pub fn replace_with_static_atlas(
        &mut self,
        atlas_data: Option<js_sys::Uint8Array>,
    ) -> Result<(), JsValue> {
        let atlas_config = match atlas_data {
            Some(data) => {
                let bytes = data.to_vec();
                FontAtlasData::from_binary(&bytes)
                    .map_err(|e| JsValue::from_str(&format!("Failed to parse atlas data: {e}")))?
            },
            None => FontAtlasData::default(),
        };

        Ok(self
            .terminal
            .replace_with_static_atlas(atlas_config)?)
    }

    /// Replace the current font atlas with a new dynamic atlas.
    ///
    /// This method enables runtime font switching by creating a new dynamic atlas
    /// with the specified font family and size. All existing cell content is
    /// preserved and translated to the new atlas.
    ///
    /// # Arguments
    /// * `font_family` - Array of font family names (e.g., `["Hack", "JetBrains Mono"]`)
    /// * `font_size` - Font size in pixels
    ///
    /// # Example
    /// ```javascript
    /// renderer.replaceWithDynamicAtlas(["Fira Code", "monospace"], 18.0);
    /// ```
    #[wasm_bindgen(js_name = "replaceWithDynamicAtlas")]
    pub fn replace_with_dynamic_atlas(
        &mut self,
        font_family: js_sys::Array,
        font_size: f32,
    ) -> Result<(), JsValue> {
        let font_families: Vec<String> = font_family
            .iter()
            .filter_map(|v| v.as_string())
            .collect();

        if font_families.is_empty() {
            return Err(JsValue::from_str("font_family array cannot be empty"));
        }

        let refs: Vec<&str> = font_families.iter().map(String::as_str).collect();
        Ok(self
            .terminal
            .replace_with_dynamic_atlas(&refs, font_size)?)
    }
}

// Convert between Rust and WASM types
impl From<SelectionMode> for RustSelectionMode {
    fn from(mode: SelectionMode) -> Self {
        match mode {
            SelectionMode::Block => RustSelectionMode::Block,
            SelectionMode::Linear => RustSelectionMode::Linear,
        }
    }
}

impl From<RustSelectionMode> for SelectionMode {
    fn from(mode: RustSelectionMode) -> Self {
        match mode {
            RustSelectionMode::Block => SelectionMode::Block,
            RustSelectionMode::Linear => SelectionMode::Linear,
        }
    }
}

impl From<TerminalMouseEvent> for MouseEvent {
    fn from(event: TerminalMouseEvent) -> Self {
        use crate::mouse::MouseEventType as RustMouseEventType;

        let event_type = match event.event_type {
            RustMouseEventType::MouseDown => MouseEventType::MouseDown,
            RustMouseEventType::MouseUp => MouseEventType::MouseUp,
            RustMouseEventType::MouseMove => MouseEventType::MouseMove,
            RustMouseEventType::Click => MouseEventType::Click,
            RustMouseEventType::MouseEnter => MouseEventType::MouseEnter,
            RustMouseEventType::MouseLeave => MouseEventType::MouseLeave,
        };

        MouseEvent {
            event_type,
            col: event.col,
            row: event.row,
            button: event.button(),
            ctrl_key: event.ctrl_key(),
            shift_key: event.shift_key(),
            alt_key: event.alt_key(),
            meta_key: event.meta_key(),
        }
    }
}

impl From<ModifierKeys> for RustModifierKeys {
    fn from(keys: ModifierKeys) -> Self {
        RustModifierKeys::from_bits_truncate(keys.0)
    }
}

/// Initialize the WASM module
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console::log_1(&"beamterm WASM module loaded".into());
}
