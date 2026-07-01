//! Axis scales: mappings from data coordinates to scaled (display) coordinates.
//!
//! Each [`Scale`] provides a forward [`Scale::transform`] and its
//! [`Scale::inverse`], plus an optional [`Scale::limit_range_for_scale`] hook
//! that clamps an axis range to the domain supported by the scale (mirroring
//! matplotlib's behaviour for log/logit scales).
//!
//! Ported from matplotlib's `lib/matplotlib/scale.py`.

/// A separable transformation acting on a single axis dimension.
///
/// Implementations map data coordinates to scaled coordinates via
/// [`transform`](Scale::transform) and back via [`inverse`](Scale::inverse).
/// The two functions must round-trip over the scale's valid domain.
pub trait Scale {
    /// Map a data coordinate to a scaled (display) coordinate.
    fn transform(&self, x: f64) -> f64;

    /// Map a scaled coordinate back to a data coordinate.
    ///
    /// This is the inverse of [`transform`](Scale::transform).
    fn inverse(&self, x: f64) -> f64;

    /// Restrict `(vmin, vmax)` to the domain supported by this scale.
    ///
    /// `minpos` is the smallest positive value in the data, used by scales
    /// (such as log) whose domain excludes non-positive numbers. The default
    /// implementation imposes no restriction.
    fn limit_range_for_scale(&self, vmin: f64, vmax: f64, _minpos: f64) -> (f64, f64) {
        (vmin, vmax)
    }
}

/// The default linear scale: an identity transform.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearScale;

impl LinearScale {
    /// Construct a new linear scale.
    pub fn new() -> Self {
        LinearScale
    }
}

impl Scale for LinearScale {
    fn transform(&self, x: f64) -> f64 {
        x
    }

    fn inverse(&self, x: f64) -> f64 {
        x
    }
}

/// A standard logarithmic scale.
///
/// The transform is `log(x) / log(base)`; the domain is `x > 0`. For the
/// common bases 10 and 2 the dedicated [`f64::log10`]/[`f64::log2`] (and
/// `powi`-free) routines are used to avoid the floating-point error that
/// `log(x) / log(base)` would otherwise introduce.
#[derive(Debug, Clone, Copy)]
pub struct LogScale {
    /// The base of the logarithm. Must be `> 0` and `!= 1`.
    pub base: f64,
}

impl LogScale {
    /// Construct a logarithmic scale with the given `base`.
    pub fn new(base: f64) -> Self {
        LogScale { base }
    }
}

impl Scale for LogScale {
    fn transform(&self, x: f64) -> f64 {
        if self.base == 10.0 {
            x.log10()
        } else if self.base == 2.0 {
            x.log2()
        } else {
            x.ln() / self.base.ln()
        }
    }

    fn inverse(&self, x: f64) -> f64 {
        if self.base == 2.0 {
            x.exp2()
        } else {
            (x * self.base.ln()).exp()
        }
    }

    /// Clamp the domain to positive values, replacing non-positive bounds with
    /// `minpos` (a tiny positive fallback when `minpos` is not finite).
    fn limit_range_for_scale(&self, vmin: f64, vmax: f64, minpos: f64) -> (f64, f64) {
        let minpos = if minpos.is_finite() { minpos } else { 1e-300 };
        (
            if vmin <= 0.0 { minpos } else { vmin },
            if vmax <= 0.0 { minpos } else { vmax },
        )
    }
}

/// The symmetrical logarithmic ("symlog") scale.
///
/// The scale is linear within `[-linthresh, linthresh]` and logarithmic
/// beyond it, joining continuously at `±linthresh`. The `linscale` parameter
/// stretches the linear region relative to the logarithmic range; internally
/// it is applied as `linscale_adj = linscale / (1 - 1/base)`.
#[derive(Debug, Clone, Copy)]
pub struct SymlogScale {
    /// The base of the logarithm. Must be `> 1`.
    pub base: f64,
    /// Half-width of the linear region around zero. Must be `> 0`.
    pub linthresh: f64,
    /// Stretch factor for the linear region. Must be `> 0`.
    pub linscale: f64,
}

impl SymlogScale {
    /// Construct a symlog scale with the given `base`, `linthresh`, and
    /// `linscale`.
    pub fn new(base: f64, linthresh: f64, linscale: f64) -> Self {
        SymlogScale {
            base,
            linthresh,
            linscale,
        }
    }

    /// The adjusted linear-scale factor, `linscale / (1 - 1/base)`.
    fn linscale_adj(&self) -> f64 {
        self.linscale / (1.0 - 1.0 / self.base)
    }
}

impl Scale for SymlogScale {
    fn transform(&self, x: f64) -> f64 {
        let linscale_adj = self.linscale_adj();
        let abs_x = x.abs();
        if abs_x <= self.linthresh {
            x * linscale_adj
        } else {
            let log_base = self.base.ln();
            x.signum()
                * self.linthresh
                * (linscale_adj - self.linthresh.ln() / log_base + abs_x.ln() / log_base)
        }
    }

    fn inverse(&self, x: f64) -> f64 {
        let linscale_adj = self.linscale_adj();
        // The scaled coordinate of `linthresh` under the forward transform.
        let invlinthresh = self.linthresh * linscale_adj;
        let abs_x = x.abs();
        if abs_x <= invlinthresh {
            x / linscale_adj
        } else {
            x.signum()
                * self.linthresh
                * ((abs_x / self.linthresh - linscale_adj) * self.base.ln()).exp()
        }
    }
}

/// The inverse-hyperbolic-sine scale.
///
/// The transform is `asinh(x / linear_width)`, which is approximately linear
/// around zero and logarithmic in both tails. Unlike [`LogScale`] and
/// [`LogitScale`], the asinh scale accepts the entire real line.
#[derive(Debug, Clone, Copy)]
pub struct AsinhScale {
    /// Width of the quasi-linear region around zero. Must be finite and
    /// positive.
    pub linear_width: f64,
}

impl AsinhScale {
    /// Construct an asinh scale with the given positive linear-region width.
    pub fn new(linear_width: f64) -> Self {
        assert!(
            linear_width.is_finite() && linear_width > 0.0,
            "asinh scale linear_width must be finite and > 0"
        );
        AsinhScale { linear_width }
    }
}

impl Default for AsinhScale {
    /// Default to a unit-width linear region.
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Scale for AsinhScale {
    fn transform(&self, x: f64) -> f64 {
        (x / self.linear_width).asinh()
    }

    fn inverse(&self, x: f64) -> f64 {
        self.linear_width * x.sinh()
    }
}

/// The logit scale for probabilities in the open interval `(0, 1)`.
///
/// The transform is `log(x / (1 - x))`, mapping `(0, 1)` onto the whole real
/// line; the inverse is the logistic function `1 / (1 + exp(-y))`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogitScale;

impl LogitScale {
    /// Construct a new logit scale.
    pub fn new() -> Self {
        LogitScale
    }
}

impl Scale for LogitScale {
    fn transform(&self, x: f64) -> f64 {
        (x / (1.0 - x)).ln()
    }

    fn inverse(&self, x: f64) -> f64 {
        1.0 / (1.0 + (-x).exp())
    }

    /// Clamp the domain to `(minpos, 1 - minpos)`, using a tiny positive
    /// fallback when `minpos` is not finite.
    fn limit_range_for_scale(&self, vmin: f64, vmax: f64, minpos: f64) -> (f64, f64) {
        let minpos = if minpos.is_finite() { minpos } else { 1e-7 };
        (
            if vmin <= 0.0 { minpos } else { vmin },
            if vmax >= 1.0 { 1.0 - minpos } else { vmax },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    #[test]
    fn linear_is_identity() {
        let s = LinearScale::new();
        for &x in &[-3.5, 0.0, 1.0, 42.0] {
            approx(s.transform(x), x);
            approx(s.inverse(x), x);
        }
    }

    #[test]
    fn log10_maps_powers() {
        let s = LogScale::new(10.0);
        approx(s.transform(1.0), 0.0);
        approx(s.transform(10.0), 1.0);
        approx(s.transform(100.0), 2.0);
        for &x in &[0.5, 1.0, 10.0, 12345.0] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn log2_maps_eight() {
        let s = LogScale::new(2.0);
        approx(s.transform(8.0), 3.0);
        approx(s.inverse(3.0), 8.0);
    }

    #[test]
    fn log_arbitrary_base_round_trips() {
        let s = LogScale::new(7.0);
        for &x in &[0.1, 1.0, 7.0, 49.0, 1000.0] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn log_limit_clamps_nonpositive() {
        let s = LogScale::new(10.0);
        let (lo, hi) = s.limit_range_for_scale(-5.0, 0.0, 1e-3);
        approx(lo, 1e-3);
        approx(hi, 1e-3);
        let (lo, hi) = s.limit_range_for_scale(2.0, 50.0, 1e-3);
        approx(lo, 2.0);
        approx(hi, 50.0);
    }

    #[test]
    fn symlog_continuous_at_linthresh() {
        let s = SymlogScale::new(10.0, 2.0, 1.0);
        let eps = 1e-7;
        let below = s.transform(s.linthresh - eps);
        let above = s.transform(s.linthresh + eps);
        assert!(
            (below - above).abs() < 1e-5,
            "discontinuity: {below} vs {above}"
        );
    }

    #[test]
    fn symlog_round_trips() {
        let s = SymlogScale::new(10.0, 2.0, 1.0);
        for &x in &[-1000.0, -50.0, -2.0, -0.3, 0.0, 0.1, 2.0, 37.0, 5000.0] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn symlog_round_trips_other_params() {
        let s = SymlogScale::new(2.0, 0.5, 2.0);
        for &x in &[-100.0, -0.5, -0.01, 0.0, 0.01, 0.5, 8.0, 64.0] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn asinh_maps_zero_and_is_odd() {
        let s = AsinhScale::new(2.0);

        approx(s.transform(0.0), 0.0);
        approx(s.inverse(0.0), 0.0);
        approx(s.transform(-10.0), -s.transform(10.0));
        approx(s.inverse(-2.0), -s.inverse(2.0));
    }

    #[test]
    fn asinh_is_linear_near_zero() {
        let s = AsinhScale::new(4.0);

        let x = 1e-6;
        assert!((s.transform(x) - x / s.linear_width).abs() < 1e-18);
    }

    #[test]
    fn asinh_has_logarithmic_tails() {
        let s = AsinhScale::new(2.0);

        let x = 1e12;
        let expected = (2.0 * x / s.linear_width).ln();
        assert!((s.transform(x) - expected).abs() < 1e-10);
    }

    #[test]
    fn asinh_round_trips_whole_real_line() {
        let s = AsinhScale::new(0.5);
        for &x in &[-1e6, -10.0, -0.1, 0.0, 0.1, 10.0, 1e6] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn logit_maps_half_and_round_trips() {
        let s = LogitScale::new();
        approx(s.transform(0.5), 0.0);
        for &x in &[0.01, 0.25, 0.5, 0.75, 0.99] {
            approx(s.inverse(s.transform(x)), x);
        }
    }

    #[test]
    fn logit_limit_clamps() {
        let s = LogitScale::new();
        let (lo, hi) = s.limit_range_for_scale(0.0, 1.0, 1e-7);
        approx(lo, 1e-7);
        approx(hi, 1.0 - 1e-7);
    }
}
