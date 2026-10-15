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

    /// Map `value` without clipping — matplotlib's `clip=False`, which is what
    /// colormapping uses. Data below the norm's range lands below `0` and data
    /// above it beyond `1`, where a colormap draws its under and over colors;
    /// `NaN` marks a value the norm cannot place (a non-positive value under a
    /// log norm), drawn with the bad color.
    ///
    /// The default clips, for norms without an out-of-range notion.
    fn normalize_unclipped(&self, value: f64) -> f64 {
        self.normalize(value)
    }

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
        self.normalize_unclipped(value).clamp(0.0, 1.0)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        if self.vmax <= self.vmin {
            return 0.0;
        }
        (value - self.vmin) / (self.vmax - self.vmin)
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

    /// matplotlib masks non-positive data under a log norm: those values
    /// are bad (`NaN`) rather than below the range.
    fn normalize_unclipped(&self, value: f64) -> f64 {
        if self.vmin <= 0.0 || self.vmax <= self.vmin {
            return 0.0;
        }
        if value <= 0.0 {
            return f64::NAN;
        }
        let lo = self.vmin.log10();
        (value.log10() - lo) / (self.vmax.log10() - lo)
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
        self.normalize_unclipped(value).clamp(0.0, 1.0)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        if self.vmax <= self.vmin {
            return 0.0;
        }
        let res = (value - self.vmin) / (self.vmax - self.vmin);
        // matplotlib only applies the power to strictly positive residuals.
        if res > 0.0 { res.powf(self.gamma) } else { res }
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

    /// Data below the first boundary is under the range and data at or above
    /// the last is over it, as matplotlib's `BoundaryNorm` reports with
    /// `extend="neither"`.
    fn normalize_unclipped(&self, value: f64) -> f64 {
        let n = self.boundaries.len();
        if value.is_nan() {
            return f64::NAN;
        }
        if n >= 2 && value < self.boundaries[0] {
            return -1.0;
        }
        if n >= 2 && value >= self.boundaries[n - 1] {
            return 2.0;
        }
        self.normalize(value)
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

/// Diverging normalization with two linear slopes meeting at a center:
/// `[vmin, vcenter]` maps onto `[0, 0.5]` and `[vcenter, vmax]` onto
/// `[0.5, 1]`.
///
/// Mirrors matplotlib's `TwoSlopeNorm`: a diverging colormap's midpoint lands
/// on `vcenter` however lopsided the range around it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoSlopeNorm {
    /// Lower data bound, mapped to `0.0`.
    pub vmin: f64,
    /// Center, mapped to `0.5`.
    pub vcenter: f64,
    /// Upper data bound, mapped to `1.0`.
    pub vmax: f64,
}

impl TwoSlopeNorm {
    /// Construct a two-slope norm over `[vmin, vmax]` centered on `vcenter`,
    /// which should lie strictly between them.
    #[must_use]
    pub const fn new(vmin: f64, vcenter: f64, vmax: f64) -> Self {
        Self {
            vmin,
            vcenter,
            vmax,
        }
    }
}

impl Normalize for TwoSlopeNorm {
    fn normalize(&self, value: f64) -> f64 {
        self.normalize_unclipped(value).clamp(0.0, 1.0)
    }

    /// matplotlib interpolates through `(vmin, 0)`, `(vcenter, 0.5)` and
    /// `(vmax, 1)`, sending everything outside the range to `±∞`.
    fn normalize_unclipped(&self, value: f64) -> f64 {
        if !(self.vmin < self.vcenter && self.vcenter < self.vmax) {
            return 0.0;
        }
        if value.is_nan() {
            f64::NAN
        } else if value < self.vmin {
            f64::NEG_INFINITY
        } else if value > self.vmax {
            f64::INFINITY
        } else if value <= self.vcenter {
            0.5 * (value - self.vmin) / (self.vcenter - self.vmin)
        } else {
            0.5 + 0.5 * (value - self.vcenter) / (self.vmax - self.vcenter)
        }
    }

    fn inverse(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.5 {
            self.vmin + 2.0 * t * (self.vcenter - self.vmin)
        } else {
            self.vcenter + 2.0 * (t - 0.5) * (self.vmax - self.vcenter)
        }
    }
}

/// Symmetric linear normalization about a center: `[vcenter - halfrange,
/// vcenter + halfrange]` onto `[0, 1]`.
///
/// Mirrors matplotlib's `CenteredNorm`, the usual partner of a diverging
/// colormap when the data should be read as deviations from `vcenter`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenteredNorm {
    /// Center, mapped to `0.5`.
    pub vcenter: f64,
    /// Distance from the center to either end of the range.
    pub halfrange: f64,
}

impl CenteredNorm {
    /// Construct a norm spanning `vcenter ± halfrange`.
    #[must_use]
    pub const fn new(vcenter: f64, halfrange: f64) -> Self {
        Self { vcenter, halfrange }
    }

    /// The equivalent linear norm.
    fn linear(&self) -> LinearNorm {
        LinearNorm::new(self.vcenter - self.halfrange, self.vcenter + self.halfrange)
    }
}

impl Normalize for CenteredNorm {
    fn normalize(&self, value: f64) -> f64 {
        self.linear().normalize(value)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        self.linear().normalize_unclipped(value)
    }

    fn inverse(&self, t: f64) -> f64 {
        self.linear().inverse(t)
    }
}

/// Symmetric-log normalization: linear within `±linthresh` of zero and
/// logarithmic beyond, over `[vmin, vmax]`.
///
/// Mirrors matplotlib's `SymLogNorm` (its `SymmetricalLogTransform`): data of
/// both signs spanning many decades, where `LogNorm` would discard everything
/// non-positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymLogNorm {
    /// Half-width of the linear region around zero (must be `> 0`).
    pub linthresh: f64,
    /// Length of the linear region in decades, as matplotlib's `linscale`.
    pub linscale: f64,
    /// Logarithm base (must be `> 1`).
    pub base: f64,
    /// Lower data bound, mapped to `0.0`.
    pub vmin: f64,
    /// Upper data bound, mapped to `1.0`.
    pub vmax: f64,
}

impl SymLogNorm {
    /// Construct a base-10 symlog norm with `linscale = 1`, matplotlib's
    /// defaults, over `[vmin, vmax]`.
    #[must_use]
    pub const fn new(linthresh: f64, vmin: f64, vmax: f64) -> Self {
        Self {
            linthresh,
            linscale: 1.0,
            base: 10.0,
            vmin,
            vmax,
        }
    }

    /// Set the linear region's length in decades.
    #[must_use]
    pub const fn with_linscale(mut self, linscale: f64) -> Self {
        self.linscale = linscale;
        self
    }

    /// Set the logarithm base.
    #[must_use]
    pub const fn with_base(mut self, base: f64) -> Self {
        self.base = base;
        self
    }

    /// matplotlib's `_linscale_adj`.
    fn linscale_adj(&self) -> f64 {
        self.linscale / (1.0 - 1.0 / self.base)
    }

    /// The symmetric-log transform of `v`.
    fn transform(&self, v: f64) -> f64 {
        let a = v.abs();
        if a <= self.linthresh {
            v * self.linscale_adj()
        } else {
            v.signum()
                * self.linthresh
                * (self.linscale_adj() + (a / self.linthresh).ln() / self.base.ln())
        }
    }

    /// The inverse of [`transform`](Self::transform).
    fn untransform(&self, y: f64) -> f64 {
        let a = y.abs();
        if a <= self.linthresh * self.linscale_adj() {
            y / self.linscale_adj()
        } else {
            y.signum() * self.linthresh * self.base.powf(a / self.linthresh - self.linscale_adj())
        }
    }
}

impl Normalize for SymLogNorm {
    fn normalize(&self, value: f64) -> f64 {
        self.normalize_unclipped(value).clamp(0.0, 1.0)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        if self.linthresh <= 0.0 || self.base <= 1.0 || self.vmax <= self.vmin {
            return 0.0;
        }
        let lo = self.transform(self.vmin);
        (self.transform(value) - lo) / (self.transform(self.vmax) - lo)
    }

    fn inverse(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let lo = self.transform(self.vmin);
        self.untransform(lo + t * (self.transform(self.vmax) - lo))
    }
}

/// Any of rizzma's norms, as an image or color mapping stores it.
///
/// Every norm type converts into a `Norm`, so methods taking
/// `impl Into<Norm>` accept them directly: `.norm(LogNorm::new(1.0, 1e4))`.
/// A closed set, so figures that use a norm stay exportable. It serializes as
/// `{"kind": "log", "params": [vmin, vmax]}`, the parameters in constructor
/// order (a [`BoundaryNorm`] lists `ncolors` first, then its boundaries).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(into = "NormWire", try_from = "NormWire")]
pub enum Norm {
    /// See [`LinearNorm`].
    Linear(LinearNorm),
    /// See [`LogNorm`].
    Log(LogNorm),
    /// See [`PowerNorm`].
    Power(PowerNorm),
    /// See [`BoundaryNorm`].
    Boundary(BoundaryNorm),
    /// See [`TwoSlopeNorm`].
    TwoSlope(TwoSlopeNorm),
    /// See [`CenteredNorm`].
    Centered(CenteredNorm),
    /// See [`SymLogNorm`].
    SymLog(SymLogNorm),
}

impl Norm {
    /// The wrapped norm.
    fn inner(&self) -> &dyn Normalize {
        match self {
            Norm::Linear(n) => n,
            Norm::Log(n) => n,
            Norm::Power(n) => n,
            Norm::Boundary(n) => n,
            Norm::TwoSlope(n) => n,
            Norm::Centered(n) => n,
            Norm::SymLog(n) => n,
        }
    }
}

impl Normalize for Norm {
    fn normalize(&self, value: f64) -> f64 {
        self.inner().normalize(value)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        self.inner().normalize_unclipped(value)
    }

    fn inverse(&self, t: f64) -> f64 {
        self.inner().inverse(t)
    }
}

/// The wire form of a [`Norm`]: one shape for every kind, so a decoder needs
/// one struct visitor rather than one per norm type.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormWire {
    kind: NormKind,
    params: Vec<f64>,
}

/// Which norm a [`NormWire`] holds.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum NormKind {
    Linear,
    Log,
    Power,
    Boundary,
    TwoSlope,
    Centered,
    SymLog,
}

impl From<Norm> for NormWire {
    fn from(norm: Norm) -> Self {
        let (kind, params) = match norm {
            Norm::Linear(n) => (NormKind::Linear, vec![n.vmin, n.vmax]),
            Norm::Log(n) => (NormKind::Log, vec![n.vmin, n.vmax]),
            Norm::Power(n) => (NormKind::Power, vec![n.gamma, n.vmin, n.vmax]),
            Norm::Boundary(n) => {
                let mut params = vec![n.ncolors as f64];
                params.extend(n.boundaries);
                (NormKind::Boundary, params)
            }
            Norm::TwoSlope(n) => (NormKind::TwoSlope, vec![n.vmin, n.vcenter, n.vmax]),
            Norm::Centered(n) => (NormKind::Centered, vec![n.vcenter, n.halfrange]),
            Norm::SymLog(n) => (
                NormKind::SymLog,
                vec![n.linthresh, n.linscale, n.base, n.vmin, n.vmax],
            ),
        };
        NormWire { kind, params }
    }
}

impl TryFrom<NormWire> for Norm {
    type Error = String;

    fn try_from(wire: NormWire) -> Result<Self, String> {
        let p = &wire.params;
        let want = |n: usize| {
            if p.len() == n {
                Ok(())
            } else {
                Err(format!("norm expects {n} parameters, got {}", p.len()))
            }
        };
        Ok(match wire.kind {
            NormKind::Linear => {
                want(2)?;
                Norm::Linear(LinearNorm::new(p[0], p[1]))
            }
            NormKind::Log => {
                want(2)?;
                Norm::Log(LogNorm::new(p[0], p[1]))
            }
            NormKind::Power => {
                want(3)?;
                Norm::Power(PowerNorm::new(p[0], p[1], p[2]))
            }
            NormKind::Boundary => {
                let Some((&ncolors, boundaries)) = p.split_first() else {
                    return Err("boundary norm needs a color count".to_owned());
                };
                if !(ncolors >= 0.0 && ncolors.fract() == 0.0 && ncolors <= f64::from(u32::MAX)) {
                    return Err(format!(
                        "boundary norm color count {ncolors} is not a count"
                    ));
                }
                Norm::Boundary(BoundaryNorm::new(boundaries.to_vec(), ncolors as usize))
            }
            NormKind::TwoSlope => {
                want(3)?;
                Norm::TwoSlope(TwoSlopeNorm::new(p[0], p[1], p[2]))
            }
            NormKind::Centered => {
                want(2)?;
                Norm::Centered(CenteredNorm::new(p[0], p[1]))
            }
            NormKind::SymLog => {
                want(5)?;
                Norm::SymLog(SymLogNorm {
                    linthresh: p[0],
                    linscale: p[1],
                    base: p[2],
                    vmin: p[3],
                    vmax: p[4],
                })
            }
        })
    }
}

macro_rules! norm_from {
    ($($ty:ident => $variant:ident),* $(,)?) => {
        $(impl From<$ty> for Norm {
            fn from(norm: $ty) -> Self {
                Norm::$variant(norm)
            }
        })*
    };
}

norm_from!(
    LinearNorm => Linear,
    LogNorm => Log,
    PowerNorm => Power,
    BoundaryNorm => Boundary,
    TwoSlopeNorm => TwoSlope,
    CenteredNorm => Centered,
    SymLogNorm => SymLog,
);

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

    #[test]
    fn new_norms_invert_their_own_mapping() {
        let norms: [Norm; 3] = [
            TwoSlopeNorm::new(-3.0, 0.5, 10.0).into(),
            CenteredNorm::new(2.0, 5.0).into(),
            SymLogNorm::new(0.5, -1e3, 1e4).with_base(2.0).into(),
        ];
        for norm in norms {
            for k in 0..=20 {
                let t = f64::from(k) / 20.0;
                let back = norm.normalize(norm.inverse(t));
                assert!((back - t).abs() < 1e-12, "{norm:?} at {t}: {back}");
            }
        }
    }

    #[test]
    fn two_slope_puts_the_center_at_one_half_and_outliers_beyond() {
        let n = TwoSlopeNorm::new(-1.0, 0.0, 9.0);
        approx(n.normalize(0.0), 0.5);
        approx(n.normalize(-0.5), 0.25);
        approx(n.normalize(4.5), 0.75);
        assert_eq!(n.normalize_unclipped(-2.0), f64::NEG_INFINITY);
        assert_eq!(n.normalize_unclipped(10.0), f64::INFINITY);
    }

    #[test]
    fn symlog_is_linear_inside_the_threshold_and_odd() {
        let n = SymLogNorm::new(1.0, -100.0, 100.0);
        approx(n.normalize(0.0), 0.5);
        approx(n.normalize(-30.0) + n.normalize(30.0), 1.0);
        // Inside ±linthresh the transform is linear.
        let (a, b) = (n.normalize(0.25), n.normalize(0.5));
        approx(b - 0.5, 2.0 * (a - 0.5));
    }

    #[cfg(feature = "portable")]
    #[test]
    fn every_norm_round_trips_its_wire_form_and_bad_params_are_refused() {
        let norms: Vec<Norm> = vec![
            LinearNorm::new(-1.0, 2.0).into(),
            LogNorm::new(0.5, 50.0).into(),
            PowerNorm::new(0.3, 0.0, 9.0).into(),
            BoundaryNorm::new(vec![0.0, 1.0, 4.0], 7).into(),
            TwoSlopeNorm::new(-2.0, 1.0, 8.0).into(),
            CenteredNorm::new(3.0, 0.25).into(),
            SymLogNorm::new(0.1, -5.0, 5.0)
                .with_linscale(2.0)
                .with_base(2.0)
                .into(),
        ];
        for norm in norms {
            let json = serde_json::to_string(&norm).unwrap();
            assert_eq!(serde_json::from_str::<Norm>(&json).unwrap(), norm, "{json}");
        }
        assert_eq!(
            serde_json::to_string(&Norm::from(LogNorm::new(1.0, 10.0))).unwrap(),
            r#"{"kind":"log","params":[1.0,10.0]}"#
        );
        for bad in [
            r#"{"kind":"log","params":[1.0]}"#,
            r#"{"kind":"sym_log","params":[1,2,3,4]}"#,
            r#"{"kind":"boundary","params":[]}"#,
            r#"{"kind":"boundary","params":[2.5, 0, 1]}"#,
            r#"{"kind":"warp","params":[1, 2]}"#,
            r#"{"kind":"log","params":[1, 2],"extra":0}"#,
        ] {
            assert!(serde_json::from_str::<Norm>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn log_unclipped_marks_non_positive_values_bad() {
        let n = LogNorm::new(1.0, 100.0);
        assert!(n.normalize_unclipped(0.0).is_nan());
        assert!(n.normalize_unclipped(-3.0).is_nan());
        approx(n.normalize_unclipped(0.1), -0.5);
        approx(n.normalize_unclipped(1000.0), 1.5);
    }
}
