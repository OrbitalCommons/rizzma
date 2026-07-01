//! Statistical Tier-1 plotting methods on [`Axes`]: scatter, histogram, and
//! error bars.
//!
//! These mirror matplotlib's `scatter`, `hist`, and `errorbar` helpers,
//! translating their arguments into [`Collection`], [`Patch`], and [`Line2D`]
//! artists stored on the [`Axes`]. They live alongside the other Tier-1 helpers
//! in [`plotting`](super::plotting) but are split out to keep that module
//! focused on the line/patch convenience methods.

use crate::artist::{Collection, Line2D, Patch};
use crate::core::color::{DEFAULT_COLOR_CYCLE, LinearNorm, Rgba, colormap, to_rgba_array, viridis};

use crate::figure::Axes;

/// Fallback scatter/histogram face color (matplotlib `C0`, a muted blue) used
/// when the property cycle hex cannot be parsed.
const FALLBACK_FACE: Rgba = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);

/// Fraction of the x-range used for the horizontal cap half-width drawn at each
/// end of an [`errorbar`](Axes::errorbar) error line.
const ERRORBAR_CAP_FRAC: f64 = 0.01;

/// Resolve the cycle color at `index` from [`DEFAULT_COLOR_CYCLE`], falling back
/// to a fixed blue if the hex cannot be parsed.
fn cycle_color(index: usize) -> Rgba {
    let hex = DEFAULT_COLOR_CYCLE[index % DEFAULT_COLOR_CYCLE.len()];
    Rgba::from_hex(hex).unwrap_or(FALLBACK_FACE)
}

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
        let face = cycle_color(self.prop_cycle_index);
        self.prop_cycle_index += 1;
        let coll = Collection::scatter(offsets).with_facecolors(vec![face]);
        self.collections.push(coll);
        self.collections
            .last_mut()
            .expect("just pushed a collection")
    }

    /// Scatter `y` against `x` with per-point face colors mapped from `c`.
    ///
    /// Like [`scatter`](Axes::scatter), but each marker's face color is taken by
    /// normalizing `c` linearly over its own `(vmin, vmax)` and sampling the
    /// named colormap `cmap_name` (falling back to `viridis` for an unknown
    /// name). The property cycle is left untouched. Only the common prefix of
    /// `x`, `y`, and `c` is used. Returns a mutable reference for further
    /// styling.
    pub fn scatter_mapped(
        &mut self,
        x: &[f64],
        y: &[f64],
        c: &[f64],
        cmap_name: &str,
    ) -> &mut Collection {
        let n = x.len().min(y.len()).min(c.len());
        let offsets: Vec<[f64; 2]> = (0..n).map(|i| [x[i], y[i]]).collect();
        let (vmin, vmax) = finite_min_max(&c[..n]).unwrap_or((0.0, 1.0));
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = colormap(cmap_name).unwrap_or_else(|| Box::new(viridis()));
        let facecolors = to_rgba_array(&c[..n], &norm, &*cmap);
        let coll = Collection::scatter(offsets).with_facecolors(facecolors);
        self.collections.push(coll);
        self.collections
            .last_mut()
            .expect("just pushed a collection")
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

        let face = cycle_color(self.prop_cycle_index);
        self.prop_cycle_index += 1;
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
        let n = x.len().min(y.len()).min(yerr.len());
        if n == 0 {
            return;
        }
        // Central line through the data points.
        self.add_line(Line2D::new(x[..n].to_vec(), y[..n].to_vec()));

        // Cap half-width as a fraction of the x data range.
        let cap = match finite_min_max(&x[..n]) {
            Some((lo, hi)) if (hi - lo).abs() > f64::EPSILON => (hi - lo) * ERRORBAR_CAP_FRAC,
            _ => ERRORBAR_CAP_FRAC,
        };

        for i in 0..n {
            let (xi, yi, ei) = (x[i], y[i], yerr[i]);
            let (lo, hi) = (yi - ei, yi + ei);
            // Vertical bar.
            self.add_line(Line2D::new(vec![xi, xi], vec![lo, hi]));
            // Horizontal caps at each end.
            self.add_line(Line2D::new(vec![xi - cap, xi + cap], vec![lo, lo]));
            self.add_line(Line2D::new(vec![xi - cap, xi + cap], vec![hi, hi]));
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
}
