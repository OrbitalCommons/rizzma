//! Tier-2 step-family plotting methods on [`Axes`]: [`stem`](Axes::stem) and
//! [`stairs`](Axes::stairs).
//!
//! These mirror matplotlib's `stem` and `stairs` helpers. They translate their
//! arguments into [`Line2D`] / [`Collection`] artists stored on the [`Axes`],
//! so their extents feed autoscaling via [`data_limits`](Axes::data_limits) and
//! their default colors honor the property cycle. They live alongside the other
//! plotting helpers but are split out to keep [`plotting`](super::plotting)
//! focused on the Tier-1 line/patch methods.

use crate::artist::{Collection, Line2D};
use crate::core::color::Rgba;

use crate::figure::Axes;

/// Baseline color for a [`stem`](Axes::stem) plot: a neutral dark gray, slightly
/// lighter than pure black so it reads as a reference rather than data.
const STEM_BASELINE_COLOR: Rgba = Rgba::new(0.3, 0.3, 0.3, 1.0);

/// Default y value of the [`stem`](Axes::stem) baseline.
const STEM_BASELINE: f64 = 0.0;

impl Axes {
    /// Draw a stem plot: a vertical stem from the baseline to each `(xi, yi)`,
    /// a marker at each tip, and one horizontal baseline across the x-range.
    ///
    /// Mirrors matplotlib's `stem`: for each pair `(xi, yi)` a vertical
    /// [`Line2D`] rises from the baseline (`y = 0`) to `(xi, yi)`, all stems
    /// share the first property-cycle color, the tips are marked by a single
    /// `'o'`-marker [`Collection`], and one horizontal baseline [`Line2D`] spans
    /// the x data range at `y = 0` in a neutral dark gray. The property cycle is
    /// advanced once (by the shared stem color). Only the common prefix of `x`
    /// and `y` is used; empty input draws nothing.
    ///
    /// All pieces are plain artists, so their extents feed autoscaling.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.stem(&[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0]);
    /// let limits = ax.data_limits().expect("stem contributes data limits");
    /// assert_eq!(limits.ymax(), 3.0);
    /// ```
    // TODO: group the stems, markers, and baseline into a dedicated
    // `StemContainer` artist once one exists, rather than loose artists.
    pub fn stem(&mut self, x: &[f64], y: &[f64]) {
        let n = x.len().min(y.len());
        if n == 0 {
            return;
        }
        let stem_color = self.next_cycle_color();

        // One vertical stem per point, from the baseline up (or down) to the tip.
        for i in 0..n {
            self.add_line(
                Line2D::new(vec![x[i], x[i]], vec![STEM_BASELINE, y[i]]).with_color(stem_color),
            );
        }

        // A single marker collection at the tips.
        let offsets: Vec<[f64; 2]> = (0..n).map(|i| [x[i], y[i]]).collect();
        self.collections
            .push(Collection::scatter(offsets).with_facecolors(vec![stem_color]));

        // One horizontal baseline across the x data range.
        let (mut xmin, mut xmax) = (x[0], x[0]);
        for &xi in &x[..n] {
            xmin = xmin.min(xi);
            xmax = xmax.max(xi);
        }
        self.add_line(
            Line2D::new(vec![xmin, xmax], vec![STEM_BASELINE, STEM_BASELINE])
                .with_color(STEM_BASELINE_COLOR),
        );
    }

    /// Draw a staircase outline from `values` and `edges` (matplotlib `stairs`).
    ///
    /// Traces `values.len()` piecewise-constant levels bounded by `edges`, as a
    /// single unfilled [`Line2D`]: the outline starts at the left edge at the
    /// first level, steps horizontally across each interval and vertically at
    /// each interior edge, and ends at the right edge. The line takes the next
    /// property-cycle color (advancing it once).
    ///
    /// # Length contract
    ///
    /// `edges` must have exactly one more element than `values`
    /// (`edges.len() == values.len() + 1`): the `i`-th level spans
    /// `edges[i]..=edges[i + 1]`. **Panics** if the contract is violated, or if
    /// `values` is empty (and thus `edges` would need a single element, leaving
    /// no interval to draw).
    ///
    /// All pieces fold into autoscaling via [`data_limits`](Axes::data_limits).
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.stairs(&[1.0, 3.0, 2.0], &[0.0, 1.0, 2.0, 3.0]);
    /// let limits = ax.data_limits().expect("stairs contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 3.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (1.0, 3.0));
    /// ```
    pub fn stairs(&mut self, values: &[f64], edges: &[f64]) {
        assert!(
            !values.is_empty(),
            "stairs requires at least one value (got {} values)",
            values.len()
        );
        assert!(
            edges.len() == values.len() + 1,
            "stairs requires edges.len() == values.len() + 1 (got {} edges, {} values)",
            edges.len(),
            values.len()
        );

        // Trace the outline: start at the left edge of the first level, then for
        // each level draw across to its right edge, inserting a vertical riser
        // when the next level changes height.
        let mut sx: Vec<f64> = Vec::with_capacity(2 * values.len());
        let mut sy: Vec<f64> = Vec::with_capacity(2 * values.len());
        sx.push(edges[0]);
        sy.push(values[0]);
        for i in 0..values.len() {
            // Horizontal segment across interval i at its level.
            sx.push(edges[i + 1]);
            sy.push(values[i]);
            // Vertical riser to the next level (if any).
            if i + 1 < values.len() {
                sx.push(edges[i + 1]);
                sy.push(values[i + 1]);
            }
        }

        let color = self.next_cycle_color();
        self.add_line(Line2D::new(sx, sy).with_color(color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artist::Artist;
    use crate::core::{Affine2D, Bbox, Path};
    use crate::render::{GraphicsContext, Renderer};

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    /// A [`Renderer`] that records the vertices of each stroked path, used to
    /// read back a [`Line2D`]'s outline.
    #[derive(Default)]
    struct VertexRecorder {
        paths: Vec<Vec<[f64; 2]>>,
    }

    impl Renderer for VertexRecorder {
        fn draw_path(
            &mut self,
            _gc: &GraphicsContext,
            path: &Path,
            _transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            self.paths.push(path.vertices().to_vec());
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    /// Draw `line` through a [`VertexRecorder`] and return its polyline vertices.
    fn line_vertices(line: &Line2D) -> Vec<[f64; 2]> {
        let mut r = VertexRecorder::default();
        line.draw(&mut r, &Affine2D::identity());
        r.paths.into_iter().next().expect("line draws one path")
    }

    #[test]
    fn stem_adds_stems_baseline_and_markers() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y = [1.0, 3.0, 2.0];
        ax.stem(&x, &y);
        // One vertical stem per point plus a single baseline line.
        assert_eq!(ax.lines.len(), x.len() + 1);
        // A single marker collection at the tips.
        assert_eq!(ax.collections.len(), 1);
    }

    #[test]
    fn stem_data_limits_cover_points_and_baseline() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.stem(&[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0]);
        let e = ax.data_limits().expect("stem contributes data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 2.0);
        // Baseline pulls the minimum down to 0.
        approx(e.ymin(), 0.0);
        approx(e.ymax(), 3.0);
    }

    #[test]
    fn stem_empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.stem(&[], &[]);
        assert!(ax.lines.is_empty());
        assert!(ax.collections.is_empty());
    }

    #[test]
    fn stairs_produces_expected_polyline_vertices() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.stairs(&[1.0, 3.0, 2.0], &[0.0, 1.0, 2.0, 3.0]);
        assert_eq!(ax.lines.len(), 1);
        let pts = line_vertices(&ax.lines[0]);
        // Expected outline: (0,1)-(1,1)-(1,3)-(2,3)-(2,2)-(3,2).
        let expected = [
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 3.0],
            [2.0, 3.0],
            [2.0, 2.0],
            [3.0, 2.0],
        ];
        assert_eq!(pts.len(), expected.len());
        for (p, e) in pts.iter().zip(expected.iter()) {
            approx(p[0], e[0]);
            approx(p[1], e[1]);
        }
    }

    #[test]
    fn stairs_data_limits_cover_extents() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.stairs(&[1.0, 3.0, 2.0], &[0.0, 1.0, 2.0, 3.0]);
        let e = ax.data_limits().expect("stairs contributes data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 3.0);
        approx(e.ymin(), 1.0);
        approx(e.ymax(), 3.0);
    }

    #[test]
    #[should_panic(expected = "edges.len() == values.len() + 1")]
    fn stairs_length_mismatch_panics() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // edges should have 4 elements for 3 values; 3 is a mismatch.
        ax.stairs(&[1.0, 3.0, 2.0], &[0.0, 1.0, 2.0]);
    }
}
