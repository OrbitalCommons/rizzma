//! Scalar-to-color mapping: a colormap, a norm, and the colors for values the
//! colormap cannot place — the state matplotlib keeps on a `ScalarMappable`.
//!
//! [`map_color`] is the one place a value becomes a color. A non-finite value,
//! or one the norm cannot place, takes the *bad* color (transparent unless
//! set). The norm maps the rest without clipping: below `0` takes the *under*
//! color and above `1` the *over* color, falling back to the colormap's ends
//! when they are unset, which is matplotlib's `Colormap.__call__`.

use super::{
    CmapSpec, Colormap, DEFAULT_COLORMAP, LinearNorm, LinearSegmentedColormap, ListedColormap,
    Norm, Normalize, Rgba,
};

/// Colors for values a colormap cannot place: matplotlib's
/// `Colormap.with_extremes(bad=…, over=…, under=…)`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Extremes {
    /// For NaN, infinite or masked values; transparent when `None`.
    pub bad: Option<Rgba>,
    /// For values above the norm's range; the colormap's top color when
    /// `None`.
    pub over: Option<Rgba>,
    /// For values below the norm's range; the colormap's bottom color when
    /// `None`.
    pub under: Option<Rgba>,
}

/// The color for `value` through `norm`, `cmap` and `extremes` (see the
/// module docs).
///
/// ```
/// use rizzma::core::color::{Extremes, LogNorm, Rgba, map_color, viridis};
/// let norm = LogNorm::new(1.0, 1000.0);
/// let ext = Extremes { bad: Some(Rgba::BLACK), over: Some(Rgba::WHITE), under: None };
/// assert_eq!(map_color(-1.0, &norm, &viridis(), &ext), Rgba::BLACK); // non-positive: bad
/// assert_eq!(map_color(5e3, &norm, &viridis(), &ext), Rgba::WHITE); // over
/// ```
#[must_use]
pub fn map_color(
    value: f64,
    norm: &dyn Normalize,
    cmap: &dyn Colormap,
    extremes: &Extremes,
) -> Rgba {
    let bad = extremes.bad.unwrap_or(Rgba::TRANSPARENT);
    if !value.is_finite() {
        return bad;
    }
    let t = norm.normalize_unclipped(value);
    if t.is_nan() {
        bad
    } else if t < 0.0 {
        extremes.under.unwrap_or_else(|| cmap.sample(0.0))
    } else if t > 1.0 {
        extremes.over.unwrap_or_else(|| cmap.sample(1.0))
    } else {
        cmap.sample(t)
    }
}

/// How scalar data becomes color — a named colormap, a norm (or linear
/// limits), and [`Extremes`] — for the plotting methods that take one, such
/// as [`Axes::scatter_mapped`](crate::figure::Axes::scatter_mapped) and
/// [`Axes::pcolormesh_mapped`](crate::figure::Axes::pcolormesh_mapped).
///
/// A colormap name converts into a mapping, so a bare `"viridis"` works
/// wherever a mapping is taken. Without a norm the data map linearly between
/// `vmin` and `vmax`, each defaulting to the finite data's own extreme; an
/// explicit norm carries its own limits and wins over them.
///
/// ```
/// use rizzma::core::color::{ColorMapping, LogNorm, Rgba};
/// let mapping = ColorMapping::new("magma")
///     .norm(LogNorm::new(1.0, 1e4))
///     .bad(Rgba::BLACK)
///     .over(Rgba::WHITE);
/// # let _ = mapping;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ColorMapping {
    cmap: CmapSpec,
    norm: Option<Norm>,
    vmin: Option<f64>,
    vmax: Option<f64>,
    extremes: Extremes,
}

impl Default for ColorMapping {
    fn default() -> Self {
        Self::new(DEFAULT_COLORMAP)
    }
}

impl From<&str> for ColorMapping {
    fn from(cmap: &str) -> Self {
        Self::new(cmap)
    }
}

impl From<String> for ColorMapping {
    fn from(cmap: String) -> Self {
        Self::new(cmap)
    }
}

impl From<CmapSpec> for ColorMapping {
    fn from(cmap: CmapSpec) -> Self {
        Self::new(cmap)
    }
}

impl From<ListedColormap> for ColorMapping {
    fn from(cmap: ListedColormap) -> Self {
        Self::new(cmap)
    }
}

impl From<LinearSegmentedColormap> for ColorMapping {
    fn from(cmap: LinearSegmentedColormap) -> Self {
        Self::new(cmap)
    }
}

impl ColorMapping {
    /// A linear mapping over the data's range through `cmap`: a colormap
    /// name (an unknown one falls back to the default map) or a colormap of
    /// your own.
    #[must_use]
    pub fn new(cmap: impl Into<CmapSpec>) -> Self {
        Self {
            cmap: cmap.into(),
            norm: None,
            vmin: None,
            vmax: None,
            extremes: Extremes::default(),
        }
    }

    /// Normalize through `norm`, which carries its own limits.
    #[must_use]
    pub fn norm(mut self, norm: impl Into<Norm>) -> Self {
        self.norm = Some(norm.into());
        self
    }

    /// Fix the lower limit of the linear mapping used when no norm is set.
    #[must_use]
    pub fn vmin(mut self, vmin: f64) -> Self {
        self.vmin = Some(vmin);
        self
    }

    /// Fix the upper limit of the linear mapping used when no norm is set.
    #[must_use]
    pub fn vmax(mut self, vmax: f64) -> Self {
        self.vmax = Some(vmax);
        self
    }

    /// Color NaN, infinite and unplaceable values `color`.
    #[must_use]
    pub fn bad(mut self, color: Rgba) -> Self {
        self.extremes.bad = Some(color);
        self
    }

    /// Color values above the range `color`.
    #[must_use]
    pub fn over(mut self, color: Rgba) -> Self {
        self.extremes.over = Some(color);
        self
    }

    /// Color values below the range `color`.
    #[must_use]
    pub fn under(mut self, color: Rgba) -> Self {
        self.extremes.under = Some(color);
        self
    }

    /// The colormap, by name or value.
    #[must_use]
    pub fn cmap(&self) -> &CmapSpec {
        &self.cmap
    }

    /// The norm this mapping applies to `data`: the explicit norm (with any
    /// limits it takes from the data fixed from `data`), else a linear one
    /// over `vmin`/`vmax`, each defaulting to the finite data's extreme
    /// (`(0, 1)` when there is none).
    #[must_use]
    pub fn resolved_norm(&self, data: &[f64]) -> Norm {
        if let Some(norm) = &self.norm {
            return norm.autoscaled(data);
        }
        let (lo, hi) = finite_range(data).unwrap_or((0.0, 1.0));
        Norm::Linear(LinearNorm::new(
            self.vmin.unwrap_or(lo),
            self.vmax.unwrap_or(hi),
        ))
    }

    /// This mapping fixed against `data`: its norm resolved, as the artist it
    /// colors remembers it for a colorbar.
    #[must_use]
    pub fn resolve(&self, data: &[f64]) -> ColorScale {
        ColorScale {
            cmap: self.cmap.clone(),
            norm: self.resolved_norm(data),
            extremes: self.extremes,
        }
    }

    /// The colors of `data` under this mapping.
    #[must_use]
    pub fn colors(&self, data: &[f64]) -> Vec<Rgba> {
        self.resolve(data).colors(data)
    }
}

/// A resolved color mapping — the colormap, a concrete [`Norm`], and
/// [`Extremes`] — as a colormapped artist keeps it so a
/// [`Colorbar`](crate::figure::Colorbar) can describe the artist exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorScale {
    /// The colormap, by name or value (see [`colormap`](super::colormap)).
    pub cmap: CmapSpec,
    /// The norm the data were mapped through.
    pub norm: Norm,
    /// Colors for bad, over and under values.
    pub extremes: Extremes,
}

impl ColorScale {
    /// The color of `value`.
    #[must_use]
    pub fn color(&self, value: f64) -> Rgba {
        map_color(
            value,
            &self.norm,
            self.cmap.resolve().as_ref(),
            &self.extremes,
        )
    }

    /// The colors of `data`.
    #[must_use]
    pub fn colors(&self, data: &[f64]) -> Vec<Rgba> {
        let cmap = self.cmap.resolve();
        data.iter()
            .map(|&v| map_color(v, &self.norm, cmap.as_ref(), &self.extremes))
            .collect()
    }
}

/// The named colormap, or the default map for an unknown name, so a stray
/// name never produces an invisible plot. The first time a name misses it
/// is reported, with the names to use instead (see [`warn_unknown_colormap`]).
pub(crate) fn resolve_cmap(name: &str) -> Box<dyn Colormap> {
    super::try_colormap(name).unwrap_or_else(|unknown| {
        warn_unknown_colormap(&unknown, "drawing with the default \"bgyw\"");
        Box::new(super::default_colormap())
    })
}

/// Report `unknown` and what happens instead (`consequence`), once per
/// distinct name per process: on stderr natively, `console.warn` in the
/// browser. Returns whether this call reported.
pub(crate) fn warn_unknown_colormap(unknown: &super::UnknownColormap, consequence: &str) -> bool {
    // A handful of names at most, so a list beats a set (and the wasm
    // bundle carries no B-tree for it).
    static REPORTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let first = REPORTED
        .lock()
        .map(|mut reported| {
            let new = !reported.iter().any(|name| name == unknown.name());
            if new {
                reported.push(unknown.name().to_owned());
            }
            new
        })
        .unwrap_or(false);
    if first {
        let message = format!("rizzma: {unknown} ({consequence})");
        #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
        console_warn(&message);
        #[cfg(not(all(feature = "wasm", target_arch = "wasm32")))]
        eprintln!("{message}");
    }
    first
}

// `console.warn`, bound directly: web-sys's `console` feature would carry
// a descriptor for every console binding into the wasm bundle.
#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
}

/// The `(min, max)` of the finite values in `data`, if any.
fn finite_range(data: &[f64]) -> Option<(f64, f64)> {
    data.iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(None, |range, v| {
            Some(range.map_or((v, v), |(lo, hi): (f64, f64)| (lo.min(v), hi.max(v))))
        })
}
