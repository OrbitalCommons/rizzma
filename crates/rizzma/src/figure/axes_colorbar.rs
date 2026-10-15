//! Colorbars bound to an axes and one of its colormapped artists —
//! matplotlib's `fig.colorbar(mappable, ax=ax)`.
//!
//! [`Axes::colorbar`] attaches a [`Colorbar`] describing a [`Mappable`]: an
//! image, a colormapped mesh, or a colormapped collection of that axes. The
//! colorbar takes its room from the axes' own frame, as matplotlib's
//! `colorbar.make_axes` splits the parent — `fraction` of the frame for the
//! bar's slot and `pad` between them — so every other axes keeps its size. The
//! bar keeps an `aspect`:1 long-to-short ratio inside its slot, anchored
//! against the parent. It reads the artist's color scale at every draw, so it
//! cannot drift from what it describes.
//!
//! The long axis follows the norm the way matplotlib's colorbar picks a scale:
//! linear ticks for a linear norm, decades for `LogNorm`, symlog ticks for
//! `SymLogNorm`, regions of equal length with a tick per boundary for
//! `BoundaryNorm`, and for the other norms (two-slope, centered, power) data
//! ticks placed through the norm itself. The number of linear ticks follows
//! the bar's length as matplotlib's `nbins="auto"` does.

use crate::axis::axis::{Axis, AxisSide};
use crate::axis::scale::{LinearScale, LogScale, Scale, SymlogScale};
use crate::axis::ticker::{
    FixedLocator, FuncFormatter, LogLocator, MaxNLocator, NBins, ScalarFormatter,
    SymlogFormatterMathtext, SymlogLocator,
};
use crate::core::color::mapping::resolve_cmap;
use crate::core::color::{ColorScale, Norm, Normalize, Rgba};
use crate::core::{Affine2D, Bbox, Path};
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

/// Number of color slices along the bar: one per colormap lookup entry.
const SLICES: usize = 256;

/// matplotlib's default tick-label size, 10 pt. Its `nbins="auto"` fits one
/// tick per two label heights of axis length, so the linear tick count of a
/// colorbar follows its length the same way here.
const MPL_TICK_LABEL_PT: f64 = 10.0;

/// Which side of its axes a colorbar sits on — matplotlib's `location`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorbarLocation {
    /// A vertical bar right of the axes, ticks on its right (the default).
    #[default]
    Right,
    /// A vertical bar left of the axes, ticks on its left.
    Left,
    /// A horizontal bar above the axes, ticks on top.
    Top,
    /// A horizontal bar below the axes, ticks below.
    Bottom,
}

impl ColorbarLocation {
    /// matplotlib's default gap between the axes and the bar's slot, as a
    /// fraction of the axes frame.
    fn default_pad(self) -> f64 {
        match self {
            ColorbarLocation::Right | ColorbarLocation::Top => 0.05,
            ColorbarLocation::Left => 0.10,
            ColorbarLocation::Bottom => 0.15,
        }
    }

    fn is_vertical(self) -> bool {
        matches!(self, ColorbarLocation::Right | ColorbarLocation::Left)
    }
}

/// Which colormapped artist of an axes a colorbar describes, by its index
/// among the axes' artists of that kind (in the order they were added).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mappable {
    /// An image from [`imshow`](crate::figure::Axes::imshow) and its kin.
    Image(usize),
    /// A mesh from [`pcolormesh`](crate::figure::Axes::pcolormesh) or
    /// [`pcolormesh_mapped`](crate::figure::Axes::pcolormesh_mapped).
    Mesh(usize),
    /// A collection from
    /// [`scatter_mapped`](crate::figure::Axes::scatter_mapped).
    Collection(usize),
    /// A polygon collection from
    /// [`poly_collection`](crate::figure::Axes::poly_collection).
    Polygons(usize),
}

/// A colorbar attached to an axes, describing one of its colormapped
/// artists — matplotlib's `fig.colorbar(mappable, ax=ax)`.
///
/// Returned by [`Axes::colorbar`](crate::figure::Axes::colorbar) for
/// styling. The bar takes `fraction` of its axes' frame for its slot, with
/// `pad` between the two, as matplotlib's `colorbar.make_axes` splits the
/// parent, so other axes keep their size; it keeps an `aspect`:1
/// long-to-short ratio in its slot, anchored against the parent. Its long
/// axis follows the artist's norm: linear ticks for a linear norm, decades
/// for `LogNorm`, symlog ticks for `SymLogNorm`, equal regions with a tick per
/// boundary for `BoundaryNorm`, and ticks placed through the norm for the
/// others. Defaults follow matplotlib: on the right, `fraction` 0.15, the
/// location's `pad` (0.05 right or top, 0.10 left, 0.15 bottom), `shrink` 1,
/// `aspect` 20.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colorbar {
    mappable: Mappable,
    location: ColorbarLocation,
    fraction: f64,
    pad: Option<f64>,
    shrink: f64,
    aspect: f64,
    label: Option<String>,
}

impl Colorbar {
    pub(crate) fn new(mappable: Mappable) -> Self {
        Self {
            mappable,
            location: ColorbarLocation::default(),
            fraction: 0.15,
            pad: None,
            shrink: 1.0,
            aspect: 20.0,
            label: None,
        }
    }

    /// The artist this colorbar describes.
    #[must_use]
    pub fn mappable(&self) -> Mappable {
        self.mappable
    }

    /// Which side of the axes the colorbar sits on.
    #[must_use]
    pub fn location(&self) -> ColorbarLocation {
        self.location
    }

    /// Move the colorbar to another side of its axes.
    pub fn set_location(&mut self, location: ColorbarLocation) -> &mut Self {
        self.location = location;
        self
    }

    /// The fraction of the axes frame the bar's slot takes (default 0.15).
    pub fn set_fraction(&mut self, fraction: f64) -> &mut Self {
        self.fraction = fraction;
        self
    }

    /// The gap between the axes and the bar's slot, as a fraction of the axes
    /// frame (default by location).
    pub fn set_pad(&mut self, pad: f64) -> &mut Self {
        self.pad = Some(pad);
        self
    }

    /// Scale the bar's length by `shrink` (default 1), keeping it centered.
    pub fn set_shrink(&mut self, shrink: f64) -> &mut Self {
        self.shrink = shrink;
        self
    }

    /// The bar's long-to-short ratio (default 20).
    pub fn set_aspect(&mut self, aspect: f64) -> &mut Self {
        self.aspect = aspect;
        self
    }

    /// Label the bar along its long axis, on the tick side.
    pub fn set_label(&mut self, label: impl Into<String>) -> &mut Self {
        self.label = Some(label.into());
        self
    }

    fn pad(&self) -> f64 {
        self.pad.unwrap_or_else(|| self.location.default_pad())
    }

    /// Split an axes `frame` (pixels, y up) into the axes' own frame and the
    /// bar: matplotlib's `make_axes`, then the bar's box aspect applied
    /// inside its slot against the location's anchor.
    pub(crate) fn split(&self, frame: &Bbox) -> (Bbox, Bbox) {
        let (f, p) = (self.fraction, self.pad());
        let (x0, y0, w, h) = (frame.xmin(), frame.ymin(), frame.width(), frame.height());
        let (parent, slot) = match self.location {
            ColorbarLocation::Right => (
                Bbox::from_extents(x0, y0, x0 + (1.0 - f - p) * w, frame.ymax()),
                Bbox::from_extents(x0 + (1.0 - f) * w, y0, frame.xmax(), frame.ymax()),
            ),
            ColorbarLocation::Left => (
                Bbox::from_extents(x0 + (f + p) * w, y0, frame.xmax(), frame.ymax()),
                Bbox::from_extents(x0, y0, x0 + f * w, frame.ymax()),
            ),
            ColorbarLocation::Top => (
                Bbox::from_extents(x0, y0, frame.xmax(), y0 + (1.0 - f - p) * h),
                Bbox::from_extents(x0, y0 + (1.0 - f) * h, frame.xmax(), frame.ymax()),
            ),
            ColorbarLocation::Bottom => (
                Bbox::from_extents(x0, y0 + (f + p) * h, frame.xmax(), frame.ymax()),
                Bbox::from_extents(x0, y0, frame.xmax(), y0 + f * h),
            ),
        };
        (parent, self.bar_in(&slot))
    }

    /// The bar inside its `slot`: shrunk along its length, then fitted to its
    /// aspect and anchored toward the parent (left edge of a right slot,
    /// right edge of a left one, bottom of a top one, top of a bottom one).
    fn bar_in(&self, slot: &Bbox) -> Bbox {
        let (sw, sh) = (slot.width(), slot.height());
        if self.location.is_vertical() {
            let len = sh * self.shrink;
            let (w, h) = if len / self.aspect <= sw {
                (len / self.aspect, len)
            } else {
                (sw, sw * self.aspect)
            };
            let y = slot.ymin() + (sh - h) / 2.0;
            let x = match self.location {
                ColorbarLocation::Left => slot.xmax() - w,
                _ => slot.xmin(),
            };
            Bbox::from_extents(x, y, x + w, y + h)
        } else {
            let len = sw * self.shrink;
            let (w, h) = if len / self.aspect <= sh {
                (len, len / self.aspect)
            } else {
                (sh * self.aspect, sh)
            };
            let x = slot.xmin() + (sw - w) / 2.0;
            let y = match self.location {
                ColorbarLocation::Bottom => slot.ymax() - h,
                _ => slot.ymin(),
            };
            Bbox::from_extents(x, y, x + w, y + h)
        }
    }

    /// The long axis for `scale` on a bar `length_px` long at decoration
    /// scale `s`, with the data range it spans, styled in `color`.
    fn long_axis(
        &self,
        scale: &ColorScale,
        length_px: f64,
        s: f64,
        color: Rgba,
    ) -> (Axis, (f64, f64)) {
        let side = match self.location {
            ColorbarLocation::Right => AxisSide::Right,
            ColorbarLocation::Left => AxisSide::Left,
            ColorbarLocation::Top => AxisSide::Top,
            ColorbarLocation::Bottom => AxisSide::Bottom,
        };
        let length_pt = length_px / (100.0 * s) * 72.0;
        let nbins = ((length_pt / (2.0 * MPL_TICK_LABEL_PT)).floor() as usize).clamp(1, 9);
        let linear_locator = || {
            Box::new(MaxNLocator::with_steps(
                NBins::Fixed(nbins),
                &[1.0, 2.0, 2.5, 5.0, 10.0],
                false,
                false,
                2,
            ))
        };
        let mut axis = Axis::new(side).with_color(color);
        let range = match &scale.norm {
            Norm::Linear(n) => {
                axis = axis
                    .with_scale(Box::new(LinearScale::new()))
                    .with_locator(linear_locator())
                    .with_formatter(Box::new(ScalarFormatter::new()));
                (n.vmin, n.vmax)
            }
            Norm::Log(n) => {
                axis = axis
                    .with_scale(Box::new(LogScale::new(10.0)))
                    .with_locator(Box::new(LogLocator::new(10.0)))
                    .with_formatter(Box::new(FuncFormatter::new(Box::new(sci_decade))));
                (n.vmin, n.vmax)
            }
            Norm::SymLog(n) => {
                axis = axis
                    .with_scale(Box::new(SymlogScale::new(n.base, n.linthresh, n.linscale)))
                    .with_locator(Box::new(SymlogLocator::new(n.base, n.linthresh)))
                    .with_formatter(Box::new(SymlogFormatterMathtext::new(n.base, n.linthresh)));
                (n.vmin, n.vmax)
            }
            Norm::Boundary(n) => {
                let b = &n.boundaries;
                let range = (
                    b.first().copied().unwrap_or(0.0),
                    b.last().copied().unwrap_or(1.0),
                );
                axis = axis
                    .with_scale(Box::new(BoundaryScale(b.clone())))
                    .with_locator(Box::new(FixedLocator::with_nbins(b.clone(), 10)))
                    .with_formatter(Box::new(ScalarFormatter::new()));
                range
            }
            other => {
                let range = (other.inverse(0.0), other.inverse(1.0));
                axis = axis
                    .with_scale(Box::new(NormScale(other.clone())))
                    .with_locator(linear_locator())
                    .with_formatter(Box::new(ScalarFormatter::new()));
                range
            }
        };
        if let Some(label) = &self.label {
            axis = axis.with_label(label.clone());
        }
        (axis, range)
    }

    /// The bar's ticks inside its range, with their labels, for a bar
    /// `length_px` long at decoration scale `s`.
    pub(crate) fn ticks(&self, scale: &ColorScale, length_px: f64, s: f64) -> Vec<(f64, String)> {
        let (axis, range) = self.long_axis(scale, length_px, s, Rgba::BLACK);
        let (values, labels) = axis.visible_ticks(range);
        values.into_iter().zip(labels).collect()
    }

    /// How far the bar's ticks, labels and label reach past it, in pixels at
    /// decoration scale `s` — what tight layout reserves on its side.
    pub(crate) fn decoration_extent(
        &self,
        scale: &ColorScale,
        length_px: f64,
        font: &FontSource,
        s: f64,
    ) -> f64 {
        let (axis, range) = self.long_axis(scale, length_px, s, Rgba::BLACK);
        axis.decoration_extent(range, font, s)
    }

    /// The side tight layout must reserve the decoration on:
    /// `(left, right, bottom, top)` with the extent in its slot.
    pub(crate) fn inset(&self, extent: f64) -> (f64, f64, f64, f64) {
        match self.location {
            ColorbarLocation::Right => (0.0, extent, 0.0, 0.0),
            ColorbarLocation::Left => (extent, 0.0, 0.0, 0.0),
            ColorbarLocation::Top => (0.0, 0.0, 0.0, extent),
            ColorbarLocation::Bottom => (0.0, 0.0, extent, 0.0),
        }
    }

    /// Draw the bar into `bar` (pixels, y up): the gradient through `scale`,
    /// its frame, and the long axis, in the parent axes' `ink`.
    pub(crate) fn draw(
        &self,
        renderer: &mut dyn Renderer,
        bar: &Bbox,
        scale: &ColorScale,
        font: &FontSource,
        ink: &BarInk,
    ) {
        let s = renderer.decoration_scale();
        let vertical = self.location.is_vertical();
        let length = if vertical { bar.height() } else { bar.width() };
        let (axis, range) = self.long_axis(scale, length, s, ink.color);

        // Slice i spans [i, i + 1] / SLICES of the long axis, which the
        // axis' scale maps back to data; each takes that data's color.
        let cmap = resolve_cmap(&scale.cmap);
        let to_long = axis.scale();
        let (lo, hi) = (to_long.transform(range.0), to_long.transform(range.1));
        let id = Affine2D::identity();
        for i in 0..SLICES {
            let (a, b) = (i as f64 / SLICES as f64, (i + 1) as f64 / SLICES as f64);
            let mid = to_long.inverse(lo + (a + b) / 2.0 * (hi - lo));
            let fill = cmap.sample(scale.norm.normalize(mid));
            // Overlap each slice by half a pixel so no seams show.
            let rect = if vertical {
                Bbox::from_extents(
                    bar.xmin(),
                    bar.ymin() + a * length,
                    bar.xmax(),
                    (bar.ymin() + b * length + 0.5).min(bar.ymax()),
                )
            } else {
                Bbox::from_extents(
                    bar.xmin() + a * length,
                    bar.ymin(),
                    (bar.xmin() + b * length + 0.5).min(bar.xmax()),
                    bar.ymax(),
                )
            };
            renderer.draw_path(&GraphicsContext::new(), &rect_path(&rect), &id, Some(fill));
        }

        let frame = GraphicsContext::new()
            .with_stroke(ink.edge)
            .with_line_width(ink.linewidth);
        renderer.draw_path(&frame, &rect_path(bar), &id, None);
        axis.draw(renderer, bar, range, font);
    }
}

/// The parent axes' ink a colorbar draws in: tick and label `color`, and the
/// frame's `edge` color and `linewidth`.
pub(crate) struct BarInk {
    pub(crate) color: Rgba,
    pub(crate) edge: Rgba,
    pub(crate) linewidth: f64,
}

/// A log colorbar's tick label: a decade as `$10^{k}$`, as matplotlib's
/// `LogFormatterSciNotation` writes every decade (rizzma's log axes write the
/// small ones as plain numbers), anything else as a plain number.
fn sci_decade(value: f64, _pos: Option<usize>) -> String {
    let exponent = value.log10().round();
    if value > 0.0 && (10f64.powf(exponent) - value).abs() <= 1e-9 * value {
        format!("$10^{{{exponent}}}$")
    } else {
        format!("{value}")
    }
}

/// A scale that is a norm: data map to the norm's `[0, 1]` (matplotlib's
/// `"function"` colorbar scale with `(norm, norm.inverse)`).
struct NormScale(Norm);

impl Scale for NormScale {
    fn transform(&self, x: f64) -> f64 {
        self.0.normalize_unclipped(x)
    }

    fn inverse(&self, x: f64) -> f64 {
        self.0.inverse(x)
    }
}

/// matplotlib's `spacing="uniform"` for a `BoundaryNorm`: each region between
/// consecutive boundaries takes an equal share of the bar.
struct BoundaryScale(Vec<f64>);

impl Scale for BoundaryScale {
    fn transform(&self, x: f64) -> f64 {
        let b = &self.0;
        let regions = b.len().saturating_sub(1);
        if regions == 0 {
            return 0.0;
        }
        let k = b.partition_point(|&edge| edge <= x).clamp(1, regions) - 1;
        let span = b[k + 1] - b[k];
        let within = if span == 0.0 { 0.0 } else { (x - b[k]) / span };
        (k as f64 + within) / regions as f64
    }

    fn inverse(&self, y: f64) -> f64 {
        let b = &self.0;
        let regions = b.len().saturating_sub(1);
        if regions == 0 {
            return b.first().copied().unwrap_or(0.0);
        }
        let pos = y.clamp(0.0, 1.0) * regions as f64;
        let k = (pos.floor() as usize).min(regions - 1);
        b[k] + (pos - k as f64) * (b[k + 1] - b[k])
    }
}

/// A closed-rectangle [`Path`] tracing `bbox`'s four corners.
fn rect_path(bbox: &Bbox) -> Path {
    let (x0, y0) = (bbox.xmin(), bbox.ymin());
    let (x1, y1) = (bbox.xmax(), bbox.ymax());
    Path::from_polyline(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Bbox, b: [f64; 4]) {
        let got = [a.xmin(), a.ymin(), a.xmax(), a.ymax()];
        for k in 0..4 {
            assert!((got[k] - b[k]).abs() < 1e-9, "{got:?} vs {b:?}");
        }
    }

    /// A 400 x 300 px frame at the origin: each side gives the parent what
    /// matplotlib's `make_axes` does and fits the bar to aspect 20 against
    /// the parent.
    #[test]
    fn every_location_splits_like_make_axes() {
        let frame = Bbox::from_extents(0.0, 0.0, 400.0, 300.0);
        let mut cb = Colorbar::new(Mappable::Image(0));

        let (parent, bar) = cb.split(&frame);
        close(&parent, [0.0, 0.0, 320.0, 300.0]);
        close(&bar, [340.0, 0.0, 355.0, 300.0]);

        let (parent, bar) = cb.set_location(ColorbarLocation::Left).split(&frame);
        close(&parent, [100.0, 0.0, 400.0, 300.0]);
        close(&bar, [45.0, 0.0, 60.0, 300.0]);

        let (parent, bar) = cb.set_location(ColorbarLocation::Top).split(&frame);
        close(&parent, [0.0, 0.0, 400.0, 240.0]);
        close(&bar, [0.0, 255.0, 400.0, 275.0]);

        let (parent, bar) = cb.set_location(ColorbarLocation::Bottom).split(&frame);
        close(&parent, [0.0, 90.0, 400.0, 300.0]);
        close(&bar, [0.0, 25.0, 400.0, 45.0]);

        // A short, wide slot caps the bar's length by its aspect instead.
        let (_, bar) = cb
            .set_location(ColorbarLocation::Right)
            .set_shrink(0.5)
            .set_aspect(4.0)
            .split(&frame);
        close(&bar, [340.0, 75.0, 377.5, 225.0]);
    }

    #[test]
    fn boundary_scale_spaces_regions_evenly_and_inverts() {
        let s = BoundaryScale(vec![0.0, 1.0, 10.0, 100.0]);
        assert_eq!(s.transform(0.0), 0.0);
        assert!((s.transform(1.0) - 1.0 / 3.0).abs() < 1e-12);
        assert!((s.transform(55.0) - (2.0 + 0.5) / 3.0).abs() < 1e-12);
        assert_eq!(s.transform(100.0), 1.0);
        for v in [0.0, 0.5, 3.0, 42.0, 100.0] {
            assert!((s.inverse(s.transform(v)) - v).abs() < 1e-9, "{v}");
        }
    }

    #[test]
    fn log_labels_write_every_decade_in_scientific_notation() {
        assert_eq!(sci_decade(1.0, None), "$10^{0}$");
        assert_eq!(sci_decade(1000.0, None), "$10^{3}$");
        assert_eq!(sci_decade(0.01, None), "$10^{-2}$");
        assert_eq!(sci_decade(30.0, None), "30");
    }
}
