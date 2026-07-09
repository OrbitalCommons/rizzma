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

use crate::artist::Line2D;
use crate::core::color::{Rgba, to_rgba};
use crate::figure::Figure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};

/// Parsed `plot_styled` options: each field is `Some` only when the caller's
/// style object supplied that key.
#[derive(Debug, Default, PartialEq)]
struct LineStyleSpec {
    /// Stroke color from a matplotlib-style spec (name, hex, `tab:*`, `C0`…).
    color: Option<Rgba>,
    /// Stroke width in points.
    linewidth: Option<f64>,
    /// Dash pattern; `Some(None)` is an explicit solid line.
    dashes: Option<Option<(f64, Vec<f64>)>>,
}

/// Map a matplotlib linestyle token to a dash pattern in points.
///
/// `None` means solid. Patterns match matplotlib's defaults (`lines.*_pattern`
/// rcParams), which are scaled by the renderer with line width.
fn dash_pattern(ls: &str) -> Result<Option<(f64, Vec<f64>)>, String> {
    match ls {
        "-" | "solid" => Ok(None),
        "--" | "dashed" => Ok(Some((0.0, vec![3.7, 1.6]))),
        ":" | "dotted" => Ok(Some((0.0, vec![1.0, 1.65]))),
        "-." | "dashdot" => Ok(Some((0.0, vec![6.4, 1.6, 1.0, 1.6]))),
        other => Err(format!(
            "unknown linestyle '{other}' (expected '-', '--', ':', '-.', or a long name)"
        )),
    }
}

/// Validate raw style parts into a [`LineStyleSpec`].
fn build_line_style(
    color: Option<&str>,
    lw: Option<f64>,
    ls: Option<&str>,
) -> Result<LineStyleSpec, String> {
    let color = match color {
        Some(spec) => {
            Some(to_rgba(spec, None).ok_or_else(|| format!("unrecognized color spec '{spec}'"))?)
        }
        None => None,
    };
    if let Some(w) = lw
        && !(w.is_finite() && w >= 0.0)
    {
        return Err(format!(
            "linewidth must be finite and non-negative, got {w}"
        ));
    }
    let dashes = match ls {
        Some(token) => Some(dash_pattern(token)?),
        None => None,
    };
    Ok(LineStyleSpec {
        color,
        linewidth: lw,
        dashes,
    })
}

/// Extract `{color?, lw?, ls?}` from a JS style object, rejecting unknown keys.
///
/// `undefined`/`null` mean "no styling". This is the only JS-shaped step; all
/// validation lives in [`build_line_style`], which is tested natively.
fn line_style_from_js(style: &JsValue) -> Result<LineStyleSpec, JsValue> {
    if style.is_undefined() || style.is_null() {
        return Ok(LineStyleSpec::default());
    }
    let obj: &js_sys::Object = style
        .dyn_ref()
        .ok_or_else(|| JsValue::from_str("style must be a plain object like {color, lw, ls}"))?;

    let (mut color, mut lw, mut ls) = (None, None, None);
    for key in js_sys::Object::keys(obj).iter() {
        let key = key
            .as_string()
            .ok_or_else(|| JsValue::from_str("style keys must be strings"))?;
        let value = js_sys::Reflect::get(style, &JsValue::from_str(&key))?;
        match key.as_str() {
            "color" => {
                color =
                    Some(value.as_string().ok_or_else(|| {
                        JsValue::from_str("style.color must be a string color spec")
                    })?);
            }
            "lw" => {
                lw = Some(
                    value
                        .as_f64()
                        .ok_or_else(|| JsValue::from_str("style.lw must be a number"))?,
                );
            }
            "ls" => {
                ls = Some(
                    value
                        .as_string()
                        .ok_or_else(|| JsValue::from_str("style.ls must be a linestyle string"))?,
                );
            }
            other => {
                return Err(JsValue::from_str(&format!(
                    "unknown style key '{other}' (expected color, lw, ls)"
                )));
            }
        }
    }
    build_line_style(color.as_deref(), lw, ls.as_deref()).map_err(|e| JsValue::from_str(&e))
}

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
    figure_to_rgba_scaled(fig, 1.0)
}

/// Render `fig` at `scale` × its size and DPI (see [`Figure::render_scaled`])
/// to straight RGBA8, for HiDPI canvas backing stores.
///
/// Returns `(pixels, width, height)` with `width`/`height` in **device**
/// pixels (`size_px() * scale`); present at the figure's logical size (CSS
/// pixels) for a crisp HiDPI image.
///
/// # Panics
///
/// Panics if `scale` is not finite and positive.
#[must_use]
pub fn figure_to_rgba_scaled(fig: &Figure, scale: f64) -> (Vec<u8>, u32, u32) {
    let renderer = fig.render_scaled(scale);
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
/// [`WasmFigure::size`], render it to a canvas with `WasmFigure::render`
/// (wasm only — `#[cfg(target_arch = "wasm32")]`, so it can't be an intra-doc
/// link on the host docs build), and translate cursor pixels to data
/// coordinates with [`WasmFigure::data_at`].
#[wasm_bindgen]
pub struct WasmFigure {
    /// The wrapped figure whose first axes drives the hover readout.
    fig: Figure,
}

#[wasm_bindgen]
impl WasmFigure {
    /// Create an empty `width_in` by `height_in` inch figure (default DPI).
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(width_in: f64, height_in: f64) -> WasmFigure {
        WasmFigure {
            fig: Figure::new(width_in, height_in),
        }
    }

    /// Build a [`WasmFigure`] wrapping the built-in [`sample_figure`].
    #[must_use]
    pub fn sample() -> WasmFigure {
        WasmFigure {
            fig: sample_figure(),
        }
    }

    /// Add axes at the figure-fraction rectangle `(left, bottom, width,
    /// height)`, returning the new axes' index.
    pub fn add_axes(&mut self, l: f64, b: f64, w: f64, h: f64) -> usize {
        self.fig.add_axes(l, b, w, h);
        self.fig.axes().len() - 1
    }

    /// Add axes for 1-based cell `index` of an `nrows` x `ncols` grid,
    /// returning the new axes' index.
    ///
    /// # Errors
    ///
    /// Returns an error if `index` is zero or exceeds `nrows * ncols`.
    pub fn add_subplot(
        &mut self,
        nrows: usize,
        ncols: usize,
        index: usize,
    ) -> Result<usize, JsValue> {
        self.add_subplot_impl(nrows, ncols, index).map_err(js_err)
    }

    /// Plot `y` against `x` as a line on axes `axes`, using the color cycle.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn plot(&mut self, axes: usize, x: &[f64], y: &[f64]) -> Result<(), JsValue> {
        self.plot_impl(axes, x, y).map_err(js_err)
    }

    /// Replace the data of line `line` on axes `axes` in place (live
    /// updates), keeping its style. Autoscaled limits re-derive; explicit
    /// limits are untouched.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` or `line` is out of range.
    pub fn set_line_data(
        &mut self,
        axes: usize,
        line: usize,
        x: &[f64],
        y: &[f64],
    ) -> Result<(), JsValue> {
        self.set_line_data_impl(axes, line, x, y).map_err(js_err)
    }

    /// Plot a styled line: `style` is a plain object with optional keys
    /// `color` (matplotlib color spec string), `lw` (points), and `ls`
    /// (`'-'`, `'--'`, `':'`, `'-.'` or long names). Unknown keys are errors.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range or `style` is invalid.
    pub fn plot_styled(
        &mut self,
        axes: usize,
        x: &[f64],
        y: &[f64],
        style: &JsValue,
    ) -> Result<(), JsValue> {
        let spec = line_style_from_js(style)?;
        self.plot_styled_impl(axes, x, y, spec).map_err(js_err)
    }

    /// Scatter-plot `y` against `x` on axes `axes`, using the color cycle.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn scatter(&mut self, axes: usize, x: &[f64], y: &[f64]) -> Result<(), JsValue> {
        self.scatter_impl(axes, x, y).map_err(js_err)
    }

    /// Set the title of axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_title(&mut self, axes: usize, title: &str) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_title(title);
        })
        .map_err(js_err)
    }

    /// Set the x-axis label of axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_xlabel(&mut self, axes: usize, label: &str) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_xlabel(label);
        })
        .map_err(js_err)
    }

    /// Set the y-axis label of axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_ylabel(&mut self, axes: usize, label: &str) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_ylabel(label);
        })
        .map_err(js_err)
    }

    /// Set explicit x limits on axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_xlim(&mut self, axes: usize, lo: f64, hi: f64) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_xlim(lo, hi);
        })
        .map_err(js_err)
    }

    /// Set explicit y limits on axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_ylim(&mut self, axes: usize, lo: f64, hi: f64) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_ylim(lo, hi);
        })
        .map_err(js_err)
    }

    /// The effective `[xlo, xhi, ylo, yhi]` limits of axes `axes`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn limits(&self, axes: usize) -> Result<Box<[f64]>, JsValue> {
        self.limits_impl(axes)
            .map(|l| Box::new(l) as Box<[f64]>)
            .map_err(js_err)
    }

    /// Switch axes `axes` to a log-scaled x axis with `base`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_xscale_log(&mut self, axes: usize, base: f64) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_xscale_log(base);
        })
        .map_err(js_err)
    }

    /// Switch axes `axes` to a log-scaled y axis with `base`.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range.
    pub fn set_yscale_log(&mut self, axes: usize, base: f64) -> Result<(), JsValue> {
        self.with_axes(axes, |ax| {
            ax.set_yscale_log(base);
        })
        .map_err(js_err)
    }

    /// Add a legend to axes `axes`: label `i` is paired with the color of the
    /// `i`-th plotted line.
    ///
    /// # Errors
    ///
    /// Returns an error if `axes` is out of range or there are more labels
    /// than lines.
    pub fn legend(&mut self, axes: usize, labels: Vec<String>) -> Result<(), JsValue> {
        self.legend_impl(axes, labels).map_err(js_err)
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

/// The target-agnostic core of the JS surface: every `*_impl` method returns
/// `Result<_, String>` so the forwarding logic (including error paths) runs
/// and tests natively; the `#[wasm_bindgen]` wrappers above convert errors
/// with [`js_err`] only at the boundary.
impl WasmFigure {
    /// Run `f` on axes `axes`, or report an out-of-range index.
    fn with_axes<T>(
        &mut self,
        axes: usize,
        f: impl FnOnce(&mut crate::figure::Axes) -> T,
    ) -> Result<T, String> {
        let count = self.fig.axes().len();
        self.fig
            .axes_mut()
            .get_mut(axes)
            .map(f)
            .ok_or_else(|| format!("axes index {axes} out of range (figure has {count} axes)"))
    }

    fn add_subplot_impl(
        &mut self,
        nrows: usize,
        ncols: usize,
        index: usize,
    ) -> Result<usize, String> {
        if index == 0 || index > nrows * ncols {
            return Err(format!(
                "subplot index {index} out of range for a {nrows}x{ncols} grid (1-based)"
            ));
        }
        self.fig.add_subplot(nrows, ncols, index);
        Ok(self.fig.axes().len() - 1)
    }

    fn plot_impl(&mut self, axes: usize, x: &[f64], y: &[f64]) -> Result<(), String> {
        self.with_axes(axes, |ax| {
            ax.plot(x, y);
        })
    }

    fn plot_styled_impl(
        &mut self,
        axes: usize,
        x: &[f64],
        y: &[f64],
        spec: LineStyleSpec,
    ) -> Result<(), String> {
        self.with_axes(axes, |ax| {
            let color = spec.color.unwrap_or_else(|| ax.next_cycle_color());
            let mut line = Line2D::new(x.to_vec(), y.to_vec()).with_color(color);
            if let Some(w) = spec.linewidth {
                line = line.with_linewidth(w);
            }
            if let Some(dashes) = spec.dashes {
                line = line.with_dashes(dashes);
            }
            ax.add_line(line);
        })
    }

    fn set_line_data_impl(
        &mut self,
        axes: usize,
        line: usize,
        x: &[f64],
        y: &[f64],
    ) -> Result<(), String> {
        self.with_axes(axes, |ax| ax.set_line_data(line, x, y))?
    }

    fn scatter_impl(&mut self, axes: usize, x: &[f64], y: &[f64]) -> Result<(), String> {
        self.with_axes(axes, |ax| {
            ax.scatter(x, y);
        })
    }

    fn limits_impl(&self, axes: usize) -> Result<[f64; 4], String> {
        let count = self.fig.axes().len();
        let ax =
            self.fig.axes().get(axes).ok_or_else(|| {
                format!("axes index {axes} out of range (figure has {count} axes)")
            })?;
        let ((xlo, xhi), (ylo, yhi)) = ax.effective_limits();
        Ok([xlo, xhi, ylo, yhi])
    }

    fn legend_impl(&mut self, axes: usize, labels: Vec<String>) -> Result<(), String> {
        self.with_axes(axes, |ax| {
            if labels.len() > ax.lines.len() {
                return Err(format!(
                    "{} legend labels but only {} lines",
                    labels.len(),
                    ax.lines.len()
                ));
            }
            let entries = labels
                .into_iter()
                .enumerate()
                .map(|(i, label)| (ax.lines[i].color(), label))
                .collect();
            ax.legend(entries);
            Ok(())
        })?
    }
}

/// Convert a core error message into a JS exception value.
///
/// Only called on the wasm boundary; `JsValue` construction is unimplemented
/// on native targets, which is why the `*_impl` layer carries `String`s.
fn js_err(msg: String) -> JsValue {
    JsValue::from_str(&msg)
}

#[cfg(target_arch = "wasm32")]
pub use canvas::{WasmSession, draw_sample_to_canvas, render_rgba_to_canvas};

#[cfg(target_arch = "wasm32")]
mod canvas {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
    use web_sys::{
        AddEventListenerOptions, CanvasRenderingContext2d, HtmlCanvasElement, ImageData,
        PointerEvent, WheelEvent,
    };

    use crate::figure::{Event, Interactor, MouseButton, Outcome};
    use crate::wasm::{WasmFigure, figure_to_rgba_scaled, sample_figure};

    #[wasm_bindgen]
    impl WasmFigure {
        /// Render this figure onto the canvas element with id `canvas_id`,
        /// HiDPI-crisp: the backing store is `devicePixelRatio` × the figure's
        /// logical pixel size and the canvas CSS size is set to the logical
        /// size.
        ///
        /// # Errors
        ///
        /// Returns a [`JsValue`] error if the canvas element cannot be found, is
        /// not a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
        pub fn render(&self, canvas_id: &str) -> Result<(), JsValue> {
            let (canvas, context) = canvas_context(canvas_id)?;
            let scale = device_scale();
            let (rgba, width, height) = figure_to_rgba_scaled(&self.fig, scale);
            set_css_size(&canvas, self.fig.size_px())?;
            blit(&canvas, &context, &rgba, width, height)
        }

        /// Consume this figure and attach it to the canvas with id `canvas_id`
        /// as an interactive session: HiDPI rendering plus wheel zoom
        /// (anchored at the cursor), left-drag pan, double-click reset, and a
        /// hover callback. Keep the returned [`WasmSession`] alive for as long
        /// as the canvas should stay interactive.
        ///
        /// # Errors
        ///
        /// Returns a [`JsValue`] error if the canvas element cannot be found,
        /// is not a canvas, has no 2D context, or rendering fails.
        pub fn bind(self, canvas_id: &str) -> Result<WasmSession, JsValue> {
            WasmSession::attach(self.fig, canvas_id)
        }
    }

    /// Shared mutable state behind an interactive canvas session.
    struct SessionState {
        /// The interaction state machine owning the figure.
        interactor: Interactor,
        /// The bound canvas element.
        canvas: HtmlCanvasElement,
        /// The canvas' 2D context, used for `putImageData`.
        context: CanvasRenderingContext2d,
        /// Device pixel ratio captured at bind time.
        scale: f64,
        /// Whether a `requestAnimationFrame` repaint is already queued.
        raf_pending: bool,
        /// Host hover callback: called `(axes, x, y)` over data, `(null)` on
        /// leave.
        hover_cb: Option<js_sys::Function>,
        /// A cursor trail being recorded into a line (see
        /// [`WasmSession::track_cursor`]).
        trail: Option<Trail>,
    }

    /// A rolling record of recent cursor positions, mirrored into a line
    /// artist entirely on the Rust side of the boundary.
    struct Trail {
        /// The axes whose hovers feed the trail.
        axes: usize,
        /// The line (by insertion order within `axes`) receiving the trail.
        line: usize,
        /// Maximum number of retained points; older ones fall off the tail.
        capacity: usize,
        /// The retained cursor positions in data coordinates, oldest first.
        points: std::collections::VecDeque<(f64, f64)>,
    }

    /// An interactive figure bound to a canvas.
    ///
    /// Created by [`WasmFigure::bind`]. Dropping the session leaves the last
    /// frame on the canvas but stops all interaction (the DOM listeners it
    /// owns are dropped).
    #[wasm_bindgen]
    pub struct WasmSession {
        /// Interaction + rendering state shared with the event closures.
        state: Rc<RefCell<SessionState>>,
        /// Owned DOM event closures; dropping them detaches interaction.
        _listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    }

    #[wasm_bindgen]
    impl WasmSession {
        /// The figure's **logical** pixel size as `[width, height]` (CSS
        /// pixels; multiply by `devicePixelRatio` for the backing size).
        #[wasm_bindgen(getter)]
        #[must_use]
        pub fn size(&self) -> Box<[f64]> {
            let (w, h) = self.state.borrow().interactor.figure().size_px();
            Box::new([w, h])
        }

        /// Repaint the canvas now (outside the rAF coalescing).
        ///
        /// # Errors
        ///
        /// Returns a [`JsValue`] error if `ImageData`/`putImageData` fails.
        pub fn render(&self) -> Result<(), JsValue> {
            render_now(&self.state)
        }

        /// The effective `[xlo, xhi, ylo, yhi]` limits of axes `axes` (the
        /// live values pan/zoom mutate).
        ///
        /// # Errors
        ///
        /// Returns an error if `axes` is out of range.
        pub fn limits(&self, axes: usize) -> Result<Box<[f64]>, JsValue> {
            let state = self.state.borrow();
            let fig = state.interactor.figure();
            let ax = fig.axes().get(axes).ok_or_else(|| {
                JsValue::from_str(&format!(
                    "axes index {axes} out of range (figure has {} axes)",
                    fig.axes().len()
                ))
            })?;
            let ((xlo, xhi), (ylo, yhi)) = ax.effective_limits();
            Ok(Box::new([xlo, xhi, ylo, yhi]))
        }

        /// Map a **logical canvas pixel** to `[axes, x, y]` data coordinates,
        /// or `undefined` when the pixel is over no axes.
        #[must_use]
        pub fn data_at(&self, px: f64, py: f64) -> Option<Box<[f64]>> {
            let state = self.state.borrow();
            let fig = state.interactor.figure();
            let axes = fig.axes_at(px, py)?;
            let (x, y) = fig.pixel_to_data(axes, px, py)?;
            Some(Box::new([axes as f64, x, y]))
        }

        /// Replace the data of line `line` on axes `axes` in place (live
        /// updates) and schedule a rAF-coalesced repaint: a burst of updates
        /// between frames paints once.
        ///
        /// Autoscaled limits re-derive from the new data; explicit limits —
        /// including a view the user has panned/zoomed — are untouched.
        ///
        /// # Errors
        ///
        /// Returns an error if `axes` or `line` is out of range.
        pub fn set_line_data(
            &self,
            axes: usize,
            line: usize,
            x: &[f64],
            y: &[f64],
        ) -> Result<(), JsValue> {
            {
                let mut st = self.state.borrow_mut();
                let fig = st.interactor.figure_mut();
                let ax = fig
                    .axes_mut()
                    .get_mut(axes)
                    .ok_or_else(|| JsValue::from_str(&format!("axes index {axes} out of range")))?;
                ax.set_line_data(line, x, y)
                    .map_err(|e| JsValue::from_str(&e))?;
            }
            schedule_redraw(&self.state);
            Ok(())
        }

        /// Register a hover callback, called as `cb(axes, x, y)` while the
        /// cursor is over axes data and `cb(null)` when it leaves the canvas.
        pub fn on_hover(&self, cb: js_sys::Function) {
            self.state.borrow_mut().hover_cb = Some(cb);
        }

        /// Record the cursor's data-space position into line `line` of axes
        /// `axes` as a rolling trail of up to `capacity` points — updated
        /// entirely in Rust as pointer events arrive, with no JS in the loop.
        ///
        /// Each repaint is rAF-coalesced like any other update, and pan/zoom
        /// keep working while the trail records (the trail pauses during a
        /// drag, when the cursor is panning rather than hovering).
        ///
        /// # Errors
        ///
        /// Returns an error if `axes` or `line` is out of range, or
        /// `capacity` is zero.
        pub fn track_cursor(
            &self,
            axes: usize,
            line: usize,
            capacity: usize,
        ) -> Result<(), JsValue> {
            if capacity == 0 {
                return Err(JsValue::from_str("track_cursor: capacity must be > 0"));
            }
            let mut st = self.state.borrow_mut();
            let fig = st.interactor.figure();
            let ax = fig
                .axes()
                .get(axes)
                .ok_or_else(|| JsValue::from_str(&format!("axes index {axes} out of range")))?;
            if line >= ax.line_count() {
                return Err(JsValue::from_str(&format!(
                    "line index {line} out of range (axes has {} lines)",
                    ax.line_count()
                )));
            }
            st.trail = Some(Trail {
                axes,
                line,
                capacity,
                points: std::collections::VecDeque::with_capacity(capacity),
            });
            Ok(())
        }

        /// Build the session: size the canvas for HiDPI, paint the first
        /// frame, and attach the DOM listeners.
        fn attach(fig: crate::figure::Figure, canvas_id: &str) -> Result<WasmSession, JsValue> {
            let (canvas, context) = canvas_context(canvas_id)?;
            set_css_size(&canvas, fig.size_px())?;
            let state = Rc::new(RefCell::new(SessionState {
                interactor: Interactor::new(fig),
                canvas,
                context,
                scale: device_scale(),
                raf_pending: false,
                hover_cb: None,
                trail: None,
            }));
            render_now(&state)?;

            let mut listeners = Vec::new();
            add_pointer_listeners(&state, &mut listeners)?;
            add_wheel_listener(&state, &mut listeners)?;
            Ok(WasmSession {
                state,
                _listeners: listeners,
            })
        }
    }

    /// Map a DOM `MouseEvent.button` code to a [`MouseButton`].
    fn dom_button(code: i16) -> Option<MouseButton> {
        match code {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Middle),
            2 => Some(MouseButton::Right),
            _ => None,
        }
    }

    /// Normalize a DOM wheel delta to "lines" (one detent ≈ 1.0).
    fn wheel_lines(ev: &WheelEvent) -> f64 {
        match ev.delta_mode() {
            WheelEvent::DOM_DELTA_PIXEL => ev.delta_y() / 120.0,
            WheelEvent::DOM_DELTA_PAGE => ev.delta_y() * 10.0,
            // DOM_DELTA_LINE and anything else pass through unscaled.
            _ => ev.delta_y(),
        }
    }

    /// Attach the pointer listeners (`down`/`move`/`up`/`leave`, `dblclick`).
    ///
    /// Positions use `offsetX`/`offsetY`, which are CSS pixels relative to the
    /// canvas — identical to logical figure pixels because the canvas CSS size
    /// is pinned to the figure's logical size.
    fn add_pointer_listeners(
        state: &Rc<RefCell<SessionState>>,
        listeners: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
    ) -> Result<(), JsValue> {
        add_listener(state, listeners, "pointerdown", |ev| {
            let pe: &PointerEvent = ev.dyn_ref()?;
            let button = dom_button(pe.button())?;
            // Capture the pointer so a pan drag keeps flowing after the
            // cursor leaves the canvas.
            if let Some(canvas) = ev
                .current_target()
                .and_then(|t| t.dyn_into::<HtmlCanvasElement>().ok())
            {
                let _ = canvas.set_pointer_capture(pe.pointer_id());
            }
            Some(Event::MouseDown {
                x: f64::from(pe.offset_x()),
                y: f64::from(pe.offset_y()),
                button,
            })
        })?;
        add_listener(state, listeners, "pointermove", |ev| {
            let pe: &PointerEvent = ev.dyn_ref()?;
            Some(Event::MouseMove {
                x: f64::from(pe.offset_x()),
                y: f64::from(pe.offset_y()),
            })
        })?;
        add_listener(state, listeners, "pointerup", |ev| {
            let pe: &PointerEvent = ev.dyn_ref()?;
            let button = dom_button(pe.button())?;
            // Release the capture taken on pointerdown; harmless if the
            // browser already released it.
            if let Some(canvas) = ev
                .current_target()
                .and_then(|t| t.dyn_into::<HtmlCanvasElement>().ok())
            {
                let _ = canvas.release_pointer_capture(pe.pointer_id());
            }
            Some(Event::MouseUp {
                x: f64::from(pe.offset_x()),
                y: f64::from(pe.offset_y()),
                button,
            })
        })?;
        add_listener(state, listeners, "pointerleave", |_| Some(Event::Leave))?;
        // Touch/pen interruptions and capture loss must cancel an in-progress
        // drag, or the interactor keeps panning against a stale anchor when
        // the pointer later reappears.
        add_listener(state, listeners, "pointercancel", |_| Some(Event::Leave))?;
        add_listener(state, listeners, "lostpointercapture", |_| {
            Some(Event::Leave)
        })?;
        add_listener(state, listeners, "dblclick", |ev| {
            let me: &web_sys::MouseEvent = ev.dyn_ref()?;
            Some(Event::DoubleClick {
                x: f64::from(me.offset_x()),
                y: f64::from(me.offset_y()),
            })
        })
    }

    /// Attach the wheel listener non-passively so zooming can suppress page
    /// scroll with `preventDefault`.
    fn add_wheel_listener(
        state: &Rc<RefCell<SessionState>>,
        listeners: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
    ) -> Result<(), JsValue> {
        let st = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
            let Some(we) = ev.dyn_ref::<WheelEvent>() else {
                return;
            };
            ev.prevent_default();
            let event = Event::Wheel {
                x: f64::from(we.offset_x()),
                y: f64::from(we.offset_y()),
                dy: wheel_lines(we),
            };
            dispatch(&st, event);
        });
        let canvas = state.borrow().canvas.clone();
        let opts = AddEventListenerOptions::new();
        opts.set_passive(false);
        canvas.add_event_listener_with_callback_and_add_event_listener_options(
            "wheel",
            cb.as_ref().unchecked_ref(),
            &opts,
        )?;
        listeners.push(cb);
        Ok(())
    }

    /// Attach one passive listener that maps the DOM event through `map` and
    /// dispatches the result.
    fn add_listener(
        state: &Rc<RefCell<SessionState>>,
        listeners: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
        name: &str,
        map: impl Fn(&web_sys::Event) -> Option<Event> + 'static,
    ) -> Result<(), JsValue> {
        let st = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
            if let Some(event) = map(&ev) {
                dispatch(&st, event);
            }
        });
        let canvas = state.borrow().canvas.clone();
        canvas.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())?;
        listeners.push(cb);
        Ok(())
    }

    /// Feed one event to the interactor and act on the outcome.
    ///
    /// The state borrow is dropped before any JS callback runs, so a hover
    /// callback may freely call back into the session.
    fn dispatch(state: &Rc<RefCell<SessionState>>, event: Event) {
        let outcome = state.borrow_mut().interactor.handle(event);
        match outcome {
            Outcome::NeedsRedraw => schedule_redraw(state),
            Outcome::Hover { axes, x, y } => {
                if record_trail_point(state, axes, x, y) {
                    schedule_redraw(state);
                }
                let cb = state.borrow().hover_cb.clone();
                if let Some(cb) = cb {
                    let _ = cb.call3(
                        &JsValue::NULL,
                        &JsValue::from_f64(axes as f64),
                        &JsValue::from_f64(x),
                        &JsValue::from_f64(y),
                    );
                }
            }
            Outcome::Unchanged => {
                if matches!(event, Event::Leave) {
                    let cb = state.borrow().hover_cb.clone();
                    if let Some(cb) = cb {
                        let _ = cb.call1(&JsValue::NULL, &JsValue::NULL);
                    }
                }
            }
        }
    }

    /// Append a hovered data point to the session's cursor trail (if one is
    /// tracking `axes`), mirroring the retained points into the trail's line.
    /// Returns whether the figure changed and needs a repaint.
    fn record_trail_point(state: &Rc<RefCell<SessionState>>, axes: usize, x: f64, y: f64) -> bool {
        let mut st = state.borrow_mut();
        // Split the borrow so the trail and the figure can be held together.
        let st = &mut *st;
        let Some(trail) = st.trail.as_mut() else {
            return false;
        };
        if trail.axes != axes {
            return false;
        }
        trail.points.push_back((x, y));
        while trail.points.len() > trail.capacity {
            trail.points.pop_front();
        }
        let xs: Vec<f64> = trail.points.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = trail.points.iter().map(|p| p.1).collect();
        st.interactor
            .figure_mut()
            .axes_mut()
            .get_mut(trail.axes)
            .and_then(|ax| ax.set_line_data(trail.line, &xs, &ys).ok())
            .is_some()
    }

    /// Queue a repaint on the next animation frame, coalescing bursts: any
    /// number of `NeedsRedraw` outcomes between frames paint exactly once.
    fn schedule_redraw(state: &Rc<RefCell<SessionState>>) {
        {
            let mut st = state.borrow_mut();
            if st.raf_pending {
                return;
            }
            st.raf_pending = true;
        }
        let st = state.clone();
        // `once_into_js` hands ownership to the JS GC; the closure memory is
        // reclaimed after it runs, so per-frame scheduling does not leak.
        let cb = Closure::once_into_js(move || {
            st.borrow_mut().raf_pending = false;
            let _ = render_now(&st);
        });
        let scheduled = web_sys::window()
            .map(|win| win.request_animation_frame(cb.unchecked_ref()).is_ok())
            .unwrap_or(false);
        if !scheduled {
            // Never leave `raf_pending` wedged when scheduling fails (no
            // window, rAF error): reset the flag and paint synchronously so
            // redraws keep flowing.
            state.borrow_mut().raf_pending = false;
            let _ = render_now(state);
        }
    }

    /// Render the session's figure at the *current* DPR and blit it.
    ///
    /// The device pixel ratio is re-read every frame (browser zoom or a move
    /// to a different-density monitor changes it after bind); the blit resizes
    /// the backing store when it changes, and the CSS size stays pinned to the
    /// logical figure size, so event coordinates are unaffected.
    fn render_now(state: &Rc<RefCell<SessionState>>) -> Result<(), JsValue> {
        state.borrow_mut().scale = device_scale();
        let st = state.borrow();
        let (rgba, width, height) = figure_to_rgba_scaled(st.interactor.figure(), st.scale);
        blit(&st.canvas, &st.context, &rgba, width, height)
    }

    /// The window's `devicePixelRatio`, guarded to a sane positive value.
    fn device_scale() -> f64 {
        web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0)
    }

    /// Pin the canvas' CSS size to the figure's logical pixel size, so the
    /// HiDPI backing store is presented at 1 logical px = 1 CSS px.
    fn set_css_size(canvas: &HtmlCanvasElement, logical: (f64, f64)) -> Result<(), JsValue> {
        let style = canvas.style();
        style.set_property("width", &format!("{}px", logical.0))?;
        style.set_property("height", &format!("{}px", logical.1))
    }

    /// Size the canvas backing store to `(width, height)` device pixels and
    /// `putImageData` the buffer at the origin.
    fn blit(
        canvas: &HtmlCanvasElement,
        context: &CanvasRenderingContext2d,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), JsValue> {
        if canvas.width() != width {
            canvas.set_width(width);
        }
        if canvas.height() != height {
            canvas.set_height(height);
        }
        let clamped = wasm_bindgen::Clamped(rgba);
        let image_data = ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)?;
        context.put_image_data(&image_data, 0.0, 0.0)
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

    /// Render the built-in [`sample_figure`] onto the canvas element with id
    /// `canvas_id` (HiDPI-crisp, non-interactive).
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if the canvas element cannot be found, is not
    /// a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
    #[wasm_bindgen]
    pub fn draw_sample_to_canvas(canvas_id: &str) -> Result<(), JsValue> {
        WasmFigure {
            fig: sample_figure(),
        }
        .render(canvas_id)
    }

    /// Blit a straight-RGBA8 buffer onto the canvas element with id
    /// `canvas_id`, sizing the backing store to `width` by `height` device
    /// pixels (no CSS sizing — presentation scale is the caller's concern).
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
        blit(&canvas, &context, rgba, width, height)
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
    fn dash_patterns_map_matplotlib_tokens() {
        assert_eq!(dash_pattern("-").unwrap(), None);
        assert_eq!(dash_pattern("solid").unwrap(), None);
        assert_eq!(dash_pattern("--").unwrap(), Some((0.0, vec![3.7, 1.6])));
        assert_eq!(dash_pattern(":").unwrap(), Some((0.0, vec![1.0, 1.65])));
        assert_eq!(
            dash_pattern("-.").unwrap(),
            Some((0.0, vec![6.4, 1.6, 1.0, 1.6]))
        );
        assert!(dash_pattern("~~").is_err());
    }

    #[test]
    fn build_line_style_validates_parts() {
        let spec = build_line_style(Some("tab:orange"), Some(2.5), Some("--")).unwrap();
        assert!(spec.color.is_some());
        assert_eq!(spec.linewidth, Some(2.5));
        assert_eq!(spec.dashes, Some(Some((0.0, vec![3.7, 1.6]))));

        assert_eq!(
            build_line_style(None, None, None).unwrap(),
            LineStyleSpec::default()
        );
        assert!(build_line_style(Some("not-a-color"), None, None).is_err());
        assert!(build_line_style(None, Some(-1.0), None).is_err());
        assert!(build_line_style(None, Some(f64::NAN), None).is_err());
        assert!(build_line_style(None, None, Some("wavy")).is_err());
    }

    #[test]
    fn js_surface_builds_a_real_figure() {
        // Drive the target-agnostic `_impl` layer the browser wrappers
        // forward to (the JsValue boundary itself is covered by the
        // wasm-pack browser tests).
        let mut wf = WasmFigure::new(4.0, 3.0);
        let ax = wf.add_axes(0.1, 0.1, 0.8, 0.8);
        assert_eq!(ax, 0);
        wf.plot_impl(0, &[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0]).unwrap();
        let styled = build_line_style(Some("tab:red"), Some(3.0), Some("--")).unwrap();
        wf.plot_styled_impl(0, &[0.0, 2.0], &[4.0, 0.0], styled)
            .unwrap();
        wf.scatter_impl(0, &[0.5, 1.5], &[0.2, 2.0]).unwrap();
        wf.with_axes(0, |ax| {
            ax.set_title("t");
            ax.set_xlabel("x");
            ax.set_ylabel("y");
            ax.set_xlim(0.0, 2.0);
            ax.set_ylim(0.0, 4.0);
        })
        .unwrap();
        wf.legend_impl(0, vec!["line".into(), "styled".into()])
            .unwrap();

        assert_eq!(wf.limits_impl(0).unwrap(), [0.0, 2.0, 0.0, 4.0]);
        // Out-of-range axes and over-long legends error.
        assert!(wf.plot_impl(3, &[0.0], &[0.0]).is_err());
        assert!(wf.limits_impl(3).is_err());
        assert!(
            wf.legend_impl(0, vec!["a".into(), "b".into(), "c".into()])
                .is_err()
        );

        // The built figure actually renders ink.
        let (rgba, w, h) = figure_to_rgba(&wf.fig);
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        assert!(rgba.chunks_exact(4).any(|px| px != [255, 255, 255, 255]));
    }

    #[test]
    fn set_line_data_updates_autoscale_but_preserves_explicit_limits() {
        let mut wf = WasmFigure::new(4.0, 3.0);
        wf.add_axes(0.1, 0.1, 0.8, 0.8);
        wf.plot_impl(0, &[0.0, 1.0], &[0.0, 1.0]).unwrap();

        // Untouched view: autoscale re-derives from the new data.
        wf.set_line_data_impl(0, 0, &[0.0, 10.0], &[0.0, 100.0])
            .unwrap();
        let auto = wf.limits_impl(0).unwrap();
        assert!(
            auto[1] >= 10.0 && auto[3] >= 100.0,
            "autoscale follows: {auto:?}"
        );

        // Framed view: explicit limits survive a data update.
        wf.with_axes(0, |ax| {
            ax.set_xlim(2.0, 4.0);
            ax.set_ylim(0.0, 50.0);
        })
        .unwrap();
        wf.set_line_data_impl(0, 0, &[0.0, 1000.0], &[0.0, 9999.0])
            .unwrap();
        assert_eq!(wf.limits_impl(0).unwrap(), [2.0, 4.0, 0.0, 50.0]);

        // Bad indices error cleanly.
        assert!(wf.set_line_data_impl(0, 5, &[0.0], &[0.0]).is_err());
        assert!(wf.set_line_data_impl(3, 0, &[0.0], &[0.0]).is_err());
    }

    #[test]
    fn subplot_index_validation() {
        let mut wf = WasmFigure::new(4.0, 3.0);
        assert!(wf.add_subplot_impl(2, 2, 0).is_err());
        assert!(wf.add_subplot_impl(2, 2, 5).is_err());
        assert_eq!(wf.add_subplot_impl(2, 2, 1).unwrap(), 0);
        assert_eq!(wf.add_subplot_impl(2, 2, 4).unwrap(), 1);
    }

    #[test]
    fn scaled_render_matches_double_dpi_render() {
        // The W1 acceptance criterion: rendering at scale 2 must be
        // bit-identical to rendering the same figure built at 2x DPI.
        let fig = sample_figure();
        let scaled = fig.render_scaled(2.0);

        let mut hidpi = sample_figure();
        let dpi = hidpi.dpi();
        hidpi = hidpi.with_dpi(dpi * 2.0);
        let reference = hidpi.render();

        assert_eq!(scaled.pixmap().width(), reference.pixmap().width());
        assert_eq!(scaled.pixmap().height(), reference.pixmap().height());
        assert_eq!(
            scaled.pixmap().data(),
            reference.pixmap().data(),
            "scale-2 render must be bit-identical to a 2x-DPI render"
        );
    }

    #[test]
    fn decorations_scale_with_render_dpi() {
        // Text (and other decoration geometry) must grow with the DPI, not
        // stay pixel-fixed: the title band's ink should roughly quadruple at
        // scale 2 (2x in each dimension, modulo antialiased edges). A
        // pixel-fixed title would hold the ratio near 1. The axes is turned
        // off so the band holds nothing but glyphs.
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.15, 0.15, 0.78, 0.74);
        ax.set_axis_off();
        ax.set_title("A Title To Measure");
        let band_ink = |scale: f64| {
            let r = fig.render_scaled(scale);
            let px = r.pixmap();
            let (w, h) = (px.width(), px.height());
            let band = (f64::from(h) * 0.12) as u32;
            let mut n = 0u32;
            for y in 0..band {
                for x in 0..w {
                    let p = px.pixel(x, y).expect("in bounds").demultiply();
                    if (p.red(), p.green(), p.blue()) != (255, 255, 255) {
                        n += 1;
                    }
                }
            }
            n
        };
        let one = band_ink(1.0);
        let two = band_ink(2.0);
        assert!(one > 0, "the title band must contain ink");
        let ratio = f64::from(two) / f64::from(one);
        assert!(
            ratio > 2.5,
            "title ink must scale ~quadratically with DPI, got ratio {ratio:.2}"
        );
    }

    #[test]
    fn scaled_rgba_doubles_dimensions() {
        let fig = sample_figure();
        let (rgba1, w1, h1) = figure_to_rgba_scaled(&fig, 1.0);
        let (rgba2, w2, h2) = figure_to_rgba_scaled(&fig, 2.0);
        assert_eq!((w2, h2), (w1 * 2, h1 * 2));
        assert_eq!(rgba2.len(), rgba1.len() * 4);
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
