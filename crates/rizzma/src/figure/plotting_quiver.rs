//! [`quiver`](Axes::quiver): a 2D vector-field arrow plot.
//!
//! Mirrors matplotlib's `quiver` in its simplest form: at each base point
//! `(x, y)` an arrow is drawn pointing along the vector `(u, v)`. Each arrow is
//! a [`Line2D`] shaft plus a filled [`Patch`] triangle arrowhead, so both fold
//! into autoscaling via [`data_limits`](Axes::data_limits). Arrow lengths are
//! auto-scaled deterministically from the vector magnitudes (no RNG), so a
//! longer vector always yields a longer arrow.

use crate::artist::{Line2D, Patch};
use crate::core::color::Rgba;

use crate::figure::Axes;

/// Fraction of the data span used as the longest arrow's length.
const ARROW_LENGTH_FRACTION: f64 = 0.15;

/// Arrowhead length as a fraction of an individual arrow's length.
const HEAD_LENGTH_FRACTION: f64 = 0.35;

/// Arrowhead half-width as a fraction of an individual arrow's length.
const HEAD_HALF_WIDTH_FRACTION: f64 = 0.2;

impl Axes {
    /// Draw a 2D vector field: an arrow at each base `(x[i], y[i])` pointing
    /// along `(u[i], v[i])`.
    ///
    /// Mirrors matplotlib's `quiver`. The four slices give the arrow base
    /// positions (`x`, `y`) and the vector components (`u`, `v`); they should
    /// share a length `N`. Ragged inputs use the shortest length, and empty
    /// input draws nothing (never panics).
    ///
    /// Arrow lengths are auto-scaled deterministically so the longest vector
    /// spans `0.15` of the larger base-point span (`max(xmax - xmin,
    /// ymax - ymin)`, guarded to `1.0` when degenerate). With `mag[i] =
    /// hypot(u[i], v[i])` and `max_mag = max(mag)`, the scale is `s =
    /// L / max_mag` (or `0` when every vector is zero), giving each arrow a tip
    /// `T = (x[i] + u[i]*s, y[i] + v[i]*s)`. A longer vector therefore always
    /// produces a longer arrow.
    ///
    /// Each arrow is a [`Line2D`] shaft from the base to `T - hl*dir` (so the
    /// head is not doubled) plus a filled triangular [`Patch`] arrowhead at
    /// `T`, where `dir = (u, v)/mag`, head length `hl = 0.35 * len_i` and
    /// half-width `hw = 0.2 * len_i` for this arrow's length `len_i = mag[i]*s`.
    /// A zero-magnitude vector contributes no shaft or head. All arrows share a
    /// single color (the next property-cycle color), and the data limits expand
    /// to cover every base and every tip. Returns `&mut Self`.
    ///
    /// ![quiver](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_quiver.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A single rightward unit vector at the origin.
    /// ax.quiver(&[0.0], &[0.0], &[1.0], &[0.0]);
    /// let limits = ax.data_limits().expect("quiver contributes data limits");
    /// // Span is degenerate (one point), so it is guarded to 1.0 and the arrow
    /// // reaches x = 0.15.
    /// assert!((limits.xmax() - 0.15).abs() < 1e-9);
    /// ```
    pub fn quiver(&mut self, x: &[f64], y: &[f64], u: &[f64], v: &[f64]) -> &mut Self {
        let n = x.len().min(y.len()).min(u.len()).min(v.len());
        if n == 0 {
            return self;
        }

        // Per-vector magnitudes and the largest, for deterministic auto-scale.
        let mut max_mag = 0.0_f64;
        let mut mag = Vec::with_capacity(n);
        for i in 0..n {
            let m = u[i].hypot(v[i]);
            mag.push(m);
            if m > max_mag {
                max_mag = m;
            }
        }

        // Base-point span; guard a zero/degenerate extent to 1.0.
        let (mut xmin, mut xmax) = (x[0], x[0]);
        let (mut ymin, mut ymax) = (y[0], y[0]);
        for i in 0..n {
            xmin = xmin.min(x[i]);
            xmax = xmax.max(x[i]);
            ymin = ymin.min(y[i]);
            ymax = ymax.max(y[i]);
        }
        let span = (xmax - xmin).max(ymax - ymin);
        let span = if span > 0.0 { span } else { 1.0 };

        let target = ARROW_LENGTH_FRACTION * span;
        let scale = if max_mag > 0.0 { target / max_mag } else { 0.0 };

        let color = self.next_cycle_color();

        for i in 0..n {
            let base = [x[i], y[i]];
            let tip = [x[i] + u[i] * scale, y[i] + v[i] * scale];

            // A zero-magnitude vector has no direction: draw nothing for it.
            if mag[i] == 0.0 || scale == 0.0 {
                continue;
            }

            let len_i = mag[i] * scale;
            let dir = [u[i] / mag[i], v[i] / mag[i]];
            let perp = [-dir[1], dir[0]];
            let hl = HEAD_LENGTH_FRACTION * len_i;
            let hw = HEAD_HALF_WIDTH_FRACTION * len_i;

            // Shaft ends where the arrowhead begins so the head is not doubled.
            let neck = [tip[0] - hl * dir[0], tip[1] - hl * dir[1]];
            self.add_line(
                Line2D::new(vec![base[0], neck[0]], vec![base[1], neck[1]]).with_color(color),
            );

            // Filled triangle arrowhead at the tip, pointing along `dir`.
            let left = [neck[0] + hw * perp[0], neck[1] + hw * perp[1]];
            let right = [neck[0] - hw * perp[0], neck[1] - hw * perp[1]];
            let head = Patch::polygon(&[tip, left, right])
                .facecolor(Some(color))
                .edgecolor(Some(color));
            self.add_patch(head);
        }

        self
    }

    /// Draw a 2D vector field as matplotlib's `quiver` does, with `options`
    /// for the arrow length scale, its units, the angle convention, and the
    /// arrow and head sizes.
    ///
    /// Arrows are laid out in display space at draw time, exactly as
    /// matplotlib's `Quiver` lays them out: heads keep their shape on axes
    /// whose x and y scales differ, and with
    /// `scale_units(QuiverUnits::Xy).angles(QuiverAngles::Xy).scale(1.0)` an
    /// arrow `(u, v)` spans exactly `(u, v)` in data units. Without an
    /// explicit scale, arrows auto-scale as matplotlib's do. Vectors with a
    /// non-finite component are not drawn. The arrows take the options'
    /// color (black by default, as in matplotlib) and only the base points
    /// count toward autoscaling. [`quiver`](Axes::quiver) keeps its simpler
    /// auto-scaled arrows.
    ///
    /// ![quiver options](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_quiver_options.png)
    ///
    /// ```
    /// use rizzma::Figure;
    /// use rizzma::figure::{QuiverAngles, QuiverOptions, QuiverUnits};
    ///
    /// let mut fig = Figure::new(5.0, 3.0);
    /// let ax = fig.add_subplot(1, 1, 1);
    /// // Each arrow ends where its vector, in data units, says.
    /// ax.quiver_with(
    ///     &[0.0, 1.0],
    ///     &[0.0, 0.0],
    ///     &[1.0, 0.5],
    ///     &[2.0, 1.0],
    ///     &QuiverOptions::new()
    ///         .scale(1.0)
    ///         .scale_units(QuiverUnits::Xy)
    ///         .angles(QuiverAngles::Xy),
    /// );
    /// ax.set_xlim(-1.0, 3.0).set_ylim(-1.0, 3.0);
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn quiver_with(
        &mut self,
        x: &[f64],
        y: &[f64],
        u: &[f64],
        v: &[f64],
        options: &QuiverOptions,
    ) -> &mut Quiver {
        let n = x.len().min(y.len()).min(u.len()).min(v.len());
        let quiver = Quiver {
            x: x[..n].to_vec(),
            y: y[..n].to_vec(),
            u: u[..n].to_vec(),
            v: v[..n].to_vec(),
            options: options.clone(),
            zorder: 1.0,
            visible: true,
        };
        self.quivers.push(quiver);
        self.quivers.last_mut().expect("just pushed a quiver")
    }
}

/// The units a quiver's arrow width (and scale) are measured in:
/// matplotlib's `units` and `scale_units`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuiverUnits {
    /// The axes width in pixels (matplotlib's default `"width"`).
    #[default]
    Width,
    /// The axes height in pixels.
    Height,
    /// Display pixels.
    Dots,
    /// Inches.
    Inches,
    /// x data units.
    X,
    /// y data units.
    Y,
    /// Data units along the axes diagonal (`"xy"`).
    Xy,
}

/// How a quiver's arrow angles are read: matplotlib's `angles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuiverAngles {
    /// From the vector components as screen directions (`"uv"`).
    #[default]
    Uv,
    /// In data space, so an arrow points from `(x, y)` to `(x + u, y + v)`
    /// whatever the axes' aspect (`"xy"`).
    Xy,
}

/// Which part of an arrow sits on its point: matplotlib's `pivot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuiverPivot {
    /// The tail (the default).
    #[default]
    Tail,
    /// The middle.
    Middle,
    /// The tip.
    Tip,
}

/// Options for [`Axes::quiver_with`], with matplotlib's defaults and
/// meanings: `scale`, `scale_units`, `angles`, `units`, `width`,
/// `headwidth`, `headlength`, `headaxislength`, `minshaft`, `minlength`,
/// `pivot` and `color`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuiverOptions {
    scale: Option<f64>,
    scale_units: Option<QuiverUnits>,
    angles: QuiverAngles,
    units: QuiverUnits,
    width: Option<f64>,
    headwidth: f64,
    headlength: f64,
    headaxislength: f64,
    minshaft: f64,
    minlength: f64,
    pivot: QuiverPivot,
    color: Rgba,
}

impl Default for QuiverOptions {
    fn default() -> Self {
        Self {
            scale: None,
            scale_units: None,
            angles: QuiverAngles::Uv,
            units: QuiverUnits::Width,
            width: None,
            headwidth: 3.0,
            headlength: 5.0,
            headaxislength: 4.5,
            minshaft: 1.0,
            minlength: 1.0,
            pivot: QuiverPivot::Tail,
            color: Rgba::BLACK,
        }
    }
}

impl QuiverOptions {
    /// matplotlib's defaults: auto-scaled black arrows, widths in axes-width
    /// units, angles from the vector components.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Data units per arrow length unit: larger values give shorter arrows.
    /// Auto-scaled when unset.
    #[must_use]
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = Some(scale);
        self
    }

    /// The units the arrow length is scaled in; by default those of the
    /// arrow width.
    #[must_use]
    pub fn scale_units(mut self, units: QuiverUnits) -> Self {
        self.scale_units = Some(units);
        self
    }

    /// How the arrow angles are read.
    #[must_use]
    pub fn angles(mut self, angles: QuiverAngles) -> Self {
        self.angles = angles;
        self
    }

    /// The units of the arrow width and every other arrow dimension.
    #[must_use]
    pub fn units(mut self, units: QuiverUnits) -> Self {
        self.units = units;
        self
    }

    /// The shaft width, in `units`; by default from the axes size and the
    /// number of arrows.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    /// The head width, as a multiple of the shaft width (default 3).
    #[must_use]
    pub fn headwidth(mut self, headwidth: f64) -> Self {
        self.headwidth = headwidth;
        self
    }

    /// The head length, as a multiple of the shaft width (default 5).
    #[must_use]
    pub fn headlength(mut self, headlength: f64) -> Self {
        self.headlength = headlength;
        self
    }

    /// The head length at the shaft, as a multiple of the shaft width
    /// (default 4.5).
    #[must_use]
    pub fn headaxislength(mut self, headaxislength: f64) -> Self {
        self.headaxislength = headaxislength;
        self
    }

    /// Below this length, in head lengths, an arrow shrinks whole rather
    /// than losing its shaft (default 1).
    #[must_use]
    pub fn minshaft(mut self, minshaft: f64) -> Self {
        self.minshaft = minshaft;
        self
    }

    /// Below this length, in shaft widths, an arrow becomes a dot (default 1).
    #[must_use]
    pub fn minlength(mut self, minlength: f64) -> Self {
        self.minlength = minlength;
        self
    }

    /// Which part of the arrow sits on its point.
    #[must_use]
    pub fn pivot(mut self, pivot: QuiverPivot) -> Self {
        self.pivot = pivot;
        self
    }

    /// The arrow color (black by default).
    #[must_use]
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = color;
        self
    }
}

/// A matplotlib-style vector field drawn by [`Axes::quiver_with`]: its
/// points, vectors and options, laid out in display space when drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Quiver {
    pub(crate) x: Vec<f64>,
    pub(crate) y: Vec<f64>,
    pub(crate) u: Vec<f64>,
    pub(crate) v: Vec<f64>,
    pub(crate) options: QuiverOptions,
    pub(crate) zorder: f64,
    pub(crate) visible: bool,
}

/// What a quiver needs from its axes to lay out its arrows: matplotlib's
/// `axes.bbox`, `viewLim`, `dataLim`, the figure DPI and the data transform.
pub(crate) struct QuiverFrame<'a> {
    /// The axes rectangle in display pixels.
    pub(crate) axes_px: crate::core::Bbox,
    /// The view limits in data units.
    pub(crate) xlim: (f64, f64),
    pub(crate) ylim: (f64, f64),
    /// The largest magnitude among the data limits' extents.
    pub(crate) data_extent: f64,
    /// Pixels per inch.
    pub(crate) dpi: f64,
    /// Data point to display pixel (y-up).
    pub(crate) to_display: &'a dyn Fn(f64, f64) -> [f64; 2],
}

impl Quiver {
    /// Set the stacking order; higher draws on top.
    pub fn set_zorder(&mut self, zorder: f64) -> &mut Self {
        self.zorder = zorder;
        self
    }

    /// Show or hide the arrows.
    pub fn set_visible(&mut self, visible: bool) -> &mut Self {
        self.visible = visible;
        self
    }

    /// The base points' data extent, for autoscaling (the arrows themselves
    /// do not count, as in matplotlib).
    pub(crate) fn data_extents(&self) -> Option<crate::core::Bbox> {
        let mut points = self
            .x
            .iter()
            .zip(&self.y)
            .filter(|(x, y)| x.is_finite() && y.is_finite());
        let (&x0, &y0) = points.next()?;
        let (lo, hi) = points.fold(([x0, y0], [x0, y0]), |(lo, hi), (&x, &y)| {
            ([lo[0].min(x), lo[1].min(y)], [hi[0].max(x), hi[1].max(y)])
        });
        Some(crate::core::Bbox::from_extents(lo[0], lo[1], hi[0], hi[1]))
    }

    /// Pixels per `units` (matplotlib's `Quiver._dots_per_unit`).
    fn dots_per_unit(units: QuiverUnits, frame: &QuiverFrame<'_>) -> f64 {
        let (bw, bh) = (frame.axes_px.width(), frame.axes_px.height());
        let (vw, vh) = (frame.xlim.1 - frame.xlim.0, frame.ylim.1 - frame.ylim.0);
        match units {
            QuiverUnits::X => bw / vw,
            QuiverUnits::Y => bh / vh,
            QuiverUnits::Xy => bw.hypot(bh) / vw.hypot(vh),
            QuiverUnits::Width => bw,
            QuiverUnits::Height => bh,
            QuiverUnits::Dots => 1.0,
            QuiverUnits::Inches => frame.dpi,
        }
    }

    /// Each drawable arrow's closed outline in display pixels, as
    /// matplotlib's `Quiver._make_verts` and `_h_arrows` build it.
    pub(crate) fn polygons(&self, frame: &QuiverFrame<'_>) -> Vec<Vec<[f64; 2]>> {
        let o = &self.options;
        let n = self.x.len();
        if n == 0 {
            return Vec::new();
        }
        let masked: Vec<bool> = (0..n)
            .map(|i| !(self.u[i].is_finite() && self.v[i].is_finite()))
            .collect();
        // matplotlib fills masked components with 1 before measuring.
        let fill = |w: f64, m: bool| if m { 1.0 } else { w };
        let u: Vec<f64> = (0..n).map(|i| fill(self.u[i], masked[i])).collect();
        let v: Vec<f64> = (0..n).map(|i| fill(self.v[i], masked[i])).collect();

        let trans_scale = Self::dots_per_unit(o.units, frame);
        let span = frame.axes_px.width() / trans_scale;
        let width = o.width.unwrap_or_else(|| {
            let sn = (n as f64).sqrt().clamp(8.0, 25.0);
            0.06 * span / sn
        });

        let xy_scaled = o.scale_units == Some(QuiverUnits::Xy);
        let xy_angles = o.angles == QuiverAngles::Xy;
        let measured = (xy_angles || xy_scaled).then(|| {
            let eps = if xy_angles && xy_scaled {
                1.0
            } else {
                frame.data_extent * 0.001
            };
            (0..n)
                .map(|i| {
                    let p0 = (frame.to_display)(self.x[i], self.y[i]);
                    let p1 = (frame.to_display)(self.x[i] + eps * u[i], self.y[i] + eps * v[i]);
                    let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
                    (dy.atan2(dx), dx.hypot(dy) / eps)
                })
                .collect::<Vec<_>>()
        });
        let a: Vec<f64> = match (&measured, xy_scaled) {
            (Some(m), true) => m.iter().map(|&(_, len)| len).collect(),
            _ => (0..n).map(|i| u[i].hypot(v[i])).collect(),
        };
        let auto_scale = || {
            let sn = (n as f64).sqrt().max(10.0);
            let live: Vec<f64> = (0..n).filter(|&i| !masked[i]).map(|i| a[i]).collect();
            let amean = live.iter().sum::<f64>() / live.len() as f64;
            1.8 * amean * sn / span
        };
        let (scale, widthu_per_lenu) = match o.scale_units {
            None => (o.scale.unwrap_or_else(auto_scale), 1.0),
            Some(units) => {
                let dx = if units == QuiverUnits::Xy {
                    1.0
                } else {
                    Self::dots_per_unit(units, frame)
                };
                let per = dx / trans_scale;
                (o.scale.unwrap_or_else(|| auto_scale() * per), per)
            }
        };

        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if masked[i] || !(self.x[i].is_finite() && self.y[i].is_finite()) {
                continue;
            }
            let length = a[i] * (widthu_per_lenu / (scale * width));
            let theta = match &measured {
                Some(m) if xy_angles => m[i].0,
                _ => v[i].atan2(u[i]),
            };
            let (sin, cos) = theta.sin_cos();
            let base = (frame.to_display)(self.x[i], self.y[i]);
            let outline = self
                .arrow(length)
                .into_iter()
                .map(|[ax, ay]| {
                    let (rx, ry) = (ax * cos - ay * sin, ax * sin + ay * cos);
                    [
                        base[0] + trans_scale * width * rx,
                        base[1] + trans_scale * width * ry,
                    ]
                })
                .collect();
            out.push(outline);
        }
        out
    }

    /// One horizontal arrow `length` shaft widths long, in shaft-width units
    /// (matplotlib's `Quiver._h_arrows` for a single arrow).
    fn arrow(&self, length: f64) -> [[f64; 2]; 8] {
        let o = &self.options;
        let minsh = o.minshaft * o.headlength;
        let length = length.clamp(0.0, 65536.0);
        let order = [0, 1, 2, 3, 2, 1, 0, 0];
        let lower = |k: usize| (3..7).contains(&k);
        let build = |xs: [f64; 4], ys: [f64; 4]| {
            let mut pts = [[0.0; 2]; 8];
            for (k, &j) in order.iter().enumerate() {
                pts[k] = [xs[j], if lower(k) { -ys[j] } else { ys[j] }];
            }
            pts
        };
        let ys = [0.5, 0.5, 0.5 * o.headwidth, 0.0];
        let mut pts = if length < minsh {
            let shrink = if minsh == 0.0 { 0.0 } else { length / minsh };
            let xs = [0.0, minsh - o.headaxislength, minsh - o.headlength, minsh];
            build(xs.map(|x| x * shrink), ys.map(|y| y * shrink))
        } else {
            build(
                [
                    0.0,
                    length - o.headaxislength,
                    length - o.headlength,
                    length,
                ],
                ys,
            )
        };
        let tip = pts[3][0];
        match o.pivot {
            QuiverPivot::Tail => {}
            QuiverPivot::Middle => pts.iter_mut().for_each(|p| p[0] -= 0.5 * tip),
            QuiverPivot::Tip => pts.iter_mut().for_each(|p| p[0] -= tip),
        }
        if length < o.minlength {
            // A heptagonal dot for an arrow too short to draw.
            for (k, p) in pts.iter_mut().enumerate() {
                let th = k as f64 * std::f64::consts::PI / 3.0;
                *p = [th.cos() * o.minlength * 0.5, th.sin() * o.minlength * 0.5];
            }
        }
        pts
    }

    /// Flatten this quiver into its wire form, banking the data.
    #[cfg(feature = "portable")]
    pub(crate) fn to_portable(
        &self,
        bank: &mut crate::portable::data::BankWriter,
    ) -> crate::portable::spec::QuiverSpec {
        crate::portable::spec::QuiverSpec {
            x: bank.push_f64(&self.x),
            y: bank.push_f64(&self.y),
            u: bank.push_f64(&self.u),
            v: bank.push_f64(&self.v),
            options: self.options.clone(),
            zorder: self.zorder,
            visible: self.visible,
        }
    }

    /// Rebuild a quiver from its wire form; its four arrays must agree.
    #[cfg(feature = "portable")]
    pub(crate) fn from_portable(
        spec: &crate::portable::spec::QuiverSpec,
        reader: &crate::portable::data::BankReader<'_>,
    ) -> Result<Quiver, crate::portable::PortableError> {
        let (x, y, u, v) = (
            reader.f64s(&spec.x)?,
            reader.f64s(&spec.y)?,
            reader.f64s(&spec.u)?,
            reader.f64s(&spec.v)?,
        );
        if !(x.len() == y.len() && y.len() == u.len() && u.len() == v.len()) {
            return Err(crate::portable::PortableError::Malformed(
                "quiver arrays differ in length".to_owned(),
            ));
        }
        Ok(Quiver {
            x,
            y,
            u,
            v,
            options: spec.options.clone(),
            zorder: spec.zorder,
            visible: spec.visible,
        })
    }

    /// Fill each arrow (matplotlib draws them with no edge).
    pub(crate) fn draw(&self, renderer: &mut dyn crate::render::Renderer, frame: &QuiverFrame<'_>) {
        if !self.visible {
            return;
        }
        let gc = crate::render::GraphicsContext::new();
        for outline in self.polygons(frame) {
            renderer.draw_path(
                &gc,
                &crate::core::Path::from_polyline(&outline),
                &crate::core::Affine2D::identity(),
                Some(self.options.color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::Bbox;

    use crate::figure::Axes;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn quiver_adds_one_shaft_and_head_per_arrow() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 0.0, 0.0];
        let u = [1.0, 0.0, 1.0];
        let v = [0.0, 1.0, 1.0];
        ax.quiver(&x, &y, &u, &v);
        // One shaft (Line2D) and one head (Patch) per non-zero arrow.
        assert_eq!(ax.lines.len(), 3);
        assert_eq!(ax.patches.len(), 3);
    }

    #[test]
    fn quiver_longer_vector_yields_longer_arrow() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Two arrows along +x from different bases so the span is non-zero;
        // the second vector is longer.
        let x = [0.0, 10.0];
        let y = [0.0, 0.0];
        let u = [1.0, 3.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        // Each shaft is a horizontal segment; compare their lengths.
        let len0 = {
            let p = ax.lines[0].points();
            (p[1][0] - p[0][0]).abs()
        };
        let len1 = {
            let p = ax.lines[1].points();
            (p[1][0] - p[0][0]).abs()
        };
        assert!(len1 > len0, "longer vector must give a longer shaft");
    }

    #[test]
    fn quiver_zero_vector_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0];
        let y = [0.0, 0.0];
        let u = [0.0, 1.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        // Only the non-zero vector contributes a shaft and head.
        assert_eq!(ax.lines.len(), 1);
        assert_eq!(ax.patches.len(), 1);
    }

    #[test]
    fn quiver_data_limits_cover_bases_and_tips() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Bases span x in [0, 1]; both vectors point in +x, so the rightmost
        // tip (from the base at x = 1) pushes xmax past the base extent while
        // the leftmost base at x = 0 fixes xmin.
        let x = [0.0, 1.0];
        let y = [0.0, 0.0];
        let u = [1.0, 1.0];
        let v = [0.0, 0.0];
        ax.quiver(&x, &y, &u, &v);
        let limits = ax.data_limits().expect("quiver contributes data limits");
        // Leftmost base sits at x = 0.
        approx(limits.xmin(), 0.0);
        // span = 1.0 (x in [0, 1]); target = 0.15; max_mag = 1; tip at 1.15.
        approx(limits.xmax(), 1.15);
    }

    #[test]
    fn quiver_empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.quiver(&[], &[], &[], &[]);
        assert!(ax.lines.is_empty());
        assert!(ax.patches.is_empty());
        assert!(ax.data_limits().is_none());
    }

    use super::{QuiverAngles, QuiverFrame, QuiverOptions, QuiverPivot, QuiverUnits};
    use crate::figure::Figure;

    const X: [f64; 7] = [0.0, 1.5, 3.0, 0.0, 1.5, 3.0, 4.2];
    const Y: [f64; 7] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.6];
    const U: [f64; 7] = [1.0, 0.5, -0.8, 0.02, 1.2, 0.3, 0.001];
    const V: [f64; 7] = [0.5, 1.0, 0.4, 0.01, -0.6, 0.3, 0.0];

    /// The arrows' display outlines for `options` on the fixture's unequal
    /// axes, laid out as drawing lays them out.
    fn outlines(options: &QuiverOptions) -> Vec<Vec<[f64; 2]>> {
        let mut fig = Figure::new(6.0, 3.5);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.quiver_with(&X, &Y, &U, &V, options);
        ax.set_xlim(-1.0, 5.0).set_ylim(-1.0, 2.0);
        let ax = &fig.axes()[0];
        let (axes_px, td) = ax.pixel_rect_and_trans_data_in(600.0, 350.0, None, None);
        let (xlim, ylim) = ax.limits_with_override(None);
        let to_display = |x: f64, y: f64| {
            let (px, py) = td.transform_point((x, y));
            [px, py]
        };
        let b = ax.data_limits().expect("quiver points");
        let frame = QuiverFrame {
            axes_px,
            xlim,
            ylim,
            data_extent: b
                .xmin()
                .abs()
                .max(b.xmax().abs())
                .max(b.ymin().abs())
                .max(b.ymax().abs()),
            dpi: 100.0,
            to_display: &to_display,
        };
        ax.quivers[0].polygons(&frame)
    }

    #[test]
    fn quiver_with_matches_matplotlib_geometry() {
        let reference = include_str!("../../tests/fixtures/quiver_mpl.txt");
        let cases = [
            ("auto_width", QuiverOptions::new()),
            ("s10_width", QuiverOptions::new().scale(10.0)),
            ("s30_width", QuiverOptions::new().scale(30.0)),
            ("auto_xy", QuiverOptions::new().units(QuiverUnits::Xy)),
            (
                "s10_xy",
                QuiverOptions::new().units(QuiverUnits::Xy).scale(10.0),
            ),
            (
                "s30_xy",
                QuiverOptions::new().units(QuiverUnits::Xy).scale(30.0),
            ),
            (
                "xy_xy_1",
                QuiverOptions::new()
                    .scale_units(QuiverUnits::Xy)
                    .angles(QuiverAngles::Xy)
                    .scale(1.0),
            ),
            (
                "mid_inches",
                QuiverOptions::new()
                    .units(QuiverUnits::Inches)
                    .width(0.02)
                    .pivot(QuiverPivot::Middle)
                    .scale(8.0),
            ),
        ];
        for (name, options) in cases {
            let ours = outlines(&options);
            let theirs: Vec<Vec<f64>> = reference
                .lines()
                .filter(|l| l.starts_with(&format!("{name} ")))
                .map(|l| {
                    l.split_whitespace()
                        .skip(2)
                        .map(|v| v.parse().unwrap())
                        .collect()
                })
                .collect();
            assert_eq!(ours.len(), theirs.len(), "{name}: arrow count");
            for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
                for (k, p) in a.iter().enumerate() {
                    let (dx, dy) = (p[0] - b[2 * k], p[1] - b[2 * k + 1]);
                    assert!(
                        dx.abs() < 1e-6 && dy.abs() < 1e-6,
                        "{name} arrow {i} vertex {k}: {p:?} vs ({}, {})",
                        b[2 * k],
                        b[2 * k + 1]
                    );
                }
            }
        }
    }

    #[test]
    fn xy_scaled_arrows_span_their_vector_in_data_units() {
        let options = QuiverOptions::new()
            .scale_units(QuiverUnits::Xy)
            .angles(QuiverAngles::Xy)
            .scale(1.0);
        let mut fig = Figure::new(6.0, 3.5);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.quiver_with(&X, &Y, &U, &V, &options);
        ax.set_xlim(-1.0, 5.0).set_ylim(-1.0, 2.0);
        let ax = &fig.axes()[0];
        let (_, td) = ax.pixel_rect_and_trans_data_in(600.0, 350.0, None, None);
        for (i, outline) in outlines(&options).iter().enumerate() {
            // Vertex 3 is the tip of an arrow long enough for its shaft.
            let tip = td.transform_point((X[i] + U[i], Y[i] + V[i]));
            if U[i].hypot(V[i]) > 0.1 {
                let p = outline[3];
                assert!(
                    (p[0] - tip.0).abs() < 1e-9 && (p[1] - tip.1).abs() < 1e-9,
                    "arrow {i}: tip {p:?} vs data tip {tip:?}"
                );
            }
        }
    }
}
