//! [`quiver`](Axes::quiver): a 2D vector-field arrow plot.
//!
//! Mirrors matplotlib's `quiver` in its simplest form: at each base point
//! `(x, y)` an arrow is drawn pointing along the vector `(u, v)`. Each arrow is
//! a [`Line2D`] shaft plus a filled [`Patch`] triangle arrowhead, so both fold
//! into autoscaling via [`data_limits`](Axes::data_limits). Arrow lengths are
//! auto-scaled deterministically from the vector magnitudes (no RNG), so a
//! longer vector always yields a longer arrow.

use crate::artist::{Line2D, Patch};

use crate::figure::Axes;

/// Fraction of the data span used as the longest arrow's length.
const ARROW_LENGTH_FRACTION: f64 = 0.15;

/// Arrowhead length as a fraction of an individual arrow's length.
const HEAD_LENGTH_FRACTION: f64 = 0.35;

/// Arrowhead half-width as a fraction of an individual arrow's length.
const HEAD_HALF_WIDTH_FRACTION: f64 = 0.2;

impl Axes {
    /// Draw a 2D vector field: an arrow at each base `(x[i], y[i])` pointing
    /// along `(u[i], v[i])`.
    ///
    /// Mirrors matplotlib's `quiver`. The four slices give the arrow base
    /// positions (`x`, `y`) and the vector components (`u`, `v`); they should
    /// share a length `N`. Ragged inputs use the shortest length, and empty
    /// input draws nothing (never panics).
    ///
    /// Arrow lengths are auto-scaled deterministically so the longest vector
    /// spans `0.15` of the larger base-point span (`max(xmax - xmin,
    /// ymax - ymin)`, guarded to `1.0` when degenerate). With `mag[i] =
    /// hypot(u[i], v[i])` and `max_mag = max(mag)`, the scale is `s =
    /// L / max_mag` (or `0` when every vector is zero), giving each arrow a tip
    /// `T = (x[i] + u[i]*s, y[i] + v[i]*s)`. A longer vector therefore always
    /// produces a longer arrow.
    ///
    /// Each arrow is a [`Line2D`] shaft from the base to `T - hl*dir` (so the
    /// head is not doubled) plus a filled triangular [`Patch`] arrowhead at
    /// `T`, where `dir = (u, v)/mag`, head length `hl = 0.35 * len_i` and
    /// half-width `hw = 0.2 * len_i` for this arrow's length `len_i = mag[i]*s`.
    /// A zero-magnitude vector contributes no shaft or head. All arrows share a
    /// single color (the next property-cycle color), and the data limits expand
    /// to cover every base and every tip. Returns `&mut Self`.
    ///
    /// ![quiver](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_quiver.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A single rightward unit vector at the origin.
    /// ax.quiver(&[0.0], &[0.0], &[1.0], &[0.0]);
    /// let limits = ax.data_limits().expect("quiver contributes data limits");
    /// // Span is degenerate (one point), so it is guarded to 1.0 and the arrow
    /// // reaches x = 0.15.
    /// assert!((limits.xmax() - 0.15).abs() < 1e-9);
    /// ```
    pub fn quiver(&mut self, x: &[f64], y: &[f64], u: &[f64], v: &[f64]) -> &mut Self {
        let n = x.len().min(y.len()).min(u.len()).min(v.len());
        if n == 0 {
            return self;
        }

        // Per-vector magnitudes and the largest, for deterministic auto-scale.
        let mut max_mag = 0.0_f64;
        let mut mag = Vec::with_capacity(n);
        for i in 0..n {
            let m = u[i].hypot(v[i]);
            mag.push(m);
            if m > max_mag {
                max_mag = m;
            }
        }

        // Base-point span; guard a zero/degenerate extent to 1.0.
        let (mut xmin, mut xmax) = (x[0], x[0]);
        let (mut ymin, mut ymax) = (y[0], y[0]);
        for i in 0..n {
            xmin = xmin.min(x[i]);
            xmax = xmax.max(x[i]);
            ymin = ymin.min(y[i]);
            ymax = ymax.max(y[i]);
        }
        let span = (xmax - xmin).max(ymax - ymin);
        let span = if span > 0.0 { span } else { 1.0 };

        let target = ARROW_LENGTH_FRACTION * span;
        let scale = if max_mag > 0.0 { target / max_mag } else { 0.0 };

        let color = self.next_cycle_color();

        for i in 0..n {
            let base = [x[i], y[i]];
            let tip = [x[i] + u[i] * scale, y[i] + v[i] * scale];

            // A zero-magnitude vector has no direction: draw nothing for it.
            if mag[i] == 0.0 || scale == 0.0 {
                continue;
            }

            let len_i = mag[i] * scale;
            let dir = [u[i] / mag[i], v[i] / mag[i]];
            let perp = [-dir[1], dir[0]];
            let hl = HEAD_LENGTH_FRACTION * len_i;
            let hw = HEAD_HALF_WIDTH_FRACTION * len_i;

            // Shaft ends where the arrowhead begins so the head is not doubled.
            let neck = [tip[0] - hl * dir[0], tip[1] - hl * dir[1]];
            self.add_line(
                Line2D::new(vec![base[0], neck[0]], vec![base[1], neck[1]]).with_color(color),
            );

            // Filled triangle arrowhead at the tip, pointing along `dir`.
            let left = [neck[0] + hw * perp[0], neck[1] + hw * perp[1]];
            let right = [neck[0] - hw * perp[0], neck[1] - hw * perp[1]];
            let head = Patch::polygon(&[tip, left, right])
                .facecolor(Some(color))
                .edgecolor(Some(color));
            self.add_patch(head);
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use crate::core::Bbox;

    use crate::figure::Axes;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn quiver_adds_one_shaft_and_head_per_arrow() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 0.0, 0.0];
        let u = [1.0, 0.0, 1.0];
        let v = [0.0, 1.0, 1.0];
        ax.quiver(&x, &y, &u, &v);
        // One shaft (Line2D) and one head (Patch) per non-zero arrow.
        assert_eq!(ax.lines.len(), 3);
        assert_eq!(ax.patches.len(), 3);
    }

    #[test]
    fn quiver_longer_vector_yields_longer_arrow() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Two arrows along +x from different bases so the span is non-zero;
        // the second vector is longer.
        let x = [0.0, 10.0];
        let y = [0.0, 0.0];
        let u = [1.0, 3.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        // Each shaft is a horizontal segment; compare their lengths.
        let len0 = {
            let p = ax.lines[0].points();
            (p[1][0] - p[0][0]).abs()
        };
        let len1 = {
            let p = ax.lines[1].points();
            (p[1][0] - p[0][0]).abs()
        };
        assert!(len1 > len0, "longer vector must give a longer shaft");
    }

    #[test]
    fn quiver_zero_vector_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0];
        let y = [0.0, 0.0];
        let u = [0.0, 1.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        // Only the non-zero vector contributes a shaft and head.
        assert_eq!(ax.lines.len(), 1);
        assert_eq!(ax.patches.len(), 1);
    }

    #[test]
    fn quiver_data_limits_cover_bases_and_tips() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Bases span x in [0, 1]; both vectors point in +x, so the rightmost
        // tip (from the base at x = 1) pushes xmax past the base extent while
        // the leftmost base at x = 0 fixes xmin.
        let x = [0.0, 1.0];
        let y = [0.0, 0.0];
        let u = [1.0, 1.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        let limits = ax.data_limits().expect("quiver contributes data limits");
        // Leftmost base sits at x = 0.
        approx(limits.xmin(), 0.0);
        // span = 1.0 (x in [0, 1]); target = 0.15; max_mag = 1; tip at 1.15.
        approx(limits.xmax(), 1.15);
    }

    #[test]
    fn quiver_empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.quiver(&[], &[], &[], &[]);
        assert!(ax.lines.is_empty());
        assert!(ax.patches.is_empty());
        assert!(ax.data_limits().is_none());
    }
}
