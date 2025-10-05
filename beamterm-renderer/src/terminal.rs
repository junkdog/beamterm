use std::{cell::RefCell, rc::Rc};

use beamterm_data::FontAtlasData;
use compact_str::CompactString;
use wasm_bindgen::prelude::*;

use crate::{
    gl::{CellQuery, SelectionMode},
    mouse::{
        DefaultSelectionHandler, MouseEventCallback, TerminalMouseEvent, TerminalMouseHandler,
    },
    CellData, Error, FontAtlas, Renderer, TerminalGrid,
};

/// High-performance WebGL2 terminal renderer.
///
/// `Terminal` encapsulates the complete terminal rendering system, providing a
/// simplified API over the underlying [`Renderer`] and [`TerminalGrid`] components.
///
///  ## Selection and Mouse Input
///
/// The renderer supports mouse-driven text selection with automatic clipboard
/// integration:
///
/// ```rust,no_run
/// // Enable default selection handler
/// use beamterm_renderer::{SelectionMode, Terminal};
///
/// let terminal = Terminal::builder("#canvas")
///     .default_mouse_input_handler(SelectionMode::Linear, true)
///     .build().unwrap();
///
/// // Or implement custom mouse handling
/// let terminal = Terminal::builder("#canvas")
///     .mouse_input_handler(|event, grid| {
///         // Custom handler logic
///     })
///     .build().unwrap();
///```
///
/// # Examples
///
/// ```rust,no_run
/// use beamterm_renderer::{CellData, Terminal};
///
/// // Create and render a simple terminal
/// let mut terminal = Terminal::builder("#canvas").build().unwrap();
///
/// // Update cells with content
/// let cells: Vec<CellData> = unimplemented!();
/// terminal.update_cells(cells.into_iter()).unwrap();
///
/// // Render frame
/// terminal.render_frame().unwrap();
///
/// // Handle window resize
/// let (new_width, new_height) = (800, 600);
/// terminal.resize(new_width, new_height).unwrap();
/// ```
#[derive(Debug)]
pub struct Terminal {
    renderer: Renderer,
    grid: Rc<RefCell<TerminalGrid>>,
    mouse_handler: Option<TerminalMouseHandler>, // 🐀
}

impl Terminal {
    /// Creates a new terminal builder with the specified canvas source.
    ///
    /// # Parameters
    /// * `canvas` - Canvas identifier (CSS selector) or `HtmlCanvasElement`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// // Using CSS selector
    /// use web_sys::HtmlCanvasElement;
    /// use beamterm_renderer::Terminal;
    ///
    /// let terminal = Terminal::builder("my-terminal").build().unwrap();
    ///
    /// // Using canvas element
    /// let canvas: &HtmlCanvasElement = unimplemented!("document.get_element_by_id(...)");
    /// let terminal = Terminal::builder(canvas).build().unwrap();
    /// ```
    #[allow(private_bounds)]
    pub fn builder(canvas: impl Into<CanvasSource>) -> TerminalBuilder {
        TerminalBuilder::new(canvas.into())
    }

    /// Updates terminal cell content efficiently.
    ///
    /// This method batches all cell updates and uploads them to the GPU in a single
    /// operation. For optimal performance, collect all changes and update in one call
    /// rather than making multiple calls for individual cells.
    ///
    /// Delegates to [`TerminalGrid::update_cells`].
    pub fn update_cells<'a>(
        &mut self,
        cells: impl Iterator<Item = CellData<'a>>,
    ) -> Result<(), Error> {
        self.grid
            .borrow_mut()
            .update_cells(self.renderer.gl(), cells)
    }

    /// Updates terminal cell content efficiently.
    ///
    /// This method batches all cell updates and uploads them to the GPU in a single
    /// operation. For optimal performance, collect all changes and update in one call
    /// rather than making multiple calls for individual cells.
    ///
    /// Delegates to [`TerminalGrid::update_cells_by_position`].
    pub fn update_cells_by_position<'a>(
        &mut self,
        cells: impl Iterator<Item = (u16, u16, CellData<'a>)>,
    ) -> Result<(), Error> {
        self.grid
            .borrow_mut()
            .update_cells_by_position(self.renderer.gl(), cells)
    }

    /// Returns the WebGL2 rendering context.
    pub fn gl(&self) -> &web_sys::WebGl2RenderingContext {
        self.renderer.gl()
    }

    /// Resizes the terminal to fit new canvas dimensions.
    ///
    /// This method updates both the renderer viewport and terminal grid to match
    /// the new canvas size. The terminal dimensions (in cells) are automatically
    /// recalculated based on the cell size from the font atlas.
    ///
    /// Combines [`Renderer::resize`] and [`TerminalGrid::resize`] operations.
    pub fn resize(&mut self, width: i32, height: i32) -> Result<(), Error> {
        self.renderer.resize(width, height);
        self.grid
            .borrow_mut()
            .resize(self.renderer.gl(), (width, height))?;

        if let Some(mouse_input) = &mut self.mouse_handler {
            let (cols, rows) = self.grid.borrow_mut().terminal_size();
            mouse_input.update_dimensions(cols, rows);
        }

        Ok(())
    }

    /// Returns the terminal dimensions in cells.
    pub fn terminal_size(&self) -> (u16, u16) {
        self.grid.borrow().terminal_size()
    }

    /// Returns the total number of cells in the terminal grid.
    pub fn cell_count(&self) -> usize {
        self.grid.borrow().cell_count()
    }

    /// Returns the size of the canvas in pixels.
    pub fn canvas_size(&self) -> (i32, i32) {
        self.renderer.canvas_size()
    }

    /// Returns the size of each cell in pixels.
    pub fn cell_size(&self) -> (i32, i32) {
        self.grid.borrow().cell_size()
    }

    /// Returns a reference to the HTML canvas element used for rendering.
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        self.renderer.canvas()
    }

    /// Returns a reference to the underlying renderer.
    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    /// Returns a reference to the terminal grid.
    pub fn grid(&self) -> Rc<RefCell<TerminalGrid>> {
        self.grid.clone()
    }

    /// Returns the textual content of the specified cell selection.
    pub fn get_text(&self, selection: CellQuery) -> CompactString {
        self.grid.borrow().get_text(selection)
    }

    /// Renders the current terminal state to the canvas.
    ///
    /// This method performs the complete render pipeline: frame setup, grid rendering,
    /// and frame finalization. Call this after updating terminal content to display
    /// the changes.
    ///
    /// Combines [`Renderer::begin_frame`], [`Renderer::render`], and [`Renderer::end_frame`].
    pub fn render_frame(&mut self) -> Result<(), Error> {
        self.grid
            .borrow_mut()
            .flush_cells(self.renderer.gl())?;

        self.renderer.begin_frame();
        self.renderer.render(&*self.grid.borrow());
        self.renderer.end_frame();
        Ok(())
    }

    /// Returns a sorted list of all glyphs that were requested but not found in the font atlas.
    pub fn missing_glyphs(&self) -> Vec<CompactString> {
        let mut glyphs: Vec<_> = self
            .grid
            .borrow()
            .atlas()
            .glyph_tracker()
            .missing_glyphs()
            .into_iter()
            .collect();
        glyphs.sort();
        glyphs
    }

    /// Exposes this terminal instance to the browser console for debugging.
    ///
    /// After calling this method, you can access the terminal from the console:
    /// ```javascript
    /// // In browser console:
    /// window.__beamterm_debug.getMissingGlyphs();
    /// ```
    ///
    /// Note: This creates a live reference that will show current missing glyphs
    /// each time you call it.
    fn expose_to_console(&self) {
        let debug_api = TerminalDebugApi { grid: self.grid.clone() };

        let window = web_sys::window().expect("no window");
        js_sys::Reflect::set(
            &window,
            &"__beamterm_debug".into(),
            &JsValue::from(debug_api),
        )
        .unwrap();

        web_sys::console::log_1(
            &"Terminal debugging API exposed at window.__beamterm_debug".into(),
        );
    }
}

/// Canvas source for terminal initialization.
///
/// Supports both CSS selector strings and direct `HtmlCanvasElement` references
/// for flexible terminal creation.
enum CanvasSource {
    /// CSS selector string for canvas lookup (e.g., "#terminal", "canvas").
    Id(CompactString),
    /// Direct reference to an existing canvas element.
    Element(web_sys::HtmlCanvasElement),
}

/// Builder for configuring and creating a [`Terminal`].
///
/// Provides a fluent API for terminal configuration with sensible defaults.
/// The terminal will use the default embedded font atlas unless explicitly configured.
///
/// # Examples
///
/// ```rust,no_run
/// // Simple terminal with default configuration
/// use beamterm_renderer::{FontAtlas, FontAtlasData, Terminal};
///
/// let terminal = Terminal::builder("#canvas").build().unwrap();
///
/// // Terminal with custom font atlas
/// let atlas = FontAtlasData::from_binary(unimplemented!(".atlas data")).unwrap();
/// let terminal = Terminal::builder("#canvas")
///     .font_atlas(atlas)
///     .fallback_glyph("X".into())
///     .build().unwrap();
/// ```
pub struct TerminalBuilder {
    canvas: CanvasSource,
    atlas_data: Option<FontAtlasData>,
    fallback_glyph: Option<CompactString>,
    input_handler: Option<InputHandler>,
    canvas_padding_color: u32,
    enable_debug_api: bool,
}

impl TerminalBuilder {
    /// Creates a new terminal builder with the specified canvas source.
    fn new(canvas: CanvasSource) -> Self {
        TerminalBuilder {
            canvas,
            atlas_data: None,
            fallback_glyph: None,
            input_handler: None,
            canvas_padding_color: 0x000000,
            enable_debug_api: false,
        }
    }

    /// Sets a custom font atlas for the terminal.
    ///
    /// By default, the terminal uses an embedded font atlas. Use this method
    /// to provide a custom atlas with different fonts, sizes, or character sets.
    pub fn font_atlas(mut self, atlas: FontAtlasData) -> Self {
        self.atlas_data = Some(atlas);
        self
    }

    /// Sets the fallback glyph for missing characters.
    ///
    /// When a character is not found in the font atlas, this glyph will be
    /// displayed instead. Defaults to a space character if not specified.
    pub fn fallback_glyph(mut self, glyph: &str) -> Self {
        self.fallback_glyph = Some(glyph.into());
        self
    }

    /// Sets the background color for the canvas area outside the terminal grid.
    ///
    /// When the canvas dimensions don't align perfectly with the terminal cell grid,
    /// there may be unused pixels around the edges. This color fills those padding
    /// areas to maintain a consistent appearance.
    pub fn canvas_padding_color(mut self, color: u32) -> Self {
        self.canvas_padding_color = color;
        self
    }

    /// Enables the debug API that will be exposed to the browser console.
    ///
    /// When enabled, a debug API will be available at `window.__beamterm_debug`
    /// with methods like `getMissingGlyphs()` for inspecting the terminal state.
    pub fn enable_debug_api(mut self) -> Self {
        self.enable_debug_api = true;
        self
    }

    /// Sets a callback for handling terminal mouse input events.
    pub fn mouse_input_handler<F>(mut self, callback: F) -> Self
    where
        F: FnMut(TerminalMouseEvent, &TerminalGrid) + 'static,
    {
        self.input_handler = Some(InputHandler::Mouse(Box::new(callback)));
        self
    }

    /// Sets a default selection handler for mouse input events. Left
    /// button selects text, `Ctrl/Cmd + C` copies the selected text to
    /// the clipboard.
    pub fn default_mouse_input_handler(
        mut self,
        selection_mode: SelectionMode,
        trim_trailing_whitespace: bool,
    ) -> Self {
        self.input_handler =
            Some(InputHandler::Internal { selection_mode, trim_trailing_whitespace });
        self
    }

    /// Builds the terminal with the configured options.
    pub fn build(self) -> Result<Terminal, Error> {
        // setup renderer
        let renderer = match self.canvas {
            CanvasSource::Id(id) => Renderer::create(&id)?,
            CanvasSource::Element(element) => Renderer::create_with_canvas(element)?,
        };
        let renderer = renderer.canvas_padding_color(self.canvas_padding_color);

        // load font atlas
        let gl = renderer.gl();
        let atlas = FontAtlas::load(gl, self.atlas_data.unwrap_or_default())?;

        // create terminal grid
        let canvas_size = renderer.canvas_size();
        let mut grid = TerminalGrid::new(gl, atlas, canvas_size)?;
        if let Some(fallback) = self.fallback_glyph {
            grid.set_fallback_glyph(&fallback)
        };
        let grid = Rc::new(RefCell::new(grid));

        // initialize mouse handler if needed
        let selection = grid.borrow().selection_tracker();
        match self.input_handler {
            None => Ok(Terminal { renderer, grid, mouse_handler: None }),
            Some(InputHandler::Internal { selection_mode, trim_trailing_whitespace }) => {
                let handler = DefaultSelectionHandler::new(
                    grid.clone(),
                    selection_mode,
                    trim_trailing_whitespace,
                );

                let mut mouse_input = TerminalMouseHandler::new(
                    renderer.canvas(),
                    grid.clone(),
                    handler.create_event_handler(selection),
                )?;
                mouse_input.default_input_handler = Some(handler);

                Ok(Terminal { renderer, grid, mouse_handler: Some(mouse_input) })
            },
            Some(InputHandler::Mouse(callback)) => {
                let mouse_input =
                    TerminalMouseHandler::new(renderer.canvas(), grid.clone(), callback)?;
                Ok(Terminal { renderer, grid, mouse_handler: Some(mouse_input) })
            },
        }
        .inspect(|terminal| {
            if self.enable_debug_api {
                terminal.expose_to_console();
            }
        })
    }
}

enum InputHandler {
    Mouse(MouseEventCallback),
    Internal {
        selection_mode: SelectionMode,
        trim_trailing_whitespace: bool,
    },
}

/// Debug API exposed to browser console for terminal inspection.
#[wasm_bindgen]
pub struct TerminalDebugApi {
    grid: Rc<RefCell<TerminalGrid>>,
}

#[wasm_bindgen]
impl TerminalDebugApi {
    /// Returns an array of glyphs that were requested but not found in the font atlas.
    #[wasm_bindgen(js_name = "getMissingGlyphs")]
    pub fn get_missing_glyphs(&self) -> js_sys::Array {
        let missing_set = self
            .grid
            .borrow()
            .atlas()
            .glyph_tracker()
            .missing_glyphs();
        let mut missing: Vec<_> = missing_set.into_iter().collect();
        missing.sort();

        let js_array = js_sys::Array::new();
        for glyph in missing {
            js_array.push(&JsValue::from_str(&glyph));
        }
        js_array
    }

    /// Returns the terminal size in cells as an object with `cols` and `rows` fields.
    #[wasm_bindgen(js_name = "getTerminalSize")]
    pub fn get_terminal_size(&self) -> JsValue {
        let (cols, rows) = self.grid.borrow().terminal_size();
        let obj = js_sys::Object::new();

        js_sys::Reflect::set(&obj, &"cols".into(), &JsValue::from(cols)).unwrap();
        js_sys::Reflect::set(&obj, &"rows".into(), &JsValue::from(rows)).unwrap();

        obj.into()
    }

    /// Returns the canvas size in pixels as an object with `width` and `height` fields.
    #[wasm_bindgen(js_name = "getCanvasSize")]
    pub fn get_canvas_size(&self) -> JsValue {
        let (width, height) = self.grid.borrow().canvas_size();
        let obj = js_sys::Object::new();

        js_sys::Reflect::set(&obj, &"width".into(), &JsValue::from(width)).unwrap();
        js_sys::Reflect::set(&obj, &"height".into(), &JsValue::from(height)).unwrap();

        obj.into()
    }

    /// Returns the number of glyphs available in the font atlas.
    #[wasm_bindgen(js_name = "getGlyphCount")]
    pub fn get_glyph_count(&self) -> u32 {
        self.grid.borrow().atlas().glyph_count()
    }

    /// Returns the base glyph ID for a given symbol, or null if not found.
    #[wasm_bindgen(js_name = "getBaseGlyphId")]
    pub fn get_base_glyph_id(&self, symbol: &str) -> Option<u16> {
        self.grid
            .borrow()
            .atlas()
            .get_base_glyph_id(symbol)
    }

    /// Returns the symbol for a given glyph ID, or null if not found.
    #[wasm_bindgen(js_name = "getSymbol")]
    pub fn get_symbol(&self, glyph_id: u16) -> Option<String> {
        self.grid
            .borrow()
            .atlas()
            .get_symbol(glyph_id)
            .map(|s| s.to_string())
    }

    /// Returns the cell size in pixels as an object with `width` and `height` fields.
    #[wasm_bindgen(js_name = "getCellSize")]
    pub fn get_cell_size(&self) -> JsValue {
        let (width, height) = self.grid.borrow().atlas().cell_size();
        let obj = js_sys::Object::new();

        js_sys::Reflect::set(&obj, &"width".into(), &JsValue::from(width)).unwrap();
        js_sys::Reflect::set(&obj, &"height".into(), &JsValue::from(height)).unwrap();

        obj.into()
    }

    #[wasm_bindgen(js_name = "getAtlasLookup")]
    pub fn get_symbol_lookup(&self) -> js_sys::Array {
        let grid = self.grid.borrow();
        let atlas = grid.atlas();
        let mut glyphs: Vec<_> = atlas.get_symbol_lookup()
            .iter()
            .collect();

        glyphs.sort();

        let js_array = js_sys::Array::new();
        for (glyph_id, symbol) in glyphs.into_iter() {
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &"glyph_id".into(), &JsValue::from(*glyph_id)).unwrap();
            js_sys::Reflect::set(&obj, &"symbol".into(), &JsValue::from(symbol.as_str())).unwrap();

            js_array.push(&obj.into());
        }
        js_array
    }
}

impl<'a> From<&'a str> for CanvasSource {
    fn from(id: &'a str) -> Self {
        CanvasSource::Id(id.into())
    }
}

impl From<web_sys::HtmlCanvasElement> for CanvasSource {
    fn from(element: web_sys::HtmlCanvasElement) -> Self {
        CanvasSource::Element(element)
    }
}

impl<'a> From<&'a web_sys::HtmlCanvasElement> for CanvasSource {
    fn from(value: &'a web_sys::HtmlCanvasElement) -> Self {
        value.clone().into()
    }
}
