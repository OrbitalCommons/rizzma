//! Data normalization: mapping data values into the unit interval `[0, 1]`.
//!
//! Mirrors the `Normalize` family from `matplotlib.colors`. A [`Normalize`]
//! maps a data value to `[0, 1]` (clipped), which a [`Colormap`] then samples.
//! The inverse direction maps a normalized value back to data space where it
//! is well defined.
//!
//! [`Colormap`]: super::Colormap

/// Maps data values into the unit interval `[0, 1]`.
///
/// Implementors map a raw data value to a normalized position in `[0, 1]`
/// (clipped to that range) via [`normalize`], and where invertible map a
/// normalized value back to data space via [`inverse`].
///
/// [`normalize`]: Normalize::normalize
/// [`inverse`]: Normalize::inverse
pub trait Normalize {
    /// Map `value` to a normalized position in `[0, 1]`, clipped to that range.
    fn normalize(&self, value: f64) -> f64;

    /// Map a normalized value `t` back to data space.
    ///
    /// `t` is clamped to `[0, 1]` before inversion. For non-invertible norms
    /// this returns a best-effort value (see the implementor's docs).
    fn inverse(&self, t: f64) -> f64;
}

/// Linear normalization mapping `[vmin, vmax]` onto `[0, 1]`.
///
/// Mirrors matplotlib's `Normalize`. Values outside `[vmin, vmax]` are clipped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearNorm {
    /// Lower data bound, mapped to `0.0`.
    pub vmin: f64,
    /// Upper data bound, mapped to `1.0`.
    pub vmax: f64,
}

impl LinearNorm {
    /// Construct a linear norm spanning `[vmin, vmax]`.
    #[must_use]
    pub const fn new(vmin: f64, vmax: f64) -> Self {
        Self { vmin, vmax }
    }
}

impl Normalize for LinearNorm {
    fn normalize(&self, value: f64) -> f64 {
        if self.vmax <= self.vmin {
            return 0.0;
        }
        ((value - self.vmin) / (self.vmax - self.vmin)).clamp(0.0, 1.0)
    }

    fn inverse(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        self.vmin + t * (self.vmax - self.vmin)
    }
}

/// Logarithmic normalization mapping `[vmin, vmax]` onto `[0, 1]` in log space.
///
/// Mirrors matplotlib's `LogNorm`. Both bounds must be strictly positive;
/// non-positive or unordered bounds yield `0.0` from [`normalize`].
///
/// [`normalize`]: Normalize::normalize
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogNorm {
    /// Lower data bound (must be `> 0`), mapped to `0.0`.
    pub vmin: f64,
    /// Upper data bound (must be `> 0`), mapped to `1.0`.
    pub vmax: f64,
}

impl LogNorm {
    /// Construct a log norm spanning `[vmin, vmax]`.
    #[must_use]
    pub const fn new(vmin: f64, vmax: f64) -> Self {
        Self { vmin, vmax }
    }
}

impl Normalize for LogNorm {
    fn normalize(&self, value: f64) -> f64 {
        if self.vmin <= 0.0 || self.vmax <= self.vmin || value <= 0.0 {
            return 0.0;
        }
        let lo = self.vmin.ln();
        let hi = self.vmax.ln();
        ((value.ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
    }

    fn inverse(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if self.vmin <= 0.0 || self.vmax <= 0.0 {
            return self.vmin;
        }
        let lo = self.vmin.ln();
        let hi = self.vmax.ln();
        (lo + t * (hi - lo)).exp()
    }
}

/// Power-law normalization: linear to `[0, 1]`, then raised to `gamma`.
///
/// Mirrors matplotlib's `PowerNorm`: a value `x` is mapped to
/// `((x - vmin) / (vmax - vmin)).powf(gamma)`. The result is clipped to
/// `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerNorm {
    /// Power-law exponent.
    pub gamma: f64,
    /// Lower data bound, mapped to `0.0`.
    pub vmin: f64,
    /// Upper data bound, mapped to `1.0`.
    pub vmax: f64,
}

impl PowerNorm {
    /// Construct a power-law norm with exponent `gamma` over `[vmin, vmax]`.
    #[must_use]
    pub const fn new(gamma: f64, vmin: f64, vmax: f64) -> Self {
        Self { gamma, vmin, vmax }
    }
}

impl Normalize for PowerNorm {
    fn normalize(&self, value: f64) -> f64 {
        if self.vmax <= self.vmin {
            return 0.0;
        }
        let res = (value - self.vmin) / (self.vmax - self.vmin);
        // matplotlib only applies the power to strictly positive residuals.
        let res = if res > 0.0 { res.powf(self.gamma) } else { res };
        res.clamp(0.0, 1.0)
    }

    fn inverse(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let res = if t > 0.0 { t.powf(1.0 / self.gamma) } else { t };
        self.vmin + res * (self.vmax - self.vmin)
    }
}

/// Discrete normalization bucketing data into `ncolors` bins by boundary edges.
///
/// Mirrors matplotlib's `BoundaryNorm`. Given `N` monotonically increasing
/// `boundaries` (defining `N - 1` regions) and a target `ncolors`, a value is
/// digitized into its region and mapped to a color index, then expressed as the
/// center of that index's slot in `[0, 1]`. Values below the first boundary map
/// to `0.0`; values at or above the last boundary map to `1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryNorm {
    /// Monotonically increasing bin edges (at least two).
    pub boundaries: Vec<f64>,
    /// Number of colors in the target colormap.
    pub ncolors: usize,
}

impl BoundaryNorm {
    /// Construct a boundary norm from bin `boundaries` and a color count.
    ///
    /// `boundaries` should be monotonically increasing with at least two
    /// entries, and `ncolors` should be at least the number of regions
    /// (`boundaries.len() - 1`).
    #[must_use]
    pub fn new(boundaries: Vec<f64>, ncolors: usize) -> Self {
        Self {
            boundaries,
            ncolors,
        }
    }

    /// Return the color index in `0..ncolors` for `value`.
    ///
    /// Values below `boundaries[0]` return `0`; values at or above the last
    /// boundary return `ncolors - 1`. This mirrors the integer index that
    /// matplotlib's `BoundaryNorm` produces for in-range data.
    #[must_use]
    pub fn index(&self, value: f64) -> usize {
        let n = self.boundaries.len();
        if n < 2 || self.ncolors == 0 {
            return 0;
        }
        if value < self.boundaries[0] {
            return 0;
        }
        if value >= self.boundaries[n - 1] {
            return self.ncolors - 1;
        }
        // digitize: number of boundaries <= value, minus one => region in 0..n-1.
        let mut region = 0usize;
        for (i, &edge) in self.boundaries.iter().enumerate() {
            if value >= edge {
                region = i;
            } else {
                break;
            }
        }
        let n_regions = n - 1;
        if self.ncolors > n_regions {
            if n_regions == 1 {
                (self.ncolors - 1) / 2
            } else {
                let scaled = (self.ncolors - 1) as f64 / (n_regions - 1) as f64 * region as f64;
                scaled as usize
            }
        } else {
            region
        }
    }
}

impl Normalize for BoundaryNorm {
    fn normalize(&self, value: f64) -> f64 {
        if self.ncolors <= 1 {
            return 0.0;
        }
        let idx = self.index(value);
        // Express the color index as the center of its slot in [0, 1].
        (idx as f64 + 0.5) / self.ncolors as f64
    }

    fn inverse(&self, t: f64) -> f64 {
        // BoundaryNorm is not invertible in matplotlib; return the data-space
        // boundary closest to the requested fraction as a best effort.
        let t = t.clamp(0.0, 1.0);
        if self.boundaries.is_empty() {
            return 0.0;
        }
        let n = self.boundaries.len();
        let lo = self.boundaries[0];
        let hi = self.boundaries[n - 1];
        lo + t * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} !~ {b}");
    }

    #[test]
    fn linear_endpoints_and_midpoint() {
        let n = LinearNorm::new(0.0, 10.0);
        approx(n.normalize(0.0), 0.0);
        approx(n.normalize(10.0), 1.0);
        approx(n.normalize(5.0), 0.5);
    }

    #[test]
    fn linear_clips_out_of_range() {
        let n = LinearNorm::new(0.0, 10.0);
        approx(n.normalize(-5.0), 0.0);
        approx(n.normalize(15.0), 1.0);
    }

    #[test]
    fn linear_inverse_roundtrip() {
        let n = LinearNorm::new(-2.0, 6.0);
        for &v in &[-2.0, 0.0, 3.0, 6.0] {
            approx(n.inverse(n.normalize(v)), v);
        }
    }

    #[test]
    fn linear_degenerate_range() {
        let n = LinearNorm::new(5.0, 5.0);
        approx(n.normalize(5.0), 0.0);
    }

    #[test]
    fn log_norm() {
        let n = LogNorm::new(1.0, 100.0);
        approx(n.normalize(1.0), 0.0);
        approx(n.normalize(100.0), 1.0);
        // 10 is the geometric midpoint of [1, 100].
        approx(n.normalize(10.0), 0.5);
        // Non-positive values clip to 0.
        approx(n.normalize(0.0), 0.0);
    }

    #[test]
    fn log_inverse_roundtrip() {
        let n = LogNorm::new(1.0, 1000.0);
        for &v in &[1.0, 10.0, 100.0, 1000.0] {
            approx(n.inverse(n.normalize(v)), v);
        }
    }

    #[test]
    fn power_roundtrip() {
        let n = PowerNorm::new(2.0, 0.0, 4.0);
        approx(n.normalize(0.0), 0.0);
        approx(n.normalize(4.0), 1.0);
        // (2/4)^2 = 0.25
        approx(n.normalize(2.0), 0.25);
        for &v in &[0.0, 1.0, 2.5, 4.0] {
            approx(n.inverse(n.normalize(v)), v);
        }
    }

    #[test]
    fn boundary_bucketing() {
        // 4 boundaries -> 3 regions, ncolors == 3.
        let n = BoundaryNorm::new(vec![0.0, 1.0, 2.0, 3.0], 3);
        assert_eq!(n.index(-1.0), 0);
        assert_eq!(n.index(0.5), 0);
        assert_eq!(n.index(1.5), 1);
        assert_eq!(n.index(2.5), 2);
        assert_eq!(n.index(3.5), 2);
        // normalize gives slot centers.
        approx(n.normalize(0.5), 0.5 / 3.0);
        approx(n.normalize(1.5), 1.5 / 3.0);
        approx(n.normalize(2.5), 2.5 / 3.0);
    }

    #[test]
    fn boundary_more_colors_than_regions() {
        // 3 boundaries -> 2 regions, ncolors == 4 stretches indices.
        let n = BoundaryNorm::new(vec![0.0, 1.0, 2.0], 4);
        assert_eq!(n.index(0.5), 0);
        assert_eq!(n.index(1.5), 3);
    }
}
