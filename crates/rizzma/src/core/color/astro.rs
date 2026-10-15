//! Astronomical image normalization: astropy.visualization's intervals and
//! stretches, combined in [`ImageNormalize`].
//!
//! FITS images span huge dynamic ranges, with a sky background a hair above
//! zero and stars thousands of times brighter, so a linear min/max scale shows
//! a black frame with a few white dots. astropy splits the cure in two: an
//! [`Interval`] picks the data range to show (IRAF's `zscale`, percentiles, or
//! the extremes), and a [`Stretch`] reshapes `[0, 1]` within it (square root,
//! logarithm, asinh, …). [`ImageNormalize`] chains them into a norm any
//! colormapped artist accepts:
//!
//! ```
//! use rizzma::Figure;
//! use rizzma::core::color::{ImageNormalize, Interval, Stretch};
//! let sky: Vec<f64> = (0..64).map(|i| 100.0 + f64::from(i % 7) + if i == 27 { 5e4 } else { 0.0 }).collect();
//! let mut fig = Figure::new(3.0, 3.0);
//! fig.add_subplot(1, 1, 1)
//!     .imshow(&sky, 8, 8)
//!     .cmap("gray")
//!     .norm(ImageNormalize::new(Interval::zscale(), Stretch::Asinh(0.1)));
//! assert!(!fig.encode_png().unwrap().is_empty());
//! ```
//!
//! Each piece is a port of astropy 8's code and is tested against it.

use super::Normalize;

/// How many knots a [`Stretch::histeq`] keeps: every finite sample of a small
/// image (so it matches astropy exactly), evenly spaced order statistics of a
/// large one.
const HISTEQ_KNOTS: usize = 4096;

/// A rule for the data range an image shows — astropy's `BaseInterval`
/// subclasses. Non-finite values are ignored, as astropy does.
#[derive(Debug, Clone, PartialEq)]
pub enum Interval {
    /// The finite extremes (`MinMaxInterval`).
    MinMax,
    /// The central `p` percent of the data (`PercentileInterval`).
    Percentile(f64),
    /// From the `lower` to the `upper` percentile
    /// (`AsymmetricPercentileInterval`).
    AsymmetricPercentile(f64, f64),
    /// IRAF's `zscale` (`ZScaleInterval`).
    ZScale(ZScaleInterval),
}

impl Interval {
    /// IRAF's `zscale` with astropy's default parameters.
    #[must_use]
    pub fn zscale() -> Self {
        Interval::ZScale(ZScaleInterval::default())
    }

    /// The `(vmin, vmax)` this interval picks for `data`, or `(0, 1)` when
    /// `data` has no finite value.
    #[must_use]
    pub fn limits(&self, data: &[f64]) -> (f64, f64) {
        let values: Vec<f64> = data.iter().copied().filter(|v| v.is_finite()).collect();
        if values.is_empty() {
            return (0.0, 1.0);
        }
        match self {
            Interval::MinMax => values
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                    (lo.min(v), hi.max(v))
                }),
            Interval::Percentile(p) => {
                let lower = (100.0 - p) / 2.0;
                percentiles(values, lower, 100.0 - lower)
            }
            Interval::AsymmetricPercentile(lower, upper) => percentiles(values, *lower, *upper),
            Interval::ZScale(z) => z.limits_of_finite(&values),
        }
    }
}

/// `numpy.percentile(values, [lower, upper])` with its default linear
/// interpolation.
fn percentiles(mut values: Vec<f64>, lower: f64, upper: f64) -> (f64, f64) {
    values.sort_by(f64::total_cmp);
    let at = |q: f64| {
        let pos = (q / 100.0).clamp(0.0, 1.0) * (values.len() - 1) as f64;
        let i = pos.floor() as usize;
        let t = pos - i as f64;
        match values.get(i + 1) {
            Some(&next) => lerp(values[i], next, t),
            None => values[i],
        }
    };
    (at(lower), at(upper))
}

/// numpy's `_lerp`, which interpolates from the nearer end for accuracy.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let d = b - a;
    if t >= 0.5 {
        b - d * (1.0 - t)
    } else {
        a + d * t
    }
}

/// IRAF's `zscale` interval: fit a line through the sorted samples of an
/// image, rejecting outliers, and show the range the line's central slope
/// covers, `contrast` times steeper.
///
/// A port of astropy's `ZScaleInterval`; the fields are its parameters, with
/// its defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct ZScaleInterval {
    /// How many points to sample from the image (default 1000).
    pub n_samples: usize,
    /// The slope divisor in `(0, 1]`; smaller shows more contrast (0.25).
    pub contrast: f64,
    /// Give up and show the extremes if more than this fraction of the
    /// samples is rejected (0.5).
    pub max_reject: f64,
    /// Give up if fewer samples than this survive (5).
    pub min_npixels: usize,
    /// The rejection threshold in standard deviations (2.5).
    pub krej: f64,
    /// The most rejection rounds (5).
    pub max_iterations: usize,
}

impl Default for ZScaleInterval {
    fn default() -> Self {
        Self {
            n_samples: 1000,
            contrast: 0.25,
            max_reject: 0.5,
            min_npixels: 5,
            krej: 2.5,
            max_iterations: 5,
        }
    }
}

impl ZScaleInterval {
    /// The limits for `values`, which are all finite and not empty.
    fn limits_of_finite(&self, values: &[f64]) -> (f64, f64) {
        let stride = ((values.len() as f64 / self.n_samples as f64).max(1.0)) as usize;
        let mut samples: Vec<f64> = values
            .iter()
            .step_by(stride)
            .take(self.n_samples)
            .copied()
            .collect();
        samples.sort_by(f64::total_cmp);
        let npix = samples.len();
        let (mut vmin, mut vmax) = (samples[0], samples[npix - 1]);

        let minpix = self
            .min_npixels
            .max((npix as f64 * self.max_reject) as usize);
        let ngrow = ((npix as f64 * 0.01) as usize).max(1);
        let mut bad = vec![false; npix];
        let (mut ngood, mut last_ngood) = (npix, npix + 1);
        let mut slope = 0.0;
        for _ in 0..self.max_iterations {
            if ngood >= last_ngood || ngood < minpix {
                break;
            }
            let (fit_slope, intercept) = fit_line(&samples, &bad);
            slope = fit_slope;
            let flat: Vec<f64> = samples
                .iter()
                .enumerate()
                .map(|(i, &s)| s - (slope * i as f64 + intercept))
                .collect();
            let threshold = self.krej * population_std(&flat, &bad);
            for (b, &f) in bad.iter_mut().zip(&flat) {
                if f < -threshold || f > threshold {
                    *b = true;
                }
            }
            bad = dilate(&bad, ngrow);
            last_ngood = ngood;
            ngood = bad.iter().filter(|&&b| !b).count();
        }

        if ngood >= minpix {
            if self.contrast > 0.0 {
                slope /= self.contrast;
            }
            let center = (npix - 1) / 2;
            let median = if npix % 2 == 1 {
                samples[npix / 2]
            } else {
                (samples[npix / 2 - 1] + samples[npix / 2]) / 2.0
            };
            vmin = vmin.max(median - (center as f64 - 1.0) * slope);
            vmax = vmax.min(median + (npix - center) as f64 * slope);
        }
        (vmin, vmax)
    }
}

/// Least-squares line `(slope, intercept)` through `(i, y[i])` for the points
/// not marked `bad` — numpy's `polyfit(x, y, 1, w=~bad)`.
fn fit_line(y: &[f64], bad: &[bool]) -> (f64, f64) {
    let good = || y.iter().enumerate().filter(|(i, _)| !bad[*i]);
    let n = good().count() as f64;
    let (sx, sy) = good().fold((0.0, 0.0), |(sx, sy), (i, &v)| (sx + i as f64, sy + v));
    let (mx, my) = (sx / n, sy / n);
    let (sxy, sxx) = good().fold((0.0, 0.0), |(sxy, sxx), (i, &v)| {
        let dx = i as f64 - mx;
        (sxy + dx * (v - my), sxx + dx * dx)
    });
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    (slope, my - slope * mx)
}

/// The population standard deviation of the values not marked `bad`.
fn population_std(values: &[f64], bad: &[bool]) -> f64 {
    let good: Vec<f64> = values
        .iter()
        .zip(bad)
        .filter(|(_, b)| !**b)
        .map(|(&v, _)| v)
        .collect();
    let n = good.len() as f64;
    let mean = good.iter().sum::<f64>() / n;
    (good.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt()
}

/// Grow every `true` of `mask` over a window `width` wide — numpy's
/// `convolve(mask, ones(width), mode="same")` on booleans.
fn dilate(mask: &[bool], width: usize) -> Vec<bool> {
    let lead = (width - 1) / 2;
    (0..mask.len())
        .map(|i| {
            let hi = (i + lead).min(mask.len() - 1);
            let lo = (i + lead + 1).saturating_sub(width);
            mask[lo..=hi].iter().any(|&b| b)
        })
        .collect()
}

/// A reshaping of `[0, 1]` onto itself — astropy's `BaseStretch` subclasses,
/// applied after an [`Interval`] has mapped the data to `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stretch {
    /// `slope · x + intercept` (`LinearStretch`).
    Linear {
        /// The multiplier.
        slope: f64,
        /// The offset.
        intercept: f64,
    },
    /// `√x` (`SqrtStretch`).
    Sqrt,
    /// `xᵃ` (`PowerStretch`).
    Power(f64),
    /// `log(a·x + 1) / log(a + 1)` (`LogStretch`; astropy's default `a` is
    /// 1000).
    Log(f64),
    /// `asinh(x / a) / asinh(1 / a)` (`AsinhStretch`; default `a` 0.1).
    Asinh(f64),
    /// `sinh(x / a) / sinh(1 / a)` (`SinhStretch`; default `a` 1/3).
    Sinh(f64),
    /// Histogram equalization (`HistEqStretch`): `x` maps to the fraction of
    /// the data at or below it. Build with [`Stretch::histeq`].
    HistEq {
        /// The data's order statistics, normalized to `[0, 1]`.
        knots: Vec<f64>,
        /// The fraction of the data below each knot.
        values: Vec<f64>,
    },
}

impl Stretch {
    /// The identity stretch.
    #[must_use]
    pub fn linear() -> Self {
        Stretch::Linear {
            slope: 1.0,
            intercept: 0.0,
        }
    }

    /// A histogram-equalization stretch for `data`, as
    /// `HistEqStretch(data)` builds it: the sorted finite data normalized to
    /// `[0, 1]`, each sample at its rank. Beyond 4096 samples the knots are
    /// evenly spaced order statistics, which shifts the curve by under 1/4096.
    #[must_use]
    pub fn histeq(data: &[f64]) -> Self {
        let mut sorted: Vec<f64> = data.iter().copied().filter(|v| v.is_finite()).collect();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        if n < 2 {
            return Stretch::HistEq {
                knots: vec![0.0, 1.0],
                values: vec![0.0, 1.0],
            };
        }
        let (lo, hi) = (sorted[0], sorted[n - 1]);
        let span = if hi > lo { hi - lo } else { 1.0 };
        let picks: Vec<usize> = if n <= HISTEQ_KNOTS {
            (0..n).collect()
        } else {
            (0..HISTEQ_KNOTS)
                .map(|k| ((k as f64) * (n - 1) as f64 / (HISTEQ_KNOTS - 1) as f64).round() as usize)
                .collect()
        };
        Stretch::HistEq {
            knots: picks.iter().map(|&i| (sorted[i] - lo) / span).collect(),
            values: picks.iter().map(|&i| i as f64 / (n - 1) as f64).collect(),
        }
    }

    /// The stretch of `x`, without clipping; `NaN` where it is undefined
    /// (a square root of a negative, say).
    #[must_use]
    pub fn forward(&self, x: f64) -> f64 {
        match self {
            Stretch::Linear { slope, intercept } => slope * x + intercept,
            Stretch::Sqrt => x.sqrt(),
            Stretch::Power(a) => x.powf(*a),
            Stretch::Log(a) => (a * x + 1.0).ln() / (a + 1.0).ln(),
            Stretch::Asinh(a) => (x / a).asinh() / (1.0 / a).asinh(),
            Stretch::Sinh(a) => (x / a).sinh() / (1.0 / a).sinh(),
            Stretch::HistEq { knots, values } => interp(x, knots, values),
        }
    }

    /// The inverse of [`forward`](Stretch::forward).
    #[must_use]
    pub fn inverse(&self, y: f64) -> f64 {
        match self {
            Stretch::Linear { slope, intercept } => (y - intercept) / slope,
            Stretch::Sqrt => y * y,
            Stretch::Power(a) => y.powf(1.0 / a),
            Stretch::Log(a) => ((a + 1.0).powf(y) - 1.0) / a,
            Stretch::Asinh(a) => a * (y * (1.0 / a).asinh()).sinh(),
            Stretch::Sinh(a) => a * (y * (1.0 / a).sinh()).asinh(),
            Stretch::HistEq { knots, values } => interp(y, values, knots),
        }
    }
}

/// `numpy.interp(x, xp, fp)`: piecewise linear through `(xp, fp)`, holding
/// the end values outside, taking the last of tied knots.
fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let n = xp.len();
    if x.is_nan() || n == 0 {
        return f64::NAN;
    }
    if x < xp[0] {
        return fp[0];
    }
    if x >= xp[n - 1] {
        return fp[n - 1];
    }
    let j = xp.partition_point(|&k| k <= x) - 1;
    let slope = (fp[j + 1] - fp[j]) / (xp[j + 1] - xp[j]);
    fp[j] + slope * (x - xp[j])
}

/// An [`Interval`] then a [`Stretch`]: astropy's `ImageNormalize`, as a norm.
///
/// ![astronomical normalization](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_astro_normalize.png)
///
/// The interval needs the data, so the norm fixes its limits when it meets
/// them: when set on an image, or resolved by a
/// [`ColorMapping`](super::ColorMapping) — matplotlib's `autoscale_None`. Data
/// below the limits, or stretched to an undefined value, lands below `0` and
/// takes the under color, as astropy's `invalid=-1` arranges.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageNormalize {
    interval: Interval,
    stretch: Stretch,
    limits: Option<(f64, f64)>,
}

impl ImageNormalize {
    /// Normalize through `interval` (resolved against the data it is applied
    /// to) and then `stretch`.
    #[must_use]
    pub fn new(interval: Interval, stretch: Stretch) -> Self {
        Self {
            interval,
            stretch,
            limits: None,
        }
    }

    /// Normalize between fixed limits, then through `stretch`.
    #[must_use]
    pub fn with_limits(vmin: f64, vmax: f64, stretch: Stretch) -> Self {
        Self {
            interval: Interval::MinMax,
            stretch,
            limits: Some((vmin, vmax)),
        }
    }

    /// This norm with its limits fixed from `data` if they are not yet.
    #[must_use]
    pub fn autoscaled(&self, data: &[f64]) -> Self {
        let mut norm = self.clone();
        if norm.limits.is_none() {
            norm.limits = Some(self.interval.limits(data));
        }
        norm
    }

    /// The `(vmin, vmax)` limits, once fixed.
    #[must_use]
    pub fn limits(&self) -> Option<(f64, f64)> {
        self.limits
    }

    /// The stretch.
    #[must_use]
    pub fn stretch(&self) -> &Stretch {
        &self.stretch
    }
}

impl Normalize for ImageNormalize {
    fn normalize(&self, value: f64) -> f64 {
        self.normalize_unclipped(value).clamp(0.0, 1.0)
    }

    fn normalize_unclipped(&self, value: f64) -> f64 {
        let Some((vmin, vmax)) = self.limits else {
            return f64::NAN;
        };
        if value.is_nan() {
            return f64::NAN;
        }
        let x = if vmax == vmin {
            0.0
        } else {
            (value - vmin) / (vmax - vmin)
        };
        let y = self.stretch.forward(x);
        if y.is_nan() { -1.0 } else { y }
    }

    fn inverse(&self, t: f64) -> f64 {
        let (vmin, vmax) = self.limits.unwrap_or((0.0, 1.0));
        self.stretch.inverse(t.clamp(0.0, 1.0)) * (vmax - vmin) + vmin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dilate_matches_numpy_same_convolution() {
        let m = [false, false, true, false, false, false, true];
        // numpy.convolve(m, ones(3), "same") > 0
        assert_eq!(dilate(&m, 3), [false, true, true, true, false, true, true]);
        // An even kernel leans left: ones(2) "same" covers [i - 1, i].
        assert_eq!(
            dilate(&m, 2),
            [false, false, true, true, false, false, true]
        );
        assert_eq!(dilate(&m, 1), m);
    }

    #[test]
    fn interp_holds_the_ends_and_takes_the_last_tied_knot() {
        let (xp, fp) = ([0.0, 0.5, 0.5, 1.0], [0.0, 0.2, 0.6, 1.0]);
        assert_eq!(interp(-1.0, &xp, &fp), 0.0);
        assert_eq!(interp(2.0, &xp, &fp), 1.0);
        assert_eq!(interp(0.5, &xp, &fp), 0.6);
        assert!((interp(0.25, &xp, &fp) - 0.1).abs() < 1e-15);
    }

    #[test]
    fn stretches_invert() {
        let stretches = [
            Stretch::Linear {
                slope: 0.8,
                intercept: 0.1,
            },
            Stretch::Sqrt,
            Stretch::Power(2.5),
            Stretch::Log(1000.0),
            Stretch::Asinh(0.1),
            Stretch::Sinh(1.0 / 3.0),
            Stretch::histeq(&[3.0, 1.0, 4.0, 1.5, 9.0, 2.6, 5.0]),
        ];
        for s in stretches {
            for k in 0..=50 {
                let x = f64::from(k) / 50.0;
                let back = s.inverse(s.forward(x));
                assert!((back - x).abs() < 1e-9, "{s:?} at {x}: {back}");
            }
        }
    }

    #[test]
    fn image_normalize_sends_undefined_stretches_under() {
        let norm = ImageNormalize::with_limits(10.0, 20.0, Stretch::Sqrt);
        assert_eq!(norm.normalize_unclipped(5.0), -1.0);
        assert!((norm.normalize_unclipped(12.5) - 0.5).abs() < 1e-15);
        assert!(norm.normalize_unclipped(f64::NAN).is_nan());
        let unresolved = ImageNormalize::new(Interval::zscale(), Stretch::Sqrt);
        assert!(unresolved.normalize_unclipped(12.5).is_nan());
        assert_eq!(
            unresolved.autoscaled(&[10.0, 20.0]).limits(),
            Some((10.0, 20.0))
        );
    }
}
