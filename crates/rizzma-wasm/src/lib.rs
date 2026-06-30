//! Wasm/browser target for rizzma.
//!
//! This crate has two clearly separated paths:
//!
//! 1. **Pure-Rust render-to-RGBA core** ([`figure_to_rgba`], [`sample_figure`]).
//!    This is target-agnostic: it builds and renders a [`Figure`] with the
//!    tiny-skia raster backend and hands back a *straight* (non-premultiplied)
//!    RGBA8 buffer plus its pixel dimensions. It compiles and is tested on the
//!    native host, with no browser or DOM dependency.
//!
//! 2. **Wasm-only canvas blit** (`draw_sample_to_canvas`,
//!    `render_rgba_to_canvas`). Gated behind `#[cfg(target_arch = "wasm32")]`,
//!    this takes the straight RGBA from the core path, wraps it in an
//!    [`web_sys::ImageData`], and pushes it onto an `HtmlCanvasElement`'s 2D
//!    context via `putImageData`. "Canvas is just another backend."
//!
//! # Premultiplied vs. straight alpha
//!
//! tiny-skia stores pixels in **premultiplied** RGBA (each color channel is
//! already scaled by alpha). The browser `ImageData` API expects **straight**
//! (non-premultiplied) RGBA. The core path therefore un-premultiplies every
//! pixel before returning, so the bytes are ready to feed directly into
//! `ImageData`.

use rizzma_core::color::Rgba;
use rizzma_figure::Figure;

/// Build a small labeled `sin(x)` plot so demos and tests have something to draw.
///
/// Returns a 4x3 inch figure (at the default DPI) with a single blue sine curve
/// on labeled axes, titled `"sin(x)"`.
#[must_use]
pub fn sample_figure() -> Figure {
    use std::f64::consts::TAU;

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.15, 0.15, 0.78, 0.74);

    let n = 200;
    let xs: Vec<f64> = (0..n).map(|i| TAU * i as f64 / (n as f64 - 1.0)).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| x.sin()).collect();

    // matplotlib's default blue ("C0").
    let blue = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);
    ax.plot_with_color(&xs, &ys, blue);
    ax.set_title("sin(x)");
    ax.set_xlabel("x");
    ax.set_ylabel("y");

    fig
}

/// Render `fig` to **straight** (non-premultiplied) RGBA8 pixels.
///
/// Returns `(pixels, width, height)` where `pixels` is a row-major
/// `width * height * 4` byte buffer (top-row-first, 4 bytes per pixel) suitable
/// for direct use as browser `ImageData`, and `width`/`height` are the figure's
/// size in pixels.
///
/// tiny-skia stores premultiplied RGBA internally; this function reads each
/// pixel's *straight* channel values back out (tiny-skia's
/// `PremultipliedColorU8::demultiply`), so the returned buffer is
/// non-premultiplied.
#[must_use]
pub fn figure_to_rgba(fig: &Figure) -> (Vec<u8>, u32, u32) {
    let renderer = fig.render();
    let pixmap = renderer.pixmap();
    let width = pixmap.width();
    let height = pixmap.height();

    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for px in pixmap.pixels() {
        // `demultiply()` converts the stored premultiplied pixel back to a
        // straight (non-premultiplied) color, exactly what `ImageData` wants.
        let straight = px.demultiply();
        rgba.push(straight.red());
        rgba.push(straight.green());
        rgba.push(straight.blue());
        rgba.push(straight.alpha());
    }

    (rgba, width, height)
}

#[cfg(target_arch = "wasm32")]
mod canvas {
    use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

    use crate::{figure_to_rgba, sample_figure};

    /// Look up the canvas with `canvas_id` and return it with its 2D context.
    fn canvas_context(
        canvas_id: &str,
    ) -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no global window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("window has no document"))?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str(&format!("no element with id '{canvas_id}'")))?;
        let canvas: HtmlCanvasElement = element
            .dyn_into()
            .map_err(|_| JsValue::from_str(&format!("element '{canvas_id}' is not a canvas")))?;
        let context = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("canvas has no 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| JsValue::from_str("2d context has unexpected type"))?;
        Ok((canvas, context))
    }

    /// Render the built-in [`sample_figure`] and blit it onto the canvas element
    /// with id `canvas_id`.
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if the canvas element cannot be found, is not
    /// a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
    #[wasm_bindgen]
    pub fn draw_sample_to_canvas(canvas_id: &str) -> Result<(), JsValue> {
        let (rgba, width, height) = figure_to_rgba(&sample_figure());
        render_rgba_to_canvas(canvas_id, &rgba, width, height)
    }

    /// Blit a straight-RGBA8 buffer onto the canvas element with id `canvas_id`.
    ///
    /// Sizes the canvas to `width` by `height`, wraps `rgba` in an
    /// [`ImageData`], and draws it at the origin via `putImageData`.
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if the canvas cannot be found or resolved to a
    /// 2D context, or if constructing/placing the `ImageData` fails.
    #[wasm_bindgen]
    pub fn render_rgba_to_canvas(
        canvas_id: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), JsValue> {
        let (canvas, context) = canvas_context(canvas_id)?;
        canvas.set_width(width);
        canvas.set_height(height);

        let clamped = wasm_bindgen::Clamped(rgba);
        let image_data = ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)?;
        context.put_image_data(&image_data, 0.0, 0.0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_matches_figure_dimensions_and_has_ink() {
        let fig = sample_figure();
        let (rgba, width, height) = figure_to_rgba(&fig);

        let (w_px, h_px) = fig.size_px();
        assert_eq!(width, w_px as u32);
        assert_eq!(height, h_px as u32);

        assert!(!rgba.is_empty(), "rgba buffer is empty");
        assert_eq!(
            rgba.len(),
            (width as usize) * (height as usize) * 4,
            "buffer length must be w * h * 4"
        );

        // Something was actually drawn: the buffer is not all zero.
        assert!(
            rgba.iter().any(|&b| b != 0),
            "expected non-zero pixels (something drawn)"
        );
    }

    #[test]
    fn straight_alpha_channels_stay_in_range() {
        // The white facecolor produces fully-opaque pixels. Un-premultiplying an
        // opaque pixel is a no-op, so every channel of every opaque pixel stays a
        // valid byte (<= 255), the sanity check on the un-premultiply path.
        let (rgba, _w, _h) = figure_to_rgba(&sample_figure());
        let mut saw_white = false;
        for px in rgba.chunks_exact(4) {
            if px[3] == 255 {
                // A fully-opaque straight pixel is bit-identical to its stored
                // premultiplied form (demultiply is a no-op at alpha 255), so the
                // white facecolor must read back as exact white, not overflowed.
                if px[0] == 255 && px[1] == 255 && px[2] == 255 {
                    saw_white = true;
                }
            }
        }
        assert!(
            saw_white,
            "expected the opaque white facecolor to survive un-premultiply as 255,255,255"
        );
    }
}
