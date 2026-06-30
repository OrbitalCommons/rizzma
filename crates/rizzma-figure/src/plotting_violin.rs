//! Tier-2 statistical violin plotting on [`Axes`]:
//! [`violinplot`](Axes::violinplot).
//!
//! Mirrors matplotlib's `violinplot`: each dataset is summarized by a Gaussian
//! kernel-density estimate, mirrored left and right about its position to form a
//! filled "violin" body, with a thin center line marking the data extent. The
//! pieces are translated into [`Patch`] and [`Line2D`] artists stored on the
//! [`Axes`], so their extents feed autoscaling via
//! [`data_limits`](Axes::data_limits). They live alongside the other plotting
//! helpers but are split out to keep [`plotting`](super::plotting) focused on the
//! Tier-1 line/patch methods.

use rizzma_artist::{Line2D, Patch};
use rizzma_core::color::Rgba;

use crate::Axes;

/// Number of grid points the density is evaluated on across each dataset's
/// data range.
const GRID_POINTS: usize = 128;

/// Half-width (in data units) the peak density of each violin is scaled to, so
/// the full body spans roughly `0.8` data units like matplotlib's default.
const HALF_WIDTH: f64 = 0.4;

/// Horizontal margin (in data units) reserved on each side of a violin so
/// autoscale leaves room for the body.
const X_MARGIN: f64 = 0.5;

/// Fill opacity for the violin body (matplotlib draws a translucent body).
const BODY_ALPHA: f64 = 0.3;

/// Sample mean and (population) standard deviation of `data`, returning `None`
/// when fewer than two values are present. `data` must already be finite.
fn std_dev(data: &[f64]) -> Option<f64> {
    let n = data.len();
    if n < 2 {
        return None;
    }
    let mean = data.iter().sum::<f64>() / n as f64;
    let var = data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    Some(var.sqrt())
}

/// Gaussian kernel-density estimate of `data` at `p` using bandwidth `bw`.
///
/// `density(p) = (1 / (n * bw * sqrt(2π))) * Σ exp(-0.5 * ((p - x_i)/bw)^2)`.
fn kde(data: &[f64], bw: f64, p: f64) -> f64 {
    let norm = 1.0 / (data.len() as f64 * bw * (2.0 * std::f64::consts::PI).sqrt());
    let sum: f64 = data
        .iter()
        .map(|&x| {
            let z = (p - x) / bw;
            (-0.5 * z * z).exp()
        })
        .sum();
    norm * sum
}

impl Axes {
    /// Draw a vertical violin plot, one violin per dataset.
    ///
    /// Mirrors matplotlib's `violinplot`: dataset `i` is drawn at `positions[i]`
    /// (defaulting to integer position `i + 1`). For each dataset a Gaussian
    /// kernel-density estimate is computed with a bandwidth from Scott's rule
    /// (`bw = std_dev * n^(-1/5)`) and evaluated on an evenly spaced grid across
    /// `[min(data), max(data)]`. The density is normalized so its peak maps to a
    /// half-width of `0.4` data units, then mirrored about the position to form a
    /// closed [`Patch`] violin body (translucent, property-cycle face, solid
    /// edge). A thin center [`Line2D`] from `min(data)` to `max(data)` marks the
    /// extent, like matplotlib's default `showextrema=True`. All artists fold
    /// into [`data_limits`](Axes::data_limits) so autoscale fits the violins.
    /// Datasets with fewer than two finite values, or zero variance, draw only a
    /// degenerate center line (no body) and never produce NaN geometry.
    ///
    /// ![violinplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_violinplot.png)
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let a = [1.0, 2.0, 3.0, 4.0, 5.0];
    /// let b = [2.0, 3.0, 4.0, 5.0, 6.0];
    /// ax.violinplot(&[&a, &b], None);
    /// let limits = ax.data_limits().expect("violinplot contributes data limits");
    /// // Two violins centered at x = 1 and x = 2 with a 0.5 margin.
    /// assert_eq!(limits.xmin(), 0.5);
    /// assert_eq!(limits.xmax(), 2.5);
    /// ```
    pub fn violinplot(&mut self, data: &[&[f64]], positions: Option<&[f64]>) -> &mut Self {
        for (i, dataset) in data.iter().enumerate() {
            let pos = positions
                .and_then(|p| p.get(i).copied())
                .unwrap_or((i + 1) as f64);

            let finite: Vec<f64> = dataset.iter().copied().filter(|v| v.is_finite()).collect();
            if finite.is_empty() {
                continue;
            }
            let dmin = finite.iter().copied().fold(f64::INFINITY, f64::min);
            let dmax = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);

            // Center line spanning the data extent (matplotlib's `showextrema`).
            self.add_line(Line2D::new(vec![pos, pos], vec![dmin, dmax]).with_color(Rgba::BLACK));
            // Reserve the horizontal margin even for degenerate datasets so
            // positions read consistently under autoscale.
            self.include_data_bbox(pos - X_MARGIN, dmin, pos + X_MARGIN, dmax);

            // A standard deviation is needed for a meaningful KDE; without it the
            // violin collapses and only the center line is drawn.
            let Some(sd) = std_dev(&finite) else {
                continue;
            };
            if sd <= 0.0 || dmax <= dmin {
                continue;
            }
            let n = finite.len();
            let bw = sd * (n as f64).powf(-0.2);

            // Evaluate the density on the grid and find its peak for scaling.
            let mut grid = Vec::with_capacity(GRID_POINTS);
            let mut density = Vec::with_capacity(GRID_POINTS);
            let mut peak = 0.0_f64;
            for j in 0..GRID_POINTS {
                let t = j as f64 / (GRID_POINTS - 1) as f64;
                let y = dmin + (dmax - dmin) * t;
                let d = kde(&finite, bw, y);
                peak = peak.max(d);
                grid.push(y);
                density.push(d);
            }
            if peak <= 0.0 {
                continue;
            }

            // Build the closed body: up the right side, down the left side.
            let mut verts = Vec::with_capacity(2 * GRID_POINTS);
            for j in 0..GRID_POINTS {
                let hw = HALF_WIDTH * density[j] / peak;
                verts.push([pos + hw, grid[j]]);
            }
            for j in (0..GRID_POINTS).rev() {
                let hw = HALF_WIDTH * density[j] / peak;
                verts.push([pos - hw, grid[j]]);
            }

            let color = self.next_cycle_color();
            self.add_patch(
                Patch::polygon(&verts)
                    .facecolor(Some(color.with_alpha(BODY_ALPHA)))
                    .edgecolor(Some(color)),
            );
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rizzma_core::Bbox;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    /// Recover the right-side half-width samples of a violin body patch. The
    /// polygon is `right[0..N], left[N-1..0]` plus a closing repeat, so the
    /// first `N` vertices are the right edge.
    fn right_half_widths(patch: &Patch, pos: f64) -> Vec<f64> {
        let verts = patch.path().vertices();
        let n = (verts.len() - 1) / 2;
        verts[..n].iter().map(|v| v[0] - pos).collect()
    }

    #[test]
    fn violinplot_body_is_left_right_symmetric() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        ax.violinplot(&[&data], None);
        assert_eq!(ax.patches.len(), 1);
        let verts = ax.patches[0].path().vertices();
        let n = (verts.len() - 1) / 2;
        // Vertex j on the right mirrors vertex (2N-1-j) on the left about pos=1.
        for j in 0..n {
            let right = verts[j];
            let left = verts[2 * n - 1 - j];
            approx(right[1], left[1]);
            approx(right[0] - 1.0, 1.0 - left[0]);
        }
    }

    #[test]
    fn violinplot_normalizes_peak_and_concentration_widens_center() {
        let mut tight = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let mut spread = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Concentrated near 0 with a couple of far points; vs. a near-uniform
        // spread over the same range.
        let concentrated = [0.0, 0.0, 0.0, 0.0, 0.1, -0.1, 0.05, -0.05, 1.0, -1.0];
        let uniform = [-1.0, -0.6, -0.2, 0.2, 0.6, 1.0, -0.8, 0.8, -0.4, 0.4];
        tight.violinplot(&[&concentrated], None);
        spread.violinplot(&[&uniform], None);
        let tight_hw = right_half_widths(&tight.patches[0], 1.0);
        let spread_hw = right_half_widths(&spread.patches[0], 1.0);
        let max = |v: &[f64]| v.iter().copied().fold(0.0_f64, f64::max);
        // Both normalize so their peak reaches HALF_WIDTH.
        approx(max(&tight_hw), HALF_WIDTH);
        approx(max(&spread_hw), HALF_WIDTH);
        // The concentrated set is sharply peaked at its center, so its central
        // half-width exceeds the more uniform spread's central half-width.
        let mid = GRID_POINTS / 2;
        assert!(
            tight_hw[mid] > spread_hw[mid],
            "concentrated mid {} should exceed spread mid {}",
            tight_hw[mid],
            spread_hw[mid]
        );
    }

    #[test]
    fn violinplot_positions_and_limits_span_data() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 11.0, 12.0, 13.0, 14.0];
        ax.violinplot(&[&a, &b], None);
        let e = ax
            .data_limits()
            .expect("violinplot contributes data limits");
        // Violins at x = 1 and x = 2 with a 0.5 margin.
        approx(e.xmin(), 0.5);
        approx(e.xmax(), 2.5);
        approx(e.ymin(), 1.0);
        approx(e.ymax(), 14.0);
    }

    #[test]
    fn violinplot_custom_positions_are_honored() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 3.0, 4.0, 5.0, 6.0];
        ax.violinplot(&[&a, &b], Some(&[3.0, 7.0]));
        let e = ax
            .data_limits()
            .expect("violinplot contributes data limits");
        approx(e.xmin(), 2.5);
        approx(e.xmax(), 7.5);
    }

    #[test]
    fn violinplot_degenerate_dataset_does_not_panic() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Single value (n < 2), all-equal (zero variance), and empty datasets.
        let single = [4.0];
        let constant = [3.0, 3.0, 3.0];
        let empty: [f64; 0] = [];
        ax.violinplot(&[&single, &constant, &empty], None);
        // No body patches for any degenerate dataset.
        assert!(ax.patches.is_empty());
        // Center lines for the two non-empty datasets only.
        assert_eq!(ax.lines.len(), 2);
        // Limits stay finite (no NaN geometry from degenerate KDE).
        let e = ax
            .data_limits()
            .expect("violinplot contributes data limits");
        assert!(e.xmin().is_finite() && e.xmax().is_finite());
        assert!(e.ymin().is_finite() && e.ymax().is_finite());
    }
}
