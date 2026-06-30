//! A single set of plotting [`Axes`]: artists drawn within a data coordinate
//! system, framed by a pair of [`Axis`] spines.
//!
//! [`Axes`] mirrors matplotlib's `Axes`: it owns a list of [`Line2D`] and
//! [`Patch`] artists, an x and y [`Axis`], and the data limits that map data
//! coordinates onto a rectangular region of the figure. The region is stored in
//! **figure fractions** (`[0, 1]^2`) and resolved to pixels at draw time.
//!
//! # Coordinate convention
//!
//! Like [`rizzma_axis`], everything is a **y-UP** pixel space; the raster
//! backend applies its own Y-flip. The data-to-pixel mapping is built by
//! [`Axes::trans_data`].

use rizzma_artist::{Artist, AxesImage, Collection, Line2D, Patch, QuadMesh};
use std::borrow::Cow;

use rizzma_axis::axis::{Axis, AxisSide};
use rizzma_axis::scale::{LinearScale, LogScale, LogitScale, Scale, SymlogScale};
use rizzma_axis::ticker::{
    AutoLocator, LogFormatterMathtext, LogLocator, LogitFormatterMathtext, LogitLocator,
    ScalarFormatter, SymlogFormatterMathtext, SymlogLocator,
};
use rizzma_core::color::{DEFAULT_COLOR_CYCLE, Rgba};
use rizzma_core::{Affine2D, Bbox, Path};
use rizzma_render::{GraphicsContext, Renderer};
use rizzma_text::FontSource;

use crate::richtext::layout_rich_text;

/// Default fractional margin added on each side of the autoscaled data limits.
const DEFAULT_MARGIN: f64 = 0.05;

/// Per-axis scale state owned by [`Axes`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ScaleSpec {
    /// Linear identity scale.
    Linear,
    /// Logarithmic scale with the given base.
    Log { base: f64 },
    /// Symmetric-log scale with a linear region around zero.
    Symlog { base: f64, linthresh: f64 },
    /// Logit scale for probabilities in the open interval `(0, 1)`.
    Logit,
}

impl ScaleSpec {
    fn transform(self, value: f64) -> f64 {
        match self {
            ScaleSpec::Linear => value,
            ScaleSpec::Log { base } => LogScale::new(base).transform(value),
            ScaleSpec::Symlog { base, linthresh } => {
                SymlogScale::new(base, linthresh, 1.0).transform(value)
            }
            ScaleSpec::Logit => LogitScale::new().transform(value),
        }
    }

    fn inverse(self, value: f64) -> f64 {
        match self {
            ScaleSpec::Linear => value,
            ScaleSpec::Log { base } => LogScale::new(base).inverse(value),
            ScaleSpec::Symlog { base, linthresh } => {
                SymlogScale::new(base, linthresh, 1.0).inverse(value)
            }
            ScaleSpec::Logit => LogitScale::new().inverse(value),
        }
    }

    fn is_linear(self) -> bool {
        matches!(self, ScaleSpec::Linear)
    }

    fn limit_range(self, limits: (f64, f64), minpos: f64) -> (f64, f64) {
        match self {
            ScaleSpec::Linear => guard_range(limits),
            ScaleSpec::Log { base } => guard_log_range(limits, base, minpos),
            ScaleSpec::Symlog { .. } => guard_range(limits),
            ScaleSpec::Logit => guard_logit_range(limits, minpos),
        }
    }
}

/// Draw-time mapper from raw data coordinates to scaled data coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DataToScaled {
    x: ScaleSpec,
    y: ScaleSpec,
}

impl DataToScaled {
    /// Construct a mapper for `x` and `y` axis scales.
    pub(crate) const fn new(x: ScaleSpec, y: ScaleSpec) -> Self {
        Self { x, y }
    }

    /// Map a raw data-space point into scaled data space.
    pub(crate) fn map_point(&self, x: f64, y: f64) -> [f64; 2] {
        [self.x.transform(x), self.y.transform(y)]
    }

    /// Map a scaled-space point back into raw data space.
    pub(crate) fn inverse_point(&self, x: f64, y: f64) -> [f64; 2] {
        [self.x.inverse(x), self.y.inverse(y)]
    }

    /// Whether this mapper is exact identity in both dimensions.
    pub(crate) fn is_linear(&self) -> bool {
        self.x.is_linear() && self.y.is_linear()
    }

    /// Map every vertex in `path` into scaled data space, preserving path codes.
    pub(crate) fn map_path(&self, path: &Path) -> Path {
        let vertices = path
            .vertices()
            .iter()
            .map(|&[x, y]| self.map_point(x, y))
            .collect();
        let codes = path.codes().map(|codes| codes.to_vec());
        Path::new(vertices, codes)
    }

    /// Borrow `path` unchanged for linear scales, otherwise return mapped
    /// geometry. This keeps the default linear draw path allocation-free at the
    /// scale-mapping layer.
    pub(crate) fn map_path_cow<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        if self.is_linear() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(self.map_path(path))
        }
    }

    /// Borrow `points` unchanged for linear scales, otherwise map each point.
    pub(crate) fn map_points_cow<'a>(&self, points: &'a [[f64; 2]]) -> Cow<'a, [[f64; 2]]> {
        if self.is_linear() {
            Cow::Borrowed(points)
        } else {
            Cow::Owned(points.iter().map(|&[x, y]| self.map_point(x, y)).collect())
        }
    }

    /// Map raw x/y limits into scaled x/y limits.
    pub(crate) fn map_limits(
        &self,
        xlim: (f64, f64),
        ylim: (f64, f64),
    ) -> ((f64, f64), (f64, f64)) {
        let [x0, y0] = self.map_point(xlim.0, ylim.0);
        let [x1, y1] = self.map_point(xlim.1, ylim.1);
        ((x0, x1), (y0, y1))
    }
}

#[cfg(test)]
fn scaled_tick_position(value: f64, limits: (f64, f64), scale: ScaleSpec) -> f64 {
    let mapper = DataToScaled::new(scale, ScaleSpec::Linear);
    let (scaled_limits, _) = mapper.map_limits(limits, (0.0, 1.0));
    let scaled = mapper.map_point(value, 0.0)[0];
    let denom = scaled_limits.1 - scaled_limits.0;
    if denom == 0.0 {
        0.0
    } else {
        (scaled - scaled_limits.0) / denom
    }
}

/// A set of plotting axes within a figure.
///
/// Construct with [`Axes::new`] given a figure-fraction `position`, add data
/// with [`Axes::plot`]/[`Axes::add_line`]/[`Axes::add_patch`], style via the
/// `set_*` methods, then draw with [`Axes::draw`].
///
/// **Linear scales only**: the data transform built by [`Axes::trans_data`] is
/// purely linear.
// TODO: honor non-linear scales (log/symlog) in `trans_data` once the scale is
// threaded through from the `Axis`.
pub struct Axes {
    /// The axes region in figure fractions (`[0, 1]^2`).
    position: Bbox,
    /// Explicit x data limits `(min, max)`, or `None` to autoscale.
    xlim: Option<(f64, f64)>,
    /// Explicit y data limits `(min, max)`, or `None` to autoscale.
    ylim: Option<(f64, f64)>,
    /// Fractional margin added on each side when autoscaling.
    margins: f64,
    /// X-axis scale state.
    xscale: ScaleSpec,
    /// Y-axis scale state.
    yscale: ScaleSpec,
    /// Background fill color of the axes region.
    facecolor: Rgba,
    /// Line artists, drawn in ascending zorder.
    pub(crate) lines: Vec<Line2D>,
    /// Patch artists, drawn in ascending zorder.
    pub(crate) patches: Vec<Patch>,
    /// Scatter [`Collection`] artists, drawn in ascending zorder.
    pub(crate) collections: Vec<Collection>,
    /// Colormapped raster images, drawn beneath the other artists.
    pub(crate) images: Vec<AxesImage>,
    /// Colormapped quad meshes (`pcolormesh`), drawn beneath the other artists.
    pub(crate) meshes: Vec<QuadMesh>,
    /// Extra data extents folded into autoscaling that have no owning artist,
    /// e.g. the grid extent recorded by [`Axes::contour`] so the field is
    /// fitted even when no contour line crosses it.
    pub(crate) extra_data_bbox: Option<Bbox>,
    /// The bottom (x) axis.
    xaxis: Axis,
    /// The left (y) axis.
    yaxis: Axis,
    /// Optional title drawn above the axes.
    title: Option<String>,
    /// Whether to stroke the axes frame (border rectangle).
    frame: bool,
    /// When `true`, the axes pixel rectangle is shrunk so data units span the
    /// same number of pixels in x and y (matplotlib's `set_aspect("equal")`).
    /// See [`Axes::set_aspect_equal`].
    pub(crate) aspect_equal: bool,
    /// Whether the frame stroke and the x/y axis spines (ticks, tick labels)
    /// are drawn. When `false`, only the background, artists, and title appear.
    /// See [`Axes::set_axis_off`].
    pub(crate) axis_visible: bool,
    /// Index into the property color cycle, advanced as cycled colors are
    /// consumed (e.g. by [`Axes::bar`]).
    pub(crate) prop_cycle_index: usize,
    /// Full-span reference lines, resolved against the effective limits at draw
    /// time (see [`Axes::axhline`]/[`Axes::axvline`]).
    pub(crate) span_lines: Vec<SpanLine>,
    /// Full-span shaded rectangles, resolved against the effective limits at
    /// draw time (see [`Axes::axhspan`]/[`Axes::axvspan`]).
    pub(crate) span_rects: Vec<SpanRect>,
    /// Legend entries, drawn as a boxed key in the upper-right corner (see
    /// [`Axes::legend`]). Empty means no legend.
    pub(crate) legend: Vec<crate::legend::LegendEntry>,
}

/// Orientation of a full-span reference line or shaded band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpanOrientation {
    /// Constant value along the y-axis, spanning the full x range.
    Horizontal,
    /// Constant value along the x-axis, spanning the full y range.
    Vertical,
}

/// A full-span reference line at a constant data value, drawn spanning the
/// resolved effective limits of the opposite axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpanLine {
    /// Whether the line is horizontal (`y == value`) or vertical (`x == value`).
    pub(crate) orientation: SpanOrientation,
    /// The constant data coordinate the line sits at.
    pub(crate) value: f64,
    /// Stroke color.
    pub(crate) color: Rgba,
    /// Stroke width in points.
    pub(crate) linewidth: f64,
}

/// A full-span shaded band between two data values, drawn spanning the resolved
/// effective limits of the opposite axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpanRect {
    /// Whether the band runs across x (horizontal) or across y (vertical).
    pub(crate) orientation: SpanOrientation,
    /// Lower bound of the band in data coordinates.
    pub(crate) lo: f64,
    /// Upper bound of the band in data coordinates.
    pub(crate) hi: f64,
    /// Fill color.
    pub(crate) facecolor: Rgba,
}

enum DrawableArtist<'a> {
    Line(&'a Line2D),
    Patch(&'a Patch),
    Collection(&'a Collection),
}

impl DrawableArtist<'_> {
    fn zorder(&self) -> f64 {
        match self {
            DrawableArtist::Line(line) => line.zorder(),
            DrawableArtist::Patch(patch) => patch.zorder(),
            DrawableArtist::Collection(collection) => collection.zorder(),
        }
    }

    fn visible(&self) -> bool {
        match self {
            DrawableArtist::Line(line) => line.visible(),
            DrawableArtist::Patch(patch) => patch.visible(),
            DrawableArtist::Collection(collection) => collection.visible(),
        }
    }

    fn draw_scaled(
        &self,
        renderer: &mut dyn Renderer,
        transform: &Affine2D,
        mapper: &DataToScaled,
    ) {
        match self {
            DrawableArtist::Line(line) => {
                let points = line.points();
                let path = Path::from_polyline(&points);
                let path = mapper.map_path_cow(&path);
                line.draw_path(renderer, path.as_ref(), transform);
            }
            DrawableArtist::Patch(patch) => {
                let path = mapper.map_path_cow(patch.path());
                patch.draw_path(renderer, path.as_ref(), transform);
            }
            DrawableArtist::Collection(collection) => {
                let offsets = mapper.map_points_cow(collection.offsets());
                collection.draw_with_offsets(renderer, offsets.as_ref(), transform);
            }
        }
    }
}

impl Axes {
    /// Create axes occupying `position` (figure fractions) with matplotlib-ish
    /// defaults: autoscaled limits, a `0.05` margin, a white background, a
    /// bottom x-axis, a left y-axis, no title, and a visible frame.
    #[must_use]
    pub fn new(position: Bbox) -> Self {
        Self {
            position,
            xlim: None,
            ylim: None,
            margins: DEFAULT_MARGIN,
            xscale: ScaleSpec::Linear,
            yscale: ScaleSpec::Linear,
            facecolor: Rgba::WHITE,
            lines: Vec::new(),
            patches: Vec::new(),
            collections: Vec::new(),
            images: Vec::new(),
            meshes: Vec::new(),
            extra_data_bbox: None,
            xaxis: Axis::new(AxisSide::Bottom),
            yaxis: Axis::new(AxisSide::Left),
            title: None,
            frame: true,
            aspect_equal: false,
            axis_visible: true,
            prop_cycle_index: 0,
            span_lines: Vec::new(),
            span_rects: Vec::new(),
            legend: Vec::new(),
        }
    }

    /// Fallback color (`tab10` C0 blue) used when a cycle hex fails to parse,
    /// which cannot happen for the built-in [`DEFAULT_COLOR_CYCLE`].
    const FALLBACK_CYCLE_COLOR: Rgba = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);

    /// Resolve the property-cycle color at `index` (modulo the cycle length)
    /// from the core [`DEFAULT_COLOR_CYCLE`] (matplotlib's `tab10`), without
    /// advancing the per-axes cycle position.
    ///
    /// `cycle_color(0)` is C0 (`#1f77b4`).
    #[must_use]
    pub fn cycle_color(&self, index: usize) -> Rgba {
        let hex = DEFAULT_COLOR_CYCLE[index % DEFAULT_COLOR_CYCLE.len()];
        Rgba::from_hex(hex).unwrap_or(Self::FALLBACK_CYCLE_COLOR)
    }

    /// Return the next property-cycle color and advance the per-axes cycle
    /// position by one.
    ///
    /// Successive calls yield C0, C1, C2, … (`#1f77b4`, `#ff7f0e`, `#2ca02c`,
    /// …), wrapping after the ten `tab10` entries. Reset with
    /// [`set_prop_cycle_reset`](Axes::set_prop_cycle_reset).
    pub fn next_cycle_color(&mut self) -> Rgba {
        let color = self.cycle_color(self.prop_cycle_index);
        self.prop_cycle_index += 1;
        color
    }

    /// Reset the property-cycle position back to the start, so the next cycled
    /// color is C0 (`#1f77b4`) again.
    pub fn set_prop_cycle_reset(&mut self) -> &mut Self {
        self.prop_cycle_index = 0;
        self
    }

    /// Plot `y` against `x` as a new [`Line2D`], returning a mutable reference
    /// to the pushed line for further styling.
    ///
    /// The line is created with the next property-cycle color (C0, C1, C2, …
    /// across successive calls), advancing the per-axes cycle. To pick a
    /// specific color instead, use [`plot_with_color`](Axes::plot_with_color),
    /// or overwrite the returned handle, e.g.
    /// `*ax.plot(x, y) = Line2D::new(x, y).with_color(c)`.
    pub fn plot(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        let color = self.next_cycle_color();
        self.lines
            .push(Line2D::new(x.to_vec(), y.to_vec()).with_color(color));
        self.lines.last_mut().expect("just pushed a line")
    }

    /// Plot `y` against `x` as a new [`Line2D`] with an explicit `color`,
    /// returning a mutable reference to the pushed line.
    ///
    /// Unlike [`plot`](Axes::plot), this does not consult or advance the
    /// property cycle: the given color always wins.
    pub fn plot_with_color(&mut self, x: &[f64], y: &[f64], color: Rgba) -> &mut Line2D {
        self.lines
            .push(Line2D::new(x.to_vec(), y.to_vec()).with_color(color));
        self.lines.last_mut().expect("just pushed a line")
    }

    /// Add a pre-built [`Line2D`], returning a mutable reference to it.
    pub fn add_line(&mut self, line: Line2D) -> &mut Line2D {
        self.lines.push(line);
        self.lines.last_mut().expect("just pushed a line")
    }

    /// Add a [`Patch`], returning a mutable reference to it.
    pub fn add_patch(&mut self, patch: Patch) -> &mut Patch {
        self.patches.push(patch);
        self.patches.last_mut().expect("just pushed a patch")
    }

    /// Fold the rectangle `[xmin, xmax] x [ymin, ymax]` into autoscaling without
    /// an owning artist (used by [`contour`](Axes::contour) to record the grid
    /// extent even when no contour line crosses the field).
    pub(crate) fn include_data_bbox(&mut self, xmin: f64, ymin: f64, xmax: f64, ymax: f64) {
        let bbox = Bbox::from_extents(xmin, ymin, xmax, ymax);
        self.extra_data_bbox = Some(match self.extra_data_bbox {
            Some(b) => b.union(&bbox),
            None => bbox,
        });
    }

    /// Set explicit x limits `(min, max)`, disabling x autoscaling.
    pub fn set_xlim(&mut self, min: f64, max: f64) -> &mut Self {
        self.xlim = Some((min, max));
        self
    }

    /// Set explicit y limits `(min, max)`, disabling y autoscaling.
    pub fn set_ylim(&mut self, min: f64, max: f64) -> &mut Self {
        self.ylim = Some((min, max));
        self
    }

    /// Set the axes title (drawn centered above the axes).
    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.title = Some(title.into());
        self
    }

    /// Set the x-axis label.
    pub fn set_xlabel(&mut self, label: impl Into<String>) -> &mut Self {
        self.xaxis = std::mem::replace(&mut self.xaxis, Axis::new(AxisSide::Bottom))
            .with_label(label.into());
        self
    }

    /// Set the y-axis label.
    pub fn set_ylabel(&mut self, label: impl Into<String>) -> &mut Self {
        self.yaxis =
            std::mem::replace(&mut self.yaxis, Axis::new(AxisSide::Left)).with_label(label.into());
        self
    }

    /// Set the background fill color of the axes region.
    pub fn set_facecolor(&mut self, color: Rgba) -> &mut Self {
        self.facecolor = color;
        self
    }

    /// Set the autoscale margin (fraction added on each side).
    pub fn set_margins(&mut self, margins: f64) -> &mut Self {
        self.margins = margins;
        self
    }

    /// Use a linear x-axis scale and default linear ticks.
    pub fn set_xscale_linear(&mut self) -> &mut Self {
        self.xscale = ScaleSpec::Linear;
        self.xaxis
            .set_scale(Box::new(LinearScale::new()))
            .set_locator(Box::new(AutoLocator::new()))
            .set_formatter(Box::new(ScalarFormatter::new()));
        self
    }

    /// Use a linear y-axis scale and default linear ticks.
    pub fn set_yscale_linear(&mut self) -> &mut Self {
        self.yscale = ScaleSpec::Linear;
        self.yaxis
            .set_scale(Box::new(LinearScale::new()))
            .set_locator(Box::new(AutoLocator::new()))
            .set_formatter(Box::new(ScalarFormatter::new()));
        self
    }

    /// Use a logarithmic x-axis scale with `base`.
    ///
    /// Limits, autoscaling, and public data APIs remain in raw data units; the
    /// log transform is applied at draw time. Line, patch, collection, span,
    /// and reference-line geometry is log-scaled; raster images and quad meshes
    /// are intentionally unsupported on log axes in this first implementation.
    /// `base` must be finite and greater than one.
    ///
    /// # Panics
    ///
    /// Panics when `base` is not finite or is less than or equal to one.
    pub fn set_xscale_log(&mut self, base: f64) -> &mut Self {
        assert!(
            base.is_finite() && base > 1.0,
            "log scale base must be finite and > 1"
        );
        self.xscale = ScaleSpec::Log { base };
        self.xaxis
            .set_scale(Box::new(LogScale::new(base)))
            .set_locator(Box::new(LogLocator::new(base)))
            .set_formatter(Box::new(LogFormatterMathtext::new(base)));
        self
    }

    /// Use a logarithmic y-axis scale with `base`.
    ///
    /// Limits, autoscaling, and public data APIs remain in raw data units; the
    /// log transform is applied at draw time. Line, patch, collection, span,
    /// and reference-line geometry is log-scaled; raster images and quad meshes
    /// are intentionally unsupported on log axes in this first implementation.
    /// `base` must be finite and greater than one.
    ///
    /// # Panics
    ///
    /// Panics when `base` is not finite or is less than or equal to one.
    pub fn set_yscale_log(&mut self, base: f64) -> &mut Self {
        assert!(
            base.is_finite() && base > 1.0,
            "log scale base must be finite and > 1"
        );
        self.yscale = ScaleSpec::Log { base };
        self.yaxis
            .set_scale(Box::new(LogScale::new(base)))
            .set_locator(Box::new(LogLocator::new(base)))
            .set_formatter(Box::new(LogFormatterMathtext::new(base)));
        self
    }

    /// Use a symmetric-log x-axis scale with `base` and `linthresh`.
    ///
    /// The symlog transform is linear within `[-linthresh, linthresh]` and
    /// logarithmic in both tails, so negative values and zero remain valid.
    /// Limits, autoscaling, and public data APIs remain in raw data units; the
    /// transform is applied at draw time. Line, patch, collection, span, and
    /// reference-line geometry is symlog-scaled; raster images and quad meshes
    /// are intentionally unsupported on nonlinear axes in this implementation.
    ///
    /// # Panics
    ///
    /// Panics when `base` is not finite or is less than or equal to one, or
    /// when `linthresh` is not finite or is less than or equal to zero.
    pub fn set_xscale_symlog(&mut self, base: f64, linthresh: f64) -> &mut Self {
        assert!(
            base.is_finite() && base > 1.0,
            "symlog scale base must be finite and > 1"
        );
        assert!(
            linthresh.is_finite() && linthresh > 0.0,
            "symlog scale linthresh must be finite and > 0"
        );
        self.xscale = ScaleSpec::Symlog { base, linthresh };
        self.xaxis
            .set_scale(Box::new(SymlogScale::new(base, linthresh, 1.0)))
            .set_locator(Box::new(SymlogLocator::new(base, linthresh)))
            .set_formatter(Box::new(SymlogFormatterMathtext::new(base, linthresh)));
        self
    }

    /// Use a symmetric-log y-axis scale with `base` and `linthresh`.
    ///
    /// The symlog transform is linear within `[-linthresh, linthresh]` and
    /// logarithmic in both tails, so negative values and zero remain valid.
    /// Limits, autoscaling, and public data APIs remain in raw data units; the
    /// transform is applied at draw time. Line, patch, collection, span, and
    /// reference-line geometry is symlog-scaled; raster images and quad meshes
    /// are intentionally unsupported on nonlinear axes in this implementation.
    ///
    /// # Panics
    ///
    /// Panics when `base` is not finite or is less than or equal to one, or
    /// when `linthresh` is not finite or is less than or equal to zero.
    pub fn set_yscale_symlog(&mut self, base: f64, linthresh: f64) -> &mut Self {
        assert!(
            base.is_finite() && base > 1.0,
            "symlog scale base must be finite and > 1"
        );
        assert!(
            linthresh.is_finite() && linthresh > 0.0,
            "symlog scale linthresh must be finite and > 0"
        );
        self.yscale = ScaleSpec::Symlog { base, linthresh };
        self.yaxis
            .set_scale(Box::new(SymlogScale::new(base, linthresh, 1.0)))
            .set_locator(Box::new(SymlogLocator::new(base, linthresh)))
            .set_formatter(Box::new(SymlogFormatterMathtext::new(base, linthresh)));
        self
    }

    /// Use a logit x-axis scale for probabilities in `(0, 1)`.
    ///
    /// Limits, autoscaling, and public data APIs remain in raw probability
    /// units; the logit transform is applied at draw time. Values outside the
    /// open interval are clamped out of the rendered view immediately before
    /// scaling. Line, patch, collection, span, and reference-line geometry is
    /// logit-scaled; raster images and quad meshes are intentionally
    /// unsupported on nonlinear axes in this implementation.
    pub fn set_xscale_logit(&mut self) -> &mut Self {
        self.xscale = ScaleSpec::Logit;
        self.xaxis
            .set_scale(Box::new(LogitScale::new()))
            .set_locator(Box::new(LogitLocator::new()))
            .set_formatter(Box::new(LogitFormatterMathtext::new()));
        self
    }

    /// Use a logit y-axis scale for probabilities in `(0, 1)`.
    ///
    /// Limits, autoscaling, and public data APIs remain in raw probability
    /// units; the logit transform is applied at draw time. Values outside the
    /// open interval are clamped out of the rendered view immediately before
    /// scaling. Line, patch, collection, span, and reference-line geometry is
    /// logit-scaled; raster images and quad meshes are intentionally
    /// unsupported on nonlinear axes in this implementation.
    pub fn set_yscale_logit(&mut self) -> &mut Self {
        self.yscale = ScaleSpec::Logit;
        self.yaxis
            .set_scale(Box::new(LogitScale::new()))
            .set_locator(Box::new(LogitLocator::new()))
            .set_formatter(Box::new(LogitFormatterMathtext::new()));
        self
    }

    /// Plot with a logarithmic x-axis and linear y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.semilogx(&[1.0, 10.0, 100.0], &[0.0, 1.0, 2.0]);
    /// ```
    pub fn semilogx(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_xscale_log(10.0);
        self.plot(x, y)
    }

    /// Plot with a linear x-axis and logarithmic y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.semilogy(&[0.0, 1.0, 2.0], &[1.0, 10.0, 100.0]);
    /// ```
    pub fn semilogy(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_yscale_log(10.0);
        self.plot(x, y)
    }

    /// Plot with logarithmic x and y axes.
    ///
    /// ![loglog](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_loglog.png)
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.loglog(&[1.0, 10.0, 100.0], &[1.0, 100.0, 10_000.0]);
    /// ```
    pub fn loglog(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_xscale_log(10.0).set_yscale_log(10.0);
        self.plot(x, y)
    }

    /// Plot with a symmetric-log x-axis and linear y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.symlogx(&[-100.0, -1.0, 0.0, 1.0, 100.0], &[1.0, 2.0, 3.0, 2.0, 1.0]);
    /// ```
    pub fn symlogx(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_xscale_symlog(10.0, 1.0);
        self.plot(x, y)
    }

    /// Plot with a linear x-axis and symmetric-log y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.symlogy(&[-2.0, -1.0, 0.0, 1.0, 2.0], &[-100.0, -1.0, 0.0, 1.0, 100.0]);
    /// ```
    pub fn symlogy(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_yscale_symlog(10.0, 1.0);
        self.plot(x, y)
    }

    /// Plot with a logit x-axis and linear y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.logitx(&[0.01, 0.1, 0.5, 0.9, 0.99], &[0.0, 1.0, 2.0, 1.0, 0.0]);
    /// ```
    pub fn logitx(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_xscale_logit();
        self.plot(x, y)
    }

    /// Plot with a linear x-axis and logit y-axis.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.logity(&[0.0, 1.0, 2.0, 3.0, 4.0], &[0.01, 0.1, 0.5, 0.9, 0.99]);
    /// ```
    pub fn logity(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_yscale_logit();
        self.plot(x, y)
    }

    /// Constrain the axes to an **equal** aspect ratio, so one data unit covers
    /// the same number of pixels along x and y (matplotlib's
    /// `set_aspect("equal")`).
    ///
    /// The pixel rectangle is computed normally, then the over-long dimension is
    /// shrunk (centered within the original rect) until
    /// `xrange / width == yrange / height` for the resolved effective limits.
    /// This keeps circles round and is applied in the shared forward path, so
    /// drawing and coordinate inversion stay consistent.
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.set_xlim(-1.0, 1.0).set_ylim(-1.0, 1.0);
    /// ax.set_aspect_equal();
    /// ```
    pub fn set_aspect_equal(&mut self) -> &mut Self {
        self.aspect_equal = true;
        self
    }

    /// Hide the axes frame and the x/y spines (ticks and tick labels).
    ///
    /// After this, [`draw`](Axes::draw) still paints the background, artists, and
    /// title, but draws no border rectangle and no tick marks or tick labels —
    /// matplotlib's `set_axis_off`. Re-enable with [`set_axis_on`](Axes::set_axis_on).
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.set_axis_off();
    /// ```
    pub fn set_axis_off(&mut self) -> &mut Self {
        self.axis_visible = false;
        self
    }

    /// Show the axes frame and the x/y spines again, undoing
    /// [`set_axis_off`](Axes::set_axis_off).
    pub fn set_axis_on(&mut self) -> &mut Self {
        self.axis_visible = true;
        self
    }

    /// The figure-fraction position of this axes.
    #[must_use]
    pub fn position(&self) -> Bbox {
        self.position
    }

    /// The union of all artists' data extents, or `None` when there is no
    /// finite data (every artist is empty).
    #[must_use]
    pub fn data_limits(&self) -> Option<Bbox> {
        let mut acc: Option<Bbox> = None;
        let line_extents = self.lines.iter().filter_map(Artist::data_extents);
        let patch_extents = self.patches.iter().filter_map(Artist::data_extents);
        let collection_extents = self.collections.iter().filter_map(Artist::data_extents);
        let image_extents = self.images.iter().filter_map(Artist::data_extents);
        let mesh_extents = self.meshes.iter().filter_map(Artist::data_extents);
        for e in line_extents
            .chain(patch_extents)
            .chain(collection_extents)
            .chain(image_extents)
            .chain(mesh_extents)
            .chain(self.extra_data_bbox)
        {
            acc = Some(match acc {
                Some(a) => a.union(&e),
                None => e,
            });
        }
        acc
    }

    /// Resolve the effective `(xlim, ylim)` used for drawing.
    ///
    /// Explicit limits are used when set; otherwise the data limits are
    /// expanded by the axes `margins` on each side. With no data at all
    /// the fallback range is `(0.0, 1.0)`. Zero-width ranges are nudged apart so
    /// the data transform never divides by zero.
    #[must_use]
    pub fn effective_limits(&self) -> ((f64, f64), (f64, f64)) {
        let data = self.data_limits();
        let xlim = self.xlim.unwrap_or_else(|| {
            data.map_or((0.0, 1.0), |b| {
                expand_range(b.xmin(), b.xmax(), self.margins)
            })
        });
        let ylim = self.ylim.unwrap_or_else(|| {
            data.map_or((0.0, 1.0), |b| {
                expand_range(b.ymin(), b.ymax(), self.margins)
            })
        });
        (guard_range(xlim), guard_range(ylim))
    }

    /// Resolve raw limits after applying the active scales' domain guards.
    ///
    /// Public limits remain raw data units. This helper only clamps ranges that
    /// cannot be transformed by the active scale, such as non-positive log
    /// bounds, immediately before draw-time scaling.
    fn scale_limited_effective_limits(&self) -> ((f64, f64), (f64, f64)) {
        let (xlim, ylim) = self.effective_limits();
        let (xminpos, yminpos) = self.min_positive_data();
        (
            self.xscale.limit_range(xlim, xminpos),
            self.yscale.limit_range(ylim, yminpos),
        )
    }

    fn min_positive_data(&self) -> (f64, f64) {
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        for line in &self.lines {
            for [x, y] in line.points() {
                if x.is_finite() && x > 0.0 {
                    xmin = xmin.min(x);
                }
                if y.is_finite() && y > 0.0 {
                    ymin = ymin.min(y);
                }
            }
        }
        for patch in &self.patches {
            for &[x, y] in patch.path().vertices() {
                if x.is_finite() && x > 0.0 {
                    xmin = xmin.min(x);
                }
                if y.is_finite() && y > 0.0 {
                    ymin = ymin.min(y);
                }
            }
        }
        for collection in &self.collections {
            for &[x, y] in collection.offsets() {
                if x.is_finite() && x > 0.0 {
                    xmin = xmin.min(x);
                }
                if y.is_finite() && y > 0.0 {
                    ymin = ymin.min(y);
                }
            }
        }
        if let Some(bbox) = self.extra_data_bbox {
            for x in [bbox.xmin(), bbox.xmax()] {
                if x.is_finite() && x > 0.0 {
                    xmin = xmin.min(x);
                }
            }
            for y in [bbox.ymin(), bbox.ymax()] {
                if y.is_finite() && y > 0.0 {
                    ymin = ymin.min(y);
                }
            }
        }
        let xmin = if xmin.is_finite() { xmin } else { 1.0 };
        let ymin = if ymin.is_finite() { ymin } else { 1.0 };
        (xmin, ymin)
    }

    /// Build the linear data-to-pixel [`Affine2D`].
    ///
    /// `axes_px` is the axes rectangle in pixels and `xlim`/`ylim` are the
    /// effective data ranges. The transform maps the data corner
    /// `(xmin, ymin)` to `axes_px`'s lower-left corner and `(xmax, ymax)` to its
    /// upper-right corner:
    /// `x -> axes_px.xmin + (x - xmin) * sx`, `y -> axes_px.ymin + (y - ymin) * sy`.
    #[must_use]
    pub fn trans_data(&self, axes_px: &Bbox, xlim: (f64, f64), ylim: (f64, f64)) -> Affine2D {
        let (xmin, xmax) = xlim;
        let (ymin, ymax) = ylim;
        let sx = axes_px.width() / (xmax - xmin);
        let sy = axes_px.height() / (ymax - ymin);
        // Translate the data origin to (0,0), scale into pixels, then offset to
        // the axes-rectangle origin: p = (data - (xmin,ymin)) * (sx,sy) + axes_px.min.
        Affine2D::from_translation(-xmin, -ymin)
            .scale(sx, sy)
            .translate(axes_px.xmin(), axes_px.ymin())
    }

    pub(crate) fn data_to_scaled(&self) -> DataToScaled {
        DataToScaled::new(self.xscale, self.yscale)
    }

    /// Resolve this axes' pixel rectangle and the linear data-to-display
    /// transform for a figure of size `fig_w_px` × `fig_h_px`.
    ///
    /// This reproduces the exact forward path used at the top of
    /// [`Axes::draw`]: the figure-fraction [`position`](Axes::position) is
    /// resolved against the figure size to a pixel [`Bbox`], the effective
    /// `(xlim, ylim)` come from [`effective_limits`](Axes::effective_limits),
    /// and the returned [`Affine2D`] is the same `trans_data` used to draw the
    /// artists. The transform maps data coordinates into **y-up** display
    /// pixels (no backend Y-flip applied here).
    ///
    /// Both [`Axes::draw`] and the figure-level coordinate-inversion helpers
    /// call this so they cannot drift apart.
    #[must_use]
    pub(crate) fn pixel_rect_and_trans_data(
        &self,
        fig_w_px: f64,
        fig_h_px: f64,
    ) -> (Bbox, Affine2D) {
        let mut axes_px = Bbox::from_extents(
            self.position.xmin() * fig_w_px,
            self.position.ymin() * fig_h_px,
            self.position.xmax() * fig_w_px,
            self.position.ymax() * fig_h_px,
        );
        let (xlim, ylim) = self.scale_limited_effective_limits();
        let mapper = self.data_to_scaled();
        let (scaled_xlim, scaled_ylim) = mapper.map_limits(xlim, ylim);
        if self.aspect_equal {
            axes_px = equalize_aspect(&axes_px, scaled_xlim, scaled_ylim);
        }
        let td = self.trans_data(&axes_px, scaled_xlim, scaled_ylim);
        (axes_px, td)
    }

    /// Draw the axes (background, artists, frame, axis spines, and title) into
    /// `renderer`.
    ///
    /// `fig_w_px`/`fig_h_px` are the figure size in pixels; the figure-fraction
    /// `position` is resolved against them. `font` supplies glyph outlines for
    /// the axis and title text.
    pub fn draw(
        &self,
        renderer: &mut dyn Renderer,
        fig_w_px: f64,
        fig_h_px: f64,
        font: &FontSource,
    ) {
        // 1. Resolve the pixel rectangle and the data transform via the shared
        // forward path (also used by `Figure`'s coordinate inversion).
        let (axes_px, td) = self.pixel_rect_and_trans_data(fig_w_px, fig_h_px);
        let (xlim, ylim) = self.scale_limited_effective_limits();
        let mapper = self.data_to_scaled();

        // 2. Fill the axes background.
        let rect = rect_path(&axes_px);
        let fill_gc = GraphicsContext::new();
        renderer.draw_path(&fill_gc, &rect, &Affine2D::identity(), Some(self.facecolor));

        // 3a. Draw colormapped images first (lowest zorder), beneath every
        // other artist, mapping their data-space extent through the data
        // transform.
        if mapper.is_linear() {
            for image in &self.images {
                if image.visible() {
                    image.draw(renderer, &td);
                }
            }
        }

        // 3a'. Draw colormapped quad meshes (pcolormesh) beneath the other
        // artists, mapping their data-space corners through the data transform.
        if mapper.is_linear() {
            for mesh in &self.meshes {
                if mesh.visible() {
                    mesh.draw(renderer, &td);
                }
            }
        }

        // 3b. Draw full-span shaded bands (axhspan/axvspan) beneath the artists,
        // resolving their open extent against the effective limits.
        for span in &self.span_rects {
            let rect = match span.orientation {
                SpanOrientation::Horizontal => {
                    rect_path(&Bbox::from_extents(xlim.0, span.lo, xlim.1, span.hi))
                }
                SpanOrientation::Vertical => {
                    rect_path(&Bbox::from_extents(span.lo, ylim.0, span.hi, ylim.1))
                }
            };
            renderer.draw_path(
                &GraphicsContext::new(),
                &mapper.map_path(&rect),
                &td,
                Some(span.facecolor),
            );
        }

        // 4. Draw artists in ascending zorder.
        // TODO: clip artists to `axes_px` once clip plumbing lands.
        let mut artists: Vec<DrawableArtist<'_>> =
            Vec::with_capacity(self.lines.len() + self.patches.len() + self.collections.len());
        artists.extend(self.lines.iter().map(DrawableArtist::Line));
        artists.extend(self.patches.iter().map(DrawableArtist::Patch));
        artists.extend(self.collections.iter().map(DrawableArtist::Collection));
        let mut order: Vec<usize> = (0..artists.len())
            .filter(|&i| artists[i].visible())
            .collect();
        order.sort_by(|&a, &b| {
            artists[a]
                .zorder()
                .partial_cmp(&artists[b].zorder())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in order {
            artists[i].draw_scaled(renderer, &td, &mapper);
        }

        // 4a. Draw full-span reference lines (axhline/axvline) above the
        // artists, spanning the resolved limits of the opposite axis.
        for span in &self.span_lines {
            let points = match span.orientation {
                SpanOrientation::Horizontal => [[xlim.0, span.value], [xlim.1, span.value]],
                SpanOrientation::Vertical => [[span.value, ylim.0], [span.value, ylim.1]],
            };
            let path = Path::from_polyline(&points);
            let gc = GraphicsContext::new()
                .with_stroke(span.color)
                .with_line_width(span.linewidth);
            renderer.draw_path(&gc, &mapper.map_path(&path), &td, None);
        }

        // 5. Stroke the frame (suppressed when the axis is turned off).
        if self.frame && self.axis_visible {
            let frame_gc = GraphicsContext::new()
                .with_stroke(Rgba::BLACK)
                .with_line_width(0.8);
            renderer.draw_path(&frame_gc, &rect, &Affine2D::identity(), None);
        }

        // 6. Draw the axes spines (suppressed when the axis is turned off).
        if self.axis_visible {
            self.xaxis.draw(renderer, &axes_px, xlim, font);
            self.yaxis.draw(renderer, &axes_px, ylim, font);
        }

        // 6a. Draw the legend box inside the upper-right of the axes.
        self.draw_legend(renderer, &axes_px, font);

        // 7. Draw the title, centered above the axes. Math spans (`$...$`) are
        // laid out by the mathtext engine via `layout_rich_text`; plain titles
        // reduce to the previous single-string path.
        if let Some(title) = &self.title
            && !title.is_empty()
        {
            let title_size = 12.0;
            let pad = 6.0;
            let rich = layout_rich_text(font, title, title_size);
            let cx = (axes_px.xmin() + axes_px.xmax()) / 2.0;
            let x = cx - rich.width / 2.0;
            // Place the baseline `pad` above the top spine, exactly as the
            // previous single-string path did; the rich paths are in a
            // baseline-relative y-up frame.
            let y = axes_px.ymax() + pad;
            let shift = Affine2D::from_translation(x, y);
            for path in &rich.paths {
                renderer.draw_path(
                    &GraphicsContext::new(),
                    &path.transformed(&shift),
                    &Affine2D::identity(),
                    Some(Rgba::BLACK),
                );
            }
        }
    }
}

/// A closed-rectangle [`Path`] tracing `bbox`'s four corners.
fn rect_path(bbox: &Bbox) -> Path {
    let (x0, y0) = (bbox.xmin(), bbox.ymin());
    let (x1, y1) = (bbox.xmax(), bbox.ymax());
    Path::from_polyline(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]])
}

/// Shrink `rect` (centered within itself) so data-units-per-pixel are equal in
/// x and y for the given effective limits.
///
/// The required pixels-per-data-unit is the smaller of the two axes' available
/// scales; the over-long pixel dimension is reduced to match while staying
/// centered on the original rectangle. The non-binding dimension is left intact.
fn equalize_aspect(rect: &Bbox, xlim: (f64, f64), ylim: (f64, f64)) -> Bbox {
    let xrange = xlim.1 - xlim.0;
    let yrange = ylim.1 - ylim.0;
    let (w, h) = (rect.width(), rect.height());
    // Pixels per data unit currently afforded by each dimension.
    let scale_x = w / xrange;
    let scale_y = h / yrange;
    // Use the tighter scale so the data fits, then size both dimensions to it.
    let scale = scale_x.min(scale_y);
    let new_w = xrange * scale;
    let new_h = yrange * scale;
    let cx = (rect.xmin() + rect.xmax()) / 2.0;
    let cy = (rect.ymin() + rect.ymax()) / 2.0;
    Bbox::from_extents(
        cx - new_w / 2.0,
        cy - new_h / 2.0,
        cx + new_w / 2.0,
        cy + new_h / 2.0,
    )
}

/// Expand `(min, max)` outward by `margin` times the range on each side.
fn expand_range(min: f64, max: f64, margin: f64) -> (f64, f64) {
    let span = max - min;
    let pad = span * margin;
    (min - pad, max + pad)
}

/// Nudge a degenerate (zero- or negative-width) range apart so it has a finite,
/// positive width suitable for division.
fn guard_range((min, max): (f64, f64)) -> (f64, f64) {
    if (max - min).abs() > f64::EPSILON {
        return (min, max);
    }
    // Expand symmetrically around the value; matplotlib uses a unit pad for a
    // truly zero-width range and a relative pad otherwise.
    let pad = if min.abs() > f64::EPSILON {
        min.abs() * 0.05
    } else {
        0.5
    };
    (min - pad, max + pad)
}

fn guard_log_range((a, b): (f64, f64), base: f64, minpos: f64) -> (f64, f64) {
    let reversed = a > b;
    let (mut lo, mut hi) = if reversed { (b, a) } else { (a, b) };
    let minpos = if minpos.is_finite() && minpos > 0.0 {
        minpos
    } else {
        1.0
    };

    if !lo.is_finite() || !hi.is_finite() || hi <= 0.0 {
        lo = minpos;
        hi = minpos * base;
    } else {
        if lo <= 0.0 {
            lo = minpos.min(hi / base);
        }
        if hi <= lo {
            lo = (lo / base).max(f64::MIN_POSITIVE);
            hi *= base;
        }
    }

    if reversed { (hi, lo) } else { (lo, hi) }
}

fn guard_logit_range((a, b): (f64, f64), minpos: f64) -> (f64, f64) {
    let reversed = a > b;
    let (mut lo, mut hi) = if reversed { (b, a) } else { (a, b) };
    let eps = if minpos.is_finite() && minpos > 0.0 && minpos < 0.5 {
        minpos.min(1e-7)
    } else {
        1e-7
    };

    if !lo.is_finite() || !hi.is_finite() || hi <= 0.0 || lo >= 1.0 {
        lo = eps;
        hi = 1.0 - eps;
    } else {
        if lo <= 0.0 {
            lo = eps;
        }
        if hi >= 1.0 {
            hi = 1.0 - eps;
        }
        if hi <= lo {
            lo = eps;
            hi = 1.0 - eps;
        }
    }

    if reversed { (hi, lo) } else { (lo, hi) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn trans_data_maps_corners_to_axes_pixels() {
        let axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let axes_px = Bbox::from_extents(50.0, 60.0, 250.0, 360.0);
        let xlim = (-1.0, 3.0);
        let ylim = (10.0, 20.0);
        let td = axes.trans_data(&axes_px, xlim, ylim);

        // (xmin, ymin) -> lower-left; (xmax, ymax) -> upper-right; and the
        // remaining two data corners to the remaining pixel corners.
        let ll = td.transform_point((xlim.0, ylim.0));
        approx(ll.0, axes_px.xmin());
        approx(ll.1, axes_px.ymin());

        let ur = td.transform_point((xlim.1, ylim.1));
        approx(ur.0, axes_px.xmax());
        approx(ur.1, axes_px.ymax());

        let lr = td.transform_point((xlim.1, ylim.0));
        approx(lr.0, axes_px.xmax());
        approx(lr.1, axes_px.ymin());

        let ul = td.transform_point((xlim.0, ylim.1));
        approx(ul.0, axes_px.xmin());
        approx(ul.1, axes_px.ymax());
    }

    #[test]
    fn data_to_scaled_linear_is_exact_identity_for_points_paths_and_limits() {
        let mapper = DataToScaled::new(ScaleSpec::Linear, ScaleSpec::Linear);
        let path = Path::unit_rectangle();

        assert_eq!(mapper.map_point(-3.0, 4.5), [-3.0, 4.5]);
        assert_eq!(mapper.map_path(&path), path);
        assert_eq!(
            mapper.map_limits((-1.0, 3.0), (10.0, 20.0)),
            ((-1.0, 3.0), (10.0, 20.0))
        );
    }

    #[test]
    fn data_to_scaled_log_maps_points_paths_and_limits() {
        let mapper = DataToScaled::new(ScaleSpec::Log { base: 10.0 }, ScaleSpec::Linear);
        let path = Path::from_polyline(&[[1.0, 2.0], [10.0, 4.0], [100.0, 8.0]]);
        let scaled = mapper.map_path(&path);

        assert_eq!(mapper.map_point(1000.0, 5.0), [3.0, 5.0]);
        assert_eq!(scaled.vertices(), &[[0.0, 2.0], [1.0, 4.0], [2.0, 8.0]]);
        assert_eq!(scaled.codes(), path.codes());
        assert_eq!(
            mapper.map_limits((1.0, 1000.0), (2.0, 8.0)),
            ((0.0, 3.0), (2.0, 8.0))
        );
    }

    #[test]
    fn data_to_scaled_symlog_maps_negative_zero_and_positive_points() {
        let scale = ScaleSpec::Symlog {
            base: 10.0,
            linthresh: 1.0,
        };
        let mapper = DataToScaled::new(scale, ScaleSpec::Linear);
        let path = Path::from_polyline(&[[-100.0, 2.0], [0.0, 4.0], [100.0, 8.0]]);
        let scaled = mapper.map_path(&path);

        approx(mapper.map_point(0.0, 5.0)[0], 0.0);
        approx(
            mapper.map_point(100.0, 5.0)[0],
            -mapper.map_point(-100.0, 5.0)[0],
        );
        approx(scaled.vertices()[1][0], 0.0);
        assert!(scaled.vertices()[0][0] < 0.0);
        assert!(scaled.vertices()[2][0] > 0.0);
        assert_eq!(scaled.codes(), path.codes());
    }

    #[test]
    fn data_to_scaled_logit_maps_probability_points_and_limits() {
        let mapper = DataToScaled::new(ScaleSpec::Logit, ScaleSpec::Linear);
        let path = Path::from_polyline(&[[0.1, 2.0], [0.5, 4.0], [0.9, 8.0]]);
        let scaled = mapper.map_path(&path);

        approx(mapper.map_point(0.5, 5.0)[0], 0.0);
        approx(
            mapper.map_point(0.9, 5.0)[0],
            -mapper.map_point(0.1, 5.0)[0],
        );
        approx(scaled.vertices()[1][0], 0.0);
        assert!(scaled.vertices()[0][0] < 0.0);
        assert!(scaled.vertices()[2][0] > 0.0);
        assert_eq!(scaled.codes(), path.codes());
        let ((x0, x1), ylim) = mapper.map_limits((0.1, 0.9), (2.0, 8.0));
        approx(x0, -x1);
        assert_eq!(ylim, (2.0, 8.0));
    }

    #[test]
    fn log_tick_position_uses_scaled_coordinate_fraction() {
        let scale = ScaleSpec::Log { base: 10.0 };

        approx(scaled_tick_position(10.0, (1.0, 1000.0), scale), 1.0 / 3.0);
        approx(scaled_tick_position(100.0, (1.0, 1000.0), scale), 2.0 / 3.0);
    }

    #[test]
    fn symlog_tick_position_uses_scaled_coordinate_fraction() {
        let scale = ScaleSpec::Symlog {
            base: 10.0,
            linthresh: 1.0,
        };

        approx(scaled_tick_position(0.0, (-100.0, 100.0), scale), 0.5);
        assert!(scaled_tick_position(-10.0, (-100.0, 100.0), scale) < 0.5);
        assert!(scaled_tick_position(10.0, (-100.0, 100.0), scale) > 0.5);
    }

    #[test]
    fn logit_tick_position_uses_scaled_coordinate_fraction() {
        approx(
            scaled_tick_position(0.5, (0.01, 0.99), ScaleSpec::Logit),
            0.5,
        );
        assert!(scaled_tick_position(0.1, (0.01, 0.99), ScaleSpec::Logit) < 0.5);
        assert!(scaled_tick_position(0.9, (0.01, 0.99), ScaleSpec::Logit) > 0.5);
    }

    #[test]
    fn symlog_effective_limits_preserve_negative_zero_positive_domain() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(
            &[-100.0, -1.0, 0.0, 1.0, 100.0],
            &[-1.0, 0.0, 1.0, 0.0, -1.0],
        );
        axes.set_xscale_symlog(10.0, 1.0).set_xlim(-100.0, 100.0);

        let (xlim, _) = axes.scale_limited_effective_limits();
        assert_eq!(xlim, (-100.0, 100.0));
        let (axes_px, td) = axes.pixel_rect_and_trans_data(200.0, 100.0);
        assert!(axes_px.width().is_finite());
        assert!(td.transform_point((0.0, 0.0)).0.is_finite());
    }

    #[test]
    fn logit_effective_limits_clamp_to_open_probability_domain() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(&[0.01, 0.5, 0.99], &[0.0, 1.0, 2.0]);
        axes.set_xscale_logit().set_xlim(-1.0, 2.0);

        let (xlim, _) = axes.scale_limited_effective_limits();
        assert!(xlim.0 > 0.0, "logit lower bound must be open: {xlim:?}");
        assert!(xlim.1 < 1.0, "logit upper bound must be open: {xlim:?}");
        let (axes_px, td) = axes.pixel_rect_and_trans_data(200.0, 100.0);
        assert!(axes_px.width().is_finite());
        assert!(
            td.transform_point((LogitScale::new().transform(xlim.0), 0.0))
                .0
                .is_finite()
        );
    }

    #[test]
    fn symlog_wrappers_set_expected_axis_scales() {
        let mut x_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        x_axes.symlogx(&[-10.0, 0.0, 10.0], &[1.0, 2.0, 3.0]);
        assert_eq!(
            x_axes.xscale,
            ScaleSpec::Symlog {
                base: 10.0,
                linthresh: 1.0
            }
        );
        assert_eq!(x_axes.yscale, ScaleSpec::Linear);

        let mut y_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        y_axes.symlogy(&[-1.0, 0.0, 1.0], &[-10.0, 0.0, 10.0]);
        assert_eq!(y_axes.xscale, ScaleSpec::Linear);
        assert_eq!(
            y_axes.yscale,
            ScaleSpec::Symlog {
                base: 10.0,
                linthresh: 1.0
            }
        );
    }

    #[test]
    fn logit_wrappers_set_expected_axis_scales() {
        let mut x_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        x_axes.logitx(&[0.01, 0.5, 0.99], &[1.0, 2.0, 3.0]);
        assert_eq!(x_axes.xscale, ScaleSpec::Logit);
        assert_eq!(x_axes.yscale, ScaleSpec::Linear);

        let mut y_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        y_axes.logity(&[1.0, 2.0, 3.0], &[0.01, 0.5, 0.99]);
        assert_eq!(y_axes.xscale, ScaleSpec::Linear);
        assert_eq!(y_axes.yscale, ScaleSpec::Logit);
    }

    #[derive(Default)]
    struct GeometryRecorder {
        paths: Vec<Vec<[f64; 2]>>,
        origins: Vec<[f64; 2]>,
    }

    impl Renderer for GeometryRecorder {
        fn draw_path(
            &mut self,
            _gc: &GraphicsContext,
            path: &Path,
            transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            let (ox, oy) = transform.transform_point((0.0, 0.0));
            self.origins.push([ox, oy]);
            self.paths.push(
                path.vertices()
                    .iter()
                    .map(|&[x, y]| {
                        let (tx, ty) = transform.transform_point((x, y));
                        [tx, ty]
                    })
                    .collect(),
            );
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    #[test]
    fn draw_scaled_maps_line_patch_and_collection_geometry() {
        let mapper = DataToScaled::new(ScaleSpec::Log { base: 10.0 }, ScaleSpec::Linear);
        let id = Affine2D::identity();

        let line = Line2D::new(vec![1.0, 10.0, 100.0], vec![2.0, 4.0, 8.0]);
        let patch = Patch::rectangle(1.0, 2.0, 9.0, 3.0);
        let collection = Collection::scatter(vec![[1.0, 2.0], [100.0, 8.0]]);

        let mut line_renderer = GeometryRecorder::default();
        DrawableArtist::Line(&line).draw_scaled(&mut line_renderer, &id, &mapper);
        assert_eq!(
            line_renderer.paths[0],
            vec![[0.0, 2.0], [1.0, 4.0], [2.0, 8.0]]
        );

        let mut patch_renderer = GeometryRecorder::default();
        DrawableArtist::Patch(&patch).draw_scaled(&mut patch_renderer, &id, &mapper);
        assert_eq!(patch_renderer.paths[0][0], [0.0, 2.0]);
        assert_eq!(patch_renderer.paths[0][1], [1.0, 2.0]);

        let mut collection_renderer = GeometryRecorder::default();
        DrawableArtist::Collection(&collection).draw_scaled(&mut collection_renderer, &id, &mapper);
        assert_eq!(collection_renderer.origins, vec![[0.0, 2.0], [2.0, 8.0]]);
    }

    #[test]
    fn scale_limited_effective_limits_clamp_nonpositive_log_bounds() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(&[1.0, 10.0, 100.0], &[0.0, 1.0, 2.0]);
        axes.set_xscale_log(10.0).set_xlim(-10.0, 100.0);

        let (xlim, _) = axes.scale_limited_effective_limits();
        assert!(xlim.0 > 0.0, "log lower bound must be positive: {xlim:?}");
        assert_eq!(xlim.1, 100.0);
        let (axes_px, td) = axes.pixel_rect_and_trans_data(200.0, 100.0);
        assert!(axes_px.width().is_finite());
        assert!(td.transform_point((xlim.0.log10(), 0.0)).0.is_finite());
    }

    #[test]
    fn pixel_rect_and_trans_data_linear_matches_direct_affine() {
        let mut axes = Axes::new(Bbox::from_extents(0.1, 0.2, 0.9, 0.8));
        axes.set_xlim(-1.0, 3.0).set_ylim(10.0, 20.0);
        let (axes_px, td) = axes.pixel_rect_and_trans_data(500.0, 400.0);
        let direct = axes.trans_data(&axes_px, (-1.0, 3.0), (10.0, 20.0));

        let points = [[-1.0, 10.0], [3.0, 20.0], [1.5, 13.0]];
        for [x, y] in points {
            assert_eq!(td.transform_point((x, y)), direct.transform_point((x, y)));
        }
    }

    #[test]
    fn autoscale_expands_data_by_margins() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let xs: Vec<f64> = (0..=10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| (x / 10.0) * 2.0 - 1.0).collect();
        axes.plot(&xs, &ys);

        let (xlim, ylim) = axes.effective_limits();
        // x in [0, 10] expanded by 0.05*10 = 0.5 each side.
        approx(xlim.0, -0.5);
        approx(xlim.1, 10.5);
        // y in [-1, 1] expanded by 0.05*2 = 0.1 each side.
        approx(ylim.0, -1.1);
        approx(ylim.1, 1.1);
    }

    #[test]
    fn no_data_falls_back_to_unit_limits() {
        let axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (xlim, ylim) = axes.effective_limits();
        assert_eq!(xlim, (0.0, 1.0));
        assert_eq!(ylim, (0.0, 1.0));
    }

    #[test]
    fn explicit_limits_override_autoscale() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(&[0.0, 1.0], &[0.0, 1.0]);
        axes.set_xlim(-5.0, 5.0);
        let (xlim, _) = axes.effective_limits();
        assert_eq!(xlim, (-5.0, 5.0));
    }

    #[test]
    fn zero_width_range_is_guarded() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.set_xlim(3.0, 3.0);
        let (xlim, _) = axes.effective_limits();
        assert!(xlim.1 > xlim.0, "range should be widened: {xlim:?}");
    }

    /// A [`Renderer`] that records the stroke color of each `draw_path` call,
    /// used to read back a [`Line2D`]'s effective stroke color.
    #[derive(Default)]
    struct StrokeRecorder {
        strokes: Vec<Option<Rgba>>,
    }

    impl Renderer for StrokeRecorder {
        fn draw_path(
            &mut self,
            gc: &GraphicsContext,
            _path: &Path,
            _transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            self.strokes.push(gc.stroke);
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    /// Draw `line` through a [`StrokeRecorder`] and return its stroke color.
    fn line_stroke(line: &Line2D) -> Rgba {
        let mut r = StrokeRecorder::default();
        line.draw(&mut r, &Affine2D::identity());
        r.strokes[0].expect("line strokes with a color")
    }

    #[test]
    fn successive_plots_advance_the_color_cycle() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(&[0.0, 1.0], &[0.0, 1.0]);
        axes.plot(&[0.0, 1.0], &[1.0, 0.0]);

        // C0 = #1f77b4 (tab10 blue), C1 = #ff7f0e (tab10 orange).
        assert_eq!(
            line_stroke(&axes.lines[0]),
            Rgba::from_hex("#1f77b4").unwrap()
        );
        assert_eq!(
            line_stroke(&axes.lines[1]),
            Rgba::from_hex("#ff7f0e").unwrap()
        );
    }

    #[test]
    fn explicit_override_on_returned_line_wins() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = vec![0.0, 1.0];
        let y = vec![0.0, 1.0];
        // Overwrite the cycled handle with an explicitly colored line.
        *axes.plot(&x, &y) = Line2D::new(x.clone(), y.clone()).with_color(Rgba::RED);
        assert_eq!(line_stroke(&axes.lines[0]), Rgba::RED);
    }

    #[test]
    fn plot_with_color_does_not_advance_the_cycle() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot_with_color(&[0.0, 1.0], &[0.0, 1.0], Rgba::RED);
        // The cycle is untouched, so the next plot is still C0.
        axes.plot(&[0.0, 1.0], &[1.0, 0.0]);
        assert_eq!(line_stroke(&axes.lines[0]), Rgba::RED);
        assert_eq!(
            line_stroke(&axes.lines[1]),
            Rgba::from_hex("#1f77b4").unwrap()
        );
    }

    #[test]
    fn aspect_equal_yields_equal_data_units_per_pixel() {
        // A wide, non-square axes with equal x/y data ranges: without equal
        // aspect the x and y pixel scales differ; with it they match.
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.set_xlim(-1.0, 1.0).set_ylim(-1.0, 1.0);
        let (fig_w, fig_h) = (400.0, 200.0);

        let (_, td_default) = axes.pixel_rect_and_trans_data(fig_w, fig_h);
        let sx =
            td_default.transform_point((1.0, 0.0)).0 - td_default.transform_point((0.0, 0.0)).0;
        let sy =
            td_default.transform_point((0.0, 1.0)).1 - td_default.transform_point((0.0, 0.0)).1;
        assert!(
            (sx - sy).abs() > 1.0,
            "default (non-equal) scales should differ: sx={sx}, sy={sy}"
        );

        axes.set_aspect_equal();
        let (_, td_equal) = axes.pixel_rect_and_trans_data(fig_w, fig_h);
        let ex = td_equal.transform_point((1.0, 0.0)).0 - td_equal.transform_point((0.0, 0.0)).0;
        let ey = td_equal.transform_point((0.0, 1.0)).1 - td_equal.transform_point((0.0, 0.0)).1;
        approx(ex, ey);
    }

    #[test]
    fn default_aspect_leaves_rect_unchanged() {
        // With no equal-aspect request the resolved rect spans the full
        // figure-fraction position exactly.
        let axes = Axes::new(Bbox::from_extents(0.1, 0.2, 0.9, 0.8));
        let (rect, _) = axes.pixel_rect_and_trans_data(400.0, 200.0);
        approx(rect.xmin(), 40.0);
        approx(rect.xmax(), 360.0);
        approx(rect.ymin(), 40.0);
        approx(rect.ymax(), 160.0);
    }

    #[test]
    fn axis_off_then_on_toggles_visibility() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        assert!(axes.axis_visible);
        axes.set_axis_off();
        assert!(!axes.axis_visible);
        axes.set_axis_on();
        assert!(axes.axis_visible);
    }

    #[test]
    fn reset_returns_cycle_to_start() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(&[0.0, 1.0], &[0.0, 1.0]);
        axes.set_prop_cycle_reset();
        axes.plot(&[0.0, 1.0], &[1.0, 0.0]);
        assert_eq!(
            line_stroke(&axes.lines[1]),
            Rgba::from_hex("#1f77b4").unwrap()
        );
    }
}
