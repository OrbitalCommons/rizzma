//! The [`Axes::contour`] line-contour helper.
//!
//! Mirrors matplotlib's `Axes.contour` for the regular-grid case: it runs the
//! standard marching-squares algorithm over a row-major `nrows x ncols` scalar
//! field (corners at integer `x = 0..ncols-1`, `y = 0..nrows-1`), tracing the
//! level sets of a set of evenly-spaced contour levels. Each contour crossing of
//! a grid cell is emitted as a short [`Line2D`] segment colored by its level
//! through a [`LinearNorm`] and the default colormap, and the grid extent
//! folds into [`data_limits`](super::Axes::data_limits) for autoscaling.

use crate::artist::Line2D;
use crate::core::color::{Colormap, LinearNorm, Normalize, default_colormap};

use crate::figure::Axes;

/// Default number of contour levels for [`Axes::contour`].
const DEFAULT_N_LEVELS: usize = 7;

/// The finite min and max of `data`, or `(0.0, 1.0)` when there is no finite
/// value (empty or all-NaN input).
fn data_min_max(data: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min <= max { (min, max) } else { (0.0, 1.0) }
}

/// Linearly interpolate the crossing fraction where the segment from value `a`
/// to value `b` equals `level`.
///
/// Returns the parameter `t in [0, 1]` with `a + t * (b - a) == level`. The
/// caller guarantees `a` and `b` straddle `level`, so `b != a`.
fn crossing(a: f64, b: f64, level: f64) -> f64 {
    (level - a) / (b - a)
}

/// Append the marching-squares line segment(s) for one grid cell to `segments`.
///
/// The cell's four corner values are passed as `corners` in the order
/// `[top_left, top_right, bottom_left, bottom_right]`, where "top" is row `r`
/// and "bottom" is row `r + 1`; `c` is the left column. Corner positions are at
/// integer grid coordinates. Each emitted segment is a pair of crossing points
/// in data space.
///
/// The ambiguous saddle cases (5 and 10) are resolved by comparing the cell's
/// average corner value against the level: this is the standard "midpoint"
/// convention and connects the crossings so the region above the level stays
/// connected through the saddle.
fn cell_segments(corners: [f64; 4], r: f64, c: f64, level: f64, segments: &mut Vec<[[f64; 2]; 2]>) {
    let [tl, tr, bl, br] = corners;
    // Classify each corner as above (1) or below/equal (0) the level. The bits
    // are ordered tl=8, tr=4, br=2, bl=1 (clockwise from the top-left) to match
    // the canonical marching-squares case table.
    let mut case = 0u8;
    if tl > level {
        case |= 8;
    }
    if tr > level {
        case |= 4;
    }
    if br > level {
        case |= 2;
    }
    if bl > level {
        case |= 1;
    }

    // Edge crossing points (when the bounding corners straddle the level):
    //   top:    between tl (x=c)   and tr (x=c+1) at y=r
    //   right:  between tr (y=r)   and br (y=r+1) at x=c+1
    //   bottom: between bl (x=c)   and br (x=c+1) at y=r+1
    //   left:   between tl (y=r)   and bl (y=r+1) at x=c
    let top = || [c + crossing(tl, tr, level), r];
    let right = || [c + 1.0, r + crossing(tr, br, level)];
    let bottom = || [c + crossing(bl, br, level), r + 1.0];
    let left = || [c, r + crossing(tl, bl, level)];

    match case {
        // No crossings.
        0 | 15 => {}
        // One corner differs: a single segment cutting off that corner.
        1 | 14 => segments.push([left(), bottom()]),
        2 | 13 => segments.push([bottom(), right()]),
        4 | 11 => segments.push([top(), right()]),
        8 | 7 => segments.push([left(), top()]),
        // Two adjacent corners: a single segment across the cell.
        3 | 12 => segments.push([left(), right()]),
        6 | 9 => segments.push([top(), bottom()]),
        // Saddles: two diagonal corners differ. Resolve by the cell average.
        5 | 10 => {
            let avg = (tl + tr + bl + br) / 4.0;
            if (case == 5) == (avg > level) {
                // Connect top-to-right and bottom-to-left.
                segments.push([top(), right()]);
                segments.push([left(), bottom()]);
            } else {
                // Connect top-to-left and bottom-to-right.
                segments.push([left(), top()]);
                segments.push([bottom(), right()]);
            }
        }
        _ => unreachable!("marching-squares case is a 4-bit value"),
    }
}

/// Collect the marching-squares contour segments of `z` at a single `level`.
///
/// `z` is row-major `nrows * ncols`; the returned segments are pairs of points
/// in grid/data space. Grids smaller than `2 x 2` produce no segments.
fn level_segments(z: &[f64], nrows: usize, ncols: usize, level: f64) -> Vec<[[f64; 2]; 2]> {
    let mut segments = Vec::new();
    if nrows < 2 || ncols < 2 {
        return segments;
    }
    for r in 0..nrows - 1 {
        for c in 0..ncols - 1 {
            let tl = z[r * ncols + c];
            let tr = z[r * ncols + c + 1];
            let bl = z[(r + 1) * ncols + c];
            let br = z[(r + 1) * ncols + c + 1];
            cell_segments([tl, tr, bl, br], r as f64, c as f64, level, &mut segments);
        }
    }
    segments
}

impl Axes {
    /// Draw line contours of row-major scalar `z` (`nrows x ncols`) at the
    /// default seven levels.
    ///
    /// Equivalent to [`contour_levels`](Axes::contour_levels) with
    /// `n_levels = 7`. The levels are evenly spaced strictly between the finite
    /// data minimum and maximum.
    ///
    /// # Panics
    ///
    /// Panics if `z.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A 3x3 ramp increasing along x; contours are vertical lines.
    /// let z = [0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    /// ax.contour(&z, 3, 3);
    /// let limits = ax.data_limits().expect("contour provides data limits");
    /// // The grid spans x in [0, 2], y in [0, 2].
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 2.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    /// ```
    pub fn contour(&mut self, z: &[f64], nrows: usize, ncols: usize) {
        self.contour_levels(z, nrows, ncols, DEFAULT_N_LEVELS);
    }

    /// Draw `n_levels` line contours of row-major scalar `z` (`nrows x ncols`).
    ///
    /// The grid corners sit at integer coordinates (`x = 0..ncols-1`,
    /// `y = 0..nrows-1`). The `n_levels` contour levels are evenly spaced
    /// strictly between the finite data minimum `zmin` and maximum `zmax`
    /// (level `k` is `zmin + (k + 1) / (n_levels + 1) * (zmax - zmin)`). For
    /// each level the standard 16-case marching-squares algorithm runs over
    /// every `2 x 2` cell: corners are classified above/below the level, edge
    /// crossings are found by linear interpolation, and the crossings are
    /// connected into one or two segments. The two ambiguous saddle cases are
    /// resolved by comparing the cell's mean corner value against the level
    /// (the midpoint convention). Each segment becomes a two-point [`Line2D`]
    /// colored by its level via a [`LinearNorm`] over `[zmin, zmax]` and the
    /// default colormap, and the grid extent folds into
    /// [`data_limits`](Axes::data_limits).
    ///
    /// A grid smaller than `2 x 2`, a flat field (`zmin == zmax`), or
    /// `n_levels == 0` draws no contour lines but still records the grid extent.
    ///
    // TODO: LineCollection batching — emitting one Line2D per segment is
    // correctness-first; a contour LineCollection would cut artist overhead for
    // large grids.
    ///
    /// # Panics
    ///
    /// Panics if `z.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let z = [0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    /// ax.contour_levels(&z, 3, 3, 3);
    /// assert!(ax.data_limits().is_some());
    /// ```
    pub fn contour_levels(&mut self, z: &[f64], nrows: usize, ncols: usize, n_levels: usize) {
        assert_eq!(
            z.len(),
            nrows * ncols,
            "contour: z length {} must equal nrows * ncols = {}",
            z.len(),
            nrows * ncols
        );

        // Record the grid extent so autoscaling fits it even when no contour
        // crosses (flat field, sub-2x2 grid, or zero levels).
        if ncols >= 1 && nrows >= 1 {
            self.include_data_bbox(0.0, 0.0, (ncols - 1) as f64, (nrows - 1) as f64);
            // Contours are flush with their grid, like matplotlib's.
            self.sticky_x.push(0.0);
            self.sticky_x.push((ncols - 1) as f64);
            self.sticky_y.push(0.0);
            self.sticky_y.push((nrows - 1) as f64);
        }

        let (zmin, zmax) = data_min_max(z);
        if zmax <= zmin || n_levels == 0 {
            return;
        }

        let norm = LinearNorm::new(zmin, zmax);
        let cmap = default_colormap();
        let span = zmax - zmin;

        for k in 0..n_levels {
            let level = zmin + (k + 1) as f64 / (n_levels + 1) as f64 * span;
            let color = cmap.sample(norm.normalize(level));
            for [p0, p1] in level_segments(z, nrows, ncols, level) {
                self.add_line(
                    Line2D::new(vec![p0[0], p1[0]], vec![p0[1], p1[1]]).with_color(color),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bbox;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn ramp_field_produces_vertical_contours_at_expected_x() {
        // z = x: a 3-column, 3-row ramp. One level at x where z == level.
        let z = [0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
        // Level 1.0 sits at x = 1.0; level 1.5 sits at x = 1.5.
        for level in [1.0, 1.5] {
            let segs = level_segments(&z, 3, 3, level);
            assert!(!segs.is_empty(), "level {level} should cross the field");
            for seg in &segs {
                // Each crossing point lands on the expected vertical line x=level.
                approx(seg[0][0], level);
                approx(seg[1][0], level);
            }
        }
    }

    #[test]
    fn single_corner_above_yields_one_segment() {
        // Only the top-left corner exceeds the level → exactly one segment.
        let z = [2.0, 0.0, 0.0, 0.0];
        let segs = level_segments(&z, 2, 2, 1.0);
        assert_eq!(segs.len(), 1);
        // It cuts the top-left corner: a left-edge point to a top-edge point.
        let [a, b] = segs[0];
        // Both crossings are at the midpoint of their edges (linear interp of 2→0).
        approx(a[0], 0.0);
        approx(a[1], 0.5);
        approx(b[0], 0.5);
        approx(b[1], 0.0);
    }

    #[test]
    fn flat_field_draws_no_lines_but_records_limits() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.contour(&[3.0, 3.0, 3.0, 3.0], 2, 2);
        assert!(ax.lines.is_empty());
        let e = ax.data_limits().expect("grid extent is recorded");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 1.0);
        approx(e.ymin(), 0.0);
        approx(e.ymax(), 1.0);
    }

    #[test]
    fn data_limits_cover_the_grid() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // 4-row, 5-column ramp; the grid spans x in [0, 4], y in [0, 3].
        let (nr, nc) = (4usize, 5usize);
        let z: Vec<f64> = (0..nr * nc).map(|i| (i % nc) as f64).collect();
        ax.contour(&z, nr, nc);
        let e = ax.data_limits().expect("contour records data limits");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), (nc - 1) as f64);
        approx(e.ymin(), 0.0);
        approx(e.ymax(), (nr - 1) as f64);
    }

    #[test]
    fn default_contour_uses_seven_levels() {
        // A ramp where every level crosses every row band exactly once.
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // 2 rows, 9 columns: z = x in 0..8. The 7 default levels are
        // 1,2,...,7, each crossing the single row band once.
        let z: Vec<f64> = (0..2 * 9).map(|i| (i % 9) as f64).collect();
        ax.contour(&z, 2, 9);
        // 7 levels x 1 crossing each = 7 line segments.
        assert_eq!(ax.lines.len(), 7);
    }
}
