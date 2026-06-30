//! Tier-2 statistical box-and-whisker plotting on [`Axes`]:
//! [`boxplot`](Axes::boxplot).
//!
//! Mirrors matplotlib's `boxplot`: each dataset is summarized by its quartiles
//! and drawn as a box (Q1 to Q3) with a median line, whiskers reaching the most
//! extreme points within `1.5 * IQR` of the box, horizontal caps, and markers
//! for any outliers beyond the whiskers. The pieces are translated into
//! [`Patch`], [`Line2D`], and [`Collection`] artists stored on the [`Axes`], so
//! their extents feed autoscaling via [`data_limits`](Axes::data_limits). They
//! live alongside the other plotting helpers but are split out to keep
//! [`plotting`](super::plotting) focused on the Tier-1 line/patch methods.

use rizzma_artist::{Collection, Line2D, MarkerStyle, Patch};
use rizzma_core::color::Rgba;

use crate::Axes;

/// Default full width (in data units) of each box.
const BOX_WIDTH: f64 = 0.5;

/// Whisker reach as a multiple of the inter-quartile range (matplotlib default).
const WHISKER_IQR_MULTIPLE: f64 = 1.5;

/// Light face color for the box interior (matplotlib draws an open/white box).
const BOX_FACE: Rgba = Rgba::WHITE;

/// Marker size (in points) for outlier fliers.
const FLIER_SIZE: f64 = 6.0;

/// Linear-interpolation percentile of a pre-sorted slice (matplotlib's default
/// `'linear'` interpolation), with `q` in `[0, 100]`.
///
/// The rank position is `(n - 1) * q / 100`; the result interpolates linearly
/// between the two bracketing order statistics. `sorted` must be sorted
/// ascending and non-empty.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    debug_assert!(!sorted.is_empty(), "percentile requires non-empty data");
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let rank = (n - 1) as f64 * q / 100.0;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    let frac = rank - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

/// The five-number-ish summary used to draw a single box.
struct BoxStats {
    q1: f64,
    median: f64,
    q3: f64,
    /// Lower whisker bound: most extreme datum within `Q1 - k*IQR`.
    whisker_lo: f64,
    /// Upper whisker bound: most extreme datum within `Q3 + k*IQR`.
    whisker_hi: f64,
    /// Points beyond the whiskers.
    fliers: Vec<f64>,
}

/// Compute the box statistics for one dataset (a copy is sorted internally).
fn box_stats(data: &[f64]) -> BoxStats {
    let mut sorted: Vec<f64> = data.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("filtered finite values"));

    let q1 = percentile(&sorted, 25.0);
    let median = percentile(&sorted, 50.0);
    let q3 = percentile(&sorted, 75.0);
    let iqr = q3 - q1;
    let lo_fence = q1 - WHISKER_IQR_MULTIPLE * iqr;
    let hi_fence = q3 + WHISKER_IQR_MULTIPLE * iqr;

    // Whiskers reach the most extreme data points inside the fences.
    let whisker_lo = sorted
        .iter()
        .copied()
        .find(|&v| v >= lo_fence)
        .unwrap_or(q1);
    let whisker_hi = sorted
        .iter()
        .rev()
        .copied()
        .find(|&v| v <= hi_fence)
        .unwrap_or(q3);

    let fliers: Vec<f64> = sorted
        .iter()
        .copied()
        .filter(|&v| v < whisker_lo || v > whisker_hi)
        .collect();

    BoxStats {
        q1,
        median,
        q3,
        whisker_lo,
        whisker_hi,
        fliers,
    }
}

impl Axes {
    /// Draw a vertical box-and-whisker plot, one box per dataset.
    ///
    /// Mirrors matplotlib's `boxplot`: dataset `i` is drawn at integer position
    /// `i + 1`. For each dataset the quartiles `Q1`, median, and `Q3` are
    /// computed by linear-interpolation percentiles; `IQR = Q3 - Q1`; the
    /// whiskers reach the most extreme data points within
    /// `[Q1 - 1.5*IQR, Q3 + 1.5*IQR]`; and points beyond the whiskers are
    /// outliers. Each box is a [`Patch::rectangle`] from `Q1` to `Q3` (light
    /// face, black edge) of width `0.5`, crossed by a median [`Line2D`], with
    /// whisker lines (vertical from the box edges to the whisker bounds) capped
    /// by short horizontal lines, plus a [`Collection`] of `'o'` markers at any
    /// outliers. All artists are black-lined and fold into
    /// [`data_limits`](Axes::data_limits) so autoscale fits the boxes,
    /// whiskers, and outliers; the boxes are spaced and labeled by their
    /// integer position. Empty input (or an empty dataset) draws nothing for
    /// that slot.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let a = [1.0, 2.0, 3.0, 4.0, 5.0];
    /// let b = [2.0, 3.0, 4.0, 5.0, 6.0];
    /// ax.boxplot(&[&a, &b]);
    /// let limits = ax.data_limits().expect("boxplot contributes data limits");
    /// // Two boxes sit at x = 1 and x = 2 with width 0.5.
    /// assert_eq!(limits.xmin(), 0.75);
    /// assert_eq!(limits.xmax(), 2.25);
    /// ```
    pub fn boxplot(&mut self, data: &[&[f64]]) {
        let half = BOX_WIDTH / 2.0;
        let cap = BOX_WIDTH / 4.0;

        for (i, dataset) in data.iter().enumerate() {
            if dataset.iter().all(|v| !v.is_finite()) {
                continue;
            }
            let pos = (i + 1) as f64;
            let stats = box_stats(dataset);

            // Box from Q1 to Q3.
            self.add_patch(
                Patch::rectangle(pos - half, stats.q1, BOX_WIDTH, stats.q3 - stats.q1)
                    .facecolor(Some(BOX_FACE))
                    .edgecolor(Some(Rgba::BLACK)),
            );

            // Median line across the box.
            self.add_line(
                Line2D::new(
                    vec![pos - half, pos + half],
                    vec![stats.median, stats.median],
                )
                .with_color(Rgba::BLACK),
            );

            // Lower whisker: stem from the box edge down to the bound, plus cap.
            self.add_line(
                Line2D::new(vec![pos, pos], vec![stats.q1, stats.whisker_lo])
                    .with_color(Rgba::BLACK),
            );
            self.add_line(
                Line2D::new(
                    vec![pos - cap, pos + cap],
                    vec![stats.whisker_lo, stats.whisker_lo],
                )
                .with_color(Rgba::BLACK),
            );

            // Upper whisker: stem from the box edge up to the bound, plus cap.
            self.add_line(
                Line2D::new(vec![pos, pos], vec![stats.q3, stats.whisker_hi])
                    .with_color(Rgba::BLACK),
            );
            self.add_line(
                Line2D::new(
                    vec![pos - cap, pos + cap],
                    vec![stats.whisker_hi, stats.whisker_hi],
                )
                .with_color(Rgba::BLACK),
            );

            // Outlier markers, if any.
            if !stats.fliers.is_empty() {
                let offsets: Vec<[f64; 2]> = stats.fliers.iter().map(|&v| [pos, v]).collect();
                let marker = MarkerStyle::from_char('o')
                    .expect("'o' is a known marker")
                    .path()
                    .clone();
                self.collections.push(
                    Collection::scatter(offsets)
                        .with_marker(marker)
                        .with_sizes(vec![FLIER_SIZE])
                        .with_facecolors(Vec::new())
                        .with_edgecolors(vec![Rgba::BLACK]),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rizzma_core::Bbox;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn percentile_matches_known_quartiles() {
        let sorted = [1.0, 2.0, 3.0, 4.0, 5.0];
        approx(percentile(&sorted, 0.0), 1.0);
        approx(percentile(&sorted, 25.0), 2.0);
        approx(percentile(&sorted, 50.0), 3.0);
        approx(percentile(&sorted, 75.0), 4.0);
        approx(percentile(&sorted, 100.0), 5.0);
    }

    #[test]
    fn percentile_interpolates_between_order_statistics() {
        // n = 4: rank for q=25 is (3)*0.25 = 0.75, between sorted[0] and sorted[1].
        let sorted = [10.0, 20.0, 30.0, 40.0];
        approx(percentile(&sorted, 25.0), 17.5);
        approx(percentile(&sorted, 50.0), 25.0);
        approx(percentile(&sorted, 75.0), 32.5);
    }

    #[test]
    fn boxplot_adds_expected_artists_per_box() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 3.0, 4.0, 5.0, 6.0];
        ax.boxplot(&[&a, &b]);
        // One box patch per dataset.
        assert_eq!(ax.patches.len(), 2);
        // Per box: median + 2 whisker stems + 2 caps = 5 lines.
        assert_eq!(ax.lines.len(), 2 * 5);
        // No outliers in these tidy datasets.
        assert!(ax.collections.is_empty());
    }

    #[test]
    fn boxplot_positions_and_limits_span_data() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 11.0, 12.0, 13.0, 14.0];
        ax.boxplot(&[&a, &b]);
        let e = ax.data_limits().expect("boxplot contributes data limits");
        // Boxes at x = 1 and x = 2, width 0.5 → [0.75, 2.25].
        approx(e.xmin(), 0.75);
        approx(e.xmax(), 2.25);
        // y spans the lowest whisker (1.0) to the highest (14.0).
        approx(e.ymin(), 1.0);
        approx(e.ymax(), 14.0);
    }

    #[test]
    fn boxplot_clear_outlier_produces_a_marker() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // A tight cluster with one far-away point that lands beyond 1.5*IQR.
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0];
        ax.boxplot(&[&data]);
        // The outlier collection exists and carries the outlier point.
        assert_eq!(ax.collections.len(), 1);
        let e = ax.data_limits().expect("boxplot contributes data limits");
        // The 100.0 outlier pushes the upper data limit out to it.
        approx(e.ymax(), 100.0);
    }

    #[test]
    fn boxplot_empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.boxplot(&[]);
        assert!(ax.patches.is_empty());
        assert!(ax.lines.is_empty());
        assert!(ax.collections.is_empty());
    }
}
