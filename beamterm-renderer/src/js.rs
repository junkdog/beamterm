use js_sys::wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement};

use crate::error::Error;

pub(crate) fn document() -> Result<Document, Error> {
    web_sys::window()
        .ok_or(Error::window_not_found())
        .and_then(|w| w.document().ok_or(Error::document_not_found()))
}

pub(crate) fn get_canvas_by_id(canvas_id: &str) -> Result<HtmlCanvasElement, Error> {
    let document = document()?;
    document
        .query_selector(canvas_id)
        .map_err(|_| Error::canvas_not_found())?
        .ok_or(Error::canvas_not_found())?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| Error::canvas_not_found())
}

pub(crate) fn get_webgl2_context(
    canvas: &HtmlCanvasElement,
) -> Result<web_sys::WebGl2RenderingContext, Error> {
    canvas
        .get_context("webgl2")
        .map_err(|_| Error::canvas_context_failed())?
        .ok_or(Error::webgl_context_failed())?
        .dyn_into::<web_sys::WebGl2RenderingContext>()
        .map_err(|_| Error::webgl_context_failed())
}

/// Creates a glow context from the WebGL2 context of the given canvas.
///
/// On wasm32, this wraps the WebGL2 context in a glow context. The raw
/// WebGL2 context is also returned for `is_context_lost()` checks.
#[cfg(target_arch = "wasm32")]
pub(crate) fn create_glow_context(
    canvas: &HtmlCanvasElement,
) -> Result<(glow::Context, web_sys::WebGl2RenderingContext), Error> {
    let webgl2_ctx = get_webgl2_context(canvas)?;
    let gl = glow::Context::from_webgl2_context(webgl2_ctx.clone());
    Ok((gl, webgl2_ctx))
}

/// Stub for non-wasm targets (clippy on native host).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn create_glow_context(
    _canvas: &HtmlCanvasElement,
) -> Result<(glow::Context, web_sys::WebGl2RenderingContext), Error> {
    unimplemented!("create_glow_context is only available on wasm32")
}

/// Returns the current device pixel ratio, or 1.0 if unavailable.
pub(crate) fn device_pixel_ratio() -> f32 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio() as f32)
        .unwrap_or(1.0)
}
