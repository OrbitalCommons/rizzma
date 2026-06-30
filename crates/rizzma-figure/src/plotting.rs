//! Tier-1 plotting methods on [`Axes`].
//!
//! These mirror matplotlib's convenience plotting helpers, each translating its
//! arguments into [`Line2D`] / [`Patch`] artists (or, for the full-span
//! reference helpers, into draw-time-resolved spans stored on the [`Axes`]).
//! Keeping them here leaves [`axes.rs`](super::axes) focused on the core
//! coordinate/limit machinery.

use rizzma_artist::{Line2D, Patch};
use rizzma_core::color::Rgba;

use crate::Axes;
use crate::axes::{SpanLine, SpanOrientation, SpanRect};

/// Default width (in data units) of a vertical [`bar`](Axes::bar).
const DEFAULT_BAR_WIDTH: f64 = 0.8;
/// Default height (in data units) of a horizontal [`barh`](Axes::barh) bar.
const DEFAULT_BARH_HEIGHT: f64 = 0.8;
/// Default fill opacity for [`fill_between`](Axes::fill_between) regions.
const FILL_ALPHA: f64 = 0.4;
/// Default fill opacity for [`axhspan`](Axes::axhspan)/[`axvspan`](Axes::axvspan).
const SPAN_ALPHA: f64 = 0.5;

/// Expand `(x, y)` into a `"pre"` staircase polyline.
///
/// For `n >= 2` input points the result has `2 * n - 1` vertices: each `y[i]`
/// jumps in at the previous `x[i - 1]` and holds across to `x[i]`. Inputs are
/// assumed equal length and at least two points long.
fn step_pre(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = x.len();
    let mut sx: Vec<f64> = Vec::with_capacity(2 * n - 1);
    let mut sy: Vec<f64> = Vec::with_capacity(2 * n - 1);
    sx.push(x[0]);
    sy.push(y[0]);
    for i in 1..n {
        // Rise to the new y at the previous x, then move across to x[i].
        sx.push(x[i - 1]);
        sy.push(y[i]);
        sx.push(x[i]);
        sy.push(y[i]);
    }
    (sx, sy)
}

impl Axes {
    /// Draw a vertical bar chart: one rectangle per `(x, height)` pair.
    ///
    /// Each bar is centered horizontally on its `x` value with the matplotlib
    /// default width of `0.8`, rising from `0.0` to `height`. The face color is
    /// taken from the next entry of the property cycle (advancing it once) and
    /// the edge is black. Only the common prefix of `x` and `height` is used.
    pub fn bar(&mut self, x: &[f64], height: &[f64]) {
        self.bar_with(x, height, DEFAULT_BAR_WIDTH, 0.0);
    }

    /// Draw a vertical bar chart with an explicit `width` and `bottom`.
    ///
    /// Like [`bar`](Axes::bar), but each rectangle spans
    /// `x - width/2 ..= x + width/2` horizontally and `bottom ..= bottom +
    /// height` vertically. The face color advances the property cycle once and
    /// the edge is black.
    pub fn bar_with(&mut self, x: &[f64], height: &[f64], width: f64, bottom: f64) {
        let face = self.next_cycle_color();
        for (&xc, &h) in x.iter().zip(height.iter()) {
            let patch = Patch::rectangle(xc - width / 2.0, bottom, width, h)
                .facecolor(Some(face))
                .edgecolor(Some(Rgba::BLACK));
            self.add_patch(patch);
        }
    }

    /// Draw a horizontal bar chart: one rectangle per `(y, width)` pair.
    ///
    /// Each bar is centered vertically on its `y` value with the matplotlib
    /// default height of `0.8`, extending from `0.0` to `width`. The face color
    /// advances the property cycle once and the edge is black. Only the common
    /// prefix of `y` and `width` is used.
    pub fn barh(&mut self, y: &[f64], width: &[f64]) {
        self.barh_with(y, width, DEFAULT_BARH_HEIGHT, 0.0);
    }

    /// Draw a horizontal bar chart with an explicit `height` and `left`.
    ///
    /// Each rectangle spans `left ..= left + width` horizontally and
    /// `y - height/2 ..= y + height/2` vertically.
    pub fn barh_with(&mut self, y: &[f64], width: &[f64], height: f64, left: f64) {
        let face = self.next_cycle_color();
        for (&yc, &w) in y.iter().zip(width.iter()) {
            let patch = Patch::rectangle(left, yc - height / 2.0, w, height)
                .facecolor(Some(face))
                .edgecolor(Some(Rgba::BLACK));
            self.add_patch(patch);
        }
    }

    /// Fill the region between the curves `(x, y1)` and `(x, y2)`.
    ///
    /// Builds one closed [`Patch::polygon`] tracing `(x, y1)` left-to-right then
    /// `(x, y2)` right-to-left. The default face is a semi-transparent C0 blue
    /// (alpha `0.4`) with no edge. Only the common prefix of the three slices is
    /// used.
    pub fn fill_between(&mut self, x: &[f64], y1: &[f64], y2: &[f64]) {
        let n = x.len().min(y1.len()).min(y2.len());
        if n == 0 {
            return;
        }
        let mut points: Vec<[f64; 2]> = Vec::with_capacity(2 * n);
        for i in 0..n {
            points.push([x[i], y1[i]]);
        }
        for i in (0..n).rev() {
            points.push([x[i], y2[i]]);
        }
        let face = self
            .cycle_color(self.prop_cycle_index)
            .with_alpha(FILL_ALPHA);
        let patch = Patch::polygon(&points)
            .facecolor(Some(face))
            .edgecolor(None);
        self.add_patch(patch);
    }

    /// Fill the region between the curve `(x, y1)` and the line `y = 0`.
    ///
    /// A convenience wrapper over [`fill_between`](Axes::fill_between) with the
    /// lower curve fixed at zero.
    pub fn fill_between_y0(&mut self, x: &[f64], y1: &[f64]) {
        let zeros = vec![0.0; x.len().min(y1.len())];
        self.fill_between(x, y1, &zeros);
    }

    /// Plot `y` against `x` as a piecewise-constant ("staircase") line.
    ///
    /// Uses matplotlib's default `"pre"` step style: the value of `y[i]`
    /// extends back to `x[i-1]`, so the vertical jump happens at the left edge
    /// of each interval. Returns a mutable reference to the pushed [`Line2D`].
    /// With fewer than two points the line is added unchanged.
    pub fn step(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        let n = x.len().min(y.len());
        if n < 2 {
            return self.plot(&x[..n], &y[..n]);
        }
        let (sx, sy) = step_pre(&x[..n], &y[..n]);
        self.plot(&sx, &sy)
    }

    /// Draw a horizontal line segment at each value in `y`, from `xmin` to
    /// `xmax`.
    ///
    /// Each value yields one black [`Line2D`] segment.
    pub fn hlines(&mut self, y: &[f64], xmin: f64, xmax: f64) {
        for &yc in y {
            self.add_line(Line2D::new(vec![xmin, xmax], vec![yc, yc]));
        }
    }

    /// Draw a vertical line segment at each value in `x`, from `ymin` to `ymax`.
    ///
    /// Each value yields one black [`Line2D`] segment.
    pub fn vlines(&mut self, x: &[f64], ymin: f64, ymax: f64) {
        for &xc in x {
            self.add_line(Line2D::new(vec![xc, xc], vec![ymin, ymax]));
        }
    }

    /// Add a full-width horizontal reference line at `y`.
    ///
    /// The line spans the full resolved x range at draw time (black, `1.0`-point
    /// width).
    pub fn axhline(&mut self, y: f64) {
        self.span_lines.push(SpanLine {
            orientation: SpanOrientation::Horizontal,
            value: y,
            color: Rgba::BLACK,
            linewidth: 1.0,
        });
    }

    /// Add a full-height vertical reference line at `x`.
    ///
    /// The line spans the full resolved y range at draw time (black, `1.0`-point
    /// width).
    pub fn axvline(&mut self, x: f64) {
        self.span_lines.push(SpanLine {
            orientation: SpanOrientation::Vertical,
            value: x,
            color: Rgba::BLACK,
            linewidth: 1.0,
        });
    }

    /// Add a full-width horizontal shaded band between `ymin` and `ymax`.
    ///
    /// The band spans the full resolved x range at draw time with a
    /// semi-transparent gray fill.
    pub fn axhspan(&mut self, ymin: f64, ymax: f64) {
        self.span_rects.push(SpanRect {
            orientation: SpanOrientation::Horizontal,
            lo: ymin,
            hi: ymax,
            facecolor: Rgba::new(0.5, 0.5, 0.5, SPAN_ALPHA),
        });
    }

    /// Add a full-height vertical shaded band between `xmin` and `xmax`.
    ///
    /// The band spans the full resolved y range at draw time with a
    /// semi-transparent gray fill.
    pub fn axvspan(&mut self, xmin: f64, xmax: f64) {
        self.span_rects.push(SpanRect {
            orientation: SpanOrientation::Vertical,
            lo: xmin,
            hi: xmax,
            facecolor: Rgba::new(0.5, 0.5, 0.5, SPAN_ALPHA),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rizzma_artist::Artist;
    use rizzma_core::Bbox;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn bar_adds_one_rectangle_per_height_with_correct_extents() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let h = [1.0, 3.0, 2.0];
        ax.bar(&x, &h);
        assert_eq!(ax.patches.len(), 3);
        for (i, p) in ax.patches.iter().enumerate() {
            let e = p.data_extents().expect("rectangle has extents");
            // x - width/2 .. x + width/2 with the default width of 0.8.
            approx(e.xmin(), x[i] - 0.4);
            approx(e.xmax(), x[i] + 0.4);
            // 0 .. h.
            approx(e.ymin(), 0.0);
            approx(e.ymax(), h[i]);
        }
    }

    #[test]
    fn fill_between_adds_one_polygon_covering_the_data() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y1 = [1.0, 2.0, 1.5];
        let y2 = [0.0, 0.5, -1.0];
        ax.fill_between(&x, &y1, &y2);
        assert_eq!(ax.patches.len(), 1);
        let p = &ax.patches[0];
        // The polygon is closed (first vertex repeated at the end).
        let verts = p.path().vertices();
        assert_eq!(verts.first(), verts.last());
        let e = p.data_extents().expect("polygon has extents");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 2.0);
        approx(e.ymin(), -1.0);
        approx(e.ymax(), 2.0);
    }

    #[test]
    fn step_pre_produces_expected_staircase_vertex_count() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 0.0, 1.0];
        let (sx, sy) = step_pre(&x, &y);
        // "pre" expands n points into 2n - 1 vertices.
        assert_eq!(sx.len(), 2 * x.len() - 1);
        assert_eq!(sy.len(), 2 * x.len() - 1);
    }

    #[test]
    fn step_adds_a_single_line_through_axes() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.step(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
        assert_eq!(ax.lines.len(), 1);
    }

    #[test]
    fn axhline_and_axvline_store_spans() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.axhline(0.5);
        ax.axvline(1.0);
        assert_eq!(ax.span_lines.len(), 2);
    }

    #[test]
    fn axhspan_stores_a_span_rect() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.axhspan(0.2, 0.4);
        assert_eq!(ax.span_rects.len(), 1);
    }
}
