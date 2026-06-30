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
use wasm_bindgen::prelude::wasm_bindgen;

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

/// A [`Figure`] owned across the wasm boundary, with an interactive
/// pixel-to-data readout for DOM hover.
///
/// Construct one with [`WasmFigure::sample`], read its pixel size via
/// [`WasmFigure::size`], render it to a canvas with
/// [`WasmFigure::render`] (wasm only), and translate cursor pixels to data
/// coordinates with [`WasmFigure::data_at`].
#[wasm_bindgen]
pub struct WasmFigure {
    /// The wrapped figure whose first axes drives the hover readout.
    fig: Figure,
}

#[wasm_bindgen]
impl WasmFigure {
    /// Build a [`WasmFigure`] wrapping the built-in [`sample_figure`].
    #[must_use]
    pub fn sample() -> WasmFigure {
        WasmFigure {
            fig: sample_figure(),
        }
    }

    /// The figure's pixel size as a 2-element `[width, height]` array.
    ///
    /// Exposed across the wasm boundary as a `Float64Array`; callers size the
    /// target canvas to `size[0]` by `size[1]`.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn size(&self) -> Box<[f64]> {
        let (w, h) = self.fig.size_px();
        Box::new([w, h])
    }

    /// Map a **top-down canvas pixel** `(px, py)` to data coordinates in the
    /// figure's first axes.
    ///
    /// Returns `Some([x, y])` when the pixel falls inside the axes rectangle,
    /// else `None`. Across the wasm boundary this maps to a
    /// `Float64Array | undefined`, so a hover readout can show `undefined`
    /// (off-axes) versus a concrete `[x, y]`.
    #[must_use]
    pub fn data_at(&self, px: f64, py: f64) -> Option<Box<[f64]>> {
        self.fig
            .pixel_to_data(0, px, py)
            .map(|(x, y)| Box::new([x, y]) as Box<[f64]>)
    }
}

#[cfg(target_arch = "wasm32")]
mod canvas {
    use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

    use crate::{WasmFigure, figure_to_rgba, sample_figure};

    #[wasm_bindgen]
    impl WasmFigure {
        /// Render this figure onto the canvas element with id `canvas_id`.
        ///
        /// Rasterizes via the shared RGBA path and blits the result with
        /// `putImageData`, sizing the canvas to the figure's pixel size.
        ///
        /// # Errors
        ///
        /// Returns a [`JsValue`] error if the canvas element cannot be found, is
        /// not a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
        pub fn render(&self, canvas_id: &str) -> Result<(), JsValue> {
            let (rgba, width, height) = figure_to_rgba(&self.fig);
            render_rgba_to_canvas(canvas_id, &rgba, width, height)
        }
    }

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

    #[test]
    fn data_at_round_trips_a_known_in_axes_pixel() {
        let wf = WasmFigure::sample();
        // Pick a data point inside the sample's plotted range (sin(x) over
        // [0, TAU]), forward-map it to a canvas pixel, then read it back.
        let (data_x, data_y) = (std::f64::consts::PI, 0.0);
        let (px, py) = wf
            .fig
            .data_to_pixel(0, data_x, data_y)
            .expect("axes 0 exists");

        let got = wf.data_at(px, py).expect("pixel is inside the axes");
        assert_eq!(got.len(), 2, "data_at returns [x, y]");
        assert!(got[0].is_finite() && got[1].is_finite());
        assert!((got[0] - data_x).abs() < 1e-6, "x round-trips: {}", got[0]);
        assert!((got[1] - data_y).abs() < 1e-6, "y round-trips: {}", got[1]);
    }

    #[test]
    fn data_at_center_is_within_data_range() {
        let wf = WasmFigure::sample();
        let size = wf.size();
        let (cx, cy) = (size[0] / 2.0, size[1] / 2.0);

        let got = wf
            .data_at(cx, cy)
            .expect("canvas center sits over the axes");
        // sin(x) is plotted over x in [0, TAU] with y in [-1, 1]; the
        // margin-expanded extents stay comfortably within these bounds.
        assert!(
            (0.0..=std::f64::consts::TAU).contains(&got[0]),
            "x near center should be in [0, TAU]: {}",
            got[0]
        );
        assert!(
            got[1].abs() <= 2.0,
            "y near center should be near the sine range: {}",
            got[1]
        );
    }

    #[test]
    fn data_at_off_canvas_is_none() {
        let wf = WasmFigure::sample();
        // Far outside the canvas in both axes: well past the axes rect.
        assert!(wf.data_at(-100.0, -100.0).is_none());
        let size = wf.size();
        assert!(
            wf.data_at(size[0] + 1000.0, size[1] + 1000.0).is_none(),
            "a pixel far past the canvas is off-axes"
        );
    }
}
