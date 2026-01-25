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

/// Returns the current device pixel ratio, or 1.0 if unavailable.
pub(crate) fn device_pixel_ratio() -> f32 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio() as f32)
        .unwrap_or(1.0)
}
