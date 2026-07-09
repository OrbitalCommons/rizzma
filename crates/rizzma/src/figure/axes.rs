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
//! Like [`crate::axis`], everything is a **y-UP** pixel space; the raster
//! backend applies its own Y-flip. The data-to-pixel mapping is built by
//! [`Axes::trans_data`].

use crate::artist::{Artist, AxesImage, Collection, Line2D, Patch, QuadMesh};
use std::borrow::Cow;

use crate::axis::axis::{Axis, AxisSide};
use crate::axis::dates::{AutoDateLocator, ConciseDateFormatter};
use crate::axis::scale::{AsinhScale, LinearScale, LogScale, LogitScale, Scale, SymlogScale};
use crate::axis::ticker::{
    AsinhFormatterMathtext, AsinhLocator, AutoLocator, LogFormatterMathtext, LogLocator,
    LogitFormatterMathtext, LogitLocator, ScalarFormatter, SymlogFormatterMathtext, SymlogLocator,
};
use crate::core::color::{DEFAULT_COLOR_CYCLE, Rgba};
use crate::core::{Affine2D, Bbox, Path};
use crate::render::{ClippedRenderer, GraphicsContext, Renderer};
use crate::text::FontSource;

use crate::figure::richtext::layout_rich_text;

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
    /// Inverse-hyperbolic-sine scale with a quasi-linear region around zero.
    Asinh { linear_width: f64 },
}

const DEFAULT_TITLE_SIZE: f64 = 12.0;
const DEFAULT_TITLE_PAD: f64 = 6.0;

/// Default annotation/text font size in pixels (matplotlib's `font.size`).
const DEFAULT_ANNOTATION_SIZE: f64 = 10.0;
/// Gap between an annotation's text anchor and the start of its leader arrow,
/// in pixels.
const ANNOTATION_ARROW_GAP: f64 = 4.0;
/// Pull-back of the arrow tip from the annotated point, in pixels
/// (matplotlib's `shrinkB`).
const ANNOTATION_ARROW_SHRINK: f64 = 3.0;
/// Arrowhead length along the shaft, in pixels.
const ANNOTATION_HEAD_LEN: f64 = 8.0;
/// Arrowhead half-width across the shaft, in pixels.
const ANNOTATION_HEAD_HALF_WIDTH: f64 = 3.5;

/// Oscilloscope background: a near-black CRT face.
const SCOPE_BG: Rgba = Rgba::new(0.035, 0.05, 0.04, 1.0);
/// Oscilloscope graticule lines (dim phosphor).
const SCOPE_GRID: Rgba = Rgba::new(0.0, 0.9, 0.35, 0.16);
/// Oscilloscope center crosshair lines (brighter than the graticule).
const SCOPE_GRID_CENTER: Rgba = Rgba::new(0.0, 0.9, 0.35, 0.32);
/// Oscilloscope bezel (the border stroked over the trace).
const SCOPE_BEZEL: Rgba = Rgba::new(0.0, 0.9, 0.35, 0.55);
/// Oscilloscope readout text color.
const SCOPE_TEXT: Rgba = Rgba::new(0.25, 1.0, 0.45, 0.92);
/// Oscilloscope graticule divisions along x.
const SCOPE_X_DIVS: usize = 10;
/// Oscilloscope graticule divisions along y.
const SCOPE_Y_DIVS: usize = 4;
/// Oscilloscope corner-readout font size, in pixels at 100 DPI.
const SCOPE_TEXT_SIZE: f64 = 8.0;
/// Gap between the bezel and the corner readouts, in pixels at 100 DPI.
const SCOPE_TEXT_PAD: f64 = 3.0;
/// The phosphor trace cycle used instead of tab10 in scope mode.
const SCOPE_CYCLE: [Rgba; 4] = [
    Rgba::new(0.22, 1.0, 0.08, 1.0), // phosphor green
    Rgba::new(1.0, 0.69, 0.0, 1.0),  // amber
    Rgba::new(0.0, 0.9, 1.0, 1.0),   // cyan
    Rgba::new(1.0, 0.36, 0.88, 1.0), // magenta
];

/// Compact adaptive formatting for oscilloscope corner readouts: plain
/// decimals in a comfortable range, scientific notation outside it, so the
/// readouts stay a handful of glyphs at any magnitude.
fn format_scope_value(v: f64) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    let a = v.abs();
    if a != 0.0 && !(0.01..10_000.0).contains(&a) {
        format!("{v:.1e}")
    } else if a >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

/// A secondary (top) x axis: an affine unit conversion of the primary x.
#[derive(Debug, Clone)]
struct SecondaryXAxis {
    /// Conversion slope: `secondary = scale * x + offset`.
    scale: f64,
    /// Conversion intercept.
    offset: f64,
    /// Optional axis label above the tick labels.
    label: Option<String>,
}

/// A text annotation in data coordinates, optionally with a leader arrow.
#[derive(Debug, Clone)]
struct Annotation {
    /// The label; `$...$` spans render as mathtext.
    text: String,
    /// The annotated data point (the arrow's target when `text_at` is set).
    xy: (f64, f64),
    /// Where the text's baseline-left anchor sits, in data coordinates.
    /// `None` anchors the text at `xy` and draws no arrow.
    text_at: Option<(f64, f64)>,
    /// Text (and arrow) color.
    color: Rgba,
    /// Font size in pixels.
    size: f64,
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
            ScaleSpec::Asinh { linear_width } => AsinhScale::new(linear_width).transform(value),
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
            ScaleSpec::Asinh { linear_width } => AsinhScale::new(linear_width).inverse(value),
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
            ScaleSpec::Asinh { .. } => guard_range(limits),
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
    /// When set, the frame rect is derived at layout time: this envelope
    /// (figure fractions) minus the measured decoration extents minus a pad —
    /// matplotlib's tight layout. `add_subplot` axes get one; `add_axes`
    /// rects stay literal.
    pub(crate) layout_envelope: Option<Bbox>,
    /// Sticky x values (matplotlib's sticky edges): autoscale margins never
    /// expand a limit *past* one of these. Bars pin their baseline, images
    /// their extents, lines their x data range.
    pub(crate) sticky_x: Vec<f64>,
    /// Sticky y values; see [`sticky_x`](Axes::sticky_x).
    pub(crate) sticky_y: Vec<f64>,
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
    /// When `true` the x axis (spine, ticks, labels) is not drawn — set on
    /// twin axes, whose x decoration belongs to the primary.
    xaxis_hidden: bool,
    /// Index of the axes whose effective x-limits this axes mirrors
    /// ([`Figure::twinx`](crate::figure::Figure::twinx)); resolved by the
    /// figure, which threads the shared limits into drawing and pixel mapping.
    pub(crate) xlim_link: Option<usize>,
    /// Optional secondary (top) x axis showing an affine unit conversion of
    /// the primary x limits.
    secondary_x: Option<SecondaryXAxis>,
    /// Optional title drawn above the axes.
    title: Option<String>,
    /// Text annotations (with optional leader arrows) in data coordinates.
    annotations: Vec<Annotation>,
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
    /// Oscilloscope styling (see [`Axes::oscilloscope`]): CRT background,
    /// fixed-division graticule, phosphor trace cycle, and in-frame corner
    /// readouts instead of axis decorations.
    scope: bool,
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
    pub(crate) legend: Vec<crate::figure::legend::LegendEntry>,
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
            layout_envelope: None,
            sticky_x: Vec::new(),
            sticky_y: Vec::new(),
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
            xaxis_hidden: false,
            xlim_link: None,
            secondary_x: None,
            title: None,
            annotations: Vec::new(),
            frame: true,
            aspect_equal: false,
            axis_visible: true,
            scope: false,
            prop_cycle_index: 0,
            span_lines: Vec::new(),
            span_rects: Vec::new(),
            legend: Vec::new(),
        }
    }

    /// Switch this axes to **oscilloscope** styling: a chart built to stay
    /// legible at any size, down to sparkline-height strips.
    ///
    /// Everything draws *inside* the frame, so the axes reserves no layout
    /// room for decorations: a near-black CRT background, a fixed
    /// 10×4-division phosphor graticule (with a brighter center crosshair)
    /// instead of data-driven ticks, traces cycling through phosphor colors
    /// (green, amber, cyan, magenta), a dim bezel stroke, and small corner
    /// readouts — y-max top-right, y-min bottom-right, x-span bottom-left —
    /// that track the *effective* limits, so they follow live pan/zoom.
    ///
    /// Call before plotting so the traces pick up the phosphor cycle.
    /// Interaction, [`set_line_data`](Axes::set_line_data) streaming, and
    /// [`sharex`](crate::figure::Figure::sharex) all work as usual.
    ///
    /// On docs.rs the strips below are live — streaming, zoomable, x-linked;
    /// elsewhere they fall back to the static gallery render.
    ///
    /// <div class="rizzma-live" data-demo="scope">
    ///
    /// ![oscilloscope](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_oscilloscope.png)
    ///
    /// </div>
    pub fn oscilloscope(&mut self) -> &mut Self {
        self.scope = true;
        self.facecolor = SCOPE_BG;
        self.axis_visible = false;
        self.frame = false;
        self
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
        if self.scope {
            return SCOPE_CYCLE[index % SCOPE_CYCLE.len()];
        }
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
        self.stick_x_extremes(x);
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
        self.stick_x_extremes(x);
        self.lines
            .push(Line2D::new(x.to_vec(), y.to_vec()).with_color(color));
        self.lines.last_mut().expect("just pushed a line")
    }

    /// Add a pre-built [`Line2D`], returning a mutable reference to it.
    pub fn add_line(&mut self, line: Line2D) -> &mut Line2D {
        self.lines.push(line);
        self.lines.last_mut().expect("just pushed a line")
    }

    /// The number of lines plotted on this axes (indexable by
    /// [`set_line_data`](Axes::set_line_data)).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Show or hide the x axis' tick labels and axis label (tick marks and
    /// the spine always draw), releasing their layout room when hidden —
    /// matplotlib's `label_outer` for the inner axes of a shared-x stack.
    pub fn set_x_tick_labels_visible(&mut self, visible: bool) -> &mut Self {
        self.xaxis.set_tick_labels_visible(visible);
        self
    }

    /// Replace the data of line `line` in place (for live/streaming updates),
    /// keeping its style.
    ///
    /// Autoscaled limits re-derive from the new data on the next draw;
    /// explicit [`set_xlim`](Axes::set_xlim)/[`set_ylim`](Axes::set_ylim)
    /// (including limits stored by interaction) are untouched, so updating
    /// data never yanks a view the user has framed.
    ///
    /// # Errors
    ///
    /// Returns an error naming the valid range when `line` is out of range.
    pub fn set_line_data(&mut self, line: usize, x: &[f64], y: &[f64]) -> Result<(), String> {
        let count = self.lines.len();
        let entry = self
            .lines
            .get_mut(line)
            .ok_or_else(|| format!("line index {line} out of range (axes has {count} lines)"))?;
        entry.set_data(x, y);
        Ok(())
    }

    /// Replace the offsets of collection `collection` in place (for
    /// live/streaming scatter updates), keeping its markers, sizes, and
    /// colors. Only the common prefix of `x` and `y` is used.
    ///
    /// Autoscaled limits re-derive from the new offsets on the next draw;
    /// explicit limits (including limits stored by interaction) are
    /// untouched, exactly like [`set_line_data`](Axes::set_line_data).
    ///
    /// # Errors
    ///
    /// Returns an error naming the valid range when `collection` is out of
    /// range.
    pub fn set_collection_offsets(
        &mut self,
        collection: usize,
        x: &[f64],
        y: &[f64],
    ) -> Result<(), String> {
        let count = self.collections.len();
        let entry = self.collections.get_mut(collection).ok_or_else(|| {
            format!("collection index {collection} out of range (axes has {count} collections)")
        })?;
        let n = x.len().min(y.len());
        entry.set_offsets((0..n).map(|i| [x[i], y[i]]).collect());
        Ok(())
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

    /// Place `s` at the data point `(x, y)` (baseline-left anchored, like
    /// matplotlib's `Axes.text`). `$...$` spans render as mathtext.
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(3.0, 2.0);
    /// let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    /// ax.set_xlim(0.0, 10.0).set_ylim(0.0, 10.0);
    /// ax.text(2.0, 5.0, "local $\\mu$");
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn text(&mut self, x: f64, y: f64, s: impl Into<String>) -> &mut Self {
        self.annotations.push(Annotation {
            text: s.into(),
            xy: (x, y),
            text_at: None,
            color: Rgba::BLACK,
            size: DEFAULT_ANNOTATION_SIZE,
        });
        self
    }

    /// Annotate the data point `xy` with `s`, drawing the text at `xytext`
    /// (both in data coordinates) and a leader arrow from the text toward the
    /// point — matplotlib's `annotate(s, xy=…, xytext=…,
    /// arrowprops={'arrowstyle': '->'})`.
    ///
    /// The gallery's annotated peak
    /// (![annotate](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_annotate.png))
    /// is drawn exactly this way.
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(4.0, 3.0);
    /// let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    /// let x: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
    /// let y: Vec<f64> = x.iter().map(|v| (-((v - 5.0).powi(2))).exp()).collect();
    /// ax.plot(&x, &y);
    /// ax.annotate("the peak", (5.0, 1.0), (6.5, 0.8));
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn annotate(
        &mut self,
        s: impl Into<String>,
        xy: (f64, f64),
        xytext: (f64, f64),
    ) -> &mut Self {
        self.annotations.push(Annotation {
            text: s.into(),
            xy,
            text_at: Some(xytext),
            color: Rgba::BLACK,
            size: DEFAULT_ANNOTATION_SIZE,
        });
        self
    }

    /// Reconfigure this axes as a twin sharing another axes' x mapping
    /// (transparent background, no frame, hidden x axis, y axis on the right).
    /// Called by [`Figure::twinx`](crate::figure::Figure::twinx).
    pub(crate) fn configure_as_twinx(&mut self, source: usize) {
        self.facecolor = Rgba::TRANSPARENT;
        self.frame = false;
        self.xaxis_hidden = true;
        self.yaxis = Axis::new(AxisSide::Right);
        self.xlim_link = Some(source);
    }

    /// Add a secondary (top) x axis showing the affine unit conversion
    /// `secondary = scale * x + offset` of this axes' x limits — matplotlib's
    /// `secondary_xaxis(functions=(forward, inverse))` for the affine case
    /// that covers unit conversions (mm ↔ inches, seconds ↔ samples, …).
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(4.0, 3.0);
    /// let ax = fig.add_axes(0.12, 0.12, 0.76, 0.70);
    /// ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
    /// ax.set_xlabel("inches");
    /// // Top axis in millimeters.
    /// ax.secondary_xaxis_linear(25.4, 0.0, Some("mm"));
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn secondary_xaxis_linear(
        &mut self,
        scale: f64,
        offset: f64,
        label: Option<&str>,
    ) -> &mut Self {
        self.secondary_x = Some(SecondaryXAxis {
            scale,
            offset,
            label: label.map(str::to_string),
        });
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
    /// The sticky edges in force right now: the statically pinned values
    /// (bar/stem baselines, line x extremes) plus the **live** extents of
    /// every image and mesh — rasters and meshes stay flush with wherever
    /// their extent currently is, including after
    /// [`AxesImage::set_extent`](crate::artist::AxesImage::set_extent).
    fn gathered_sticky_edges(&self) -> (Vec<f64>, Vec<f64>) {
        let mut sx = self.sticky_x.clone();
        let mut sy = self.sticky_y.clone();
        let extents = self
            .images
            .iter()
            .filter_map(Artist::data_extents)
            .chain(self.meshes.iter().filter_map(Artist::data_extents));
        for e in extents {
            sx.push(e.xmin());
            sx.push(e.xmax());
            sy.push(e.ymin());
            sy.push(e.ymax());
        }
        (sx, sy)
    }

    /// Pin the finite extremes of `x` as sticky x edges, so line plots are
    /// tight in x (the y margin still applies). Matplotlib pads lines in both
    /// axes; rizzma deliberately keeps xy line charts flush left/right.
    fn stick_x_extremes(&mut self, x: &[f64]) {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in x {
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if lo <= hi {
            self.sticky_x.push(lo);
            self.sticky_x.push(hi);
        }
    }

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

    /// Use date ticks and concise date labels on the x-axis.
    ///
    /// Data values remain numeric days since the Unix epoch, as produced by
    /// [`crate::axis::dates::date2num`]. The axis scale stays linear; only the
    /// locator and formatter are replaced.
    pub fn set_xaxis_date(&mut self) -> &mut Self {
        self.xscale = ScaleSpec::Linear;
        self.xaxis
            .set_scale(Box::new(LinearScale::new()))
            .set_locator(Box::new(AutoDateLocator::new()))
            .set_formatter(Box::new(ConciseDateFormatter::new()));
        self
    }

    /// Use date ticks and concise date labels on the y-axis.
    ///
    /// Data values remain numeric days since the Unix epoch, as produced by
    /// [`crate::axis::dates::date2num`]. The axis scale stays linear; only the
    /// locator and formatter are replaced.
    pub fn set_yaxis_date(&mut self) -> &mut Self {
        self.yscale = ScaleSpec::Linear;
        self.yaxis
            .set_scale(Box::new(LinearScale::new()))
            .set_locator(Box::new(AutoDateLocator::new()))
            .set_formatter(Box::new(ConciseDateFormatter::new()));
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

    /// Use an inverse-hyperbolic-sine x-axis scale with `linear_width`.
    ///
    /// The asinh transform is quasi-linear near zero and logarithmic in both
    /// tails, so negative values and zero remain valid. Limits, autoscaling,
    /// and public data APIs remain in raw data units; the transform is applied
    /// at draw time. Line, patch, collection, span, and reference-line
    /// geometry is asinh-scaled; raster images and quad meshes are
    /// intentionally unsupported on nonlinear axes in this implementation.
    ///
    /// # Panics
    ///
    /// Panics when `linear_width` is not finite or is less than or equal to
    /// zero.
    pub fn set_xscale_asinh(&mut self, linear_width: f64) -> &mut Self {
        assert!(
            linear_width.is_finite() && linear_width > 0.0,
            "asinh scale linear_width must be finite and > 0"
        );
        self.xscale = ScaleSpec::Asinh { linear_width };
        self.xaxis
            .set_scale(Box::new(AsinhScale::new(linear_width)))
            .set_locator(Box::new(AsinhLocator::new(linear_width)))
            .set_formatter(Box::new(AsinhFormatterMathtext::new(10.0, linear_width)));
        self
    }

    /// Use an inverse-hyperbolic-sine y-axis scale with `linear_width`.
    ///
    /// The asinh transform is quasi-linear near zero and logarithmic in both
    /// tails, so negative values and zero remain valid. Limits, autoscaling,
    /// and public data APIs remain in raw data units; the transform is applied
    /// at draw time. Line, patch, collection, span, and reference-line
    /// geometry is asinh-scaled; raster images and quad meshes are
    /// intentionally unsupported on nonlinear axes in this implementation.
    ///
    /// # Panics
    ///
    /// Panics when `linear_width` is not finite or is less than or equal to
    /// zero.
    pub fn set_yscale_asinh(&mut self, linear_width: f64) -> &mut Self {
        assert!(
            linear_width.is_finite() && linear_width > 0.0,
            "asinh scale linear_width must be finite and > 0"
        );
        self.yscale = ScaleSpec::Asinh { linear_width };
        self.yaxis
            .set_scale(Box::new(AsinhScale::new(linear_width)))
            .set_locator(Box::new(AsinhLocator::new(linear_width)))
            .set_formatter(Box::new(AsinhFormatterMathtext::new(10.0, linear_width)));
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.symlogy(&[-2.0, -1.0, 0.0, 1.0, 2.0], &[-100.0, -1.0, 0.0, 1.0, 100.0]);
    /// ```
    pub fn symlogy(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_yscale_symlog(10.0, 1.0);
        self.plot(x, y)
    }

    /// Plot with an inverse-hyperbolic-sine x-axis and linear y-axis.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.asinhx(&[-100.0, -1.0, 0.0, 1.0, 100.0], &[1.0, 2.0, 3.0, 2.0, 1.0]);
    /// ```
    pub fn asinhx(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_xscale_asinh(1.0);
        self.plot(x, y)
    }

    /// Plot with a linear x-axis and inverse-hyperbolic-sine y-axis.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.asinhy(&[-2.0, -1.0, 0.0, 1.0, 2.0], &[-100.0, -1.0, 0.0, 1.0, 100.0]);
    /// ```
    pub fn asinhy(&mut self, x: &[f64], y: &[f64]) -> &mut Line2D {
        self.set_yscale_asinh(1.0);
        self.plot(x, y)
    }

    /// Plot with a logit x-axis and linear y-axis.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
        let (sticky_x, sticky_y) = self.gathered_sticky_edges();
        // A scope sweeps edge-to-edge: no x margin, so the trace meets the
        // bezel exactly (y keeps its headroom for the corner readouts).
        let x_margin = if self.scope { 0.0 } else { self.margins };
        let xlim = self.xlim.unwrap_or_else(|| {
            data.map_or((0.0, 1.0), |b| {
                expand_range_sticky(b.xmin(), b.xmax(), x_margin, &sticky_x)
            })
        });
        let ylim = self.ylim.unwrap_or_else(|| {
            data.map_or((0.0, 1.0), |b| {
                expand_range_sticky(b.ymin(), b.ymax(), self.margins, &sticky_y)
            })
        });
        (guard_range(xlim), guard_range(ylim))
    }

    /// Resolve raw limits after applying the active scales' domain guards.
    ///
    /// Public limits remain raw data units. This helper only clamps ranges that
    /// cannot be transformed by the active scale, such as non-positive log
    /// bounds, immediately before draw-time scaling.
    pub(crate) fn scale_limited_effective_limits(&self) -> ((f64, f64), (f64, f64)) {
        let (xlim, ylim) = self.effective_limits();
        self.clamp_limits_to_scale(xlim, ylim)
    }

    /// Clamp candidate `(xlim, ylim)` to the active scales' domains — the same
    /// guards applied at draw time (non-positive log bounds, logit bounds
    /// outside `(0, 1)`, degenerate ranges). Interaction code runs candidate
    /// limits through this before storing them, so one extreme zoom or pan
    /// cannot poison the axes with out-of-domain explicit limits.
    pub(crate) fn clamp_limits_to_scale(
        &self,
        xlim: (f64, f64),
        ylim: (f64, f64),
    ) -> ((f64, f64), (f64, f64)) {
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
    /// The frame's pixel rectangle and data transform, with the x-limits
    /// optionally replaced (a twin sharing its source's x mapping) and the
    /// rectangle optionally replaced (the figure's tight-layout-resolved rect
    /// for auto-layout axes).
    pub(crate) fn pixel_rect_and_trans_data_in(
        &self,
        fig_w_px: f64,
        fig_h_px: f64,
        xlim_override: Option<(f64, f64)>,
        rect_override: Option<Bbox>,
    ) -> (Bbox, Affine2D) {
        let mut axes_px = rect_override.unwrap_or_else(|| {
            Bbox::from_extents(
                self.position.xmin() * fig_w_px,
                self.position.ymin() * fig_h_px,
                self.position.xmax() * fig_w_px,
                self.position.ymax() * fig_h_px,
            )
        });
        let (xlim, ylim) = self.limits_with_override(xlim_override);
        let mapper = self.data_to_scaled();
        let (scaled_xlim, scaled_ylim) = mapper.map_limits(xlim, ylim);
        if self.aspect_equal {
            axes_px = equalize_aspect(&axes_px, scaled_xlim, scaled_ylim);
        }
        let td = self.trans_data(&axes_px, scaled_xlim, scaled_ylim);
        (axes_px, td)
    }

    /// The per-side decoration insets `(left, right, bottom, top)` in pixels
    /// at decoration scale `s`: how far the frame must sit inside its layout
    /// envelope so ticks, labels, and the title fit (tight layout).
    pub(crate) fn layout_insets(
        &self,
        font: &FontSource,
        s: f64,
        xlim_override: Option<(f64, f64)>,
    ) -> (f64, f64, f64, f64) {
        let (mut left, mut right, mut bottom, mut top) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        if self.axis_visible {
            let (xlim, ylim) = self.limits_with_override(xlim_override);
            if !self.xaxis_hidden {
                bottom = self.xaxis.decoration_extent(xlim, font, s);
                // End tick labels are centered on their ticks, so they spill
                // half their width past the frame corners sideways.
                let (lo, hi) = self.xaxis.end_label_overhangs(xlim, font, s);
                left = left.max(lo);
                right = right.max(hi);
            }
            let y_extent = self.yaxis.decoration_extent(ylim, font, s);
            // The y axis decorates whichever side it is drawn on; twins put
            // it on the right.
            if self.yaxis_is_right() {
                right = right.max(y_extent);
            } else {
                left = left.max(y_extent);
            }
            // Likewise the end y tick labels spill half their height past the
            // bottom and top frame corners.
            let (y_lo, y_hi) = self.yaxis.end_label_overhangs(ylim, font, s);
            bottom = bottom.max(y_lo);
            top = self.secondary_extent(xlim, font, s).max(y_hi);
        }
        if let Some(title) = &self.title
            && !title.is_empty()
        {
            let rich = layout_rich_text(font, title, DEFAULT_TITLE_SIZE * s);
            top += DEFAULT_TITLE_PAD * s + rich.ascent + rich.descent;
        }
        (left, right, bottom, top)
    }

    /// The measured decoration extent of the secondary top x axis (0 when
    /// there is none) — shared by tight layout and title placement so the
    /// title always clears the real decoration, not an estimate.
    fn secondary_extent(&self, xlim: (f64, f64), font: &FontSource, s: f64) -> f64 {
        let Some(sec) = &self.secondary_x else {
            return 0.0;
        };
        let converted = (
            sec.scale * xlim.0 + sec.offset,
            sec.scale * xlim.1 + sec.offset,
        );
        let mut top_axis = Axis::new(AxisSide::Top);
        if let Some(label) = &sec.label {
            top_axis = top_axis.with_label(label.clone());
        }
        top_axis.decoration_extent(converted, font, s)
    }

    /// Whether the y axis is drawn on the right side (twin axes).
    fn yaxis_is_right(&self) -> bool {
        self.yaxis.side() == AxisSide::Right
    }

    /// The scale-limited effective `(xlim, ylim)` with the x-limits optionally
    /// replaced (and re-guarded through the x scale's domain).
    pub(crate) fn limits_with_override(
        &self,
        xlim_override: Option<(f64, f64)>,
    ) -> ((f64, f64), (f64, f64)) {
        let (xlim, ylim) = self.scale_limited_effective_limits();
        match xlim_override {
            Some(over) => {
                let (x, y) = self.clamp_limits_to_scale(over, ylim);
                (x, y)
            }
            None => (xlim, ylim),
        }
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
        self.draw_with(renderer, fig_w_px, fig_h_px, font, None, None);
    }

    /// [`draw`](Axes::draw) with the x-limits optionally overridden — the
    /// figure passes a twin axes its source's effective x-limits here.
    pub(crate) fn draw_with(
        &self,
        renderer: &mut dyn Renderer,
        fig_w_px: f64,
        fig_h_px: f64,
        font: &FontSource,
        xlim_override: Option<(f64, f64)>,
        rect_override: Option<Bbox>,
    ) {
        // 1. Resolve the pixel rectangle and the data transform via the shared
        // forward path (also used by `Figure`'s coordinate inversion).
        let (axes_px, td) =
            self.pixel_rect_and_trans_data_in(fig_w_px, fig_h_px, xlim_override, rect_override);
        let (xlim, ylim) = self.limits_with_override(xlim_override);
        let mapper = self.data_to_scaled();

        // 2. Fill the axes background.
        let rect = rect_path(&axes_px);
        let fill_gc = GraphicsContext::new();
        renderer.draw_path(&fill_gc, &rect, &Affine2D::identity(), Some(self.facecolor));

        // 2a. Oscilloscope graticule: a fixed-division grid drawn beneath the
        // data (a real scope's screen etching), so its density never depends
        // on the axes size or the data.
        if self.scope {
            self.draw_scope_graticule(renderer, &axes_px);
        }

        // 3–4. Everything that represents *data* draws through a clipping
        // wrapper confined to the axes frame (matplotlib's `clip_on=True`
        // default), so a zoomed or explicitly limited view cannot spill
        // artists across the rest of the figure. Decorations (frame, axis,
        // title, legend, annotations) draw unclipped below.
        {
            let clipped = &mut ClippedRenderer::new(renderer, axes_px);

            // 3a. Draw colormapped images first (lowest zorder), beneath every
            // other artist, mapping their data-space extent through the data
            // transform.
            if mapper.is_linear() {
                for image in &self.images {
                    if image.visible() {
                        image.draw(clipped, &td);
                    }
                }
            }

            // 3a'. Draw colormapped quad meshes (pcolormesh) beneath the other
            // artists, mapping their data-space corners through the data
            // transform.
            if mapper.is_linear() {
                for mesh in &self.meshes {
                    if mesh.visible() {
                        mesh.draw(clipped, &td);
                    }
                }
            }

            // 3b. Draw full-span shaded bands (axhspan/axvspan) beneath the
            // artists, resolving their open extent against the effective
            // limits.
            for span in &self.span_rects {
                let rect = match span.orientation {
                    SpanOrientation::Horizontal => {
                        rect_path(&Bbox::from_extents(xlim.0, span.lo, xlim.1, span.hi))
                    }
                    SpanOrientation::Vertical => {
                        rect_path(&Bbox::from_extents(span.lo, ylim.0, span.hi, ylim.1))
                    }
                };
                clipped.draw_path(
                    &GraphicsContext::new(),
                    &mapper.map_path(&rect),
                    &td,
                    Some(span.facecolor),
                );
            }

            // 4. Draw artists in ascending zorder.
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
                artists[i].draw_scaled(clipped, &td, &mapper);
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
                clipped.draw_path(&gc, &mapper.map_path(&path), &td, None);
            }
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
            if !self.xaxis_hidden {
                self.xaxis.draw(renderer, &axes_px, xlim, font);
            }
            self.yaxis.draw(renderer, &axes_px, ylim, font);
            // 6'. The secondary (top) x axis: the same pixel span labeled in
            // converted units.
            if let Some(sec) = &self.secondary_x {
                let mut top = Axis::new(AxisSide::Top);
                if let Some(label) = &sec.label {
                    top = top.with_label(label.clone());
                }
                let converted = (
                    sec.scale * xlim.0 + sec.offset,
                    sec.scale * xlim.1 + sec.offset,
                );
                top.draw(renderer, &axes_px, converted, font);
            }
        }

        // 6a. Draw the legend box inside the upper-right of the axes.
        self.draw_legend(renderer, &axes_px, font);

        // 6b. Draw text annotations and their leader arrows, mapping their
        // data-space anchors through the same scale + data transform as the
        // artists.
        for ann in &self.annotations {
            self.draw_annotation(renderer, ann, &td, font);
        }

        // 7. Draw the title, centered above the axes. Math spans (`$...$`) are
        // laid out by the mathtext engine via `layout_rich_text`; plain titles
        // reduce to the previous single-string path.
        if let Some(title) = &self.title
            && !title.is_empty()
        {
            let s = renderer.decoration_scale();
            let rich = layout_rich_text(font, title, DEFAULT_TITLE_SIZE * s);
            let cx = (axes_px.xmin() + axes_px.xmax()) / 2.0;
            let x = cx - rich.width / 2.0;
            // Place the baseline `pad` above the top spine, exactly as the
            // previous single-string path did; the rich paths are in a
            // baseline-relative y-up frame.
            // A secondary top axis occupies the strip above the spine (ticks,
            // tick labels, axis label); lift the title clear of its measured
            // extent — the same number tight layout reserves.
            let secondary_clearance = self.secondary_extent(xlim, font, s);
            let y = axes_px.ymax() + DEFAULT_TITLE_PAD * s + secondary_clearance;
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

        // 8. Oscilloscope bezel and corner readouts, above everything: the
        // trace stays inside the bezel and the readouts follow the live
        // effective limits (they update as the view pans and zooms).
        if self.scope {
            self.draw_scope_overlay(renderer, &axes_px, xlim, ylim, font);
        }
    }
}

impl Axes {
    /// Stroke the fixed-division oscilloscope graticule inside `axes_px`,
    /// with the two center lines brighter (the scope's crosshair).
    fn draw_scope_graticule(&self, renderer: &mut dyn Renderer, axes_px: &Bbox) {
        let mut stroke = |points: [[f64; 2]; 2], color: Rgba| {
            let gc = GraphicsContext::new()
                .with_stroke(color)
                .with_line_width(1.0);
            renderer.draw_path(
                &gc,
                &Path::from_polyline(&points),
                &Affine2D::identity(),
                None,
            );
        };
        for i in 1..SCOPE_X_DIVS {
            let x = axes_px.xmin() + axes_px.width() * i as f64 / SCOPE_X_DIVS as f64;
            let color = if 2 * i == SCOPE_X_DIVS {
                SCOPE_GRID_CENTER
            } else {
                SCOPE_GRID
            };
            stroke([[x, axes_px.ymin()], [x, axes_px.ymax()]], color);
        }
        for j in 1..SCOPE_Y_DIVS {
            let y = axes_px.ymin() + axes_px.height() * j as f64 / SCOPE_Y_DIVS as f64;
            let color = if 2 * j == SCOPE_Y_DIVS {
                SCOPE_GRID_CENTER
            } else {
                SCOPE_GRID
            };
            stroke([[axes_px.xmin(), y], [axes_px.xmax(), y]], color);
        }
    }

    /// Draw the oscilloscope bezel and the in-frame corner readouts: y-max
    /// top-right, y-min bottom-right, and the x span bottom-left.
    fn draw_scope_overlay(
        &self,
        renderer: &mut dyn Renderer,
        axes_px: &Bbox,
        xlim: (f64, f64),
        ylim: (f64, f64),
        font: &FontSource,
    ) {
        let bezel_gc = GraphicsContext::new()
            .with_stroke(SCOPE_BEZEL)
            .with_line_width(1.0);
        renderer.draw_path(&bezel_gc, &rect_path(axes_px), &Affine2D::identity(), None);

        let s = renderer.decoration_scale();
        let size = SCOPE_TEXT_SIZE * s;
        let pad = SCOPE_TEXT_PAD * s;
        let mut draw = |text: &str, x: f64, baseline_y: f64| {
            let rich = layout_rich_text(font, text, size);
            let shift = Affine2D::from_translation(x, baseline_y);
            for path in &rich.paths {
                renderer.draw_path(
                    &GraphicsContext::new(),
                    &path.transformed(&shift),
                    &Affine2D::identity(),
                    Some(SCOPE_TEXT),
                );
            }
        };
        let right_align = |text: &str| {
            let rich = layout_rich_text(font, text, size);
            axes_px.xmax() - pad - rich.width
        };

        let top = format_scope_value(ylim.1);
        let bottom = format_scope_value(ylim.0);
        let span = format!("\u{394}{}", format_scope_value(xlim.1 - xlim.0));
        let ascent = layout_rich_text(font, &top, size).ascent;
        draw(&top, right_align(&top), axes_px.ymax() - pad - ascent);
        draw(&bottom, right_align(&bottom), axes_px.ymin() + pad);
        draw(&span, axes_px.xmin() + pad, axes_px.ymin() + pad);
    }

    /// Draw one [`Annotation`]: the text at its anchor, plus a leader arrow
    /// from the text toward the annotated point when `text_at` is set.
    ///
    /// All geometry here is in y-up display pixels; `td` maps scaled data
    /// coordinates into that frame.
    fn draw_annotation(
        &self,
        renderer: &mut dyn Renderer,
        ann: &Annotation,
        td: &Affine2D,
        font: &FontSource,
    ) {
        let mapper = self.data_to_scaled();
        let to_display = |(x, y): (f64, f64)| {
            let [sx, sy] = mapper.map_point(x, y);
            td.transform_point((sx, sy))
        };

        let s = renderer.decoration_scale();
        let anchor = to_display(ann.text_at.unwrap_or(ann.xy));
        let rich = layout_rich_text(font, &ann.text, ann.size * s);
        let shift = Affine2D::from_translation(anchor.0, anchor.1);
        for path in &rich.paths {
            renderer.draw_path(
                &GraphicsContext::new(),
                &path.transformed(&shift),
                &Affine2D::identity(),
                Some(ann.color),
            );
        }

        if ann.text_at.is_some() {
            let target = to_display(ann.xy);
            // Start the arrow at the edge of the text nearest the target: the
            // left or right end of the baseline, whichever faces the point.
            let from = if target.0 < anchor.0 {
                anchor
            } else {
                (anchor.0 + rich.width, anchor.1)
            };
            draw_annotation_arrow(renderer, from, target, ann.color, s);
        }
    }
}

/// Stroke a straight leader arrow from `from` to `to` (display pixels) with a
/// filled triangular head at the `to` end.
///
/// The tail is inset by [`ANNOTATION_ARROW_GAP`] so it clears the text, and
/// the tip pulled back by [`ANNOTATION_ARROW_SHRINK`] so it points at — not
/// covers — the annotated datum. Degenerate (near-zero-length) arrows are
/// skipped entirely.
fn draw_annotation_arrow(
    renderer: &mut dyn Renderer,
    from: (f64, f64),
    to: (f64, f64),
    color: Rgba,
    s: f64,
) {
    let (gap, shrink, head_len) = (
        ANNOTATION_ARROW_GAP * s,
        ANNOTATION_ARROW_SHRINK * s,
        ANNOTATION_HEAD_LEN * s,
    );
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = dx.hypot(dy);
    if len <= gap + shrink + head_len {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let tail = (from.0 + ux * gap, from.1 + uy * gap);
    let tip = (to.0 - ux * shrink, to.1 - uy * shrink);
    let head_base = (tip.0 - ux * head_len, tip.1 - uy * head_len);

    let gc = GraphicsContext::new()
        .with_stroke(color)
        .with_line_width(1.0);
    let shaft = Path::from_polyline(&[[tail.0, tail.1], [head_base.0, head_base.1]]);
    renderer.draw_path(&gc, &shaft, &Affine2D::identity(), None);

    // Filled triangular head: tip plus the two base corners perpendicular to
    // the shaft.
    let (px, py) = (-uy, ux);
    let head = Path::from_polyline(&[
        [tip.0, tip.1],
        [
            head_base.0 + px * ANNOTATION_HEAD_HALF_WIDTH * s,
            head_base.1 + py * ANNOTATION_HEAD_HALF_WIDTH * s,
        ],
        [
            head_base.0 - px * ANNOTATION_HEAD_HALF_WIDTH * s,
            head_base.1 - py * ANNOTATION_HEAD_HALF_WIDTH * s,
        ],
        [tip.0, tip.1],
    ]);
    renderer.draw_path(
        &GraphicsContext::new(),
        &head,
        &Affine2D::identity(),
        Some(color),
    );
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

/// [`expand_range`] clipped by sticky edges (matplotlib semantics): the margin
/// never carries a limit *past* a sticky value sitting at (or inside) the data
/// bound — a bar chart's zero baseline stays on the axis floor and an image
/// stays flush with its extent, while unsticky ends keep their full margin.
fn expand_range_sticky(min: f64, max: f64, margin: f64, sticky: &[f64]) -> (f64, f64) {
    let (mut lo, mut hi) = expand_range(min, max, margin);
    // The tightest sticky value at or below the data minimum bounds the
    // low-side expansion; symmetrically for the high side.
    let lo_bound = sticky
        .iter()
        .copied()
        .filter(|s| s.is_finite() && *s <= min)
        .fold(f64::NEG_INFINITY, f64::max);
    if lo < lo_bound {
        lo = lo_bound;
    }
    let hi_bound = sticky
        .iter()
        .copied()
        .filter(|s| s.is_finite() && *s >= max)
        .fold(f64::INFINITY, f64::min);
    if hi > hi_bound {
        hi = hi_bound;
    }
    (lo, hi)
}

/// Nudge a degenerate (zero- or negative-width) range apart so it has a finite,
/// positive width suitable for division.
fn guard_range((min, max): (f64, f64)) -> (f64, f64) {
    // Degeneracy is judged *relative* to the values' magnitude (like
    // matplotlib's `nonsingular`): an absolute epsilon would misjudge a
    // perfectly healthy tiny-magnitude range — e.g. a deep-zoomed log axis at
    // (1e-30, 1e-22), eight decades wide — as zero-width and blow it up to
    // ±0.5, off the scale's domain entirely.
    let magnitude = min.abs().max(max.abs());
    if (max - min).abs() > f64::EPSILON * magnitude {
        return (min, max);
    }
    // Expand symmetrically around the value; matplotlib uses a unit pad for a
    // truly zero-width range and a relative pad otherwise.
    let pad = if magnitude > f64::MIN_POSITIVE {
        magnitude * 0.05
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
    fn data_to_scaled_asinh_maps_negative_zero_and_positive_points() {
        let scale = ScaleSpec::Asinh { linear_width: 2.0 };
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
    fn asinh_tick_position_uses_scaled_coordinate_fraction() {
        let scale = ScaleSpec::Asinh { linear_width: 2.0 };

        approx(scaled_tick_position(0.0, (-100.0, 100.0), scale), 0.5);
        assert!(scaled_tick_position(-10.0, (-100.0, 100.0), scale) < 0.5);
        assert!(scaled_tick_position(10.0, (-100.0, 100.0), scale) > 0.5);
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
        let (axes_px, td) = axes.pixel_rect_and_trans_data_in(200.0, 100.0, None, None);
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
        let (axes_px, td) = axes.pixel_rect_and_trans_data_in(200.0, 100.0, None, None);
        assert!(axes_px.width().is_finite());
        assert!(
            td.transform_point((LogitScale::new().transform(xlim.0), 0.0))
                .0
                .is_finite()
        );
    }

    #[test]
    fn asinh_effective_limits_preserve_negative_zero_positive_domain() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.plot(
            &[-100.0, -1.0, 0.0, 1.0, 100.0],
            &[-1.0, 0.0, 1.0, 0.0, -1.0],
        );
        axes.set_xscale_asinh(2.0).set_xlim(-100.0, 100.0);

        let (xlim, _) = axes.scale_limited_effective_limits();
        assert_eq!(xlim, (-100.0, 100.0));
        let (axes_px, td) = axes.pixel_rect_and_trans_data_in(200.0, 100.0, None, None);
        assert!(axes_px.width().is_finite());
        assert!(td.transform_point((0.0, 0.0)).0.is_finite());
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

    #[test]
    fn asinh_wrappers_set_expected_axis_scales() {
        let mut x_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        x_axes.asinhx(&[-10.0, 0.0, 10.0], &[1.0, 2.0, 3.0]);
        assert_eq!(x_axes.xscale, ScaleSpec::Asinh { linear_width: 1.0 });
        assert_eq!(x_axes.yscale, ScaleSpec::Linear);

        let mut y_axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        y_axes.asinhy(&[-1.0, 0.0, 1.0], &[-10.0, 0.0, 10.0]);
        assert_eq!(y_axes.xscale, ScaleSpec::Linear);
        assert_eq!(y_axes.yscale, ScaleSpec::Asinh { linear_width: 1.0 });
    }

    #[test]
    fn date_axis_setters_keep_linear_scale() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.set_xaxis_date();
        assert_eq!(axes.xscale, ScaleSpec::Linear);
        assert_eq!(axes.yscale, ScaleSpec::Linear);

        axes.set_yscale_log(10.0).set_yaxis_date();
        assert_eq!(axes.xscale, ScaleSpec::Linear);
        assert_eq!(axes.yscale, ScaleSpec::Linear);
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
        let (axes_px, td) = axes.pixel_rect_and_trans_data_in(200.0, 100.0, None, None);
        assert!(axes_px.width().is_finite());
        assert!(td.transform_point((xlim.0.log10(), 0.0)).0.is_finite());
    }

    #[test]
    fn pixel_rect_and_trans_data_linear_matches_direct_affine() {
        let mut axes = Axes::new(Bbox::from_extents(0.1, 0.2, 0.9, 0.8));
        axes.set_xlim(-1.0, 3.0).set_ylim(10.0, 20.0);
        let (axes_px, td) = axes.pixel_rect_and_trans_data_in(500.0, 400.0, None, None);
        let direct = axes.trans_data(&axes_px, (-1.0, 3.0), (10.0, 20.0));

        let points = [[-1.0, 10.0], [3.0, 20.0], [1.5, 13.0]];
        for [x, y] in points {
            assert_eq!(td.transform_point((x, y)), direct.transform_point((x, y)));
        }
    }

    #[test]
    fn autoscale_pads_y_but_keeps_line_x_tight() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let xs: Vec<f64> = (0..=10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| (x / 10.0) * 2.0 - 1.0).collect();
        axes.plot(&xs, &ys);

        let (xlim, ylim) = axes.effective_limits();
        // Line plots pin their x data range (sticky edges): no left/right pad.
        approx(xlim.0, 0.0);
        approx(xlim.1, 10.0);
        // y in [-1, 1] expanded by 0.05*2 = 0.1 each side.
        approx(ylim.0, -1.1);
        approx(ylim.1, 1.1);
    }

    #[test]
    fn bars_and_hist_sit_on_the_zero_baseline() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.bar(&[0.0, 1.0, 2.0], &[3.0, 7.0, 5.0]);
        let (_, ylim) = axes.effective_limits();
        approx(ylim.0, 0.0); // baseline pinned, no float
        assert!(ylim.1 > 7.0); // top keeps its margin

        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.hist(&[0.0, 0.1, 0.2, 0.9, 1.0], 4);
        let (_, ylim) = axes.effective_limits();
        approx(ylim.0, 0.0);

        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.barh(&[0.0, 1.0], &[4.0, 6.0]);
        let (xlim, _) = axes.effective_limits();
        approx(xlim.0, 0.0); // horizontal baseline pinned in x
    }

    #[test]
    fn images_stay_flush_with_their_extent() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.imshow(&[0.0, 1.0, 2.0, 3.0], 2, 2);
        let (xlim, ylim) = axes.effective_limits();
        approx(xlim.0, 0.0);
        approx(xlim.1, 2.0);
        approx(ylim.0, 0.0);
        approx(ylim.1, 2.0);
    }

    #[test]
    fn image_sticky_edges_track_a_changed_extent() {
        // Change the extent AFTER imshow: the limits must be flush with the
        // new extent, not the creation-time default.
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.imshow(&[0.0, 1.0, 2.0, 3.0], 2, 2)
            .set_extent([10.0, 30.0, -5.0, 5.0]);
        let (xlim, ylim) = axes.effective_limits();
        approx(xlim.0, 10.0);
        approx(xlim.1, 30.0);
        approx(ylim.0, -5.0);
        approx(ylim.1, 5.0);
    }

    #[test]
    fn contourf_and_hist2d_meshes_stay_flush() {
        // contourf builds its QuadMesh directly (not via pcolormesh); the
        // live mesh-extent gathering must still pin it.
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let z: Vec<f64> = (0..9).map(f64::from).collect();
        axes.contourf(&z, 3, 3);
        let (xlim, ylim) = axes.effective_limits();
        approx(xlim.0, 0.0);
        approx(xlim.1, 2.0);
        approx(ylim.0, 0.0);
        approx(ylim.1, 2.0);

        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.hist2d(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 2.0, 3.0], 4);
        let (xlim, ylim) = axes.effective_limits();
        // The bin grid spans the data range exactly; no margin pad.
        approx(xlim.0, 0.0);
        approx(xlim.1, 3.0);
        approx(ylim.0, 0.0);
        approx(ylim.1, 3.0);
    }

    #[test]
    fn contour_lines_stay_flush_with_their_grid() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let z = [0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
        axes.contour(&z, 3, 3);
        let (xlim, ylim) = axes.effective_limits();
        approx(xlim.0, 0.0);
        approx(xlim.1, 2.0);
        approx(ylim.0, 0.0);
        approx(ylim.1, 2.0);
    }

    #[test]
    fn sticky_edges_do_not_shrink_data_beyond_them() {
        // A line crossing below zero on an axes that also has a bar: the
        // sticky zero must not clip the *data* range, only the margin.
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        axes.bar(&[0.0, 1.0], &[2.0, 3.0]);
        axes.plot(&[0.0, 1.0], &[-5.0, 3.0]);
        let (_, ylim) = axes.effective_limits();
        assert!(ylim.0 <= -5.0, "data below a sticky edge stays visible");
    }

    #[test]
    fn no_data_falls_back_to_unit_limits() {
        let axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (xlim, ylim) = axes.effective_limits();
        assert_eq!(xlim, (0.0, 1.0));
        assert_eq!(ylim, (0.0, 1.0));
    }

    #[test]
    fn oscilloscope_switches_to_the_phosphor_cycle() {
        let mut axes = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let tab_c0 = axes.cycle_color(0);
        axes.oscilloscope();
        let scope_c0 = axes.cycle_color(0);
        assert_ne!(tab_c0, scope_c0, "scope mode swaps the trace cycle");
        // Phosphor green first: strongly green-dominant.
        assert!(scope_c0.g > 0.9 && scope_c0.g > scope_c0.r && scope_c0.g > scope_c0.b);
        // Four distinct phosphor channels before wrapping.
        let colors: Vec<_> = (0..4).map(|i| axes.cycle_color(i)).collect();
        for i in 0..4 {
            for j in (i + 1)..4 {
                assert_ne!(colors[i], colors[j]);
            }
        }
        assert_eq!(axes.cycle_color(4), colors[0], "cycle wraps at four");
    }

    #[test]
    fn scope_value_formatting_stays_compact() {
        assert_eq!(format_scope_value(0.49), "0.49");
        assert_eq!(format_scope_value(-0.51), "-0.51");
        assert_eq!(format_scope_value(123.4), "123");
        assert_eq!(format_scope_value(0.0), "0.00");
        assert_eq!(format_scope_value(12345.0), "1.2e4");
        assert_eq!(format_scope_value(0.0004), "4.0e-4");
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

        let (_, td_default) = axes.pixel_rect_and_trans_data_in(fig_w, fig_h, None, None);
        let sx =
            td_default.transform_point((1.0, 0.0)).0 - td_default.transform_point((0.0, 0.0)).0;
        let sy =
            td_default.transform_point((0.0, 1.0)).1 - td_default.transform_point((0.0, 0.0)).1;
        assert!(
            (sx - sy).abs() > 1.0,
            "default (non-equal) scales should differ: sx={sx}, sy={sy}"
        );

        axes.set_aspect_equal();
        let (_, td_equal) = axes.pixel_rect_and_trans_data_in(fig_w, fig_h, None, None);
        let ex = td_equal.transform_point((1.0, 0.0)).0 - td_equal.transform_point((0.0, 0.0)).0;
        let ey = td_equal.transform_point((0.0, 1.0)).1 - td_equal.transform_point((0.0, 0.0)).1;
        approx(ex, ey);
    }

    #[test]
    fn default_aspect_leaves_rect_unchanged() {
        // With no equal-aspect request the resolved rect spans the full
        // figure-fraction position exactly.
        let axes = Axes::new(Bbox::from_extents(0.1, 0.2, 0.9, 0.8));
        let (rect, _) = axes.pixel_rect_and_trans_data_in(400.0, 200.0, None, None);
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
