//! Tick locators and formatters, ported from matplotlib's `ticker.py`.
//!
//! This module implements the tick-location and label-formatting engine used by
//! axes. The algorithms are faithful ports of matplotlib's reference
//! implementation (`lib/matplotlib/ticker.py`), in particular the nice-number
//! [`MaxNLocator`]. The intent is bit-for-bit parity with matplotlib's tick
//! vectors, including matplotlib's reliance on floating-point floor division and
//! the inclusive-edge epsilon fudges.
//!
//! Everything is pure `f64`; there are no external dependencies and no `unsafe`.
//! Unicode-minus handling is out of scope, so negative numbers use an ASCII
//! hyphen-minus.
//!
//! # Locators
//!
//! A [`Locator`] maps a `(vmin, vmax)` view interval to a vector of tick
//! positions. Implemented locators:
//!
//! - [`MaxNLocator`] — the nice-number locator (default of most axes).
//! - [`AutoLocator`] — a [`MaxNLocator`] preset (`nbins = auto`,
//!   `steps = [1, 2, 2.5, 5, 10]`).
//! - [`MultipleLocator`] — ticks at integer multiples of a base.
//! - [`LinearLocator`] — evenly spaced ticks via linspace.
//! - [`FixedLocator`] — a fixed set of positions (optionally subsampled).
//! - [`LogLocator`] — logarithmic ticks at powers of a base, optionally with
//!   subticks.
//! - [`SymlogLocator`] — symmetric-log ticks with a linear region around zero.
//! - [`LogitLocator`] — probability ticks clustered toward zero and one.
//! - [`NullLocator`] — no ticks.
//!
//! # Formatters
//!
//! A [`Formatter`] turns a tick value (and optional position index) into a
//! label string. Implemented formatters:
//!
//! - [`ScalarFormatter`] — picks significant figures from the tick spacing.
//! - [`LogFormatter`] — labels logarithmic major ticks.
//! - [`LogFormatterMathtext`] — labels large logarithmic powers as mathtext.
//! - [`SymlogFormatter`] — labels symmetric-log ticks across zero.
//! - [`SymlogFormatterMathtext`] — labels large symmetric-log powers as mathtext.
//! - [`LogitFormatter`] — labels probability ticks on a logit scale.
//! - [`LogitFormatterMathtext`] — labels logit probability tails as mathtext.
//! - [`EngFormatter`] — labels values with SI engineering prefixes.
//! - [`PercentFormatter`] — labels values as percentages.
//! - [`NullFormatter`] — always the empty string.
//! - [`FixedFormatter`] — fixed strings indexed by position.
//! - [`IndexFormatter`] — fixed strings indexed by rounded tick value.
//! - [`FuncFormatter`] — a user-supplied boxed closure.
//! - [`StrMethodFormatter`] — a `{x}`/`{pos}` template string.

/// Determine tick locations for an axis.
///
/// This is the Rust analogue of matplotlib's `Locator` base class. The core
/// method is [`Locator::tick_values`]; [`Locator::view_limits`] optionally
/// adjusts the view interval and defaults to the identity.
pub trait Locator {
    /// Return the tick positions for the closed view interval `[vmin, vmax]`.
    ///
    /// Locations slightly beyond the limits may be included to support
    /// autoscaling, matching matplotlib's behaviour.
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64>;

    /// Select a view interval for the data range `[vmin, vmax]`.
    ///
    /// The default implementation returns the range unchanged.
    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        (vmin, vmax)
    }
}

/// Format a tick value into a label string.
///
/// This is the Rust analogue of matplotlib's `Formatter` base class. `pos` is
/// the index of the tick among the visible ticks (matplotlib passes `None` in
/// some contexts), and is used by position-based formatters such as
/// [`FixedFormatter`].
pub trait Formatter {
    /// Return the label for `value` at the optional position index `pos`.
    fn format(&self, value: f64, pos: Option<usize>) -> String;

    /// Return labels for a whole tick vector.
    ///
    /// Most formatters can format each value independently. Context-aware
    /// formatters, such as concise date labels, override this method.
    fn format_ticks(&self, values: &[f64]) -> Vec<String> {
        values
            .iter()
            .enumerate()
            .map(|(i, &value)| self.format(value, Some(i)))
            .collect()
    }
}

/// Python-style floor division (`a // b`).
///
/// Rust's `/` on `f64` truncates toward zero only after the fact via `floor`;
/// Python's `//` floors the quotient. Matplotlib relies on floor semantics
/// (e.g. for negative `vmin`), so we replicate it explicitly.
fn floordiv(a: f64, b: f64) -> f64 {
    (a / b).floor()
}

/// Python-style `divmod(x, step)` returning `(floor(x/step), x - floor*step)`.
fn divmod(x: f64, step: f64) -> (f64, f64) {
    let d = floordiv(x, step);
    (d, x - d * step)
}

/// matplotlib `transforms._nonsingular`: expand/swap a range to avoid
/// singularities.
///
/// Mirrors the reference logic: non-finite inputs collapse to
/// `(-expander, expander)`; reversed inputs are swapped (when `increasing`);
/// and intervals that are too small relative to their magnitude are expanded.
fn nonsingular(
    mut vmin: f64,
    mut vmax: f64,
    expander: f64,
    tiny: f64,
    increasing: bool,
) -> (f64, f64) {
    if !vmin.is_finite() || !vmax.is_finite() {
        return (-expander, expander);
    }

    let mut swapped = false;
    if vmax < vmin {
        std::mem::swap(&mut vmin, &mut vmax);
        swapped = true;
    }

    let maxabsvalue = vmin.abs().max(vmax.abs());
    // `(1e6 / tiny) * f64::MIN_POSITIVE` reproduces matplotlib's
    // `(1e6 / tiny) * np.finfo(float).tiny`.
    if maxabsvalue < (1e6 / tiny) * f64::MIN_POSITIVE {
        vmin = -expander;
        vmax = expander;
    } else if vmax - vmin <= maxabsvalue * tiny {
        vmin -= expander * maxabsvalue;
        vmax += expander * maxabsvalue;
    }

    if swapped && !increasing {
        std::mem::swap(&mut vmin, &mut vmax);
    }
    (vmin, vmax)
}

/// matplotlib `ticker.scale_range`: pick a power-of-ten scale and offset.
///
/// Returns `(scale, offset)` where `scale` is `10**floor(log10(dv / n))` and
/// `offset` is a power-of-ten offset used when the mean dwarfs the span.
fn scale_range(vmin: f64, vmax: f64, n: f64, threshold: f64) -> (f64, f64) {
    let dv = (vmax - vmin).abs();
    let meanv = (vmax + vmin) / 2.0;
    let offset = if dv == 0.0 || meanv.abs() / dv < threshold {
        0.0
    } else {
        (10f64.powf(floordiv(meanv.abs().log10(), 1.0))).copysign(meanv)
    };
    let scale = 10f64.powf(floordiv((dv / n).log10(), 1.0));
    (scale, offset)
}

/// Helper that computes integer multiples of a step with float-precision slop.
///
/// Port of matplotlib's `_Edge_integer`. Used by [`MaxNLocator`] and
/// [`MultipleLocator`] to find the smallest/largest integer `n` such that
/// `n * step` brackets a value, accounting for floating-point error.
#[derive(Clone, Copy, Debug)]
struct EdgeInteger {
    step: f64,
    offset: f64,
}

impl EdgeInteger {
    /// Create an edge helper for `step` with an absolute `offset`.
    ///
    /// `step` must be positive (matplotlib raises otherwise).
    fn new(step: f64, offset: f64) -> Self {
        debug_assert!(step > 0.0, "'step' must be positive");
        EdgeInteger {
            step,
            offset: offset.abs(),
        }
    }

    /// Whether `ms` is within tolerance of the integer `edge`.
    ///
    /// Tolerance widens when the offset is large relative to the step, exactly
    /// as in matplotlib.
    fn closeto(&self, ms: f64, edge: f64) -> bool {
        let tol = if self.offset > 0.0 {
            let digits = (self.offset / self.step).log10();
            (10f64.powf(digits - 12.0)).clamp(1e-10, 0.4999)
        } else {
            1e-10
        };
        (ms - edge).abs() < tol
    }

    /// Largest `n` such that `n * step <= x`.
    fn le(&self, x: f64) -> f64 {
        let (d, m) = divmod(x, self.step);
        if self.closeto(m / self.step, 1.0) {
            d + 1.0
        } else {
            d
        }
    }

    /// Smallest `n` such that `n * step >= x`.
    fn ge(&self, x: f64) -> f64 {
        let (d, m) = divmod(x, self.step);
        if self.closeto(m / self.step, 0.0) {
            d
        } else {
            d + 1.0
        }
    }
}

/// How many bins (intervals) a [`MaxNLocator`] should use.
///
/// `Auto` mirrors matplotlib's `nbins='auto'`; with no axis attached it
/// resolves to 9, matching the reference.
#[derive(Clone, Copy, Debug)]
pub enum NBins {
    /// A fixed maximum number of intervals (one fewer than the max tick count).
    Fixed(usize),
    /// Automatic: resolves to 9 in the axis-less case used here.
    Auto,
}

/// Nice-number tick locator: evenly spaced ticks capped at `nbins + 1` ticks.
///
/// Faithful port of matplotlib's `MaxNLocator`. It finds "nice" tick locations
/// (integer multiples of `1, 2, 2.5, 5, ...` scaled by a power of ten) with no
/// more than `nbins + 1` ticks within the view limits, adding edge ticks beyond
/// the limits to support autoscaling.
pub struct MaxNLocator {
    nbins: NBins,
    /// The extended staircase of candidate step mantissas, already including the
    /// 0.1× and 10× wrap-around entries.
    extended_steps: Vec<f64>,
    integer: bool,
    symmetric: bool,
    min_n_ticks: usize,
}

impl MaxNLocator {
    /// matplotlib's default `steps=None` expands to this mantissa sequence.
    const DEFAULT_STEPS: [f64; 10] = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0];

    /// Construct a `MaxNLocator` with the given bin count and default steps.
    ///
    /// Equivalent to matplotlib's `MaxNLocator(nbins=...)` with `steps=None`,
    /// `integer=False`, `symmetric=False`, `min_n_ticks=2`.
    pub fn new(nbins: NBins) -> Self {
        Self::with_steps(nbins, &Self::DEFAULT_STEPS, false, false, 2)
    }

    /// Construct a `MaxNLocator` with full control over its parameters.
    ///
    /// `steps` is validated and normalised (prepending `1` / appending `10`)
    /// just like matplotlib's `_validate_steps`, then expanded into the
    /// extended staircase used during location.
    pub fn with_steps(
        nbins: NBins,
        steps: &[f64],
        integer: bool,
        symmetric: bool,
        min_n_ticks: usize,
    ) -> Self {
        let steps = Self::validate_steps(steps);
        let extended_steps = Self::staircase(&steps);
        MaxNLocator {
            nbins,
            extended_steps,
            integer,
            symmetric,
            min_n_ticks: min_n_ticks.max(1),
        }
    }

    /// Validate and normalise a `steps` sequence (port of `_validate_steps`).
    ///
    /// Requires a strictly increasing sequence within `[1, 10]`; prepends `1`
    /// and/or appends `10` if missing.
    fn validate_steps(steps: &[f64]) -> Vec<f64> {
        assert!(!steps.is_empty(), "steps must be non-empty");
        for w in steps.windows(2) {
            assert!(
                w[1] - w[0] > 0.0,
                "steps argument must be an increasing sequence between 1 and 10"
            );
        }
        assert!(
            *steps.last().unwrap() <= 10.0 && steps[0] >= 1.0,
            "steps argument must be an increasing sequence between 1 and 10"
        );
        let mut v: Vec<f64> = Vec::with_capacity(steps.len() + 2);
        if steps[0] != 1.0 {
            v.push(1.0);
        }
        v.extend_from_slice(steps);
        if *steps.last().unwrap() != 10.0 {
            v.push(10.0);
        }
        v
    }

    /// Build the extended staircase (port of `_staircase`).
    ///
    /// `concat(0.1 * steps[:-1], steps, [10 * steps[1]])`.
    fn staircase(steps: &[f64]) -> Vec<f64> {
        let mut out = Vec::with_capacity(steps.len() * 2);
        for &s in &steps[..steps.len() - 1] {
            out.push(0.1 * s);
        }
        out.extend_from_slice(steps);
        out.push(10.0 * steps[1]);
        out
    }

    /// Resolve `nbins` to a concrete count (axis-less case: `Auto` -> 9).
    fn nbins(&self) -> f64 {
        match self.nbins {
            NBins::Fixed(n) => n as f64,
            NBins::Auto => 9.0,
        }
    }

    /// Generate raw tick locations spanning `[vmin, vmax]` (port of `_raw_ticks`).
    ///
    /// May include one tick on either side of the range; those are trimmed by
    /// `tick_values` only via pruning (not implemented here, matching the
    /// default `prune=None`).
    fn raw_ticks(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        let nbins = self.nbins();
        let (scale, offset) = scale_range(vmin, vmax, nbins, 100.0);
        let vmin_o = vmin - offset;
        let vmax_o = vmax - offset;

        let mut steps: Vec<f64> = self.extended_steps.iter().map(|&s| s * scale).collect();
        if self.integer {
            // Keep steps < 1 or steps that are (near) integers.
            steps.retain(|&s| s < 1.0 || (s - s.round()).abs() < 0.001);
        }

        let raw_step = (vmax_o - vmin_o) / nbins;
        let large_steps: Vec<bool> = steps.iter().map(|&s| s >= raw_step).collect();

        // Index of the smallest "large" step (>= raw_step), or the last step.
        let istep = large_steps
            .iter()
            .position(|&b| b)
            .unwrap_or(steps.len() - 1);

        // Start at the smallest of the large steps and walk backwards until a
        // step yields at least `min_n_ticks` displayed ticks.
        let mut ticks: Vec<f64> = Vec::new();
        for &step0 in steps[..=istep].iter().rev() {
            let mut step = step0;
            if self.integer && (vmax_o.floor() - vmin_o.ceil()) >= (self.min_n_ticks as f64 - 1.0) {
                step = step.max(1.0);
            }
            let best_vmin = floordiv(vmin_o, step) * step;

            let edge = EdgeInteger::new(step, offset);
            let low = edge.le(vmin_o - best_vmin);
            let high = edge.ge(vmax_o - best_vmin);

            let n = (high - low) as i64;
            ticks = (0..=n)
                .map(|i| (low + i as f64) * step + best_vmin)
                .collect();

            let nticks = ticks
                .iter()
                .filter(|&&t| t <= vmax_o && t >= vmin_o)
                .count();
            if nticks >= self.min_n_ticks {
                break;
            }
        }

        ticks.iter().map(|&t| t + offset).collect()
    }
}

impl Locator for MaxNLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        let (mut vmin, mut vmax) = (vmin, vmax);
        if self.symmetric {
            vmax = vmin.abs().max(vmax.abs());
            vmin = -vmax;
        }
        let (vmin, vmax) = nonsingular(vmin, vmax, 1e-13, 1e-14, true);
        self.raw_ticks(vmin, vmax)
    }

    fn view_limits(&self, dmin: f64, dmax: f64) -> (f64, f64) {
        let (mut dmin, mut dmax) = (dmin, dmax);
        if self.symmetric {
            dmax = dmin.abs().max(dmax.abs());
            dmin = -dmax;
        }
        // Default autolimit mode is not 'round_numbers', so return as-is after
        // the singularity guard.
        nonsingular(dmin, dmax, 1e-12, 1e-13, true)
    }
}

impl Default for MaxNLocator {
    /// matplotlib's `MaxNLocator()` default: `nbins=10`, default steps.
    fn default() -> Self {
        Self::new(NBins::Fixed(10))
    }
}

/// Automatic locator: a [`MaxNLocator`] preset (`nbins=auto`, nice steps).
///
/// Port of matplotlib's `AutoLocator`, which is `MaxNLocator(nbins='auto',
/// steps=[1, 2, 2.5, 5, 10])` in modern (non-classic) mode.
pub struct AutoLocator {
    inner: MaxNLocator,
}

impl AutoLocator {
    /// Construct the default `AutoLocator`.
    pub fn new() -> Self {
        AutoLocator {
            inner: MaxNLocator::with_steps(
                NBins::Auto,
                &[1.0, 2.0, 2.5, 5.0, 10.0],
                false,
                false,
                2,
            ),
        }
    }
}

impl Default for AutoLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Locator for AutoLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        self.inner.tick_values(vmin, vmax)
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        self.inner.view_limits(vmin, vmax)
    }
}

/// Place ticks at every integer multiple of a base (plus an offset).
///
/// Port of matplotlib's `MultipleLocator`.
pub struct MultipleLocator {
    edge: EdgeInteger,
    offset: f64,
}

impl MultipleLocator {
    /// Create a locator with the given positive `base` and zero offset.
    pub fn new(base: f64) -> Self {
        Self::with_offset(base, 0.0)
    }

    /// Create a locator with the given positive `base` and additive `offset`.
    pub fn with_offset(base: f64, offset: f64) -> Self {
        MultipleLocator {
            edge: EdgeInteger::new(base, 0.0),
            offset,
        }
    }
}

impl Locator for MultipleLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        let (mut vmin, mut vmax) = (vmin, vmax);
        if vmax < vmin {
            std::mem::swap(&mut vmin, &mut vmax);
        }
        let step = self.edge.step;
        vmin -= self.offset;
        vmax -= self.offset;
        let vmin = self.edge.ge(vmin) * step;
        let n = floordiv(vmax - vmin + 0.001 * step, step);
        let count = n as i64 + 3;
        (0..count)
            .map(|i| vmin - step + i as f64 * step + self.offset)
            .collect()
    }

    fn view_limits(&self, dmin: f64, dmax: f64) -> (f64, f64) {
        // Default autolimit mode ('data'): return the data range unchanged
        // after the singularity guard.
        nonsingular(dmin, dmax, 0.001, 1e-15, true)
    }
}

/// Place `numticks` evenly spaced ticks across the view interval.
///
/// Port of matplotlib's `LinearLocator` (without the interactive preset cache,
/// which is irrelevant for stateless tick computation).
pub struct LinearLocator {
    numticks: usize,
}

impl LinearLocator {
    /// Create a locator with the given number of ticks.
    pub fn new(numticks: usize) -> Self {
        LinearLocator { numticks }
    }
}

impl Default for LinearLocator {
    /// matplotlib's default of 11 ticks.
    fn default() -> Self {
        LinearLocator { numticks: 11 }
    }
}

impl Locator for LinearLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        let (vmin, vmax) = nonsingular(vmin, vmax, 0.05, 1e-15, true);
        if self.numticks == 0 {
            return Vec::new();
        }
        if self.numticks == 1 {
            return vec![vmin];
        }
        let n = self.numticks - 1;
        // np.linspace: endpoints are exact; interior points are vmin + i*step.
        let step = (vmax - vmin) / n as f64;
        let mut out: Vec<f64> = (0..self.numticks).map(|i| vmin + i as f64 * step).collect();
        // Match numpy: force the final endpoint exactly.
        *out.last_mut().unwrap() = vmax;
        out
    }
}

/// Place ticks at a fixed set of positions, optionally subsampled to `nbins+1`.
///
/// Port of matplotlib's `FixedLocator`.
pub struct FixedLocator {
    locs: Vec<f64>,
    nbins: Option<usize>,
}

impl FixedLocator {
    /// Create a locator at exactly the given positions (no subsampling).
    pub fn new(locs: Vec<f64>) -> Self {
        FixedLocator { locs, nbins: None }
    }

    /// Create a locator that subsamples `locs` to at most `nbins + 1` ticks.
    ///
    /// `nbins` is clamped to a minimum of 2, as in matplotlib.
    pub fn with_nbins(locs: Vec<f64>, nbins: usize) -> Self {
        FixedLocator {
            locs,
            nbins: Some(nbins.max(2)),
        }
    }
}

impl Locator for FixedLocator {
    fn tick_values(&self, _vmin: f64, _vmax: f64) -> Vec<f64> {
        let nbins = match self.nbins {
            None => return self.locs.clone(),
            Some(n) => n,
        };
        let len = self.locs.len();
        if len == 0 {
            return Vec::new();
        }
        // step = max(ceil(len / nbins), 1)
        let step = ((len as f64 / nbins as f64).ceil() as usize).max(1);

        // Candidate sub-sequence starting at offset 0.
        let subsample = |start: usize| -> Vec<f64> {
            self.locs[start..].iter().step_by(step).copied().collect()
        };
        let min_abs = |v: &[f64]| v.iter().map(|x| x.abs()).fold(f64::INFINITY, f64::min);

        let mut ticks = subsample(0);
        for i in 1..step {
            let ticks1 = subsample(i);
            if !ticks1.is_empty() && min_abs(&ticks1) < min_abs(&ticks) {
                ticks = ticks1;
            }
        }
        ticks
    }
}

/// Place ticks on a logarithmic axis.
///
/// Major ticks are powers of `base`. Minor ticks can be requested by setting
/// `subs` to multiples within each decade, for example `[2, 3, ..., 9]` for
/// the usual base-10 minor ticks. The default locator produces major ticks
/// only; [`LogLocator::minor`] constructs the common minor-tick locator.
#[derive(Clone, Debug, PartialEq)]
pub struct LogLocator {
    base: f64,
    subs: Vec<f64>,
}

impl LogLocator {
    /// Construct a major-tick log locator with the given base.
    ///
    /// `base` must be finite and greater than one.
    pub fn new(base: f64) -> Self {
        Self::with_subs(base, vec![1.0])
    }

    /// Construct a log locator with explicit subtick multiples.
    ///
    /// Values in `subs` are retained only when finite and in `[1, base)`;
    /// duplicates are removed after sorting. Use `[1.0]` for major ticks.
    pub fn with_subs(base: f64, subs: Vec<f64>) -> Self {
        assert!(
            base.is_finite() && base > 1.0,
            "log locator base must be finite and > 1"
        );
        let mut subs: Vec<f64> = subs
            .into_iter()
            .filter(|s| s.is_finite() && *s >= 1.0 && *s < base)
            .collect();
        subs.sort_by(|a, b| a.partial_cmp(b).expect("finite subs are comparable"));
        subs.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        if subs.is_empty() {
            subs.push(1.0);
        }
        LogLocator { base, subs }
    }

    /// Construct the common minor-tick locator.
    ///
    /// For base 10 this yields subticks at `2..=9` times each decade.
    pub fn minor(base: f64) -> Self {
        let upper = base.floor() as i32;
        let subs = if upper > 2 {
            (2..upper).map(f64::from).collect()
        } else {
            Vec::new()
        };
        Self::with_subs(base, subs)
    }

    /// Return this locator's base.
    #[must_use]
    pub fn base(&self) -> f64 {
        self.base
    }

    /// Return this locator's decade multiples.
    #[must_use]
    pub fn subs(&self) -> &[f64] {
        &self.subs
    }
}

impl Default for LogLocator {
    /// Default base-10 major locator.
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl Locator for LogLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        if !vmin.is_finite() || !vmax.is_finite() || vmin <= 0.0 || vmax <= 0.0 {
            return Vec::new();
        }

        let (lo, hi) = if vmin <= vmax {
            (vmin, vmax)
        } else {
            (vmax, vmin)
        };
        let log_base = self.base.ln();
        let start = (lo.ln() / log_base).floor() as i32;
        let end = (hi.ln() / log_base).ceil() as i32;
        let mut ticks = Vec::new();

        for exponent in start..=end {
            let decade = self.base.powi(exponent);
            for &sub in &self.subs {
                let tick = sub * decade;
                if tick >= lo * (1.0 - 1e-12) && tick <= hi * (1.0 + 1e-12) {
                    ticks.push(tick);
                }
            }
        }

        ticks.sort_by(|a, b| a.partial_cmp(b).expect("finite ticks are comparable"));
        ticks.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0));
        if vmin > vmax {
            ticks.reverse();
        }
        ticks
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        if !vmin.is_finite() || !vmax.is_finite() || vmin <= 0.0 || vmax <= 0.0 {
            return (1.0, self.base);
        }
        let (lo, hi, reversed) = if vmin <= vmax {
            (vmin, vmax, false)
        } else {
            (vmax, vmin, true)
        };
        let log_base = self.base.ln();
        let lower = self.base.powi((lo.ln() / log_base).floor() as i32);
        let upper = self.base.powi((hi.ln() / log_base).ceil() as i32);
        if reversed {
            (upper, lower)
        } else {
            (lower, upper)
        }
    }
}

/// Place ticks on a symmetric-log axis.
///
/// Ticks are generated in three bands: negative logarithmic tail, a linear
/// region spanning `[-linthresh, linthresh]`, and positive logarithmic tail.
/// This mirrors the structure of matplotlib's symlog scale while remaining
/// independent of any concrete axis wiring.
#[derive(Clone, Debug, PartialEq)]
pub struct SymlogLocator {
    base: f64,
    linthresh: f64,
    linear_ticks: usize,
}

impl SymlogLocator {
    /// Construct a symlog locator.
    ///
    /// `base` must be finite and greater than one; `linthresh` must be finite
    /// and positive. The linear region defaults to ticks at
    /// `-linthresh`, `0`, and `linthresh`.
    pub fn new(base: f64, linthresh: f64) -> Self {
        Self::with_linear_ticks(base, linthresh, 3)
    }

    /// Construct a symlog locator with explicit linear-region tick count.
    ///
    /// `linear_ticks` is clamped to at least 2 so both edges of the linear
    /// region can be represented when visible.
    pub fn with_linear_ticks(base: f64, linthresh: f64, linear_ticks: usize) -> Self {
        assert!(
            base.is_finite() && base > 1.0,
            "symlog locator base must be finite and > 1"
        );
        assert!(
            linthresh.is_finite() && linthresh > 0.0,
            "symlog locator linthresh must be finite and > 0"
        );
        SymlogLocator {
            base,
            linthresh,
            linear_ticks: linear_ticks.max(2),
        }
    }

    /// Return this locator's logarithm base.
    #[must_use]
    pub fn base(&self) -> f64 {
        self.base
    }

    /// Return this locator's half-width of the linear region around zero.
    #[must_use]
    pub fn linthresh(&self) -> f64 {
        self.linthresh
    }
}

impl Default for SymlogLocator {
    /// Default base-10 locator with `linthresh = 1`.
    fn default() -> Self {
        Self::new(10.0, 1.0)
    }
}

impl Locator for SymlogLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        if !vmin.is_finite() || !vmax.is_finite() {
            return Vec::new();
        }

        let (lo, hi, reversed) = if vmin <= vmax {
            (vmin, vmax, false)
        } else {
            (vmax, vmin, true)
        };
        let mut ticks = Vec::new();

        if lo < -self.linthresh {
            let max_abs = (-lo).max(self.linthresh);
            let max_exp = (max_abs / self.linthresh).log(self.base).ceil() as i32;
            for exponent in (1..=max_exp).rev() {
                let tick = -self.linthresh * self.base.powi(exponent);
                if tick >= lo * (1.0 + 1e-12) && tick <= hi * (1.0 - 1e-12) {
                    ticks.push(tick);
                }
            }
        }

        let linear_lo = lo.max(-self.linthresh);
        let linear_hi = hi.min(self.linthresh);
        if linear_lo <= linear_hi {
            let step = 2.0 * self.linthresh / (self.linear_ticks - 1) as f64;
            for i in 0..self.linear_ticks {
                let tick = -self.linthresh + i as f64 * step;
                if tick >= linear_lo - 1e-12 && tick <= linear_hi + 1e-12 {
                    ticks.push(if tick.abs() < 1e-12 { 0.0 } else { tick });
                }
            }
        }

        if hi > self.linthresh {
            let max_exp = (hi / self.linthresh).log(self.base).ceil() as i32;
            for exponent in 1..=max_exp {
                let tick = self.linthresh * self.base.powi(exponent);
                if tick >= lo * (1.0 - 1e-12) && tick <= hi * (1.0 + 1e-12) {
                    ticks.push(tick);
                }
            }
        }

        ticks.sort_by(|a, b| a.partial_cmp(b).expect("finite ticks are comparable"));
        ticks.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0));
        if reversed {
            ticks.reverse();
        }
        ticks
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        let (lo, hi) = nonsingular(vmin, vmax, self.linthresh, 1e-13, true);
        let ticks = self.tick_values(lo, hi);
        match (ticks.first(), ticks.last()) {
            (Some(first), Some(last)) => {
                if vmin <= vmax {
                    (*first, *last)
                } else {
                    (*last, *first)
                }
            }
            _ => {
                if vmin <= vmax {
                    (-self.linthresh, self.linthresh)
                } else {
                    (self.linthresh, -self.linthresh)
                }
            }
        }
    }
}

/// Place ticks on a logit/probability axis.
///
/// Major ticks are powers of ten approaching zero, mirrored powers approaching
/// one, plus `0.5` when it lies in the view range. This covers the common
/// probability-axis labels without depending on any concrete Axes integration.
#[derive(Clone, Debug, PartialEq)]
pub struct LogitLocator {
    max_exponent: i32,
}

impl LogitLocator {
    /// Construct a logit locator with powers through `1e-6`.
    pub fn new() -> Self {
        Self::with_max_exponent(6)
    }

    /// Construct a logit locator with powers through `10^-max_exponent`.
    ///
    /// `max_exponent` is clamped to at least 1.
    pub fn with_max_exponent(max_exponent: i32) -> Self {
        LogitLocator {
            max_exponent: max_exponent.max(1),
        }
    }

    /// Return the largest generated power exponent.
    #[must_use]
    pub fn max_exponent(&self) -> i32 {
        self.max_exponent
    }
}

impl Default for LogitLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Locator for LogitLocator {
    fn tick_values(&self, vmin: f64, vmax: f64) -> Vec<f64> {
        if !vmin.is_finite() || !vmax.is_finite() || vmin <= 0.0 || vmax >= 1.0 {
            return Vec::new();
        }

        let (lo, hi, reversed) = if vmin <= vmax {
            (vmin, vmax, false)
        } else {
            (vmax, vmin, true)
        };
        let mut ticks = Vec::new();

        for exponent in (1..=self.max_exponent).rev() {
            let p = 10f64.powi(-exponent);
            if p >= lo * (1.0 - 1e-12) && p <= hi * (1.0 + 1e-12) {
                ticks.push(p);
            }
        }
        if 0.5 >= lo && 0.5 <= hi {
            ticks.push(0.5);
        }
        for exponent in 1..=self.max_exponent {
            let p = 1.0 - 10f64.powi(-exponent);
            if p >= lo * (1.0 - 1e-12) && p <= hi * (1.0 + 1e-12) {
                ticks.push(p);
            }
        }

        ticks.sort_by(|a, b| a.partial_cmp(b).expect("finite ticks are comparable"));
        ticks.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);
        if reversed {
            ticks.reverse();
        }
        ticks
    }

    fn view_limits(&self, vmin: f64, vmax: f64) -> (f64, f64) {
        let lower = 10f64.powi(-self.max_exponent);
        let upper = 1.0 - lower;
        if !vmin.is_finite() || !vmax.is_finite() {
            return (lower, upper);
        }
        let (lo, hi, reversed) = if vmin <= vmax {
            (vmin, vmax, false)
        } else {
            (vmax, vmin, true)
        };
        let lo = lo.clamp(lower, upper);
        let hi = hi.clamp(lower, upper);
        if reversed { (hi, lo) } else { (lo, hi) }
    }
}

/// Place no ticks at all.
///
/// Port of matplotlib's `NullLocator`.
pub struct NullLocator;

impl Locator for NullLocator {
    fn tick_values(&self, _vmin: f64, _vmax: f64) -> Vec<f64> {
        Vec::new()
    }
}

/// Format tick values as plain decimal numbers.
///
/// A reasonable subset of matplotlib's `ScalarFormatter`: the number of
/// significant figures (decimal places) is chosen from the tick spacing via the
/// same algorithm as `ScalarFormatter._set_format`. To do so the formatter must
/// be told the tick locations up front via [`ScalarFormatter::set_locs`].
///
/// # Omissions
///
/// Offset notation and scientific notation are intentionally **not** implemented
/// (`useOffset`/`useMathText` in matplotlib): the order of magnitude is fixed at
/// zero and there is no offset string. Locale handling and Unicode-minus are
/// likewise out of scope.
pub struct ScalarFormatter {
    /// Number of decimal places, `%.{decimals}f`.
    decimals: usize,
    /// Whether [`set_locs`](ScalarFormatter::set_locs) has been called.
    have_format: bool,
}

impl ScalarFormatter {
    /// Create a formatter with a default of one decimal place.
    ///
    /// Call [`set_locs`](ScalarFormatter::set_locs) with the tick vector to pick
    /// the precision from the spacing, as matplotlib does in `set_locs`.
    pub fn new() -> Self {
        ScalarFormatter {
            decimals: 1,
            have_format: false,
        }
    }

    /// Choose the number of decimal places from the tick locations.
    ///
    /// Port of `ScalarFormatter._set_format` with `offset = 0` and
    /// `orderOfMagnitude = 0` (no offset/sci-notation in this subset). With
    /// fewer than two locations the precision falls back to the default.
    pub fn set_locs(&mut self, locs: &[f64]) {
        self.have_format = true;
        if locs.len() < 2 {
            // matplotlib augments with the axis view interval here; without an
            // axis we keep the default precision.
            return;
        }
        let max = locs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min = locs.iter().cloned().fold(f64::INFINITY, f64::min);
        let mut loc_range = max - min;
        if loc_range == 0.0 {
            loc_range = locs.iter().map(|x| x.abs()).fold(0.0, f64::max);
        }
        if loc_range == 0.0 {
            loc_range = 1.0;
        }
        let loc_range_oom = loc_range.log10().floor() as i32;
        // First estimate.
        let mut sigfigs = (3 - loc_range_oom).max(0);
        // Refined estimate: drop trailing zero digits.
        let thresh = 1e-3 * 10f64.powi(loc_range_oom);
        while sigfigs >= 0 {
            let factor = 10f64.powi(sigfigs);
            let maxdev = locs
                .iter()
                .map(|&x| (x - (x * factor).round() / factor).abs())
                .fold(0.0, f64::max);
            if maxdev < thresh {
                sigfigs -= 1;
            } else {
                break;
            }
        }
        sigfigs += 1;
        self.decimals = sigfigs.max(0) as usize;
    }
}

impl Default for ScalarFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for ScalarFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        // Matplotlib rounds tiny values to exactly zero before formatting.
        let v = if value.abs() < 1e-8 { 0.0 } else { value };
        format!("{:.*}", self.decimals, v)
    }
}

/// Format logarithmic major tick values.
///
/// Exact powers of `base` are labelled. Non-decade values (typically minor
/// ticks) produce an empty label by default, matching matplotlib's default
/// minor tick behaviour.
#[derive(Clone, Debug, PartialEq)]
pub struct LogFormatter {
    base: f64,
}

impl LogFormatter {
    /// Construct a formatter for the given base.
    ///
    /// `base` must be finite and greater than one.
    pub fn new(base: f64) -> Self {
        assert!(
            base.is_finite() && base > 1.0,
            "log formatter base must be finite and > 1"
        );
        LogFormatter { base }
    }

    fn exponent(&self, value: f64) -> Option<i32> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        let exponent = (value.ln() / self.base.ln()).round();
        let decade = self.base.powf(exponent);
        let rel = ((value - decade) / decade).abs();
        if rel <= 1e-10 {
            Some(exponent as i32)
        } else {
            None
        }
    }

    fn format_exponent(&self, exponent: i32, mathtext: bool) -> String {
        let plain_label = (self.base == 10.0 && (-3..=4).contains(&exponent))
            || (self.base == 2.0 && (-3..=6).contains(&exponent));
        let value = self.base.powi(exponent);
        if plain_label {
            format_decimal(value)
        } else {
            let label = format!("{}^{{{}}}", format_decimal(self.base), exponent);
            if mathtext {
                format!("${label}$")
            } else {
                label
            }
        }
    }
}

impl Default for LogFormatter {
    /// Default base-10 log formatter.
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl Formatter for LogFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        let Some(exponent) = self.exponent(value) else {
            return String::new();
        };

        self.format_exponent(exponent, false)
    }
}

/// Format logarithmic major ticks with mathtext for exponent labels.
///
/// This formatter preserves [`LogFormatter`]'s compact decimal labels for
/// nearby powers, hides non-decade minor ticks, and wraps large exponent labels
/// in `$...$` so higher-level rich-text rendering can draw real superscripts.
#[derive(Clone, Debug, PartialEq)]
pub struct LogFormatterMathtext {
    inner: LogFormatter,
}

impl LogFormatterMathtext {
    /// Construct a mathtext log formatter for the given base.
    ///
    /// `base` must be finite and greater than one.
    pub fn new(base: f64) -> Self {
        Self {
            inner: LogFormatter::new(base),
        }
    }
}

impl Default for LogFormatterMathtext {
    /// Default base-10 mathtext log formatter.
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl Formatter for LogFormatterMathtext {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        let Some(exponent) = self.inner.exponent(value) else {
            return String::new();
        };

        self.inner.format_exponent(exponent, true)
    }
}

/// Format symmetric-log tick values.
///
/// Values in the linear region are formatted as plain decimals. Exact
/// logarithmic tail ticks are labelled as signed powers of `base` when
/// `linthresh == 1`; for other thresholds they fall back to decimal labels.
/// Off-lattice values return an empty label, which is suitable for minor ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct SymlogFormatter {
    base: f64,
    linthresh: f64,
}

impl SymlogFormatter {
    /// Construct a symlog formatter.
    ///
    /// `base` must be finite and greater than one; `linthresh` must be finite
    /// and positive.
    pub fn new(base: f64, linthresh: f64) -> Self {
        assert!(
            base.is_finite() && base > 1.0,
            "symlog formatter base must be finite and > 1"
        );
        assert!(
            linthresh.is_finite() && linthresh > 0.0,
            "symlog formatter linthresh must be finite and > 0"
        );
        SymlogFormatter { base, linthresh }
    }

    fn tail_exponent(&self, value: f64) -> Option<i32> {
        if !value.is_finite() || value.abs() <= self.linthresh {
            return None;
        }
        let abs = value.abs();
        let exponent = (abs / self.linthresh).log(self.base).round();
        let tick = self.linthresh * self.base.powf(exponent);
        let rel = ((abs - tick) / tick).abs();
        if rel <= 1e-10 {
            Some(exponent as i32)
        } else {
            None
        }
    }
}

impl Default for SymlogFormatter {
    /// Default base-10 formatter with `linthresh = 1`.
    fn default() -> Self {
        Self::new(10.0, 1.0)
    }
}

impl Formatter for SymlogFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        if value.abs() <= self.linthresh * (1.0 + 1e-12) {
            return format_decimal(if value.abs() < 1e-12 { 0.0 } else { value });
        }

        let Some(exponent) = self.tail_exponent(value) else {
            return String::new();
        };
        if (self.linthresh - 1.0).abs() > 1e-12 {
            return format_decimal(value);
        }

        let sign = if value.is_sign_negative() { "-" } else { "" };
        let label = LogFormatter::new(self.base).format_exponent(exponent, false);
        format!("{sign}{label}")
    }
}

/// Format symmetric-log tick values with mathtext for exponent labels.
///
/// This formatter preserves [`SymlogFormatter`]'s decimal labels in the linear
/// region and near logarithmic tails, hides off-lattice tail ticks, and wraps
/// large signed power labels in `$...$` so rich-text rendering can draw real
/// superscripts.
#[derive(Clone, Debug, PartialEq)]
pub struct SymlogFormatterMathtext {
    inner: SymlogFormatter,
}

impl SymlogFormatterMathtext {
    /// Construct a mathtext symlog formatter.
    ///
    /// `base` must be finite and greater than one; `linthresh` must be finite
    /// and positive.
    pub fn new(base: f64, linthresh: f64) -> Self {
        Self {
            inner: SymlogFormatter::new(base, linthresh),
        }
    }
}

impl Default for SymlogFormatterMathtext {
    /// Default base-10 formatter with `linthresh = 1`.
    fn default() -> Self {
        Self::new(10.0, 1.0)
    }
}

impl Formatter for SymlogFormatterMathtext {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        if value.abs() <= self.inner.linthresh * (1.0 + 1e-12) {
            return format_decimal(if value.abs() < 1e-12 { 0.0 } else { value });
        }

        let Some(exponent) = self.inner.tail_exponent(value) else {
            return String::new();
        };
        if (self.inner.linthresh - 1.0).abs() > 1e-12 {
            return format_decimal(value);
        }

        let label = LogFormatter::new(self.inner.base).format_exponent(exponent, true);
        if value.is_sign_negative() && label.starts_with('$') && label.ends_with('$') {
            format!("$-{}$", &label[1..label.len() - 1])
        } else if value.is_sign_negative() {
            format!("-{label}")
        } else {
            label
        }
    }
}

/// Format logit/probability tick values.
///
/// Exact powers of ten near zero are labelled as decimals for `0.1` and as
/// `10^{-n}` for smaller probabilities. Mirrored ticks near one are labelled
/// as decimals for `0.9` and `1-10^{-n}` closer to one. Off-lattice values
/// return an empty label.
#[derive(Clone, Debug, PartialEq)]
pub struct LogitFormatter {
    max_exponent: i32,
}

impl LogitFormatter {
    /// Construct a logit formatter with powers through `1e-6`.
    pub fn new() -> Self {
        Self::with_max_exponent(6)
    }

    /// Construct a logit formatter with powers through `10^-max_exponent`.
    ///
    /// `max_exponent` is clamped to at least 1.
    pub fn with_max_exponent(max_exponent: i32) -> Self {
        LogitFormatter {
            max_exponent: max_exponent.max(1),
        }
    }

    fn lower_exponent(&self, value: f64) -> Option<i32> {
        if !value.is_finite() || value <= 0.0 || value >= 0.5 {
            return None;
        }
        let exponent = -value.log10().round() as i32;
        if exponent < 1 || exponent > self.max_exponent {
            return None;
        }
        let tick = 10f64.powi(-exponent);
        if ((value - tick) / tick).abs() <= 1e-10 {
            Some(exponent)
        } else {
            None
        }
    }

    fn upper_exponent(&self, value: f64) -> Option<i32> {
        if !value.is_finite() || value <= 0.5 || value >= 1.0 {
            return None;
        }
        let q = 1.0 - value;
        let exponent = -q.log10().round() as i32;
        if exponent < 1 || exponent > self.max_exponent {
            return None;
        }
        let tick = 10f64.powi(-exponent);
        if ((q - tick) / tick).abs() <= 1e-10 {
            Some(exponent)
        } else {
            None
        }
    }
}

impl Default for LogitFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for LogitFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        if (value - 0.5).abs() <= 1e-12 {
            return "1/2".to_owned();
        }
        if let Some(exponent) = self.lower_exponent(value) {
            return if exponent == 1 {
                "0.1".to_owned()
            } else {
                format!("10^{{-{exponent}}}")
            };
        }
        if let Some(exponent) = self.upper_exponent(value) {
            return if exponent == 1 {
                "0.9".to_owned()
            } else {
                format!("1-10^{{-{exponent}}}")
            };
        }
        String::new()
    }
}

/// Format logit/probability tick values with mathtext tail labels.
///
/// This preserves [`LogitFormatter`]'s compact decimal labels for `0.1`,
/// `0.5`, and `0.9`, while wrapping smaller lower-tail labels and mirrored
/// upper-tail labels in `$...$` so rich-text rendering can draw superscripts.
#[derive(Clone, Debug, PartialEq)]
pub struct LogitFormatterMathtext {
    inner: LogitFormatter,
}

impl LogitFormatterMathtext {
    /// Construct a logit mathtext formatter with powers through `1e-6`.
    pub fn new() -> Self {
        Self::with_max_exponent(6)
    }

    /// Construct a logit mathtext formatter with powers through
    /// `10^-max_exponent`.
    ///
    /// `max_exponent` is clamped to at least 1.
    pub fn with_max_exponent(max_exponent: i32) -> Self {
        Self {
            inner: LogitFormatter::with_max_exponent(max_exponent),
        }
    }
}

impl Default for LogitFormatterMathtext {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for LogitFormatterMathtext {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        if (value - 0.5).abs() <= 1e-12 {
            return "1/2".to_owned();
        }
        if let Some(exponent) = self.inner.lower_exponent(value) {
            return if exponent == 1 {
                "0.1".to_owned()
            } else {
                format!("$10^{{-{exponent}}}$")
            };
        }
        if let Some(exponent) = self.inner.upper_exponent(value) {
            return if exponent == 1 {
                "0.9".to_owned()
            } else {
                format!("$1-10^{{-{exponent}}}$")
            };
        }
        String::new()
    }
}

/// Format values in engineering notation with SI prefixes.
///
/// Exponents are multiples of three, clamped to the standard SI prefix range
/// from yocto (`y`) through yotta (`Y`). The micro prefix is rendered as ASCII
/// `u` to keep labels backend- and font-safe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngFormatter {
    unit: String,
    places: Option<usize>,
    separator: String,
}

impl EngFormatter {
    /// Construct an engineering formatter with no unit suffix.
    pub fn new() -> Self {
        Self {
            unit: String::new(),
            places: None,
            separator: String::new(),
        }
    }

    /// Return a copy with a unit suffix, such as `"Hz"` or `"V"`.
    #[must_use]
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    /// Return a copy with a fixed number of decimal places.
    #[must_use]
    pub fn with_places(mut self, places: usize) -> Self {
        self.places = Some(places);
        self
    }

    /// Return a copy with a separator between the number and prefix/unit.
    #[must_use]
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    fn prefix(exponent: i32) -> &'static str {
        match exponent {
            -24 => "y",
            -21 => "z",
            -18 => "a",
            -15 => "f",
            -12 => "p",
            -9 => "n",
            -6 => "u",
            -3 => "m",
            0 => "",
            3 => "k",
            6 => "M",
            9 => "G",
            12 => "T",
            15 => "P",
            18 => "E",
            21 => "Z",
            24 => "Y",
            _ => "",
        }
    }
}

impl Default for EngFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for EngFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        if value == 0.0 {
            let suffix = format!("{}{}", Self::prefix(0), self.unit);
            return if suffix.is_empty() {
                "0".to_owned()
            } else {
                format!("0{}{}", self.separator, suffix)
            };
        }

        let exponent = ((value.abs().log10().floor() as i32).div_euclid(3) * 3).clamp(-24, 24);
        let scaled = value / 10f64.powi(exponent);
        let number = if let Some(places) = self.places {
            format!("{scaled:.places$}")
        } else {
            format_decimal(scaled)
        };
        let suffix = format!("{}{}", Self::prefix(exponent), self.unit);
        if suffix.is_empty() {
            number
        } else {
            format!("{number}{}{}", self.separator, suffix)
        }
    }
}

/// Format values as percentages.
///
/// `xmax` is the data value corresponding to 100 percent, matching
/// matplotlib's `PercentFormatter` convention.
#[derive(Clone, Debug, PartialEq)]
pub struct PercentFormatter {
    xmax: f64,
    decimals: usize,
    symbol: String,
}

impl PercentFormatter {
    /// Construct a formatter where `100.0` maps to `100%`.
    pub fn new() -> Self {
        Self::with_xmax(100.0)
    }

    /// Construct a formatter with a custom value for 100 percent.
    ///
    /// `xmax` must be finite and non-zero.
    pub fn with_xmax(xmax: f64) -> Self {
        assert!(
            xmax.is_finite() && xmax != 0.0,
            "percent formatter xmax must be finite and non-zero"
        );
        Self {
            xmax,
            decimals: 0,
            symbol: "%".to_owned(),
        }
    }

    /// Return a copy with a fixed number of decimal places.
    #[must_use]
    pub fn with_decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// Return a copy with a custom percent symbol suffix.
    #[must_use]
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = symbol.into();
        self
    }
}

impl Default for PercentFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl Formatter for PercentFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        let percent = value / self.xmax * 100.0;
        format!("{:.*}{}", self.decimals, percent, self.symbol)
    }
}

fn format_decimal(value: f64) -> String {
    if (value - value.round()).abs() < 1e-10 {
        format!("{:.0}", value)
    } else {
        let mut text = format!("{value:.12}");
        while text.contains('.') && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}

/// Always return the empty string.
///
/// Port of matplotlib's `NullFormatter`.
pub struct NullFormatter;

impl Formatter for NullFormatter {
    fn format(&self, _value: f64, _pos: Option<usize>) -> String {
        String::new()
    }
}

/// Return fixed label strings indexed by tick position.
///
/// Port of matplotlib's `FixedFormatter`. Should be paired with a
/// [`FixedLocator`]. Positions out of range (or `None`) yield the empty string.
pub struct FixedFormatter {
    seq: Vec<String>,
}

impl FixedFormatter {
    /// Create a formatter from the sequence of label strings.
    pub fn new(seq: Vec<String>) -> Self {
        FixedFormatter { seq }
    }
}

impl Formatter for FixedFormatter {
    fn format(&self, _value: f64, pos: Option<usize>) -> String {
        match pos {
            Some(p) if p < self.seq.len() => self.seq[p].clone(),
            _ => String::new(),
        }
    }
}

/// Return fixed label strings indexed by rounded tick value.
///
/// Port of matplotlib's `IndexFormatter`. A tick value is rounded to the
/// nearest integer and used as an index into the label vector. Non-finite,
/// negative, or out-of-range values yield the empty string.
pub struct IndexFormatter {
    labels: Vec<String>,
}

impl IndexFormatter {
    /// Create an index formatter from the sequence of label strings.
    pub fn new(labels: Vec<String>) -> Self {
        IndexFormatter { labels }
    }
}

impl Formatter for IndexFormatter {
    fn format(&self, value: f64, _pos: Option<usize>) -> String {
        if !value.is_finite() {
            return String::new();
        }
        let index = value.round();
        if index < 0.0 {
            return String::new();
        }
        let index = index as usize;
        self.labels.get(index).cloned().unwrap_or_default()
    }
}

/// Format ticks with a user-supplied closure.
///
/// Port of matplotlib's `FuncFormatter`. The closure receives the value and the
/// optional position index and returns the label.
pub struct FuncFormatter {
    func: Box<dyn Fn(f64, Option<usize>) -> String>,
}

impl FuncFormatter {
    /// Create a formatter from a boxed closure.
    pub fn new(func: Box<dyn Fn(f64, Option<usize>) -> String>) -> Self {
        FuncFormatter { func }
    }
}

impl Formatter for FuncFormatter {
    fn format(&self, value: f64, pos: Option<usize>) -> String {
        (self.func)(value, pos)
    }
}

/// Format ticks with a `{x}` / `{pos}` template string.
///
/// Port of matplotlib's `StrMethodFormatter`, restricted to plain
/// `{x}`/`{pos}` substitution (no Python format-spec mini-language). Occurrences
/// of the literal substrings `{x}` and `{pos}` are replaced with the value and
/// position respectively. A missing position renders as the empty string.
pub struct StrMethodFormatter {
    fmt: String,
}

impl StrMethodFormatter {
    /// Create a formatter from a template containing `{x}` and/or `{pos}`.
    pub fn new(fmt: impl Into<String>) -> Self {
        StrMethodFormatter { fmt: fmt.into() }
    }
}

impl Formatter for StrMethodFormatter {
    fn format(&self, value: f64, pos: Option<usize>) -> String {
        let pos_str = pos.map(|p| p.to_string()).unwrap_or_default();
        self.fmt
            .replace("{x}", &value.to_string())
            .replace("{pos}", &pos_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare two tick vectors with a tight tolerance.
    ///
    /// Matplotlib's vectors carry the usual binary-float noise (e.g.
    /// `0.30000000000000004`); we assert near-equality rather than bit-equality.
    fn assert_ticks(actual: &[f64], expected: &[f64]) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "length mismatch: got {actual:?}, expected {expected:?}"
        );
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                (a - e).abs() < 1e-9,
                "value mismatch: got {actual:?}, expected {expected:?}"
            );
        }
    }

    // Ground-truth vectors below were produced by shelling out to a working
    // matplotlib install (3.x) on the reference ranges, e.g.:
    //   python3 -c "import matplotlib.ticker as t; \
    //       print(list(t.MaxNLocator().tick_values(0.0, 1.0)))"

    #[test]
    fn maxn_default_0_1() {
        let locs = MaxNLocator::default().tick_values(0.0, 1.0);
        assert_ticks(
            &locs,
            &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
        );
    }

    #[test]
    fn maxn_default_0_100() {
        let locs = MaxNLocator::default().tick_values(0.0, 100.0);
        assert_ticks(
            &locs,
            &[
                0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
            ],
        );
    }

    #[test]
    fn maxn_default_neg3_7() {
        let locs = MaxNLocator::default().tick_values(-3.0, 7.0);
        assert_ticks(
            &locs,
            &[-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        );
    }

    #[test]
    fn maxn_default_0_09() {
        // Note: matplotlib returns 9 ticks here (0.0..=0.9, no 1.0), because
        // the upper edge tick 1.0 lies beyond vmax = 0.9.
        let locs = MaxNLocator::default().tick_values(0.0, 0.9);
        assert_ticks(&locs, &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    }

    #[test]
    fn auto_0_1() {
        let locs = AutoLocator::new().tick_values(0.0, 1.0);
        assert_ticks(&locs, &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
    }

    #[test]
    fn auto_neg3_7() {
        // AutoLocator uses nbins='auto' (=> 9) and steps [1,2,2.5,5,10],
        // yielding a step of 2 and edge ticks at -4 and 8.
        let locs = AutoLocator::new().tick_values(-3.0, 7.0);
        assert_ticks(&locs, &[-4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn multiple_half_0_2() {
        let locs = MultipleLocator::new(0.5).tick_values(0.0, 2.0);
        assert_ticks(&locs, &[-0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }

    #[test]
    fn linear_default_0_10() {
        let locs = LinearLocator::default().tick_values(0.0, 10.0);
        assert_ticks(
            &locs,
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        );
    }

    #[test]
    fn linear_5_0_1() {
        let locs = LinearLocator::new(5).tick_values(0.0, 1.0);
        assert_ticks(&locs, &[0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn reversed_range_is_sorted() {
        // matplotlib swaps reversed ranges internally.
        let locs = MaxNLocator::default().tick_values(1.0, 0.0);
        assert_ticks(
            &locs,
            &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
        );
    }

    #[test]
    fn fixed_locator_passthrough() {
        let locs = FixedLocator::new(vec![1.0, 2.0, 3.0]).tick_values(0.0, 10.0);
        assert_ticks(&locs, &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn fixed_locator_subsample_keeps_zero() {
        // locs -3..=3, nbins=3 -> step=ceil(7/3)=3; the offset that includes 0
        // has the smallest min-abs and is selected.
        let locs = FixedLocator::with_nbins(vec![-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0], 3)
            .tick_values(0.0, 0.0);
        assert!(
            locs.contains(&0.0),
            "expected the zero-containing subset: {locs:?}"
        );
    }

    #[test]
    fn log_locator_base10_decades() {
        let locs = LogLocator::new(10.0).tick_values(1.0, 1000.0);
        assert_ticks(&locs, &[1.0, 10.0, 100.0, 1000.0]);
    }

    #[test]
    fn log_locator_minor_subticks_present() {
        let locs = LogLocator::minor(10.0).tick_values(1.0, 20.0);

        assert!(locs.contains(&2.0));
        assert!(locs.contains(&9.0));
        assert!(locs.contains(&20.0));
        assert!(!locs.contains(&1.0));
        assert!(!locs.contains(&10.0));
    }

    #[test]
    fn log_locator_base2_decades() {
        let locs = LogLocator::new(2.0).tick_values(1.0, 32.0);
        assert_ticks(&locs, &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0]);
    }

    #[test]
    fn log_locator_rejects_nonpositive_or_nonfinite_domain() {
        assert!(LogLocator::new(10.0).tick_values(-1.0, 100.0).is_empty());
        assert!(
            LogLocator::new(10.0)
                .tick_values(1.0, f64::INFINITY)
                .is_empty()
        );
    }

    #[test]
    fn log_locator_view_limits_snap_to_decades() {
        let (lo, hi) = LogLocator::new(10.0).view_limits(3.0, 88.0);
        assert_eq!((lo, hi), (1.0, 100.0));
    }

    #[test]
    fn log_formatter_labels_major_ticks_and_hides_minor_ticks() {
        let formatter = LogFormatter::new(10.0);

        assert_eq!(formatter.format(1.0, None), "1");
        assert_eq!(formatter.format(10.0, None), "10");
        assert_eq!(formatter.format(100.0, None), "100");
        assert_eq!(formatter.format(1000.0, None), "1000");
        assert_eq!(formatter.format(2.0, None), "");
        assert_eq!(formatter.format(1e6, None), "10^{6}");
    }

    #[test]
    fn log_formatter_base2_labels_powers() {
        let formatter = LogFormatter::new(2.0);

        assert_eq!(formatter.format(1.0, None), "1");
        assert_eq!(formatter.format(8.0, None), "8");
        assert_eq!(formatter.format(128.0, None), "2^{7}");
        assert_eq!(formatter.format(3.0, None), "");
    }

    #[test]
    fn log_formatter_mathtext_wraps_exponent_labels() {
        let formatter = LogFormatterMathtext::new(10.0);

        assert_eq!(formatter.format(10.0, None), "10");
        assert_eq!(formatter.format(1e6, None), "$10^{6}$");
        assert_eq!(formatter.format(2.0, None), "");
    }

    #[test]
    fn log_formatter_mathtext_honors_base2() {
        let formatter = LogFormatterMathtext::new(2.0);

        assert_eq!(formatter.format(8.0, None), "8");
        assert_eq!(formatter.format(128.0, None), "$2^{7}$");
    }

    #[test]
    fn symlog_locator_spans_negative_linear_and_positive_regions() {
        let locs = SymlogLocator::new(10.0, 1.0).tick_values(-100.0, 100.0);

        assert_ticks(&locs, &[-100.0, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0]);
    }

    #[test]
    fn symlog_locator_honors_base_and_linthresh() {
        let locs = SymlogLocator::new(2.0, 0.5).tick_values(-4.0, 4.0);

        assert_ticks(&locs, &[-4.0, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 4.0]);
    }

    #[test]
    fn symlog_locator_handles_reversed_ranges() {
        let locs = SymlogLocator::new(10.0, 1.0).tick_values(100.0, -100.0);

        assert_ticks(&locs, &[100.0, 10.0, 1.0, 0.0, -1.0, -10.0, -100.0]);
    }

    #[test]
    fn symlog_locator_rejects_nonfinite_domain() {
        assert!(
            SymlogLocator::new(10.0, 1.0)
                .tick_values(f64::NEG_INFINITY, 100.0)
                .is_empty()
        );
    }

    #[test]
    fn symlog_formatter_labels_linear_and_tail_ticks() {
        let formatter = SymlogFormatter::new(10.0, 1.0);

        assert_eq!(formatter.format(-100.0, None), "-100");
        assert_eq!(formatter.format(-1.0, None), "-1");
        assert_eq!(formatter.format(0.0, None), "0");
        assert_eq!(formatter.format(1.0, None), "1");
        assert_eq!(formatter.format(100.0, None), "100");
        assert_eq!(formatter.format(20.0, None), "");
        assert_eq!(formatter.format(1e6, None), "10^{6}");
        assert_eq!(formatter.format(-1e6, None), "-10^{6}");
    }

    #[test]
    fn symlog_formatter_nonunit_linthresh_uses_decimal_tail_labels() {
        let formatter = SymlogFormatter::new(10.0, 2.0);

        assert_eq!(formatter.format(2.0, None), "2");
        assert_eq!(formatter.format(20.0, None), "20");
        assert_eq!(formatter.format(40.0, None), "");
    }

    #[test]
    fn symlog_formatter_mathtext_wraps_large_tail_labels() {
        let formatter = SymlogFormatterMathtext::new(10.0, 1.0);

        assert_eq!(formatter.format(-1.0, None), "-1");
        assert_eq!(formatter.format(0.0, None), "0");
        assert_eq!(formatter.format(1.0, None), "1");
        assert_eq!(formatter.format(100.0, None), "100");
        assert_eq!(formatter.format(1e6, None), "$10^{6}$");
        assert_eq!(formatter.format(-1e6, None), "$-10^{6}$");
        assert_eq!(formatter.format(20.0, None), "");
    }

    #[test]
    fn symlog_formatter_mathtext_nonunit_linthresh_uses_decimal_tail_labels() {
        let formatter = SymlogFormatterMathtext::new(10.0, 2.0);

        assert_eq!(formatter.format(2.0, None), "2");
        assert_eq!(formatter.format(20.0, None), "20");
        assert_eq!(formatter.format(40.0, None), "");
    }

    #[test]
    fn logit_locator_clusters_toward_zero_and_one() {
        let locs = LogitLocator::with_max_exponent(3).tick_values(0.001, 0.999);

        assert_ticks(&locs, &[0.001, 0.01, 0.1, 0.5, 0.9, 0.99, 0.999]);
    }

    #[test]
    fn logit_locator_handles_reversed_ranges() {
        let locs = LogitLocator::with_max_exponent(2).tick_values(0.99, 0.01);

        assert_ticks(&locs, &[0.99, 0.9, 0.5, 0.1, 0.01]);
    }

    #[test]
    fn logit_locator_rejects_outside_probability_domain() {
        assert!(LogitLocator::new().tick_values(0.0, 0.99).is_empty());
        assert!(LogitLocator::new().tick_values(0.01, 1.0).is_empty());
        assert!(
            LogitLocator::new()
                .tick_values(0.01, f64::INFINITY)
                .is_empty()
        );
    }

    #[test]
    fn logit_locator_view_limits_clamp_to_open_interval() {
        let (lo, hi) = LogitLocator::with_max_exponent(4).view_limits(-1.0, 2.0);

        assert_eq!((lo, hi), (1e-4, 1.0 - 1e-4));
    }

    #[test]
    fn logit_formatter_labels_probability_lattice() {
        let formatter = LogitFormatter::with_max_exponent(4);

        assert_eq!(formatter.format(0.1, None), "0.1");
        assert_eq!(formatter.format(0.01, None), "10^{-2}");
        assert_eq!(formatter.format(0.5, None), "1/2");
        assert_eq!(formatter.format(0.9, None), "0.9");
        assert_eq!(formatter.format(0.99, None), "1-10^{-2}");
        assert_eq!(formatter.format(0.2, None), "");
    }

    #[test]
    fn logit_formatter_mathtext_wraps_tail_labels() {
        let formatter = LogitFormatterMathtext::with_max_exponent(4);

        assert_eq!(formatter.format(0.1, None), "0.1");
        assert_eq!(formatter.format(0.01, None), "$10^{-2}$");
        assert_eq!(formatter.format(0.5, None), "1/2");
        assert_eq!(formatter.format(0.9, None), "0.9");
        assert_eq!(formatter.format(0.99, None), "$1-10^{-2}$");
        assert_eq!(formatter.format(0.2, None), "");
    }

    #[test]
    fn eng_formatter_uses_si_prefixes() {
        let formatter = EngFormatter::new();

        assert_eq!(formatter.format(1_000.0, None), "1k");
        assert_eq!(formatter.format(1_500_000.0, None), "1.5M");
        assert_eq!(formatter.format(0.001, None), "1m");
        assert_eq!(formatter.format(0.000_001, None), "1u");
        assert_eq!(formatter.format(-2_000.0, None), "-2k");
        assert_eq!(formatter.format(0.0, None), "0");
    }

    #[test]
    fn eng_formatter_supports_units_separator_and_places() {
        let formatter = EngFormatter::new()
            .with_unit("Hz")
            .with_separator(" ")
            .with_places(2);

        assert_eq!(formatter.format(12_300.0, None), "12.30 kHz");
        assert_eq!(formatter.format(0.0, None), "0 Hz");
    }

    #[test]
    fn percent_formatter_defaults_to_xmax_100() {
        let formatter = PercentFormatter::new();

        assert_eq!(formatter.format(25.0, None), "25%");
        assert_eq!(formatter.format(100.0, None), "100%");
    }

    #[test]
    fn percent_formatter_supports_fractional_xmax_and_decimals() {
        let formatter = PercentFormatter::with_xmax(1.0).with_decimals(1);

        assert_eq!(formatter.format(0.125, None), "12.5%");
        assert_eq!(formatter.format(1.0, None), "100.0%");
    }

    #[test]
    fn null_locator_empty() {
        assert!(NullLocator.tick_values(0.0, 10.0).is_empty());
    }

    #[test]
    fn null_formatter_empty() {
        assert_eq!(NullFormatter.format(2.5, Some(0)), "");
    }

    #[test]
    fn scalar_formatter_picks_precision() {
        let locs = MaxNLocator::default().tick_values(0.0, 1.0);
        let mut f = ScalarFormatter::new();
        f.set_locs(&locs);
        // Spacing of 0.1 over [0,1] -> one decimal place.
        assert_eq!(f.format(0.5, Some(5)), "0.5");
        assert_eq!(f.format(0.0, Some(0)), "0.0");
    }

    #[test]
    fn scalar_formatter_integers() {
        let locs = MaxNLocator::default().tick_values(0.0, 100.0);
        let mut f = ScalarFormatter::new();
        f.set_locs(&locs);
        // Integer spacing -> zero decimals.
        assert_eq!(f.format(50.0, Some(5)), "50");
        assert_eq!(f.format(0.0, Some(0)), "0");
    }

    #[test]
    fn fixed_formatter_by_position() {
        let f = FixedFormatter::new(vec!["a".into(), "b".into()]);
        assert_eq!(f.format(0.0, Some(0)), "a");
        assert_eq!(f.format(99.0, Some(1)), "b");
        assert_eq!(f.format(0.0, Some(2)), "");
        assert_eq!(f.format(0.0, None), "");
    }

    #[test]
    fn index_formatter_uses_rounded_tick_value() {
        let f = IndexFormatter::new(vec!["zero".into(), "one".into(), "two".into()]);
        assert_eq!(f.format(0.2, Some(99)), "zero");
        assert_eq!(f.format(0.6, None), "one");
        assert_eq!(f.format(2.49, None), "two");
    }

    #[test]
    fn index_formatter_out_of_range_is_empty() {
        let f = IndexFormatter::new(vec!["zero".into(), "one".into()]);
        assert_eq!(f.format(-0.6, None), "");
        assert_eq!(f.format(2.0, None), "");
        assert_eq!(f.format(f64::NAN, None), "");
    }

    #[test]
    fn func_formatter_closure() {
        let f = FuncFormatter::new(Box::new(|x, _| format!("{x:.0} km")));
        assert_eq!(f.format(10.0, None), "10 km");
    }

    #[test]
    fn str_method_formatter_template() {
        let f = StrMethodFormatter::new("{x} m");
        assert_eq!(f.format(3.0, None), "3 m");
        let g = StrMethodFormatter::new("{x}@{pos}");
        assert_eq!(g.format(2.0, Some(4)), "2@4");
    }

    #[test]
    fn view_limits_default_identity_for_multiple() {
        let (lo, hi) = MultipleLocator::new(0.5).view_limits(0.0, 2.0);
        assert!((lo - 0.0).abs() < 1e-9 && (hi - 2.0).abs() < 1e-9);
    }
}
