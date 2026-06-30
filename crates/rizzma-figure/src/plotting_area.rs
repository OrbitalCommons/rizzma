//! Tier-2 area-family plotting methods on [`Axes`]:
//! [`stackplot`](Axes::stackplot) and [`broken_barh`](Axes::broken_barh).
//!
//! These mirror matplotlib's `stackplot` and `broken_barh` helpers. Each
//! translates its arguments into filled [`Patch`] artists stored on the
//! [`Axes`], so their extents feed autoscaling via
//! [`data_limits`](Axes::data_limits) and (for `stackplot`) their default fills
//! honor the property cycle. They live alongside the other plotting helpers but
//! are split out to keep [`plotting`](super::plotting) focused on the Tier-1
//! line/patch methods.

use rizzma_artist::Patch;

use crate::Axes;

impl Axes {
    /// Draw a stacked area chart: one filled band per `ys` series.
    ///
    /// Mirrors matplotlib's `stackplot`. The series are cumulatively summed, so
    /// the first series fills the band between `y = 0` and its own values and
    /// each subsequent series fills the band between the previous cumulative
    /// baseline and the new one. Each band is a closed [`Patch::polygon`]
    /// tracing the upper boundary left-to-right then the lower boundary
    /// right-to-left, filled with the next property-cycle color (advancing the
    /// cycle once per series) and drawn with no edge.
    ///
    /// All bands are plain patches, so their extents feed autoscaling.
    ///
    /// # Panics
    ///
    /// Panics if any series in `ys` does not have the same length as `x`.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let x = [0.0, 1.0, 2.0];
    /// let a = [1.0, 2.0, 1.0];
    /// let b = [1.0, 1.0, 2.0];
    /// ax.stackplot(&x, &[&a, &b]);
    /// let limits = ax.data_limits().expect("stackplot contributes data limits");
    /// // The stack reaches the top of the cumulative sum (2 + 1 = 3 at x = 1).
    /// assert_eq!(limits.ymax(), 3.0);
    /// ```
    pub fn stackplot(&mut self, x: &[f64], ys: &[&[f64]]) {
        for (i, series) in ys.iter().enumerate() {
            assert!(
                series.len() == x.len(),
                "stackplot series {i} has length {} but x has length {} (each series must match x)",
                series.len(),
                x.len()
            );
        }
        if x.is_empty() {
            return;
        }

        // Running cumulative baseline, starting at y = 0 under the first series.
        let mut baseline = vec![0.0; x.len()];
        for series in ys {
            // The new cumulative top for this band.
            let top: Vec<f64> = baseline
                .iter()
                .zip(series.iter())
                .map(|(&b, &v)| b + v)
                .collect();

            // Trace the top boundary forward, then the baseline backward, to
            // form a closed band.
            let mut points: Vec<[f64; 2]> = Vec::with_capacity(2 * x.len());
            for (&xi, &t) in x.iter().zip(top.iter()) {
                points.push([xi, t]);
            }
            for (&xi, &b) in x.iter().zip(baseline.iter()).rev() {
                points.push([xi, b]);
            }

            let face = self.next_cycle_color();
            let patch = Patch::polygon(&points)
                .facecolor(Some(face))
                .edgecolor(None);
            self.add_patch(patch);

            baseline = top;
        }
    }

    /// Draw a row of rectangles from `xranges` at the vertical extent `yrange`.
    ///
    /// Mirrors matplotlib's `broken_barh`. For each `(x_start, x_width)` in
    /// `xranges` a [`Patch::rectangle`] is added spanning
    /// `x_start ..= x_start + x_width` horizontally and
    /// `yrange.0 ..= yrange.0 + yrange.1` vertically (i.e. `yrange` is
    /// `(y_min, height)`). All rectangles share the first property-cycle color
    /// as their face and have no edge; the property cycle is not advanced.
    ///
    /// The rectangles are plain patches, so their extents feed autoscaling.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.broken_barh(&[(0.0, 2.0), (5.0, 1.5)], (10.0, 4.0));
    /// let limits = ax.data_limits().expect("broken_barh contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 6.5));
    /// assert_eq!((limits.ymin(), limits.ymax()), (10.0, 14.0));
    /// ```
    pub fn broken_barh(&mut self, xranges: &[(f64, f64)], yrange: (f64, f64)) {
        let face = self.cycle_color(self.prop_cycle_index);
        for &(x_start, x_width) in xranges {
            let patch = Patch::rectangle(x_start, yrange.0, x_width, yrange.1)
                .facecolor(Some(face))
                .edgecolor(None);
            self.add_patch(patch);
        }
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
    fn stackplot_adds_one_band_per_series() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let a = [1.0, 2.0, 1.0];
        let b = [1.0, 1.0, 2.0];
        let c = [0.5, 0.5, 0.5];
        ax.stackplot(&x, &[&a, &b, &c]);
        assert_eq!(ax.patches.len(), 3);
        // Each band is a closed polygon (first vertex repeated at the end).
        for p in &ax.patches {
            let verts = p.path().vertices();
            assert_eq!(verts.first(), verts.last());
        }
    }

    #[test]
    fn stackplot_data_limits_reach_top_of_stack() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let a = [1.0, 2.0, 1.0];
        let b = [1.0, 1.0, 2.0];
        ax.stackplot(&x, &[&a, &b]);
        let e = ax.data_limits().expect("stackplot contributes data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 2.0);
        // The bottom band sits on y = 0.
        approx(e.ymin(), 0.0);
        // Cumulative top: max over i of a[i] + b[i] = 2 + 1 = 3 at x = 1.
        approx(e.ymax(), 3.0);
    }

    #[test]
    #[should_panic(expected = "each series must match x")]
    fn stackplot_length_mismatch_panics() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let short = [1.0, 2.0];
        ax.stackplot(&x, &[&short]);
    }

    #[test]
    fn broken_barh_adds_one_rectangle_per_range() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let ranges = [(0.0, 2.0), (5.0, 1.5), (8.0, 1.0)];
        ax.broken_barh(&ranges, (10.0, 4.0));
        assert_eq!(ax.patches.len(), ranges.len());
        for (p, &(x_start, x_width)) in ax.patches.iter().zip(ranges.iter()) {
            let e = p.data_extents().expect("rectangle has extents");
            approx(e.xmin(), x_start);
            approx(e.xmax(), x_start + x_width);
            approx(e.ymin(), 10.0);
            approx(e.ymax(), 14.0);
        }
    }

    #[test]
    fn broken_barh_data_limits_cover_all_ranges() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.broken_barh(&[(0.0, 2.0), (5.0, 1.5)], (10.0, 4.0));
        let e = ax
            .data_limits()
            .expect("broken_barh contributes data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 6.5);
        approx(e.ymin(), 10.0);
        approx(e.ymax(), 14.0);
    }
}
