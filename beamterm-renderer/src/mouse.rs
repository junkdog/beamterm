//! Mouse input handling for the beamterm terminal renderer.
//!
//! This module provides mouse event handling infrastructure for the terminal,
//! including coordinate conversion from pixel space to terminal grid space,
//! text selection with automatic clipboard integration, and customizable
//! event handling.
//!
//! # Architecture
//!
//! The mouse handling system consists of:
//! - [`TerminalMouseHandler`] - Main event handler that attaches to a canvas
//! - [`TerminalMouseEvent`] - Mouse events translated to terminal coordinates
//! - [`DefaultSelectionHandler`] - Built-in text selection implementation
//! - Internal state tracking for selection operations
//!
//! # Example
//!
//! ```rust,no_run
//! use beamterm_renderer::{Terminal, SelectionMode};
//! use beamterm_renderer::mouse::{MouseSelectOptions, ModifierKeys};
//!
//! // Enable selection with Shift+Click
//! let terminal = Terminal::builder("#canvas")
//!     .mouse_selection_handler(
//!         MouseSelectOptions::new()
//!             .selection_mode(SelectionMode::Linear)
//!             .require_modifier_keys(ModifierKeys::SHIFT)
//!             .trim_trailing_whitespace(true)
//!     )
//!     .build().unwrap();
//!
//! // Or provide custom mouse handling
//! let terminal = Terminal::builder("#canvas")
//!     .mouse_input_handler(|event, grid| {
//!         println!("Mouse event at ({}, {})", event.col, event.row);
//!     })
//!     .build().unwrap();
//! ```

use std::{
    cell::RefCell,
    fmt::{Debug, Formatter},
    rc::Rc,
};

use bitflags::bitflags;
use compact_str::CompactString;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

use crate::{
    Error, SelectionMode, TerminalGrid,
    gl::{SelectionTracker, TerminalMetrics},
    select,
};

/// Type alias for boxed mouse event callback functions.
///
/// Callbacks are invoked synchronously in the browser's event loop
pub type MouseEventCallback = Box<dyn FnMut(TerminalMouseEvent, &TerminalGrid) + 'static>;

/// Internal type for shared event handler wrapped in Rc<RefCell>.
type EventHandler = Rc<RefCell<dyn FnMut(TerminalMouseEvent, &TerminalGrid) + 'static>>;

/// Handles mouse input events for a terminal grid.
///
/// Converts browser mouse events into terminal grid coordinates and manages
/// event handlers for mouse interactions. Maintains terminal dimensions for
/// accurate coordinate mapping.
///
pub struct TerminalMouseHandler {
    /// The canvas element this handler is attached to.
    canvas: web_sys::HtmlCanvasElement,
    /// Closure for mousedown events.
    on_mouse_down: Closure<dyn FnMut(web_sys::MouseEvent)>,
    /// Closure for mouseup events.
    on_mouse_up: Closure<dyn FnMut(web_sys::MouseEvent)>,
    /// Closure for mousemove events.
    on_mouse_move: Closure<dyn FnMut(web_sys::MouseEvent)>,
    /// Cached terminal metrics (dimensions + cell size) for coordinate conversion.
    metrics: TerminalMetrics,
    /// Optional default selection handler.
    pub(crate) default_input_handler: Option<DefaultSelectionHandler>,
}

/// Mouse event data with terminal cell coordinates.
///
/// Represents a mouse event translated from pixel coordinates to terminal
/// grid coordinates, including modifier key states.`col` and `row` are 0-based
/// terminal grid coordinates
#[derive(Debug, Clone, Copy)]
pub struct TerminalMouseEvent {
    /// Type of mouse event (down, up, or move).
    pub event_type: MouseEventType,
    /// Column in the terminal grid (0-based).
    pub col: u16,
    /// Row in the terminal grid (0-based).
    pub row: u16,
    /// Mouse button pressed (0 = left, 1 = middle, 2 = right).
    button: i16,
    /// All modifier that were pressed during the event.
    modifier_keys: ModifierKeys,
}

impl TerminalMouseEvent {
    /// Returns the mouse button pressed during the event.
    pub fn button(&self) -> i16 {
        self.button
    }

    /// Returns whether the Ctrl key was pressed during the event.
    pub fn ctrl_key(&self) -> bool {
        self.modifier_keys.contains(ModifierKeys::CONTROL)
    }

    /// Returns whether the Shift key was pressed during the event.
    pub fn shift_key(&self) -> bool {
        self.modifier_keys.contains(ModifierKeys::SHIFT)
    }

    /// Returns whether the Alt key was pressed during the event.
    pub fn alt_key(&self) -> bool {
        self.modifier_keys.contains(ModifierKeys::ALT)
    }

    /// Returns whether the Meta key was pressed during the event.
    ///
    /// This is the Command key on macOS or the Windows key on Windows.
    pub fn meta_key(&self) -> bool {
        self.modifier_keys.contains(ModifierKeys::META)
    }

    /// Returns whether the pressed modifiers exactly match the specified set.
    ///
    /// Returns `true` only if the modifier keys pressed during the event
    /// are exactly equal to `mods`—no more, no less.
    pub fn has_exact_modifiers(&self, mods: ModifierKeys) -> bool {
        self.modifier_keys == mods
    }
}

/// Types of mouse events that can occur.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MouseEventType {
    /// Mouse button was pressed.
    MouseDown = 0,
    /// Mouse button was released.
    MouseUp = 1,
    /// Mouse moved while over the terminal.
    MouseMove = 2,
}

/// Configuration options for mouse-based text selection.
///
/// Use the builder pattern to configure selection behavior:
///
/// ```rust,no_run
/// use beamterm_renderer::mouse::{MouseSelectOptions, ModifierKeys};
/// use beamterm_renderer::SelectionMode;
///
/// let options = MouseSelectOptions::new()
///     .selection_mode(SelectionMode::Block)
///     .require_modifier_keys(ModifierKeys::SHIFT)
///     .trim_trailing_whitespace(true);
/// ```
#[derive(Clone, Debug, Copy, Default)]
pub struct MouseSelectOptions {
    selection_mode: SelectionMode,
    require_modifier_keys: ModifierKeys,
    trim_trailing_whitespace: bool,
}

impl MouseSelectOptions {
    /// Creates a new `MouseSelectOptions` with default settings.
    ///
    /// Defaults:
    /// - Selection mode: `Block`
    /// - Required modifier keys: none
    /// - Trim trailing whitespace: `false`
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the selection mode (Linear or Block).
    ///
    /// - `Linear`: Selects text following the natural reading order
    /// - `Block`: Selects a rectangular region of cells
    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection_mode = mode;
        self
    }

    /// Sets modifier keys that must be held for selection to activate.
    ///
    /// When set, mouse selection only begins if the specified modifier
    /// keys are pressed during the initial click. Use `ModifierKeys::empty()`
    /// to allow selection without any modifiers (the default).
    ///
    /// # Example
    /// ```rust,no_run
    /// use beamterm_renderer::mouse::{MouseSelectOptions, ModifierKeys};
    ///
    /// // Require Shift+Click to start selection
    /// let options = MouseSelectOptions::new()
    ///     .require_modifier_keys(ModifierKeys::SHIFT);
    ///
    /// // Require Ctrl+Shift+Click
    /// let options = MouseSelectOptions::new()
    ///     .require_modifier_keys(ModifierKeys::CONTROL | ModifierKeys::SHIFT);
    /// ```
    pub fn require_modifier_keys(mut self, require_modifier_keys: ModifierKeys) -> Self {
        self.require_modifier_keys = require_modifier_keys;
        self
    }

    /// Sets whether to trim trailing whitespace from selected text.
    ///
    /// When enabled, trailing spaces at the end of each line are removed
    /// from the copied text.
    pub fn trim_trailing_whitespace(mut self, trim: bool) -> Self {
        self.trim_trailing_whitespace = trim;
        self
    }
}

impl TerminalMouseHandler {
    /// Creates a new mouse handler for the given canvas and terminal grid.
    ///
    /// Sets up mouse event listeners on the canvas and converts pixel coordinates
    /// to terminal cell coordinates before invoking the provided event handler.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to attach mouse listeners to
    /// * `grid` - The terminal grid for coordinate calculations
    /// * `event_handler` - Callback invoked for each mouse event
    ///
    /// # Errors
    /// Returns `Error::Callback` if event listeners cannot be attached to the canvas.
    ///
    /// # Example
    /// ```rust,no_run
    /// use beamterm_renderer::mouse::TerminalMouseHandler;
    /// use std::{cell::RefCell, rc::Rc};
    ///
    /// let canvas = unimplemented!("canvas");
    /// let grid: Rc<RefCell<()>> = unimplemented!("TerminalGrid");
    ///
    /// // In real code, this would be TerminalGrid
    /// // let handler = TerminalMouseHandler::new(
    /// //     &canvas,
    /// //     grid.clone(),
    /// //     |event, grid| {
    /// //         println!("Click at ({}, {})", event.col, event.row);
    /// //     }
    /// // ).unwrap();
    /// ```
    pub fn new<F>(
        canvas: &web_sys::HtmlCanvasElement,
        grid: Rc<RefCell<TerminalGrid>>,
        event_handler: F,
    ) -> Result<Self, Error>
    where
        F: FnMut(TerminalMouseEvent, &TerminalGrid) + 'static,
    {
        Self::new_internal(canvas, grid, Box::new(event_handler))
    }

    /// Internal constructor that accepts a boxed event handler.
    ///
    /// # Implementation Details
    /// - Wraps handler in Rc<RefCell> for sharing between event closures
    /// - Caches terminal metrics for fast coordinate conversion
    /// - Creates three closures (one per event type) that share the handler
    fn new_internal(
        canvas: &web_sys::HtmlCanvasElement,
        grid: Rc<RefCell<TerminalGrid>>,
        event_handler: MouseEventCallback,
    ) -> Result<Self, Error> {
        // Wrap the handler in Rc<RefCell> for sharing between closures
        let shared_handler = Rc::new(RefCell::new(event_handler));

        // Get grid metrics for coordinate conversion
        let (cell_width, cell_height) = grid.borrow().cell_size();
        let (cols, rows) = grid.borrow().terminal_size();
        let metrics = TerminalMetrics::new(cols, rows, cell_width, cell_height);

        // Create pixel-to-cell coordinate converter
        let metrics_ref = metrics.clone_ref();
        let pixel_to_cell = move |event: &web_sys::MouseEvent| -> Option<(u16, u16)> {
            let x = event.offset_x() as f32;
            let y = event.offset_y() as f32;

            let m = metrics_ref.borrow();
            let col = (x / m.cell_width as f32).floor() as u16;
            let row = (y / m.cell_height as f32).floor() as u16;

            if col < m.cols && row < m.rows { Some((col, row)) } else { None }
        };

        // Create event handlers
        use MouseEventType::*;
        let on_mouse_down = create_mouse_event_closure(
            MouseDown,
            grid.clone(),
            shared_handler.clone(),
            pixel_to_cell.clone(),
        );
        let on_mouse_up = create_mouse_event_closure(
            MouseUp,
            grid.clone(),
            shared_handler.clone(),
            pixel_to_cell.clone(),
        );
        let on_mouse_move =
            create_mouse_event_closure(MouseMove, grid.clone(), shared_handler, pixel_to_cell);

        // Attach event listeners
        canvas
            .add_event_listener_with_callback("mousedown", on_mouse_down.as_ref().unchecked_ref())
            .map_err(|_| Error::Callback("Failed to add mousedown listener".into()))?;
        canvas
            .add_event_listener_with_callback("mouseup", on_mouse_up.as_ref().unchecked_ref())
            .map_err(|_| Error::Callback("Failed to add mouseup listener".into()))?;
        canvas
            .add_event_listener_with_callback("mousemove", on_mouse_move.as_ref().unchecked_ref())
            .map_err(|_| Error::Callback("Failed to add mousemove listener".into()))?;

        Ok(Self {
            canvas: canvas.clone(),
            on_mouse_down,
            on_mouse_up,
            on_mouse_move,
            metrics,
            default_input_handler: None,
        })
    }

    /// Removes all event listeners from the canvas.
    ///
    /// Called automatically on drop. Safe to call multiple times.
    pub fn cleanup(&self) {
        let _ = self.canvas.remove_event_listener_with_callback(
            "mousedown",
            self.on_mouse_down.as_ref().unchecked_ref(),
        );
        let _ = self.canvas.remove_event_listener_with_callback(
            "mouseup",
            self.on_mouse_up.as_ref().unchecked_ref(),
        );
        let _ = self.canvas.remove_event_listener_with_callback(
            "mousemove",
            self.on_mouse_move.as_ref().unchecked_ref(),
        );
    }

    /// Updates the cached terminal metrics.
    ///
    /// Should be called when the terminal is resized or the font atlas is
    /// replaced to ensure accurate pixel-to-cell coordinate conversion.
    ///
    /// # Arguments
    /// * `cols` - New column count
    /// * `rows` - New row count
    /// * `cell_width` - New cell width in pixels
    /// * `cell_height` - New cell height in pixels
    pub fn update_metrics(&mut self, cols: u16, rows: u16, cell_width: i32, cell_height: i32) {
        self.metrics
            .set(cols, rows, cell_width, cell_height);
    }
}

/// Default mouse selection handler with clipboard integration.
///
/// Provides text selection functionality with automatic clipboard copying
/// on selection completion. Supports both linear (text flow) and block
/// (rectangular) selection modes.
///
/// # Features
/// - Click and drag to select text
/// - Automatic clipboard copy on mouse release
/// - Configurable selection modes (Linear/Block)
/// - Optional trailing whitespace trimming
pub(crate) struct DefaultSelectionHandler {
    /// Current selection state machine.
    selection_state: Rc<RefCell<SelectionState>>,
    /// Terminal grid reference for text extraction.
    grid: Rc<RefCell<TerminalGrid>>,
    /// Selection mode (Linear or Block).
    options: MouseSelectOptions,
}

impl DefaultSelectionHandler {
    /// Creates a new selection handler for the given terminal grid.
    ///
    /// # Arguments
    /// * `grid` - Terminal grid for text extraction
    /// * `query_mode` - Selection mode (Linear follows text flow, Block is rectangular)
    /// * `trim_trailing_whitespace` - Whether to remove trailing spaces from selected text
    pub(crate) fn new(grid: Rc<RefCell<TerminalGrid>>, options: MouseSelectOptions) -> Self {
        Self {
            grid,
            selection_state: Rc::new(RefCell::new(SelectionState::Idle)),
            options,
        }
    }

    /// Creates the mouse event handler closure for this selection handler.
    ///
    /// Returns a boxed closure that handles mouse events, tracks selection state,
    /// and copies selected text to the clipboard on completion.
    ///
    /// # Arguments
    /// * `active_selection` - Selection tracker for visual feedback
    ///
    /// # Algorithm
    /// 1. MouseDown: Begin new selection or replace existing
    /// 2. MouseMove: Update selection end point if selecting
    /// 3. MouseUp: Complete selection and copy to clipboard
    ///
    /// Repeated single-cell clicks cancel selection rather than selecting one cell.
    pub fn create_event_handler(&self, active_selection: SelectionTracker) -> MouseEventCallback {
        let selection_state = self.selection_state.clone();
        let query_mode = self.options.selection_mode;
        let trim_trailing = self.options.trim_trailing_whitespace;
        let require_modifier_keys = self.options.require_modifier_keys;

        Box::new(move |event: TerminalMouseEvent, grid: &TerminalGrid| {
            let mut state = selection_state.borrow_mut();

            // update mouse selection state based on the event type.
            match event.event_type {
                MouseEventType::MouseDown if event.button == 0 => {
                    // if modifier keys are required, ensure they are pressed
                    if !event.has_exact_modifiers(require_modifier_keys) {
                        return;
                    }

                    // note: if there's an existing selection in progress, it
                    // means that the cursor left the terminal (canvas) area
                    // while a previous selection was ongoing. if so, we do
                    // nothing and await the MouseUp event.

                    // mouse down always begins a new *potential* selection
                    if state.is_complete() {
                        // the existing (completed) selection is replaced with
                        // a new selection which will be canceled if the mouse
                        // up event is fired on the same cell.
                        state.maybe_selecting(event.col, event.row);
                    } else if state.is_idle() {
                        // begins a new selection from a blank state
                        state.begin_selection(event.col, event.row);
                    }

                    let query = select(query_mode)
                        .start((event.col, event.row))
                        .trim_trailing_whitespace(trim_trailing);

                    active_selection.set_query(query);
                },
                MouseEventType::MouseMove if state.is_selecting() => {
                    state.update_selection(event.col, event.row);
                    active_selection.update_selection_end((event.col, event.row));
                },
                MouseEventType::MouseUp if event.button == 0 => {
                    // at this point, we're either at:
                    // a) the user has finished making the selection
                    // b) the selection was canceled by a click inside a single cell
                    if let Some((_start, _end)) = state.complete_selection(event.col, event.row) {
                        active_selection.update_selection_end((event.col, event.row));

                        // hash the selected content and store it with the query;
                        // this allows us to clear the selection if the content changes
                        let query = active_selection.query();
                        active_selection.set_content_hash(grid.hash_cells(query));

                        let selected_text = grid.get_text(active_selection.query());
                        copy_to_clipboard(selected_text);
                    } else {
                        state.clear();
                        active_selection.clear();
                    }
                },
                _ => {}, // ignore non-left button events
            }
        })
    }
}

/// Internal state machine for tracking mouse selection operations.
///
/// Manages the lifecycle of a selection from initial click through dragging
/// to final release. Handles edge cases like single-cell clicks that should
/// cancel rather than select.
///
/// # State Transitions
/// ```text
///        ┌──────────┐
///    ┌──▶│   Idle   │
///    │   └────┬─────┘
///    │        │ begin_selection
///    │        ▼
///    │   ┌──────────┐
///    │   │Selecting │◀────────────┐
///    │   └────┬─────┘             │
///    │        │ complete_selection│
///    │        ▼                   │
///    │   ┌──────────┐             │
///    │   │ Complete │             │
///    │   └────┬─────┘             │
///    │        │ maybe_selecting   │
///    │        ▼                   │
///    │   ┌──────────────┐         │
///    └───│MaybeSelecting│─────────┘
/// mouse  └──────────────┘  update_selection
///   up       
///          
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionState {
    /// No selection in progress.
    Idle,
    /// Active selection with start point and current cursor position.
    Selecting { start: (u16, u16), current: Option<(u16, u16)> },
    /// MouseDown in a cell while selection exists.
    /// Will become Selecting on MouseMove or Idle on MouseUp.
    MaybeSelecting { start: (u16, u16) },
    /// Selection completed, contains start and end coordinates.
    Complete { start: (u16, u16), end: (u16, u16) },
}

impl SelectionState {
    /// Begins a new selection from idle state.
    ///
    /// # Panics
    /// Panics if called when not in Idle state (debug only).
    fn begin_selection(&mut self, col: u16, row: u16) {
        debug_assert!(matches!(self, SelectionState::Idle));
        *self = SelectionState::Selecting { start: (col, row), current: None };
    }

    /// Updates selection end point during drag.
    ///
    /// Transitions MaybeSelecting to Selecting if needed.
    fn update_selection(&mut self, col: u16, row: u16) {
        use SelectionState::*;

        match self {
            Selecting { current, .. } => {
                *current = Some((col, row));
            },
            MaybeSelecting { start } => {
                if (col, row) != *start {
                    *self = Selecting { start: *start, current: Some((col, row)) };
                }
            },
            _ => {},
        }
    }

    /// Completes the selection on mouse release.
    ///
    /// # Returns
    /// - `Some((start, end))` if selection completed
    /// - `None` if selection was canceled (single cell click)
    fn complete_selection(&mut self, col: u16, row: u16) -> Option<((u16, u16), (u16, u16))> {
        match self {
            SelectionState::Selecting { start, .. } => {
                let result = Some((*start, (col, row)));
                *self = SelectionState::Complete { start: *start, end: (col, row) };
                result
            },
            _ => None,
        }
    }

    /// Clears the selection state back to idle.
    fn clear(&mut self) {
        *self = SelectionState::Idle;
    }

    /// Checks if currently in selecting state.
    fn is_selecting(&self) -> bool {
        matches!(
            self,
            SelectionState::Selecting { .. } | SelectionState::MaybeSelecting { .. }
        )
    }

    fn is_idle(&self) -> bool {
        matches!(self, SelectionState::Idle)
    }

    /// Begins potential new selection while one exists.
    ///
    /// Used when clicking while a selection is complete.
    fn maybe_selecting(&mut self, col: u16, row: u16) {
        *self = SelectionState::MaybeSelecting { start: (col, row) };
    }

    /// Checks if a selection has been completed.
    fn is_complete(&self) -> bool {
        matches!(self, SelectionState::Complete { .. })
    }
}

/// Creates a closure that handles browser mouse events and converts them to terminal events.
///
/// Wraps the event handler with coordinate conversion and terminal event creation logic.
fn create_mouse_event_closure(
    event_type: MouseEventType,
    grid: Rc<RefCell<TerminalGrid>>,
    event_handler: EventHandler,
    pixel_to_cell: impl Fn(&web_sys::MouseEvent) -> Option<(u16, u16)> + 'static,
) -> Closure<dyn FnMut(web_sys::MouseEvent)> {
    Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
        if let Some((col, row)) = pixel_to_cell(&event) {
            let modifiers = {
                let mut mods = ModifierKeys::empty();
                if event.ctrl_key() {
                    mods |= ModifierKeys::CONTROL;
                }
                if event.shift_key() {
                    mods |= ModifierKeys::SHIFT;
                }
                if event.alt_key() {
                    mods |= ModifierKeys::ALT;
                }
                if event.meta_key() {
                    mods |= ModifierKeys::META;
                }
                mods
            };

            let terminal_event = TerminalMouseEvent {
                event_type,
                col,
                row,
                button: event.button(),
                modifier_keys: modifiers,
            };
            let grid_ref = grid.borrow();
            event_handler.borrow_mut()(terminal_event, &grid_ref);
        }
    }) as Box<dyn FnMut(_)>)
}

/// Copies text to the system clipboard using the browser's async clipboard API.
///
/// Spawns an async task to handle the clipboard write operation. Logs success
/// or failure to the console.
///
/// # Security
/// Browser may require user gesture or HTTPS for clipboard access.
fn copy_to_clipboard(text: CompactString) {
    spawn_local(async move {
        if let Some(window) = web_sys::window() {
            let clipboard = window.navigator().clipboard();
            match wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text)).await {
                Ok(_) => {},
                Err(err) => {
                    console::error_1(&format!("Failed to copy to clipboard: {err:?}").into());
                },
            }
        }
    });
}

impl Drop for TerminalMouseHandler {
    /// Automatically removes event listeners when handler is dropped.
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl Debug for TerminalMouseHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (cols, rows, cw, ch) = self.metrics.get();
        write!(
            f,
            "TerminalMouseHandler {{ dimensions: {cols}x{rows}, cell_size: {cw}x{ch} }}"
        )
    }
}

impl Debug for DefaultSelectionHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (cols, rows) = self.grid.borrow().terminal_size();
        write!(
            f,
            "DefaultSelectionHandler {{ options: {:?}, grid: {}x{} }}",
            self.options, cols, rows
        )
    }
}

bitflags! {
    /// Keyboard modifier keys that can be held during mouse events.
    ///
    /// Flags can be combined using bitwise OR:
    ///
    /// ```rust,no_run
    /// use beamterm_renderer::mouse::ModifierKeys;
    ///
    /// let ctrl_shift = ModifierKeys::CONTROL | ModifierKeys::SHIFT;
    /// ```
    #[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
    pub struct ModifierKeys : u8 {
        /// The Control key (Ctrl on Windows/Linux).
        const CONTROL = 0b0000_0001;
        /// The Shift key.
        const SHIFT   = 0b0000_0010;
        /// The Alt key (Option on macOS).
        const ALT     = 0b0000_0100;
        /// The Meta key (Command on macOS, Windows key on Windows).
        const META    = 0b0000_1000;
    }
}
