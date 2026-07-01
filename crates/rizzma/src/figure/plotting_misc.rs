//! Tier-2 miscellaneous plotting methods on [`Axes`]:
//! [`eventplot`](Axes::eventplot), [`fill_betweenx`](Axes::fill_betweenx), and
//! [`ecdf`](Axes::ecdf).
//!
//! These mirror matplotlib's `eventplot`, `fill_betweenx`, and `ecdf` helpers.
//! Each translates its arguments into [`Line2D`] / [`Patch`] artists stored on
//! the [`Axes`], so their extents feed autoscaling via
//! [`data_limits`](Axes::data_limits). They live alongside the other plotting
//! helpers but are split out to keep [`plotting`](super::plotting) focused on
//! the Tier-1 line/patch methods.

use crate::artist::{Line2D, Patch};
use crate::core::color::Rgba;

use crate::figure::Axes;

/// Total height (in data units) of an [`eventplot`](Axes::eventplot) tick,
/// matching matplotlib's default `linelength` of `1.0` scaled to leave a small
/// gap between adjacent rows.
const EVENT_LINE_LENGTH: f64 = 0.8;

/// Default fill opacity for [`fill_betweenx`](Axes::fill_betweenx) regions,
/// matching [`fill_between`](Axes::fill_between).
const FILL_BETWEENX_ALPHA: f64 = 0.4;

impl Axes {
    /// Draw a raster / event plot: a short vertical tick at each event position.
    ///
    /// Mirrors matplotlib's `eventplot` in its default vertical orientation. Each
    /// row `i` of `positions` is drawn at integer `y = i`: every event x-value in
    /// that row becomes a short vertical [`Line2D`] tick of total height
    /// `0.8` centered on `y = i`, i.e. spanning
    /// `i - 0.4 ..= i + 0.4`. All ticks are black. Rows stack upward, so the
    /// first row sits at `y = 0`.
    ///
    /// Each tick is a plain [`Line2D`], so the ticks fold into autoscaling via
    /// [`data_limits`](Axes::data_limits).
    ///
    /// Only the vertical orientation is supported today; a horizontal
    /// orientation (ticks stacked along the x-axis) is a follow-up.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let row0 = [0.0, 1.0, 2.0];
    /// let row1 = [0.5, 1.5];
    /// ax.eventplot(&[&row0, &row1]);
    /// let limits = ax.data_limits().expect("eventplot contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 2.0));
    /// // Two rows centered at y = 0 and y = 1, each 0.8 tall.
    /// assert_eq!((limits.ymin(), limits.ymax()), (-0.4, 1.4));
    /// ```
    pub fn eventplot(&mut self, positions: &[&[f64]]) {
        let half = EVENT_LINE_LENGTH / 2.0;
        for (i, row) in positions.iter().enumerate() {
            let y = i as f64;
            for &xp in row.iter() {
                self.add_line(
                    Line2D::new(vec![xp, xp], vec![y - half, y + half]).with_color(Rgba::BLACK),
                );
            }
        }
    }

    /// Fill the region between the curves `(x1, y)` and `(x2, y)`.
    ///
    /// The x/y transpose of [`fill_between`](Axes::fill_between): builds one
    /// closed [`Patch::polygon`] tracing `(x1, y)` forward (bottom-to-top in `y`)
    /// then `(x2, y)` in reverse. The default face is a semi-transparent C0 blue
    /// (alpha `0.4`) with no edge.
    ///
    /// The patch folds into autoscaling via [`data_limits`](Axes::data_limits).
    ///
    /// # Panics
    ///
    /// Panics if `y`, `x1`, and `x2` do not all have the same length.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let y = [0.0, 1.0, 2.0];
    /// let x1 = [0.0, 0.5, 0.0];
    /// let x2 = [1.0, 1.5, 1.0];
    /// ax.fill_betweenx(&y, &x1, &x2);
    /// let limits = ax.data_limits().expect("fill_betweenx contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 1.5));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    /// ```
    pub fn fill_betweenx(&mut self, y: &[f64], x1: &[f64], x2: &[f64]) {
        assert!(
            y.len() == x1.len() && y.len() == x2.len(),
            "fill_betweenx requires equal lengths (got y: {}, x1: {}, x2: {})",
            y.len(),
            x1.len(),
            x2.len()
        );
        let n = y.len();
        if n == 0 {
            return;
        }
        let mut points: Vec<[f64; 2]> = Vec::with_capacity(2 * n);
        for i in 0..n {
            points.push([x1[i], y[i]]);
        }
        for i in (0..n).rev() {
            points.push([x2[i], y[i]]);
        }
        let face = self
            .cycle_color(self.prop_cycle_index)
            .with_alpha(FILL_BETWEENX_ALPHA);
        let patch = Patch::polygon(&points)
            .facecolor(Some(face))
            .edgecolor(None);
        self.add_patch(patch);
    }

    /// Draw the empirical cumulative distribution function (ECDF) of `data`.
    ///
    /// Mirrors matplotlib's `ecdf`. A sorted copy of `data` produces a rising
    /// staircase: the cumulative probability starts at `y = 0` and steps up by
    /// `1/n` at each sorted sample, reaching `y = 1` at the largest sample. The
    /// staircase uses a `"post"` step convention — the value `k/n` holds from
    /// sample `k` up to (but not including) sample `k + 1`, so each vertical
    /// riser sits at a sample's x-value. The line takes the next property-cycle
    /// color (advancing it once).
    ///
    /// The staircase folds into autoscaling via
    /// [`data_limits`](Axes::data_limits); empty `data` draws nothing.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.ecdf(&[3.0, 1.0, 2.0]);
    /// let limits = ax.data_limits().expect("ecdf contributes data limits");
    /// // Steps span the sorted samples and the CDF reaches 1.
    /// assert_eq!((limits.xmin(), limits.xmax()), (1.0, 3.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 1.0));
    /// ```
    pub fn ecdf(&mut self, data: &[f64]) {
        let n = data.len();
        if n == 0 {
            return;
        }
        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("ecdf data must be comparable"));

        // "post" staircase: start at the first sample at y = 0, then at each
        // sample step up by 1/n and hold across to the next sample.
        let mut sx: Vec<f64> = Vec::with_capacity(2 * n);
        let mut sy: Vec<f64> = Vec::with_capacity(2 * n);
        sx.push(sorted[0]);
        sy.push(0.0);
        for (k, &x) in sorted.iter().enumerate() {
            let y = (k + 1) as f64 / n as f64;
            // Riser at this sample, then hold across to the next sample.
            sx.push(x);
            sy.push(y);
            if k + 1 < n {
                sx.push(sorted[k + 1]);
                sy.push(y);
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

    fn line_vertices(line: &Line2D) -> Vec<[f64; 2]> {
        let mut r = VertexRecorder::default();
        line.draw(&mut r, &Affine2D::identity());
        r.paths.into_iter().next().expect("line draws one path")
    }

    #[test]
    fn eventplot_adds_one_tick_per_event() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let row0 = [0.0, 1.0, 2.0];
        let row1 = [0.5, 1.5];
        let row2 = [3.0];
        ax.eventplot(&[&row0, &row1, &row2]);
        // One tick line per event across all rows.
        assert_eq!(ax.lines.len(), row0.len() + row1.len() + row2.len());
    }

    #[test]
    fn eventplot_data_limits_cover_ticks() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let row0 = [0.0, 1.0, 2.0];
        let row1 = [0.5, 1.5];
        ax.eventplot(&[&row0, &row1]);
        let e = ax.data_limits().expect("eventplot contributes data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 2.0);
        // Row 0 centered at y = 0, row 1 at y = 1, each 0.8 tall.
        approx(e.ymin(), -0.4);
        approx(e.ymax(), 1.4);
    }

    #[test]
    fn fill_betweenx_adds_one_polygon_covering_the_data() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let y = [0.0, 1.0, 2.0];
        let x1 = [0.0, 0.5, 0.0];
        let x2 = [1.0, 1.5, 1.0];
        ax.fill_betweenx(&y, &x1, &x2);
        assert_eq!(ax.patches.len(), 1);
        let p = &ax.patches[0];
        // The polygon is closed (first vertex repeated at the end).
        let verts = p.path().vertices();
        assert_eq!(verts.first(), verts.last());
        let e = p.data_extents().expect("polygon has extents");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 1.5);
        approx(e.ymin(), 0.0);
        approx(e.ymax(), 2.0);
    }

    #[test]
    #[should_panic(expected = "fill_betweenx requires equal lengths")]
    fn fill_betweenx_length_mismatch_panics() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.fill_betweenx(&[0.0, 1.0], &[0.0], &[1.0, 2.0]);
    }

    #[test]
    fn ecdf_builds_staircase_reaching_one() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.ecdf(&[3.0, 1.0, 2.0]);
        assert_eq!(ax.lines.len(), 1);
        let pts = line_vertices(&ax.lines[0]);
        // "post" staircase over sorted [1, 2, 3] at increments of 1/3:
        // (1,0)-(1,1/3)-(2,1/3)-(2,2/3)-(3,2/3)-(3,1).
        let third = 1.0 / 3.0;
        let expected = [
            [1.0, 0.0],
            [1.0, third],
            [2.0, third],
            [2.0, 2.0 * third],
            [3.0, 2.0 * third],
            [3.0, 1.0],
        ];
        assert_eq!(pts.len(), expected.len());
        for (p, ex) in pts.iter().zip(expected.iter()) {
            approx(p[0], ex[0]);
            approx(p[1], ex[1]);
        }
        // The CDF tops out at y = 1.
        let e = ax.data_limits().expect("ecdf contributes data limits");
        approx(e.ymin(), 0.0);
        approx(e.ymax(), 1.0);
        approx(e.xmin(), 1.0);
        approx(e.xmax(), 3.0);
    }

    #[test]
    fn ecdf_empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.ecdf(&[]);
        assert!(ax.lines.is_empty());
    }
}
