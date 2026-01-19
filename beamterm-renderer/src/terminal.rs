use std::{cell::RefCell, rc::Rc};

use beamterm_data::{DebugSpacePattern, FontAtlasData};
use compact_str::{CompactString, ToCompactString};
use unicode_width::UnicodeWidthStr;
use wasm_bindgen::prelude::*;

use crate::{
    CellData, DynamicFontAtlas, Error, FontAtlas, Renderer, SelectionMode, StaticFontAtlas,
    TerminalGrid,
    gl::{CellQuery, ContextLossHandler},
    mouse::{
        DefaultSelectionHandler, MouseEventCallback, MouseSelectOptions, TerminalMouseEvent,
        TerminalMouseHandler,
    },
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
    mouse_handler: Option<TerminalMouseHandler>,
    context_loss_handler: Option<ContextLossHandler>,
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
            .update_cells_by_position(cells)
    }

    pub fn update_cells_by_index<'a>(
        &mut self,
        cells: impl Iterator<Item = (usize, CellData<'a>)>,
    ) -> Result<(), Error> {
        self.grid
            .borrow_mut()
            .update_cells_by_index(cells)
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

        self.update_mouse_handler_metrics();

        Ok(())
    }

    /// Updates the mouse handler with current grid metrics (cell size and dimensions).
    ///
    /// Called after operations that may change cell size (atlas replacement) or
    /// terminal dimensions (resize).
    fn update_mouse_handler_metrics(&mut self) {
        if let Some(mouse_input) = &mut self.mouse_handler {
            let grid = self.grid.borrow();
            let (cols, rows) = grid.terminal_size();
            let (cell_width, cell_height) = grid.cell_size();
            mouse_input.update_metrics(cols, rows, cell_width, cell_height);
        }
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

    /// Replaces the current font atlas with a new static atlas.
    ///
    /// All existing cell content is preserved and translated to the new atlas.
    /// The grid will be resized if the new atlas has different cell dimensions.
    ///
    /// # Parameters
    /// * `atlas_data` - Binary atlas data loaded from a `.atlas` file
    ///
    /// # Example
    /// ```rust,no_run
    /// use beamterm_renderer::{Terminal, FontAtlasData};
    ///
    /// let mut terminal = Terminal::builder("#canvas").build().unwrap();
    ///
    /// // Load and apply a new static atlas
    /// let atlas_data = FontAtlasData::from_binary(&atlas_bytes).unwrap();
    /// terminal.replace_with_static_atlas(atlas_data).unwrap();
    /// ```
    pub fn replace_with_static_atlas(&mut self, atlas_data: FontAtlasData) -> Result<(), Error> {
        let gl = self.renderer.gl();
        let atlas = StaticFontAtlas::load(gl, atlas_data)?;
        self.grid
            .borrow_mut()
            .replace_atlas(gl, atlas.into());

        self.update_mouse_handler_metrics();

        Ok(())
    }

    /// Replaces the current font atlas with a new dynamic atlas.
    ///
    /// The dynamic atlas rasterizes glyphs on-demand using the browser's Canvas API,
    /// enabling runtime font selection. All existing cell content is preserved and
    /// translated to the new atlas.
    ///
    /// # Parameters
    /// * `font_family` - Font family names in priority order (e.g., `&["JetBrains Mono", "Hack"]`)
    /// * `font_size` - Font size in pixels
    ///
    /// # Example
    /// ```rust,no_run
    /// use beamterm_renderer::Terminal;
    ///
    /// let mut terminal = Terminal::builder("#canvas").build().unwrap();
    ///
    /// // Switch to a different font at runtime
    /// terminal.replace_with_dynamic_atlas(&["Fira Code", "Hack"], 15.0).unwrap();
    /// ```
    pub fn replace_with_dynamic_atlas(
        &mut self,
        font_family: &[&str],
        font_size: f32,
    ) -> Result<(), Error> {
        let gl = self.renderer.gl();
        let font_family: Vec<CompactString> = font_family.iter().map(|&s| s.into()).collect();
        let atlas = DynamicFontAtlas::new(gl, &font_family, font_size, None)?;
        self.grid
            .borrow_mut()
            .replace_atlas(gl, atlas.into());

        self.update_mouse_handler_metrics();

        Ok(())
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
    /// If a WebGL context loss occurred and the context has been restored by the browser,
    /// this method will automatically recreate all GPU resources before rendering.
    /// The terminal's cell content is preserved during this process.
    ///
    /// Combines [`Renderer::begin_frame`], [`Renderer::render`], and [`Renderer::end_frame`].
    pub fn render_frame(&mut self) -> Result<(), Error> {
        if self.needs_gl_reinit() {
            self.restore_context()?;
        }

        // skip rendering if context is currently lost (waiting for restoration)
        if self.is_context_lost() {
            return Ok(());
        }

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

    /// Checks if the WebGL context has been lost.
    ///
    /// Returns `true` if the context is lost and waiting for restoration.
    fn is_context_lost(&self) -> bool {
        if let Some(handler) = &self.context_loss_handler {
            handler.is_context_lost()
        } else {
            self.renderer.is_context_lost()
        }
    }

    /// Restores all GPU resources after a WebGL context loss.
    ///
    /// # Returns
    /// * `Ok(())` - All resources successfully restored
    /// * `Err(Error)` - Failed to restore context or recreate resources
    fn restore_context(&mut self) -> Result<(), Error> {
        self.renderer.restore_context()?;

        let gl = self.renderer.gl();

        self.grid
            .borrow_mut()
            .recreate_atlas_texture(gl)?;
        self.grid.borrow_mut().recreate_resources(gl)?;
        self.grid.borrow_mut().flush_cells(gl)?;

        if let Some(handler) = &self.context_loss_handler {
            handler.clear_context_rebuild_needed();
        }

        Ok(())
    }

    /// Checks if the terminal needs to restore GPU resources after a context loss.
    fn needs_gl_reinit(&mut self) -> bool {
        self.context_loss_handler
            .as_ref()
            .map(ContextLossHandler::context_pending_rebuild)
            .unwrap_or(false)
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
#[derive(Debug)]
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
/// use beamterm_renderer::{FontAtlasData, Terminal};
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
    atlas_kind: AtlasKind,
    fallback_glyph: Option<CompactString>,
    input_handler: Option<InputHandler>,
    canvas_padding_color: u32,
    enable_debug_api: bool,
}

#[derive(Debug)]
enum AtlasKind {
    Static(Option<FontAtlasData>),
    Dynamic {
        font_size: f32,
        font_family: Vec<CompactString>,
    },
    DebugDynamic {
        font_size: f32,
        font_family: Vec<CompactString>,
        debug_space_pattern: DebugSpacePattern,
    },
}

impl TerminalBuilder {
    /// Creates a new terminal builder with the specified canvas source.
    fn new(canvas: CanvasSource) -> Self {
        TerminalBuilder {
            canvas,
            atlas_kind: AtlasKind::Static(None),
            fallback_glyph: None,
            input_handler: None,
            canvas_padding_color: 0x000000,
            enable_debug_api: false,
        }
    }

    /// Sets a custom static font atlas for the terminal.
    ///
    /// By default, the terminal uses an embedded font atlas. Use this method
    /// to provide a custom atlas with different fonts, sizes, or character sets.
    ///
    /// Static atlases are pre-generated using the `beamterm-atlas` CLI tool and
    /// loaded from binary `.atlas` files. They provide consistent rendering but
    /// require the character set to be known at build time.
    ///
    /// For dynamic glyph rasterization at runtime, see [`dynamic_font_atlas`](Self::dynamic_font_atlas).
    pub fn font_atlas(mut self, atlas: FontAtlasData) -> Self {
        self.atlas_kind = AtlasKind::Static(Some(atlas));
        self
    }

    /// Configures the terminal to use a dynamic font atlas.
    ///
    /// Unlike static atlases, the dynamic atlas rasterizes glyphs on-demand using
    /// the browser's Canvas API. This enables:
    /// - Runtime font selection without pre-generation
    /// - Support for any system font available in the browser
    /// - Automatic handling of unpredictable Unicode content
    ///
    /// # Parameters
    /// * `font_family` - Font family names in priority order (e.g., `&["JetBrains Mono", "Fira Code"]`)
    /// * `font_size` - Font size in pixels
    ///
    /// For pre-generated atlases with fixed character sets, see [`font_atlas`](Self::font_atlas).
    pub fn dynamic_font_atlas(mut self, font_family: &[&str], font_size: f32) -> Self {
        self.atlas_kind = AtlasKind::Dynamic {
            font_family: font_family.iter().map(|&s| s.into()).collect(),
            font_size,
        };
        self
    }

    /// Configures the terminal to use a dynamic font atlas with debug space pattern.
    ///
    /// This is the same as [`dynamic_font_atlas`](Self::dynamic_font_atlas), but replaces
    /// the space glyph with a checkered pattern for validating pixel-perfect rendering.
    ///
    /// # Parameters
    /// * `font_family` - Font family names in priority order
    /// * `font_size` - Font size in pixels
    /// * `pattern` - The checkered pattern to use (1px or 2x2 pixels)
    pub fn debug_dynamic_font_atlas(
        mut self,
        font_family: &[&str],
        font_size: f32,
        pattern: DebugSpacePattern,
    ) -> Self {
        self.atlas_kind = AtlasKind::DebugDynamic {
            font_family: font_family.iter().map(|&s| s.into()).collect(),
            font_size,
            debug_space_pattern: pattern,
        };
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

    /// Enables mouse-based text selection with automatic clipboard copying.
    ///
    /// When enabled, users can click and drag to select text in the terminal.
    /// Selected text is automatically copied to the clipboard on mouse release.
    ///
    /// # Example
    /// ```rust,no_run
    /// use beamterm_renderer::{Terminal, SelectionMode};
    /// use beamterm_renderer::mouse::{MouseSelectOptions, ModifierKeys};
    ///
    /// let terminal = Terminal::builder("#canvas")
    ///     .mouse_selection_handler(
    ///         MouseSelectOptions::new()
    ///             .selection_mode(SelectionMode::Linear)
    ///             .require_modifier_keys(ModifierKeys::SHIFT)
    ///             .trim_trailing_whitespace(true)
    ///     )
    ///     .build()?;
    /// ```
    pub fn mouse_selection_handler(mut self, configuration: MouseSelectOptions) -> Self {
        self.input_handler = Some(InputHandler::CopyOnSelect(configuration));
        self
    }

    /// Sets a default selection handler for mouse input events. Left
    /// button selects text, it copies the selected text to the clipboard
    /// on mouse release.
    #[deprecated(
        since = "0.13.0",
        note = "Use `mouse_selection_handler` with `MouseSelectOptions` instead"
    )]
    pub fn default_mouse_input_handler(
        mut self,
        selection_mode: SelectionMode,
        trim_trailing_whitespace: bool,
    ) -> Self {
        let options = MouseSelectOptions::new()
            .selection_mode(selection_mode)
            .trim_trailing_whitespace(trim_trailing_whitespace);

        self.mouse_selection_handler(options)
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
        let atlas: FontAtlas = match self.atlas_kind {
            AtlasKind::Static(atlas_data) => {
                StaticFontAtlas::load(gl, atlas_data.unwrap_or_default())?.into()
            },
            AtlasKind::Dynamic { font_family, font_size } => {
                DynamicFontAtlas::new(gl, &font_family, font_size, None)?.into()
            },
            AtlasKind::DebugDynamic { font_family, font_size, debug_space_pattern } => {
                DynamicFontAtlas::new(gl, &font_family, font_size, Some(debug_space_pattern))?
                    .into()
            },
        };

        // create terminal grid
        let canvas_size = renderer.canvas_size();
        let mut grid = TerminalGrid::new(gl, atlas, canvas_size)?;
        if let Some(fallback) = self.fallback_glyph {
            grid.set_fallback_glyph(&fallback)
        };
        let grid = Rc::new(RefCell::new(grid));

        // Set up context loss handler for automatic recovery
        let context_loss_handler = ContextLossHandler::new(renderer.canvas()).ok();

        // initialize mouse handler if needed
        let selection = grid.borrow().selection_tracker();
        match self.input_handler {
            None => Ok(Terminal {
                renderer,
                grid,
                mouse_handler: None,
                context_loss_handler,
            }),
            Some(InputHandler::CopyOnSelect(select)) => {
                let handler = DefaultSelectionHandler::new(grid.clone(), select);

                let mut mouse_input = TerminalMouseHandler::new(
                    renderer.canvas(),
                    grid.clone(),
                    handler.create_event_handler(selection),
                )?;
                mouse_input.default_input_handler = Some(handler);

                Ok(Terminal {
                    renderer,
                    grid,
                    mouse_handler: Some(mouse_input),
                    context_loss_handler,
                })
            },
            Some(InputHandler::Mouse(callback)) => {
                let mouse_input =
                    TerminalMouseHandler::new(renderer.canvas(), grid.clone(), callback)?;
                Ok(Terminal {
                    renderer,
                    grid,
                    mouse_handler: Some(mouse_input),
                    context_loss_handler,
                })
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
    CopyOnSelect(MouseSelectOptions),
}

/// Checks if a grapheme is double-width (emoji or fullwidth character).
pub(crate) fn is_double_width(grapheme: &str) -> bool {
    grapheme.len() > 1 && grapheme.width() == 2
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

        let mut glyphs: Vec<(u16, CompactString)> = Vec::new();
        atlas.for_each_symbol(&mut |glyph_id, symbol| {
            glyphs.push((glyph_id, symbol.to_compact_string()));
        });

        glyphs.sort();

        let js_array = js_sys::Array::new();
        for (glyph_id, symbol) in glyphs.iter() {
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
