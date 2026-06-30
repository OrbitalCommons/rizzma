//! `pyplot`-style stateful facade for rizzma.
//!
//! A slim global layer mirroring `import matplotlib.pyplot as plt`: free
//! functions in the crate root operate on an implicit "current figure" and
//! "current axes" held in thread-local state, delegating to the object-oriented
//! [`Figure`]/[`Axes`] API. The first plotting call auto-creates a default
//! figure (mirroring matplotlib's implicit `gca()`), so simple scripts never
//! touch the registry directly.
//!
//! ```no_run
//! rizzma_pyplot::plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
//! rizzma_pyplot::title("hello");
//! rizzma_pyplot::savefig("plot.png").unwrap();
//! ```
//!
//! Build-order home: Phase 8 of `design/04-implementation-plan.md`.

use std::cell::RefCell;
use std::path::Path;

use rizzma_core::color::Rgba;
use rizzma_figure::{Axes, Figure};
use rizzma_skia::PngError;

/// Default figure width in inches, matching matplotlib's `figure.figsize`.
const DEFAULT_WIDTH_IN: f64 = 6.4;
/// Default figure height in inches, matching matplotlib's `figure.figsize`.
const DEFAULT_HEIGHT_IN: f64 = 4.8;
/// Default single-axes rectangle `(left, bottom, width, height)` in figure
/// fractions, matching matplotlib's default subplot margins.
const DEFAULT_AXES_RECT: (f64, f64, f64, f64) = (0.125, 0.11, 0.775, 0.79);

/// Build a fresh `w_in` by `h_in` figure with a single full-size axes on a white
/// background.
fn new_single_axes_figure(w_in: f64, h_in: f64) -> Figure {
    let mut fig = Figure::new(w_in, h_in).with_facecolor(Rgba::WHITE);
    let (l, b, w, h) = DEFAULT_AXES_RECT;
    fig.add_axes(l, b, w, h);
    fig
}

/// The global, per-thread pyplot state: the current [`Figure`] and the index of
/// the current axes within it.
///
/// Mirrors matplotlib's module-level `_pylab_helpers` registry, scoped to a
/// single thread. Each free function in this crate borrows it briefly, mutates,
/// and releases — no borrow is held across calls.
struct PyplotState {
    /// The current figure, created lazily on first use.
    figure: Option<Figure>,
    /// Index of the current axes within [`figure`](Self::figure)'s axes list.
    current_axes: usize,
}

impl PyplotState {
    /// An empty state with no current figure.
    const fn new() -> Self {
        Self {
            figure: None,
            current_axes: 0,
        }
    }

    /// Return the current figure, creating a default `6.4 x 4.8` inch figure
    /// with a single full-size axes if none exists yet.
    ///
    /// This is the equivalent of matplotlib's implicit `gcf()` + `gca()`.
    fn ensure_figure(&mut self) -> &mut Figure {
        if self.figure.is_none() {
            self.figure = Some(new_single_axes_figure(DEFAULT_WIDTH_IN, DEFAULT_HEIGHT_IN));
            self.current_axes = 0;
        }
        self.figure.as_mut().expect("figure just ensured")
    }

    /// Return the current axes, creating a default figure + axes if needed.
    fn ensure_axes(&mut self) -> &mut Axes {
        let index = self.current_axes;
        let fig = self.ensure_figure();
        fig.axes_mut()
            .get_mut(index)
            .expect("current axes index in range")
    }
}

thread_local! {
    /// Per-thread pyplot state. See [`PyplotState`].
    static STATE: RefCell<PyplotState> = const { RefCell::new(PyplotState::new()) };
}

/// Run `f` against the current axes (auto-creating a figure/axes if needed).
fn with_axes<R>(f: impl FnOnce(&mut Axes) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().ensure_axes()))
}

/// Run `f` against the current figure (auto-creating one if needed).
fn with_figure<R>(f: impl FnOnce(&mut Figure) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().ensure_figure()))
}

/// Create a new current figure at the default size, replacing any existing one.
///
/// The new figure starts with a single full-size axes, mirroring matplotlib's
/// `plt.figure()` followed by an implicit `gca()`.
pub fn figure() {
    figure_sized(DEFAULT_WIDTH_IN, DEFAULT_HEIGHT_IN);
}

/// Create a new current figure `w_in` by `h_in` inches, replacing any existing
/// one and seeding it with a single full-size axes.
pub fn figure_sized(w_in: f64, h_in: f64) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.figure = Some(new_single_axes_figure(w_in, h_in));
        state.current_axes = 0;
    });
}

/// Create a new current figure tiled into an `nrows` by `ncols` grid of axes,
/// and set the current axes to the first cell.
///
/// Mirrors `plt.subplots(nrows, ncols)`. Subsequent plotting calls target the
/// first subplot; use [`sca`] to switch the active cell.
///
/// # Panics
///
/// Panics if `nrows` or `ncols` is zero.
pub fn subplots(nrows: usize, ncols: usize) {
    assert!(nrows >= 1 && ncols >= 1, "subplots grid must be non-empty");
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let mut fig = Figure::new(DEFAULT_WIDTH_IN, DEFAULT_HEIGHT_IN).with_facecolor(Rgba::WHITE);
        for index in 1..=(nrows * ncols) {
            fig.add_subplot(nrows, ncols, index);
        }
        state.figure = Some(fig);
        state.current_axes = 0;
    });
}

/// Set the current axes to the zero-based `index` within the current figure.
///
/// Auto-creates a default figure first if none exists. Out-of-range indices are
/// ignored, leaving the current axes unchanged.
pub fn sca(index: usize) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let len = state.ensure_figure().axes().len();
        if index < len {
            state.current_axes = index;
        }
    });
}

/// Plot `y` against `x` as a line on the current axes.
pub fn plot(x: &[f64], y: &[f64]) {
    with_axes(|ax| {
        ax.plot(x, y);
    });
}

/// Scatter `y` against `x` as markers on the current axes.
pub fn scatter(x: &[f64], y: &[f64]) {
    with_axes(|ax| {
        ax.scatter(x, y);
    });
}

/// Draw a vertical bar chart of `height` at positions `x` on the current axes.
pub fn bar(x: &[f64], height: &[f64]) {
    with_axes(|ax| ax.bar(x, height));
}

/// Draw a histogram of `data` over `bins` equal-width bins on the current axes.
pub fn hist(data: &[f64], bins: usize) {
    with_axes(|ax| {
        ax.hist(data, bins);
    });
}

/// Fill the region between the curves `(x, y1)` and `(x, y2)` on the current
/// axes.
pub fn fill_between(x: &[f64], y1: &[f64], y2: &[f64]) {
    with_axes(|ax| ax.fill_between(x, y1, y2));
}

/// Set the title of the current axes.
pub fn title(text: &str) {
    with_axes(|ax| {
        ax.set_title(text);
    });
}

/// Set the x-axis label of the current axes.
pub fn xlabel(text: &str) {
    with_axes(|ax| {
        ax.set_xlabel(text);
    });
}

/// Set the y-axis label of the current axes.
pub fn ylabel(text: &str) {
    with_axes(|ax| {
        ax.set_ylabel(text);
    });
}

/// Set explicit x limits `(lo, hi)` on the current axes.
pub fn xlim(lo: f64, hi: f64) {
    with_axes(|ax| {
        ax.set_xlim(lo, hi);
    });
}

/// Set explicit y limits `(lo, hi)` on the current axes.
pub fn ylim(lo: f64, hi: f64) {
    with_axes(|ax| {
        ax.set_ylim(lo, hi);
    });
}

/// Render the current figure to `path` as a PNG.
///
/// Always renders a PNG via [`Figure::save_png`]; the `path` extension is kept
/// as given. An interactive vector/SVG backend is a later refinement.
///
/// # Errors
///
/// Returns the underlying PNG encoding/IO error if rendering or writing fails.
pub fn savefig<P: AsRef<Path>>(path: P) -> Result<(), PngError> {
    with_figure(|fig| fig.save_png(path))
}

/// Display the current figure.
///
/// Interactive display requires a GUI/wasm backend that does not yet exist, so
/// this is currently a no-op that prints a hint. Use [`savefig`] to persist a
/// figure in the meantime.
pub fn show() {
    eprintln!("rizzma_pyplot::show() is a no-op; use savefig() to write a file.");
}

/// Clear the current figure, dropping all its axes and artists.
///
/// Mirrors `plt.clf()`. A subsequent plotting call lazily re-creates a fresh
/// default figure.
pub fn clf() {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.figure = None;
        state.current_axes = 0;
    });
}

/// Close the current figure, resetting all pyplot state.
///
/// Mirrors `plt.close()`. After this, the next plotting call starts from a
/// fresh default figure.
pub fn close() {
    clf();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path under the OS temp dir, unique per `name`.
    fn target_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(name);
        p
    }

    #[test]
    fn plot_title_savefig_writes_nonempty_png() {
        figure();
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [0.0, 1.0, 0.0, 1.0];
        plot(&xs, &ys);
        title("t");
        let path = target_path("pyplot_test.png");
        savefig(&path).expect("savefig succeeds");
        let meta = std::fs::metadata(&path).expect("file exists");
        assert!(meta.len() > 0, "PNG should be non-empty");
        close();
    }

    #[test]
    fn plot_auto_initializes_without_figure() {
        // No figure() call first: plotting must lazily create one.
        close();
        plot(&[0.0, 1.0], &[1.0, 0.0]);
        let path = target_path("pyplot_autoinit.png");
        savefig(&path).expect("savefig succeeds after auto-init");
        assert!(
            std::fs::metadata(&path).expect("file exists").len() > 0,
            "auto-init PNG should be non-empty"
        );
        close();
    }

    #[test]
    fn close_resets_state_for_fresh_plot() {
        plot(&[0.0, 5.0], &[0.0, 5.0]);
        close();
        STATE.with(|s| {
            assert!(s.borrow().figure.is_none(), "close drops the figure");
        });
        // After close, a fresh figure with exactly one axes is created.
        plot(&[0.0, 1.0], &[0.0, 1.0]);
        STATE.with(|s| {
            let st = s.borrow();
            let fig = st.figure.as_ref().expect("fresh figure exists");
            assert_eq!(fig.axes().len(), 1, "fresh figure has one axes");
        });
        close();
    }

    #[test]
    fn subplots_sets_first_axes_current() {
        subplots(2, 2);
        STATE.with(|s| {
            let st = s.borrow();
            assert_eq!(st.figure.as_ref().expect("figure").axes().len(), 4);
            assert_eq!(st.current_axes, 0);
        });
        close();
    }
}
