//! WebGL context loss detection and recovery.
//!
//! This module provides automatic handling of WebGL context loss events,
//! which can occur when the GPU is reset, system resources are low, or
//! the browser tab is backgrounded for extended periods.
//!
//! # Usage
//!
//! Context loss handling is set up automatically when building a Terminal.
//! The terminal will automatically attempt to restore itself when the
//! context is restored by the browser.

use std::{cell::RefCell, rc::Rc};

use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::WebGlContextEvent;

use crate::Error;

/// Shared state for tracking context loss across closures.
#[derive(Debug, Clone)]
pub(crate) struct ContextState {
    inner: Rc<RefCell<ContextStateInner>>,
}

#[derive(Debug, Default)]
struct ContextStateInner {
    /// True if context is currently lost
    is_lost: bool,
    /// True if context was restored and we need to recreate resources
    pending_rebuild: bool,
}

impl ContextState {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(ContextStateInner::default())),
        }
    }

    /// Returns true if the context is currently lost.
    pub(crate) fn is_lost(&self) -> bool {
        self.inner.borrow().is_lost
    }

    /// Returns true if resources need to be restored.
    pub(crate) fn pending_rebuild(&self) -> bool {
        self.inner.borrow().pending_rebuild
    }

    /// Marks the context as lost.
    fn set_lost(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.is_lost = true;
    }

    /// Marks the context as restored and needing resource recreation.
    fn set_restored(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.is_lost = false;
        inner.pending_rebuild = true;
    }

    /// Clears the needs_restoration flag after resources have been recreated.
    pub fn clear_restoration_needed(&self) {
        self.inner.borrow_mut().pending_rebuild = false;
    }
}

/// Handles WebGL context loss and restoration events.
///
/// Attaches event listeners to the canvas for `webglcontextlost` and
/// `webglcontextrestored` events. When context is lost, `preventDefault()`
/// is called to enable restoration. The terminal checks `needs_restoration()`
/// and handles resource recreation.
pub(crate) struct ContextLossHandler {
    /// The canvas element this handler is attached to.
    canvas: web_sys::HtmlCanvasElement,
    /// Closure for context lost events.
    on_context_lost: Closure<dyn FnMut(WebGlContextEvent)>,
    /// Closure for context restored events.
    on_context_restored: Closure<dyn FnMut(WebGlContextEvent)>,
    /// Shared state tracking context status
    state: ContextState,
}

impl ContextLossHandler {
    /// Creates a new context loss handler for the given canvas.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to monitor
    ///
    /// # Returns
    /// * `Ok(ContextLossHandler)` - Handler successfully created and attached
    /// * `Err(Error)` - Failed to attach event listeners
    pub(crate) fn new(canvas: &web_sys::HtmlCanvasElement) -> Result<Self, Error> {
        let state = ContextState::new();

        let state_clone = state.clone();
        let on_context_lost = Self::register_callback(canvas, "webglcontextlost", move |event| {
            event.prevent_default();
            state_clone.set_lost();
        })?;

        // Create context restored handler
        let state_clone = state.clone();
        let on_context_restored =
            Self::register_callback(canvas, "webglcontextrestored", move |_event| {
                state_clone.set_restored();
            })?;

        Ok(Self {
            canvas: canvas.clone(),
            on_context_lost,
            on_context_restored,
            state,
        })
    }

    /// Returns true if the context is currently lost.
    pub(crate) fn is_context_lost(&self) -> bool {
        self.state.is_lost()
    }

    /// Returns true if resources need to be restored.
    pub(crate) fn context_pending_rebuild(&self) -> bool {
        self.state.pending_rebuild()
    }

    /// Clears the needs_restoration flag after resources have been recreated.
    pub(crate) fn clear_context_rebuild_needed(&self) {
        self.state.clear_restoration_needed();
    }

    /// Returns the shared context state for checking status.
    fn state(&self) -> &ContextState {
        &self.state
    }

    /// Removes all event listeners from the canvas.
    fn cleanup(&self) {
        let _ = self.canvas.remove_event_listener_with_callback(
            "webglcontextlost",
            self.on_context_lost.as_ref().unchecked_ref(),
        );
        let _ = self.canvas.remove_event_listener_with_callback(
            "webglcontextrestored",
            self.on_context_restored.as_ref().unchecked_ref(),
        );
    }

    fn register_callback(
        canvas: &web_sys::HtmlCanvasElement,
        event_type: &str,
        f: impl 'static + FnMut(WebGlContextEvent),
    ) -> Result<Closure<dyn FnMut(WebGlContextEvent)>, Error> {
        let callback = Closure::wrap(Box::new(f) as Box<dyn FnMut(_)>);
        canvas
            .add_event_listener_with_callback(event_type, callback.as_ref().unchecked_ref())
            .map_err(|_| Error::Callback(format!("Failed to add {} listener", event_type)))?;

        Ok(callback)
    }
}

impl Drop for ContextLossHandler {
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl std::fmt::Debug for ContextLossHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextLossHandler")
            .field("is_lost", &self.state.is_lost())
            .field("pending_rebuild", &self.state.pending_rebuild())
            .finish()
    }
}
