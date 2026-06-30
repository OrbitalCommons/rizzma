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

use rizzma_artist::{Artist, Collection, Line2D, Patch};
use rizzma_axis::axis::{Axis, AxisSide};
use rizzma_core::color::{DEFAULT_COLOR_CYCLE, Rgba};
use rizzma_core::{Affine2D, Bbox, Path};
use rizzma_render::{GraphicsContext, Renderer};
use rizzma_text::FontSource;

/// Default fractional margin added on each side of the autoscaled data limits.
const DEFAULT_MARGIN: f64 = 0.05;

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
    /// Background fill color of the axes region.
    facecolor: Rgba,
    /// Line artists, drawn in ascending zorder.
    pub(crate) lines: Vec<Line2D>,
    /// Patch artists, drawn in ascending zorder.
    pub(crate) patches: Vec<Patch>,
    /// Scatter [`Collection`] artists, drawn in ascending zorder.
    pub(crate) collections: Vec<Collection>,
    /// The bottom (x) axis.
    xaxis: Axis,
    /// The left (y) axis.
    yaxis: Axis,
    /// Optional title drawn above the axes.
    title: Option<String>,
    /// Whether to stroke the axes frame (border rectangle).
    frame: bool,
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
            facecolor: Rgba::WHITE,
            lines: Vec::new(),
            patches: Vec::new(),
            collections: Vec::new(),
            xaxis: Axis::new(AxisSide::Bottom),
            yaxis: Axis::new(AxisSide::Left),
            title: None,
            frame: true,
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
        for e in line_extents.chain(patch_extents).chain(collection_extents) {
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
    /// expanded by [`margins`](Self::margins) on each side. With no data at all
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
        let axes_px = Bbox::from_extents(
            self.position.xmin() * fig_w_px,
            self.position.ymin() * fig_h_px,
            self.position.xmax() * fig_w_px,
            self.position.ymax() * fig_h_px,
        );

        // 2. Fill the axes background.
        let rect = rect_path(&axes_px);
        let fill_gc = GraphicsContext::new();
        renderer.draw_path(&fill_gc, &rect, &Affine2D::identity(), Some(self.facecolor));

        // 3. Resolve limits and build the data transform.
        let (xlim, ylim) = self.effective_limits();
        let td = self.trans_data(&axes_px, xlim, ylim);

        // 3a. Draw full-span shaded bands (axhspan/axvspan) beneath the artists,
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
            renderer.draw_path(&GraphicsContext::new(), &rect, &td, Some(span.facecolor));
        }

        // 4. Draw artists in ascending zorder.
        // TODO: clip artists to `axes_px` once clip plumbing lands.
        let mut artists: Vec<&dyn Artist> =
            Vec::with_capacity(self.lines.len() + self.patches.len() + self.collections.len());
        artists.extend(self.lines.iter().map(|l| l as &dyn Artist));
        artists.extend(self.patches.iter().map(|p| p as &dyn Artist));
        artists.extend(self.collections.iter().map(|c| c as &dyn Artist));
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
            artists[i].draw(renderer, &td);
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
            renderer.draw_path(&gc, &path, &td, None);
        }

        // 5. Stroke the frame.
        if self.frame {
            let frame_gc = GraphicsContext::new()
                .with_stroke(Rgba::BLACK)
                .with_line_width(0.8);
            renderer.draw_path(&frame_gc, &rect, &Affine2D::identity(), None);
        }

        // 6. Draw the axes spines.
        self.xaxis.draw(renderer, &axes_px, xlim, font);
        self.yaxis.draw(renderer, &axes_px, ylim, font);

        // 6a. Draw the legend box inside the upper-right of the axes.
        self.draw_legend(renderer, &axes_px, font);

        // 7. Draw the title, centered above the axes.
        if let Some(title) = &self.title
            && !title.is_empty()
        {
            let title_size = 12.0;
            let pad = 6.0;
            let extent = font.measure(title, title_size);
            let cx = (axes_px.xmin() + axes_px.xmax()) / 2.0;
            let x = cx - extent.width / 2.0;
            let y = axes_px.ymax() + pad;
            let path = font.text_to_path(title, title_size, [x, y]);
            renderer.draw_path(
                &GraphicsContext::new(),
                &path,
                &Affine2D::identity(),
                Some(Rgba::BLACK),
            );
        }
    }
}

/// A closed-rectangle [`Path`] tracing `bbox`'s four corners.
fn rect_path(bbox: &Bbox) -> Path {
    let (x0, y0) = (bbox.xmin(), bbox.ymin());
    let (x1, y1) = (bbox.xmax(), bbox.ymax());
    Path::from_polyline(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]])
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
