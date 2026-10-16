//! Statistical Tier-1 plotting methods on [`Axes`]: scatter, histogram, and
//! error bars.
//!
//! These mirror matplotlib's `scatter`, `hist`, and `errorbar` helpers,
//! translating their arguments into [`Collection`], [`Patch`], and [`Line2D`]
//! artists stored on the [`Axes`]. They live alongside the other Tier-1 helpers
//! in [`plotting`](super::plotting) but are split out to keep that module
//! focused on the line/patch convenience methods.

use crate::artist::{Collection, Line2D, Patch};
use crate::core::Path;
use crate::core::color::{ColorMapping, Rgba};

use crate::figure::Axes;

/// Fraction of the x-range used for the horizontal cap half-width drawn at each
/// end of an [`errorbar`](Axes::errorbar) error line.
const ERRORBAR_CAP_FRAC: f64 = 0.01;

/// The finite min/max of `data`, or `None` when there is no finite datum.
fn finite_min_max(data: &[f64]) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    for &v in data {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
            any = true;
        }
    }
    if any { Some((lo, hi)) } else { None }
}

impl Axes {
    /// Scatter `y` against `x` as a [`Collection`] of markers.
    ///
    /// Builds a default scatter collection (filled `'o'` marker, size `~6`, the
    /// next property-cycle face color, no edge) at the `(x, y)` offsets, stores
    /// it on the axes, and returns a mutable reference for further styling. The
    /// collection participates in autoscaling via [`data_limits`](Axes::data_limits).
    /// Only the common prefix of `x` and `y` is used.
    pub fn scatter(&mut self, x: &[f64], y: &[f64]) -> &mut Collection {
        let n = x.len().min(y.len());
        let offsets: Vec<[f64; 2]> = (0..n).map(|i| [x[i], y[i]]).collect();
        let face = self.next_cycle_color();
        let coll = Collection::scatter(offsets).with_facecolors(vec![face]);
        self.push_collection(coll)
    }

    /// Scatter `y` against `x` with per-point face colors mapped from `c`.
    ///
    /// Like [`scatter`](Axes::scatter), but each marker's face color is taken
    /// from `c` through `mapping`: a colormap name (`"viridis"`), which maps
    /// `c` linearly over its own finite range, or a [`ColorMapping`] with
    /// `vmin`/`vmax`, a norm, and bad/over/under colors. Points whose `c` is
    /// NaN take the bad color (transparent by default), as matplotlib leaves
    /// them unplotted. The property cycle is left untouched. Only the common
    /// prefix of `x`, `y`, and `c` is used. Returns a mutable reference for
    /// further styling.
    ///
    /// ```
    /// use rizzma::Figure;
    /// use rizzma::core::color::{ColorMapping, TwoSlopeNorm};
    /// let (x, y) = ([0.0, 1.0, 2.0, 3.0], [1.0, 3.0, 2.0, 4.0]);
    /// let residual = [-0.4, 0.1, 2.5, f64::NAN];
    /// let mut fig = Figure::new(4.0, 3.0);
    /// let ax = fig.add_subplot(1, 1, 1);
    /// ax.scatter_mapped(&x, &y, &residual, "viridis");
    /// ax.scatter_mapped(
    ///     &x,
    ///     &y,
    ///     &residual,
    ///     ColorMapping::new("RdBu").norm(TwoSlopeNorm::new(-0.5, 0.0, 3.0)),
    /// );
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn scatter_mapped(
        &mut self,
        x: &[f64],
        y: &[f64],
        c: &[f64],
        mapping: impl Into<ColorMapping>,
    ) -> &mut Collection {
        let n = x.len().min(y.len()).min(c.len());
        let offsets: Vec<[f64; 2]> = (0..n).map(|i| [x[i], y[i]]).collect();
        let scale = mapping.into().resolve(&c[..n]);
        let coll = Collection::scatter(offsets)
            .with_facecolors(scale.colors(&c[..n]))
            .with_color_scale(scale);
        self.push_collection(coll)
    }

    /// Draw a histogram of `data` over `bins` equal-width bins.
    ///
    /// Bins span `[min, max]` of the finite data; each datum is counted into its
    /// bin (the maximum value falling in the last bin). One [`Patch::rectangle`]
    /// is drawn per bin spanning the bin width at height equal to its count, with
    /// a black edge and the next property-cycle face color. Returns
    /// `(counts, edges)` where `counts` has length `bins` and `edges` has length
    /// `bins + 1`. Empty data (or `bins == 0`) yields empty vectors and draws
    /// nothing.
    ///
    /// Counts only; density normalization is a later refinement.
    pub fn hist(&mut self, data: &[f64], bins: usize) -> (Vec<f64>, Vec<f64>) {
        // Count bars rise from zero: pin the baseline.
        self.sticky_y.push(0.0);
        if bins == 0 {
            return (Vec::new(), Vec::new());
        }
        let Some((min, max)) = finite_min_max(data) else {
            return (Vec::new(), Vec::new());
        };
        // Guard a zero-width data range so bins have positive width.
        let (min, max) = if (max - min).abs() > f64::EPSILON {
            (min, max)
        } else {
            (min - 0.5, max + 0.5)
        };
        let width = (max - min) / bins as f64;

        let edges: Vec<f64> = (0..=bins).map(|i| min + i as f64 * width).collect();
        let mut counts = vec![0.0f64; bins];
        for &v in data {
            if !v.is_finite() || v < min || v > max {
                continue;
            }
            // Place v into its bin; the max value lands in the last bin.
            let idx = (((v - min) / width) as usize).min(bins - 1);
            counts[idx] += 1.0;
        }

        let face = self.next_cycle_color();
        for (i, &count) in counts.iter().enumerate() {
            let patch = Patch::rectangle(edges[i], 0.0, width, count)
                .facecolor(Some(face))
                .edgecolor(Some(Rgba::BLACK));
            self.add_patch(patch);
        }

        (counts, edges)
    }

    /// Plot `y` against `x` with symmetric vertical error bars `yerr`.
    ///
    /// Adds the central [`Line2D`] through `(x, y)` plus, per point, a vertical
    /// error [`Line2D`] from `(xi, yi - ei)` to `(xi, yi + ei)` with short
    /// horizontal caps (a few percent of the x-range wide) at each end. All
    /// pieces are added as plain lines on the axes, so their extents feed
    /// autoscaling. Only the common prefix of `x`, `y`, and `yerr` is used.
    // TODO: group these into a dedicated `ErrorbarContainer` artist once one
    // exists, rather than loose `Line2D`s.
    pub fn errorbar(&mut self, x: &[f64], y: &[f64], yerr: &[f64]) {
        self.errorbar_with(x, y, yerr, &ErrorbarOptions::default());
    }

    /// [`errorbar`](Axes::errorbar) with upper and lower limits and horizontal
    /// errors — matplotlib's `uplims`, `lolims`, `xerr`, `xuplims` and
    /// `xlolims`.
    ///
    /// A point flagged as an upper limit keeps only the lower half of its
    /// bar, from `y - err` up to `y`, and gets a caret pointing down from
    /// `y - err`; a lower limit keeps the upper half and points up from
    /// `y + err`. Its cap moves to the value itself, as matplotlib draws one
    /// when caps are on. The carets are matplotlib's `caretdownbase` /
    /// `caretupbase` markers, 6 points wide, unfilled; the x variants point
    /// left from `x - xerr` and right from `x + xerr`. Unflagged points draw
    /// exactly as [`errorbar`](Axes::errorbar) draws them.
    ///
    /// ![errorbar limits](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_errorbar_limits.png)
    ///
    /// ```
    /// use rizzma::Figure;
    /// use rizzma::figure::ErrorbarOptions;
    /// let (t, flux, err) = ([1.0, 2.0, 3.0, 4.0], [3.2, 2.0, 2.7, 1.5], [0.3, 0.6, 0.3, 0.6]);
    /// let mut fig = Figure::new(4.0, 3.0);
    /// fig.add_subplot(1, 1, 1).errorbar_with(
    ///     &t,
    ///     &flux,
    ///     &err,
    ///     &ErrorbarOptions::new().uplims(&[false, true, false, true]),
    /// );
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn errorbar_with(&mut self, x: &[f64], y: &[f64], yerr: &[f64], options: &ErrorbarOptions) {
        let n = x.len().min(y.len()).min(yerr.len());
        if n == 0 {
            return;
        }
        // Central line through the data points.
        self.add_line(Line2D::new(x[..n].to_vec(), y[..n].to_vec()));

        // Cap half-widths as a fraction of the data range across each bar.
        let cap_for = |across: &[f64]| match finite_min_max(across) {
            Some((lo, hi)) if (hi - lo).abs() > f64::EPSILON => (hi - lo) * ERRORBAR_CAP_FRAC,
            _ => ERRORBAR_CAP_FRAC,
        };
        let cap = cap_for(&x[..n]);
        let flag = |mask: &[bool], i: usize| mask.get(i).copied().unwrap_or(false);

        let mut carets = Carets::default();
        for i in 0..n {
            let (xi, yi, ei) = (x[i], y[i], yerr[i]);
            let (up, lo) = (flag(&options.uplims, i), flag(&options.lolims, i));
            let low = if lo { yi } else { yi - ei };
            let high = if up { yi } else { yi + ei };
            // Vertical bar.
            self.add_line(Line2D::new(vec![xi, xi], vec![low, high]));
            if up || lo {
                // The cap marks the value; the caret marks the open end.
                self.add_line(Line2D::new(vec![xi - cap, xi + cap], vec![yi, yi]));
                if lo {
                    carets.up.push([xi, high]);
                }
                if up {
                    carets.down.push([xi, low]);
                }
            } else {
                // Horizontal caps at each end.
                self.add_line(Line2D::new(vec![xi - cap, xi + cap], vec![low, low]));
                self.add_line(Line2D::new(vec![xi - cap, xi + cap], vec![high, high]));
            }
        }

        let m = n.min(options.xerr.len());
        let ycap = cap_for(&y[..n]);
        for i in 0..m {
            let (xi, yi, ei) = (x[i], y[i], options.xerr[i]);
            let (up, lo) = (flag(&options.xuplims, i), flag(&options.xlolims, i));
            let left = if lo { xi } else { xi - ei };
            let right = if up { xi } else { xi + ei };
            // Horizontal bar.
            self.add_line(Line2D::new(vec![left, right], vec![yi, yi]));
            if up || lo {
                self.add_line(Line2D::new(vec![xi, xi], vec![yi - ycap, yi + ycap]));
                if lo {
                    carets.right.push([right, yi]);
                }
                if up {
                    carets.left.push([left, yi]);
                }
            } else {
                // Vertical caps at each end.
                self.add_line(Line2D::new(vec![left, left], vec![yi - ycap, yi + ycap]));
                self.add_line(Line2D::new(vec![right, right], vec![yi - ycap, yi + ycap]));
            }
        }
        carets.add_to(self);
    }
}

/// Upper and lower limits and horizontal errors for
/// [`Axes::errorbar_with`] — matplotlib's `uplims`, `lolims`, `xerr`,
/// `xuplims` and `xlolims`. Masks shorter than the series leave the rest of
/// its points unflagged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErrorbarOptions {
    uplims: Vec<bool>,
    lolims: Vec<bool>,
    xerr: Vec<f64>,
    xuplims: Vec<bool>,
    xlolims: Vec<bool>,
}

impl ErrorbarOptions {
    /// No limits and no horizontal errors.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Flag the points whose value is an upper limit.
    #[must_use]
    pub fn uplims(mut self, mask: &[bool]) -> Self {
        self.uplims = mask.to_vec();
        self
    }

    /// Flag the points whose value is a lower limit.
    #[must_use]
    pub fn lolims(mut self, mask: &[bool]) -> Self {
        self.lolims = mask.to_vec();
        self
    }

    /// Add symmetric horizontal error bars.
    #[must_use]
    pub fn xerr(mut self, xerr: &[f64]) -> Self {
        self.xerr = xerr.to_vec();
        self
    }

    /// Flag the points whose `x` is an upper limit (caret pointing left).
    #[must_use]
    pub fn xuplims(mut self, mask: &[bool]) -> Self {
        self.xuplims = mask.to_vec();
        self
    }

    /// Flag the points whose `x` is a lower limit (caret pointing right).
    #[must_use]
    pub fn xlolims(mut self, mask: &[bool]) -> Self {
        self.xlolims = mask.to_vec();
        self
    }
}

/// matplotlib's default marker size, which its limit carets take when caps
/// have no size of their own.
const CARET_SIZE: f64 = 6.0;
/// matplotlib's default marker edge width, in points.
const CARET_WIDTH: f64 = 1.0;

/// Limit carets by direction, at the bar ends they mark.
#[derive(Default)]
struct Carets {
    up: Vec<[f64; 2]>,
    down: Vec<[f64; 2]>,
    left: Vec<[f64; 2]>,
    right: Vec<[f64; 2]>,
}

impl Carets {
    /// Add one unfilled marker collection per direction in use.
    fn add_to(self, ax: &mut Axes) {
        // matplotlib's `_caret_path_base` at its 0.5 scale: the base on the
        // point, the tip 0.75 marker sizes away.
        let base = [[-0.5, 0.0], [0.0, -0.75], [0.5, 0.0]];
        let turned = |f: fn([f64; 2]) -> [f64; 2]| Path::from_polyline(&base.map(f));
        let sets = [
            (self.down, turned(|p| p)),
            (self.up, turned(|[x, y]| [x, -y])),
            (self.left, turned(|[x, y]| [y, x])),
            (self.right, turned(|[x, y]| [-y, x])),
        ];
        for (offsets, marker) in sets {
            if offsets.is_empty() {
                continue;
            }
            ax.push_collection(
                Collection::scatter(offsets)
                    .with_marker(marker)
                    .with_sizes(vec![CARET_SIZE])
                    .with_facecolors(Vec::new())
                    .with_edgecolors(vec![Rgba::BLACK])
                    .linewidth(CARET_WIDTH)
                    .with_zorder(2.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artist::Artist;
    use crate::core::Bbox;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn scatter_adds_a_collection_covering_the_data() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [-1.0, 4.0, 2.0, 0.0];
        ax.scatter(&x, &y);
        assert_eq!(ax.collections.len(), 1);
        let e = ax.collections[0]
            .data_extents()
            .expect("collection has extents");
        approx(e.xmin(), 0.0);
        approx(e.xmax(), 3.0);
        approx(e.ymin(), -1.0);
        approx(e.ymax(), 4.0);
    }

    #[test]
    fn data_limits_include_scatter_collection() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.scatter(&[5.0, 7.0], &[10.0, 20.0]);
        let e = ax.data_limits().expect("scatter contributes data limits");
        approx(e.xmin(), 5.0);
        approx(e.xmax(), 7.0);
        approx(e.ymin(), 10.0);
        approx(e.ymax(), 20.0);
    }

    #[test]
    fn set_collection_offsets_replaces_data_and_checks_range() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.scatter(&[0.0, 1.0], &[0.0, 1.0]);
        ax.set_collection_offsets(0, &[5.0, 7.0, 9.0], &[10.0, 20.0])
            .expect("collection 0 exists");
        // Common prefix of x/y is used; extents re-derive from the new data.
        let e = ax.collections[0]
            .data_extents()
            .expect("collection has extents");
        approx(e.xmin(), 5.0);
        approx(e.xmax(), 7.0);
        approx(e.ymin(), 10.0);
        approx(e.ymax(), 20.0);

        let err = ax
            .set_collection_offsets(3, &[0.0], &[0.0])
            .expect_err("index 3 is out of range");
        assert!(err.contains("out of range"), "unexpected error: {err}");
    }

    #[test]
    fn scatter_mapped_sets_per_point_face_colors() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 1.0, 2.0];
        let c = [0.0, 0.5, 1.0];
        ax.scatter_mapped(&x, &y, &c, "viridis");
        assert_eq!(ax.collections.len(), 1);
        // The property cycle is untouched by the mapped variant.
        assert_eq!(ax.prop_cycle_index, 0);
    }

    #[test]
    fn hist_returns_expected_counts_and_edges() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Four bins over [0, 4]: edges 0,1,2,3,4.
        let data = [0.0, 0.5, 1.5, 1.5, 2.5, 4.0];
        let (counts, edges) = ax.hist(&data, 4);
        assert_eq!(edges.len(), 5);
        // bin0: [0,1) -> 0.0,0.5 => 2; bin1: [1,2) -> 1.5,1.5 => 2;
        // bin2: [2,3) -> 2.5 => 1; bin3: [3,4] -> 4.0 => 1.
        assert_eq!(counts, vec![2.0, 2.0, 1.0, 1.0]);
        assert_eq!(ax.patches.len(), 4);
    }

    #[test]
    fn hist_edges_are_evenly_spaced() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let data = [0.0, 10.0];
        let (_counts, edges) = ax.hist(&data, 5);
        for (i, &edge) in edges.iter().enumerate() {
            approx(edge, i as f64 * 2.0);
        }
    }

    #[test]
    fn hist_empty_data_is_empty() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (counts, edges) = ax.hist(&[], 4);
        assert!(counts.is_empty());
        assert!(edges.is_empty());
        assert!(ax.patches.is_empty());
    }

    #[test]
    fn errorbar_adds_central_line_plus_per_point_lines() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y = [1.0, 2.0, 3.0];
        let yerr = [0.5, 0.5, 0.5];
        ax.errorbar(&x, &y, &yerr);
        // 1 central line + 3 per-point (1 vertical + 2 caps) = 1 + 9 = 10.
        assert_eq!(ax.lines.len(), 1 + 3 * 3);
    }

    #[test]
    fn errorbar_extents_include_error_range() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.errorbar(&[0.0, 1.0], &[5.0, 5.0], &[2.0, 2.0]);
        let e = ax.data_limits().expect("errorbar contributes data limits");
        approx(e.ymin(), 3.0);
        approx(e.ymax(), 7.0);
    }

    #[test]
    fn limits_keep_half_the_bar_and_mark_the_open_end() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.errorbar_with(
            &[0.0, 1.0, 2.0],
            &[5.0, 5.0, 5.0],
            &[2.0, 2.0, 2.0],
            &ErrorbarOptions::new()
                .uplims(&[false, true])
                .lolims(&[false, false, true])
                .xerr(&[0.5, 0.5])
                .xuplims(&[true]),
        );
        let bar = |i: usize| ax.lines[i].points();
        // Lines: the central line, then (bar, cap, cap) for point 0 and
        // (bar, cap) for each limit point, then the x bars.
        assert_eq!(bar(1), [[0.0, 3.0], [0.0, 7.0]]);
        assert_eq!(bar(4), [[1.0, 3.0], [1.0, 5.0]]); // upper limit: below y
        assert_eq!(bar(5)[0][1], 5.0); // its cap sits on the value
        assert_eq!(bar(6), [[2.0, 5.0], [2.0, 7.0]]); // lower limit: above y
        assert_eq!(bar(8), [[-0.5, 5.0], [0.0, 5.0]]); // x upper limit: left of x
        // One caret collection per direction used: down, up, left.
        let carets: Vec<Vec<[f64; 2]>> = ax
            .collections
            .iter()
            .map(|c| c.offsets().to_vec())
            .collect();
        assert_eq!(
            carets,
            [vec![[1.0, 3.0]], vec![[2.0, 7.0]], vec![[-0.5, 5.0]]]
        );
    }

    #[test]
    fn errorbar_without_limits_adds_no_carets() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.errorbar(&[0.0, 1.0], &[5.0, 5.0], &[2.0, 2.0]);
        assert!(ax.collections.is_empty());
        assert_eq!(ax.lines.len(), 1 + 2 * 3);
    }
}
