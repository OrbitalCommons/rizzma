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
//! rizzma::pyplot::plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
//! rizzma::pyplot::title("hello");
//! rizzma::pyplot::savefig("plot.png").unwrap();
//! ```
//!
//! Build-order home: Phase 8 of `design/04-implementation-plan.md`.

use std::cell::RefCell;
use std::path::Path;

use crate::core::color::Rgba;
use crate::figure::{Axes, Figure};
use crate::skia::PngError;

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

/// Draw a horizontal bar chart of `width` at positions `y` on the current axes.
pub fn barh(y: &[f64], width: &[f64]) {
    with_axes(|ax| ax.barh(y, width));
}

/// Plot `y` against `x` as a piecewise-constant staircase line on the current
/// axes.
pub fn step(x: &[f64], y: &[f64]) {
    with_axes(|ax| {
        ax.step(x, y);
    });
}

/// Draw a stairstep plot of `values` over bin `edges` on the current axes.
pub fn stairs(values: &[f64], edges: &[f64]) {
    with_axes(|ax| ax.stairs(values, edges));
}

/// Draw a stem plot of `y` against `x` on the current axes.
pub fn stem(x: &[f64], y: &[f64]) {
    with_axes(|ax| ax.stem(x, y));
}

/// Draw stacked filled areas of the series `ys` over `x` on the current axes.
pub fn stackplot(x: &[f64], ys: &[&[f64]]) {
    with_axes(|ax| ax.stackplot(x, ys));
}

/// Draw horizontal bars spanning the `xranges` at vertical `yrange` on the
/// current axes.
pub fn broken_barh(xranges: &[(f64, f64)], yrange: (f64, f64)) {
    with_axes(|ax| ax.broken_barh(xranges, yrange));
}

/// Draw a grouped (clustered) bar chart from the `series` on the current axes.
pub fn grouped_bar(series: &[&[f64]]) {
    with_axes(|ax| {
        ax.grouped_bar(series);
    });
}

/// Plot `y` against `x` with symmetric vertical error bars `yerr` on the
/// current axes.
pub fn errorbar(x: &[f64], y: &[f64], yerr: &[f64]) {
    with_axes(|ax| ax.errorbar(x, y, yerr));
}

/// Draw a box-and-whisker plot for each dataset in `data` on the current axes.
pub fn boxplot(data: &[&[f64]]) {
    with_axes(|ax| ax.boxplot(data));
}

/// Draw a violin plot for each dataset in `data` at optional `positions` on the
/// current axes.
pub fn violinplot(data: &[&[f64]], positions: Option<&[f64]>) {
    with_axes(|ax| {
        ax.violinplot(data, positions);
    });
}

/// Draw a hexagonal binning of `(x, y)` with `gridsize` bins on the current
/// axes.
pub fn hexbin(x: &[f64], y: &[f64], gridsize: usize) {
    with_axes(|ax| {
        ax.hexbin(x, y, gridsize);
    });
}

/// Draw the empirical cumulative distribution of `data` on the current axes.
pub fn ecdf(data: &[f64]) {
    with_axes(|ax| ax.ecdf(data));
}

/// Display `data` as an `nrows` by `ncols` image on the current axes.
pub fn imshow(data: &[f64], nrows: usize, ncols: usize) {
    with_axes(|ax| {
        ax.imshow(data, nrows, ncols);
    });
}

/// Display the `nrows` by `ncols` matrix `data` as an image on the current axes.
pub fn matshow(data: &[f64], nrows: usize, ncols: usize) {
    with_axes(|ax| {
        ax.matshow(data, nrows, ncols);
    });
}

/// Visualize the sparsity pattern of the `nrows` by `ncols` matrix `data` on the
/// current axes.
pub fn spy(data: &[f64], nrows: usize, ncols: usize) {
    with_axes(|ax| {
        ax.spy(data, nrows, ncols);
    });
}

/// Draw a pseudocolor mesh of the `nrows` by `ncols` cell values `c` on the
/// current axes.
pub fn pcolormesh(c: &[f64], nrows: usize, ncols: usize) {
    with_axes(|ax| {
        ax.pcolormesh(c, nrows, ncols);
    });
}

/// Draw contour lines of the `nrows` by `ncols` scalar field `z` on the current
/// axes.
pub fn contour(z: &[f64], nrows: usize, ncols: usize) {
    with_axes(|ax| ax.contour(z, nrows, ncols));
}

/// Draw `n_levels` contour lines of the `nrows` by `ncols` scalar field `z` on
/// the current axes.
pub fn contour_levels(z: &[f64], nrows: usize, ncols: usize, n_levels: usize) {
    with_axes(|ax| ax.contour_levels(z, nrows, ncols, n_levels));
}

/// Draw a 2-D histogram of `(x, y)` with `bins` bins per axis on the current
/// axes.
pub fn hist2d(x: &[f64], y: &[f64], bins: usize) {
    with_axes(|ax| {
        ax.hist2d(x, y, bins);
    });
}

/// Draw an event raster plot: one row of ticks per dataset in `positions` on
/// the current axes.
pub fn eventplot(positions: &[&[f64]]) {
    with_axes(|ax| ax.eventplot(positions));
}

/// Fill the region between the curves `(x1, y)` and `(x2, y)` on the current
/// axes.
pub fn fill_betweenx(y: &[f64], x1: &[f64], x2: &[f64]) {
    with_axes(|ax| ax.fill_betweenx(y, x1, x2));
}

/// Draw a pie chart of `values` on the current axes.
pub fn pie(values: &[f64]) {
    with_axes(|ax| {
        ax.pie(values);
    });
}

/// Draw a horizontal line segment at each value in `y`, from `xmin` to `xmax`,
/// on the current axes.
pub fn hlines(y: &[f64], xmin: f64, xmax: f64) {
    with_axes(|ax| ax.hlines(y, xmin, xmax));
}

/// Draw a vertical line segment at each value in `x`, from `ymin` to `ymax`, on
/// the current axes.
pub fn vlines(x: &[f64], ymin: f64, ymax: f64) {
    with_axes(|ax| ax.vlines(x, ymin, ymax));
}

/// Add a full-width horizontal reference line at `y` on the current axes.
pub fn axhline(y: f64) {
    with_axes(|ax| ax.axhline(y));
}

/// Add a full-height vertical reference line at `x` on the current axes.
pub fn axvline(x: f64) {
    with_axes(|ax| ax.axvline(x));
}

/// Add a full-width horizontal shaded band between `ymin` and `ymax` on the
/// current axes.
pub fn axhspan(ymin: f64, ymax: f64) {
    with_axes(|ax| ax.axhspan(ymin, ymax));
}

/// Add a full-height vertical shaded band between `xmin` and `xmax` on the
/// current axes.
pub fn axvspan(xmin: f64, xmax: f64) {
    with_axes(|ax| ax.axvspan(xmin, xmax));
}

/// Set an equal data aspect ratio on the current axes.
pub fn axis_equal() {
    with_axes(|ax| {
        ax.set_aspect_equal();
    });
}

/// Hide the axis lines, ticks, and labels on the current axes.
pub fn axis_off() {
    with_axes(|ax| {
        ax.set_axis_off();
    });
}

/// Show the axis lines, ticks, and labels on the current axes.
pub fn axis_on() {
    with_axes(|ax| {
        ax.set_axis_on();
    });
}

/// Pad the data limits of the current axes by the fractional `margin`.
pub fn margins(margin: f64) {
    with_axes(|ax| {
        ax.set_margins(margin);
    });
}

/// Add a legend to the current axes from `(color, label)` entries.
///
/// Mirrors `plt.legend(...)`; each entry draws a color swatch beside its label.
pub fn legend(entries: Vec<(Rgba, String)>) {
    with_axes(|ax| {
        ax.legend(entries);
    });
}

/// Add a vertical colorbar to the current figure for `cmap_name` over the value
/// range `[vmin, vmax]`.
///
/// Mirrors `plt.colorbar(...)`. The colormap name matches the built-in palettes
/// (e.g. `"viridis"`, `"gray"`).
pub fn colorbar(cmap_name: &str, vmin: f64, vmax: f64) {
    with_figure(|fig| {
        fig.colorbar(cmap_name, vmin, vmax);
    });
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

/// An error from [`savefig`]: either PNG encoding or filesystem I/O failed.
#[derive(Debug)]
pub enum SaveError {
    /// PNG encoding failed.
    Png(PngError),
    /// Writing the SVG/PDF file failed.
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Png(e) => write!(f, "PNG encoding failed: {e}"),
            SaveError::Io(e) => write!(f, "writing figure file failed: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Render the current figure to `path`, choosing the format from the file
/// extension: `.svg` → SVG, `.pdf` → PDF, anything else → PNG.
///
/// Mirrors matplotlib's `plt.savefig`, which infers the backend from the
/// filename.
///
/// # Errors
///
/// Returns [`SaveError`] if rendering or writing the file fails.
pub fn savefig<P: AsRef<Path>>(path: P) -> Result<(), SaveError> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    with_figure(|fig| match ext.as_deref() {
        Some("svg") => fig.save_svg(path).map_err(SaveError::Io),
        Some("pdf") => fig.save_pdf(path).map_err(SaveError::Io),
        _ => fig.save_png(path).map_err(SaveError::Png),
    })
}

/// Display the current figure.
///
/// Interactive display requires a GUI/wasm backend that does not yet exist, so
/// this is currently a no-op that prints a hint. Use [`savefig`] to persist a
/// figure in the meantime.
pub fn show() {
    eprintln!("crate::pyplot::show() is a no-op; use savefig() to write a file.");
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
    fn savefig_picks_format_from_extension() {
        figure();
        plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);

        let svg = target_path("pyplot_ext.svg");
        savefig(&svg).expect("svg savefig succeeds");
        assert!(
            std::fs::read_to_string(&svg)
                .expect("svg file")
                .contains("<svg"),
            "expected an SVG document"
        );

        let pdf = target_path("pyplot_ext.pdf");
        savefig(&pdf).expect("pdf savefig succeeds");
        assert!(
            std::fs::read(&pdf).expect("pdf file").starts_with(b"%PDF"),
            "expected a PDF document"
        );

        let png = target_path("pyplot_ext.png");
        savefig(&png).expect("png savefig succeeds");
        assert!(
            std::fs::read(&png)
                .expect("png file")
                .starts_with(&[0x89, b'P', b'N', b'G']),
            "expected a PNG document"
        );
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
    fn expanded_wrappers_smoke_savefig() {
        figure();
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [1.0, 2.0, 1.5, 3.0];
        barh(&xs, &ys);
        step(&xs, &ys);
        stairs(&[1.0, 2.0, 3.0], &[0.0, 1.0, 2.0, 3.0]);
        errorbar(&xs, &ys, &[0.1, 0.2, 0.1, 0.3]);
        let group_a = [1.0, 2.0, 3.0];
        let group_b = [2.0, 1.0, 4.0];
        let groups: [&[f64]; 2] = [&group_a, &group_b];
        boxplot(&groups);
        violinplot(&groups, None);
        axhline(2.0);
        axvline(1.5);
        axhspan(0.5, 1.0);
        margins(0.05);
        pie(&[1.0, 2.0, 3.0]);
        let path = target_path("pyplot_expanded_smoke.png");
        savefig(&path).expect("savefig succeeds");
        assert!(
            std::fs::metadata(&path).expect("file exists").len() > 0,
            "smoke PNG should be non-empty"
        );
        close();
    }

    #[test]
    fn stackplot_threads_nested_slices() {
        figure();
        let x = [0.0, 1.0, 2.0];
        let s0 = [1.0, 2.0, 1.0];
        let s1 = [0.5, 0.5, 2.0];
        let series: [&[f64]; 2] = [&s0, &s1];
        stackplot(&x, &series);
        let path = target_path("pyplot_stackplot.png");
        savefig(&path).expect("savefig succeeds");
        assert!(std::fs::metadata(&path).expect("file exists").len() > 0);
        close();
    }

    #[test]
    fn matrix_wrappers_accept_flat_data() {
        figure();
        let data = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        imshow(&data, 2, 3);
        let path = target_path("pyplot_imshow.png");
        savefig(&path).expect("savefig succeeds");
        assert!(std::fs::metadata(&path).expect("file exists").len() > 0);
        close();
    }

    #[test]
    fn broken_barh_threads_tuple_args() {
        figure();
        broken_barh(&[(0.0, 1.0), (2.0, 1.5)], (1.0, 0.5));
        let path = target_path("pyplot_broken_barh.png");
        savefig(&path).expect("savefig succeeds");
        assert!(std::fs::metadata(&path).expect("file exists").len() > 0);
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

    #[test]
    fn legend_and_colorbar_render_to_png() {
        figure();
        plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]);
        legend(vec![
            (Rgba::new(0.2, 0.4, 0.8, 1.0), "series a".to_string()),
            (Rgba::new(0.8, 0.3, 0.2, 1.0), "series b".to_string()),
        ]);
        colorbar("viridis", 0.0, 1.0);
        let path = target_path("pyplot_legend_colorbar.png");
        savefig(&path).expect("savefig succeeds");
        assert!(
            std::fs::metadata(&path).expect("file exists").len() > 0,
            "legend + colorbar PNG should be non-empty"
        );
        close();
    }
}
