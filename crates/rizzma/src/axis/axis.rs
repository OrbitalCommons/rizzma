//! Renderable axis: spine, ticks, tick labels, optional grid, and axis label.
//!
//! An [`Axis`] ties together a [`Scale`] (data→scaled mapping), a [`Locator`]
//! (tick positions), and a [`Formatter`] (tick labels) and knows how to draw
//! itself along one edge of an axes rectangle.
//!
//! # Coordinate convention
//!
//! Everything is expressed in a **y-UP** display space measured in pixels: the
//! y axis increases upward, matching the font/path convention used by
//! [`crate::text`]. The raster backend applies its own Y-flip, so callers do
//! not flip here. `axes_bbox` is the axes rectangle in that space.
//!
//! Ticks point **out of** the axes: for a [`AxisSide::Bottom`] axis the spine
//! lies at `y = ymin` and ticks extend downward (decreasing y); for
//! [`AxisSide::Left`] the spine lies at `x = xmin` and ticks extend leftward
//! (decreasing x). [`AxisSide::Top`] and [`AxisSide::Right`] mirror these.

use crate::core::{Affine2D, Bbox, Path, color::Rgba};
use crate::mathtext::{RichText, layout_rich_text};
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

use crate::axis::scale::{LinearScale, Scale};
use crate::axis::ticker::{AutoLocator, Formatter, Locator, ScalarFormatter};

/// Which edge of the axes rectangle an [`Axis`] is drawn along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSide {
    /// The bottom edge: spine at `y = ymin`, ticks point downward.
    Bottom,
    /// The top edge: spine at `y = ymax`, ticks point upward.
    Top,
    /// The left edge: spine at `x = xmin`, ticks point leftward.
    Left,
    /// The right edge: spine at `x = xmax`, ticks point rightward.
    Right,
}

impl AxisSide {
    /// Whether this axis runs horizontally (`Bottom`/`Top`).
    fn is_horizontal(self) -> bool {
        matches!(self, AxisSide::Bottom | AxisSide::Top)
    }
}

/// A renderable axis along one edge of an axes rectangle.
///
/// Construct with [`Axis::new`] and customise via the builder setters. Draw with
/// [`Axis::draw`]. See the [module docs](crate::axis::axis) for the coordinate
/// convention.
pub struct Axis {
    /// Which edge the axis is drawn along.
    side: AxisSide,
    /// The data→scaled coordinate mapping.
    scale: Box<dyn Scale>,
    /// The tick-position generator.
    locator: Box<dyn Locator>,
    /// The tick-label generator.
    formatter: Box<dyn Formatter>,
    /// Optional axis label drawn outside the tick labels.
    label: Option<String>,
    /// Color used for the spine, ticks, and all text.
    color: Rgba,
    /// Length of each tick mark, in pixels.
    tick_length: f64,
    /// Stroke width of the spine and ticks, in pixels.
    tick_width: f64,
    /// Font size of tick labels, in pixels.
    tick_label_size: f64,
    /// Font size of the axis label, in pixels.
    axis_label_size: f64,
    /// Padding between a tick and its label, in pixels.
    tick_label_pad: f64,
    /// Padding between tick labels and the axis label, in pixels.
    axis_label_pad: f64,
    /// Whether to draw grid lines spanning the axes at each tick.
    grid: bool,
}

impl Axis {
    /// Create an axis for `side` with sensible defaults: a [`LinearScale`], an
    /// [`AutoLocator`], a [`ScalarFormatter`], black ink, `tick_length = 3.5`,
    /// `tick_width = 1.0`, 10px tick/axis labels, matplotlib-like label pads,
    /// and no grid.
    #[must_use]
    pub fn new(side: AxisSide) -> Self {
        Axis {
            side,
            scale: Box::new(LinearScale::new()),
            locator: Box::new(AutoLocator::new()),
            formatter: Box::new(ScalarFormatter::new()),
            label: None,
            color: Rgba::BLACK,
            tick_length: 3.5,
            tick_width: 1.0,
            tick_label_size: 10.0,
            axis_label_size: 10.0,
            tick_label_pad: 3.5,
            axis_label_pad: 4.0,
            grid: false,
        }
    }

    /// Which edge this axis is drawn along.
    #[must_use]
    pub fn side(&self) -> AxisSide {
        self.side
    }

    /// Set the scale, returning `self` for chaining.
    #[must_use]
    pub fn with_scale(mut self, scale: Box<dyn Scale>) -> Self {
        self.scale = scale;
        self
    }

    /// Replace the scale in-place.
    pub fn set_scale(&mut self, scale: Box<dyn Scale>) -> &mut Self {
        self.scale = scale;
        self
    }

    /// Set the locator, returning `self` for chaining.
    #[must_use]
    pub fn with_locator(mut self, locator: Box<dyn Locator>) -> Self {
        self.locator = locator;
        self
    }

    /// Replace the tick locator in-place.
    pub fn set_locator(&mut self, locator: Box<dyn Locator>) -> &mut Self {
        self.locator = locator;
        self
    }

    /// Set the formatter, returning `self` for chaining.
    #[must_use]
    pub fn with_formatter(mut self, formatter: Box<dyn Formatter>) -> Self {
        self.formatter = formatter;
        self
    }

    /// Replace the tick formatter in-place.
    pub fn set_formatter(&mut self, formatter: Box<dyn Formatter>) -> &mut Self {
        self.formatter = formatter;
        self
    }

    /// Set the axis label, returning `self` for chaining.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the ink color, returning `self` for chaining.
    #[must_use]
    pub fn with_color(mut self, color: Rgba) -> Self {
        self.color = color;
        self
    }

    /// Set the tick length in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_tick_length(mut self, tick_length: f64) -> Self {
        self.tick_length = tick_length;
        self
    }

    /// Set the spine/tick stroke width in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_tick_width(mut self, tick_width: f64) -> Self {
        self.tick_width = tick_width;
        self
    }

    /// Set tick-label and axis-label font sizes in pixels, returning `self`
    /// for chaining.
    #[must_use]
    pub fn with_label_size(mut self, label_size: f64) -> Self {
        self.tick_label_size = label_size;
        self.axis_label_size = label_size;
        self
    }

    /// Set the tick-label font size in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_tick_label_size(mut self, tick_label_size: f64) -> Self {
        self.tick_label_size = tick_label_size;
        self
    }

    /// Set the axis-label font size in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_axis_label_size(mut self, axis_label_size: f64) -> Self {
        self.axis_label_size = axis_label_size;
        self
    }

    /// Set tick-to-label and tick-label-to-axis-label padding in pixels,
    /// returning `self` for chaining.
    #[must_use]
    pub fn with_label_pad(mut self, label_pad: f64) -> Self {
        self.tick_label_pad = label_pad;
        self.axis_label_pad = label_pad;
        self
    }

    /// Set the tick-to-label padding in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_tick_label_pad(mut self, tick_label_pad: f64) -> Self {
        self.tick_label_pad = tick_label_pad;
        self
    }

    /// Set the tick-label-to-axis-label padding in pixels, returning `self`
    /// for chaining.
    #[must_use]
    pub fn with_axis_label_pad(mut self, axis_label_pad: f64) -> Self {
        self.axis_label_pad = axis_label_pad;
        self
    }

    /// Enable or disable grid lines, returning `self` for chaining.
    #[must_use]
    pub fn with_grid(mut self, grid: bool) -> Self {
        self.grid = grid;
        self
    }

    /// Map a data value to its pixel position along the axis dimension.
    ///
    /// Uses the scale's forward transform, then linearly interpolates the
    /// transformed fraction across the relevant axes-rectangle extent. For
    /// `Bottom`/`Top` the result is an x coordinate; for `Left`/`Right` it is a
    /// y coordinate. A degenerate (zero-width) scaled range collapses to the
    /// low edge.
    fn data_to_pixel(&self, v: f64, axes_bbox: &Bbox, vmin: f64, vmax: f64) -> f64 {
        let tmin = self.scale.transform(vmin);
        let tmax = self.scale.transform(vmax);
        let denom = tmax - tmin;
        let frac = if denom == 0.0 {
            0.0
        } else {
            (self.scale.transform(v) - tmin) / denom
        };
        let (lo, hi) = if self.side.is_horizontal() {
            (axes_bbox.xmin(), axes_bbox.xmax())
        } else {
            (axes_bbox.ymin(), axes_bbox.ymax())
        };
        lo + frac * (hi - lo)
    }

    /// Draw the axis into `renderer`.
    ///
    /// `axes_bbox` is the axes rectangle in y-UP pixel space, `data_lim` is the
    /// `(vmin, vmax)` data range mapped across the axis, and `font` supplies the
    /// glyph outlines for tick and axis labels.
    ///
    /// Drawing order is grid (if enabled) first, then spine, ticks, tick
    /// labels, and finally the axis label, so later strokes sit atop the grid.
    pub fn draw(
        &self,
        renderer: &mut dyn Renderer,
        axes_bbox: &Bbox,
        data_lim: (f64, f64),
        font: &FontSource,
    ) {
        let (vmin, vmax) = data_lim;
        let (lo_v, hi_v) = if vmin <= vmax {
            (vmin, vmax)
        } else {
            (vmax, vmin)
        };

        let ticks: Vec<f64> = self
            .locator
            .tick_values(vmin, vmax)
            .into_iter()
            .filter(|&t| t >= lo_v && t <= hi_v)
            .collect();

        let stroke_gc = GraphicsContext::new()
            .with_stroke(self.color)
            .with_line_width(self.tick_width.max(1.0));

        if self.grid {
            self.draw_grid(renderer, axes_bbox, &ticks, vmin, vmax);
        }

        // Decoration geometry (ticks, label sizes, pads) is authored in px at
        // the default 100 DPI and scales with the renderer's DPI.
        let s = renderer.decoration_scale();
        self.draw_spine(renderer, axes_bbox, &stroke_gc);
        self.draw_ticks(renderer, axes_bbox, &ticks, (vmin, vmax), &stroke_gc, s);
        self.draw_tick_labels(renderer, axes_bbox, &ticks, (vmin, vmax), font, s);
        self.draw_axis_label(renderer, axes_bbox, &ticks, font, s);
    }

    /// The outward extent, in pixels, this axis' decoration (ticks, tick
    /// labels, and the axis label) occupies beyond the frame edge, for limits
    /// `lim` at decoration scale `s`. Drives tight layout: the frame is inset
    /// from its envelope by exactly this much per side (plus a pad).
    pub(crate) fn decoration_extent(&self, lim: (f64, f64), font: &FontSource, s: f64) -> f64 {
        let (vmin, vmax) = lim;
        let (lo_v, hi_v) = if vmin <= vmax {
            (vmin, vmax)
        } else {
            (vmax, vmin)
        };
        let ticks: Vec<f64> = self
            .locator
            .tick_values(vmin, vmax)
            .into_iter()
            .filter(|&t| t >= lo_v && t <= hi_v)
            .collect();
        let mut extent = (self.tick_length + self.tick_label_pad) * s
            + self.tick_label_band_extent(&ticks, font, s);
        if let Some(label) = &self.label
            && !label.is_empty()
        {
            let rich = layout_rich_text(font, label, self.axis_label_size * s);
            // The axis label sits past the tick labels by axis_label_pad; its
            // occupied thickness is its line height (width when the label is
            // rotated along a vertical axis, but height bounds both since the
            // rotated label's *thickness* is still one line).
            extent += self.axis_label_pad * s + rich.ascent + rich.descent;
        }
        extent
    }

    /// The half-size overhang of the outermost tick labels past the two ends
    /// of the axis span, `(low_end, high_end)`, in pixels at decoration scale
    /// `s`.
    ///
    /// Tick labels are centered on their ticks, so a tick sitting at an axis
    /// limit spills half its label beyond the frame corner — half its width
    /// along a horizontal axis, half its height along a vertical one.
    /// matplotlib's tight layout reserves this room by measuring the axes
    /// tight bbox; this is the measured equivalent. Only ticks within 2% of
    /// an end can overhang it.
    pub(crate) fn end_label_overhangs(
        &self,
        lim: (f64, f64),
        font: &FontSource,
        s: f64,
    ) -> (f64, f64) {
        let (vmin, vmax) = lim;
        let (lo_v, hi_v) = if vmin <= vmax {
            (vmin, vmax)
        } else {
            (vmax, vmin)
        };
        let span = hi_v - lo_v;
        if !span.is_finite() || span <= 0.0 {
            return (0.0, 0.0);
        }
        let ticks: Vec<f64> = self
            .locator
            .tick_values(vmin, vmax)
            .into_iter()
            .filter(|&t| t >= lo_v && t <= hi_v)
            .collect();
        let labels = self.formatter.format_ticks(&ticks);
        let (mut low, mut high) = (0.0f64, 0.0f64);
        for (&t, label) in ticks.iter().zip(&labels) {
            if label.is_empty() {
                continue;
            }
            let frac = (t - lo_v) / span;
            if frac > 0.02 && frac < 0.98 {
                continue;
            }
            let rich = layout_rich_text(font, label, self.tick_label_size * s);
            let half = if self.side.is_horizontal() {
                rich.width / 2.0
            } else {
                (rich.ascent + rich.descent) / 2.0
            };
            if frac <= 0.02 {
                low = low.max(half);
            } else {
                high = high.max(half);
            }
        }
        (low, high)
    }

    /// Stroke the spine along the relevant edge.
    fn draw_spine(&self, renderer: &mut dyn Renderer, axes_bbox: &Bbox, gc: &GraphicsContext) {
        let (xmin, xmax) = (axes_bbox.xmin(), axes_bbox.xmax());
        let (ymin, ymax) = (axes_bbox.ymin(), axes_bbox.ymax());
        let line = match self.side {
            AxisSide::Bottom => [[xmin, ymin], [xmax, ymin]],
            AxisSide::Top => [[xmin, ymax], [xmax, ymax]],
            AxisSide::Left => [[xmin, ymin], [xmin, ymax]],
            AxisSide::Right => [[xmax, ymin], [xmax, ymax]],
        };
        let path = Path::from_polyline(&line);
        renderer.draw_path(gc, &path, &Affine2D::identity(), None);
    }

    /// Stroke light-gray grid lines spanning the axes, one per tick.
    fn draw_grid(
        &self,
        renderer: &mut dyn Renderer,
        axes_bbox: &Bbox,
        ticks: &[f64],
        vmin: f64,
        vmax: f64,
    ) {
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::rgb(0.85, 0.85, 0.85))
            .with_line_width(self.tick_width.max(1.0));
        let (xmin, xmax) = (axes_bbox.xmin(), axes_bbox.xmax());
        let (ymin, ymax) = (axes_bbox.ymin(), axes_bbox.ymax());
        for &t in ticks {
            let p = self.data_to_pixel(t, axes_bbox, vmin, vmax);
            let line = if self.side.is_horizontal() {
                [[p, ymin], [p, ymax]]
            } else {
                [[xmin, p], [xmax, p]]
            };
            let path = Path::from_polyline(&line);
            renderer.draw_path(&gc, &path, &Affine2D::identity(), None);
        }
    }

    /// Stroke a tick mark of length `tick_length` pointing out of the axes at
    /// each tick position.
    fn draw_ticks(
        &self,
        renderer: &mut dyn Renderer,
        axes_bbox: &Bbox,
        ticks: &[f64],
        lim: (f64, f64),
        gc: &GraphicsContext,
        s: f64,
    ) {
        let (vmin, vmax) = lim;
        let len = self.tick_length * s;
        for &t in ticks {
            let p = self.data_to_pixel(t, axes_bbox, vmin, vmax);
            let line = match self.side {
                // Out of axes = downward (decreasing y).
                AxisSide::Bottom => [[p, axes_bbox.ymin()], [p, axes_bbox.ymin() - len]],
                // Out of axes = upward (increasing y).
                AxisSide::Top => [[p, axes_bbox.ymax()], [p, axes_bbox.ymax() + len]],
                // Out of axes = leftward (decreasing x).
                AxisSide::Left => [[axes_bbox.xmin(), p], [axes_bbox.xmin() - len, p]],
                // Out of axes = rightward (increasing x).
                AxisSide::Right => [[axes_bbox.xmax(), p], [axes_bbox.xmax() + len, p]],
            };
            let path = Path::from_polyline(&line);
            renderer.draw_path(gc, &path, &Affine2D::identity(), None);
        }
    }

    /// Fill each tick label just outside its tick, centered on the tick.
    fn draw_tick_labels(
        &self,
        renderer: &mut dyn Renderer,
        axes_bbox: &Bbox,
        ticks: &[f64],
        lim: (f64, f64),
        font: &FontSource,
        s: f64,
    ) {
        let (vmin, vmax) = lim;
        let gc = GraphicsContext::new();
        let labels = self.formatter.format_ticks(ticks);
        for (&t, text) in ticks.iter().zip(labels.iter()) {
            if text.is_empty() {
                continue;
            }
            let rich = layout_rich_text(font, text, self.tick_label_size * s);
            let p = self.data_to_pixel(t, axes_bbox, vmin, vmax);
            let origin = self.tick_label_origin(axes_bbox, p, &rich, s);
            let shift = Affine2D::from_translation(origin[0], origin[1]);
            for path in &rich.paths {
                renderer.draw_path(
                    &gc,
                    &path.transformed(&shift),
                    &Affine2D::identity(),
                    Some(self.color),
                );
            }
        }
    }

    /// Compute the baseline origin `(x, y)` for a tick label centered on the
    /// tick position `p` and offset out of the axes by `tick_label_pad`.
    fn tick_label_origin(&self, axes_bbox: &Bbox, p: f64, rich: &RichText, s: f64) -> [f64; 2] {
        let out = (self.tick_length + self.tick_label_pad) * s;
        match self.side {
            AxisSide::Bottom => {
                let x = p - rich.width / 2.0;
                let y = axes_bbox.ymin() - out - rich.ascent;
                [x, y]
            }
            AxisSide::Top => {
                let x = p - rich.width / 2.0;
                let y = axes_bbox.ymax() + out + rich.descent;
                [x, y]
            }
            AxisSide::Left => {
                // Left of the tick: right-align the text to the offset point.
                let x = axes_bbox.xmin() - out - rich.width;
                let y = p - (rich.ascent - rich.descent) / 2.0;
                [x, y]
            }
            AxisSide::Right => {
                let x = axes_bbox.xmax() + out;
                let y = p - (rich.ascent - rich.descent) / 2.0;
                [x, y]
            }
        }
    }

    fn tick_label_band_extent(&self, ticks: &[f64], font: &FontSource, s: f64) -> f64 {
        self.formatter
            .format_ticks(ticks)
            .iter()
            .filter(|label| !label.is_empty())
            .map(|label| {
                let rich = layout_rich_text(font, label, self.tick_label_size * s);
                if self.side.is_horizontal() {
                    rich.ascent + rich.descent
                } else {
                    rich.width
                }
            })
            .fold(0.0, f64::max)
    }

    /// Fill the axis label (if any), centered along the axis and offset further
    /// out than the tick labels.
    fn draw_axis_label(
        &self,
        renderer: &mut dyn Renderer,
        axes_bbox: &Bbox,
        ticks: &[f64],
        font: &FontSource,
        s: f64,
    ) {
        let Some(label) = &self.label else {
            return;
        };
        if label.is_empty() {
            return;
        }
        let gc = GraphicsContext::new();
        let rich = layout_rich_text(font, label, self.axis_label_size * s);
        let tick_label_extent = self.tick_label_band_extent(ticks, font, s);
        let label_offset =
            (self.tick_length + self.tick_label_pad + self.axis_label_pad) * s + tick_label_extent;
        let transform = match self.side {
            AxisSide::Bottom => {
                let x = (axes_bbox.xmin() + axes_bbox.xmax()) / 2.0 - rich.width / 2.0;
                let y = axes_bbox.ymin() - label_offset - rich.ascent;
                Affine2D::from_translation(x, y)
            }
            AxisSide::Top => {
                let x = (axes_bbox.xmin() + axes_bbox.xmax()) / 2.0 - rich.width / 2.0;
                let y = axes_bbox.ymax() + label_offset + rich.descent;
                Affine2D::from_translation(x, y)
            }
            AxisSide::Left => {
                let right = axes_bbox.xmin() - label_offset;
                let x = right - rich.descent;
                let y = (axes_bbox.ymin() + axes_bbox.ymax()) / 2.0 - rich.width / 2.0;
                Affine2D::from_rotation_deg(90.0).then(&Affine2D::from_translation(x, y))
            }
            AxisSide::Right => {
                let left = axes_bbox.xmax() + label_offset;
                let x = left + rich.descent;
                let y = (axes_bbox.ymin() + axes_bbox.ymax()) / 2.0 + rich.width / 2.0;
                Affine2D::from_rotation_deg(-90.0).then(&Affine2D::from_translation(x, y))
            }
        };
        for path in &rich.paths {
            renderer.draw_path(
                &gc,
                &path.transformed(&transform),
                &Affine2D::identity(),
                Some(self.color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::ticker::{FixedFormatter, FixedLocator, NullLocator};

    /// A [`Renderer`] that counts `draw_path` calls and records the bbox of each
    /// path's vertices (after applying the transform), for assertions.
    #[derive(Default)]
    struct CountingRenderer {
        paths: usize,
        verts: Vec<[f64; 2]>,
        path_bboxes: Vec<(f64, f64, f64, f64)>,
    }

    impl Renderer for CountingRenderer {
        fn draw_path(
            &mut self,
            _gc: &GraphicsContext,
            path: &Path,
            transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            self.paths += 1;
            let mut xmin = f64::INFINITY;
            let mut xmax = f64::NEG_INFINITY;
            let mut ymin = f64::INFINITY;
            let mut ymax = f64::NEG_INFINITY;
            for &[x, y] in path.vertices() {
                let (tx, ty) = transform.transform_point((x, y));
                self.verts.push([tx, ty]);
                xmin = xmin.min(tx);
                xmax = xmax.max(tx);
                ymin = ymin.min(ty);
                ymax = ymax.max(ty);
            }
            if xmin.is_finite() {
                self.path_bboxes.push((xmin, xmax, ymin, ymax));
            }
        }

        fn canvas_size(&self) -> (f64, f64) {
            (300.0, 300.0)
        }
    }

    #[test]
    fn bottom_tick_positions_are_monotonic_and_span_bbox() {
        let axis = Axis::new(AxisSide::Bottom);
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let xs: Vec<f64> = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]
            .iter()
            .map(|&v| axis.data_to_pixel(v, &bbox, 0.0, 10.0))
            .collect();
        // Monotonic increasing.
        for w in xs.windows(2) {
            assert!(w[1] > w[0], "not monotonic: {xs:?}");
        }
        // Spans ~[50, 250].
        assert!((xs[0] - 50.0).abs() < 1e-9, "min was {}", xs[0]);
        assert!((xs[xs.len() - 1] - 250.0).abs() < 1e-9, "max was {}", xs[5]);
    }

    #[test]
    fn draw_emits_spine_ticks_and_labels() {
        let axis = Axis::new(AxisSide::Bottom);
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (0.0, 10.0), &font);

        // AutoLocator on [0,10] yields ticks {0,2,...,10} (6 within range).
        let ticks: Vec<f64> = AutoLocator::new()
            .tick_values(0.0, 10.0)
            .into_iter()
            .filter(|&t| (0.0..=10.0).contains(&t))
            .collect();
        let n = ticks.len();
        assert!(n >= 5, "unexpected tick count {n}");
        // spine(1) + one per tick + one per label.
        assert!(
            r.paths >= 1 + n + n,
            "expected >= {} draw_path calls, got {}",
            1 + 2 * n,
            r.paths
        );
    }

    #[test]
    fn left_axis_ticks_point_left_of_spine() {
        let axis = Axis::new(AxisSide::Left);
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (0.0, 10.0), &font);
        // Some tick/label vertices must lie left of the spine (x < 50).
        assert!(
            r.verts.iter().any(|&[x, _]| x < 50.0),
            "expected ink left of the left spine"
        );
    }

    #[test]
    fn tick_labels_render_mathtext_formatter_output() {
        let axis = Axis::new(AxisSide::Bottom)
            .with_locator(Box::new(FixedLocator::new(vec![1.0])))
            .with_formatter(Box::new(FixedFormatter::new(vec!["$10^{6}$".to_string()])));
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (1.0, 1.0), &font);

        // spine + tick + multiple mathtext glyph paths. The old plain-text
        // route emitted only one label path containing literal '$' and braces.
        assert!(
            r.paths > 3,
            "expected mathtext label to emit multiple paths, got {}",
            r.paths
        );
    }

    #[test]
    fn axis_label_renders_mathtext() {
        let axis = Axis::new(AxisSide::Left)
            .with_locator(Box::new(NullLocator))
            .with_label("$x^2$");
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (0.0, 1.0), &font);

        // spine + multiple axis-label mathtext paths. The old plain-text route
        // emitted the whole "$x^2$" label as one literal path.
        assert!(
            r.paths > 2,
            "expected mathtext axis label to add multiple paths, got {}",
            r.paths
        );
    }

    #[test]
    fn left_axis_label_is_rotated_vertically() {
        let axis = Axis::new(AxisSide::Left)
            .with_locator(Box::new(NullLocator))
            .with_label("Voltage");
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (0.0, 1.0), &font);

        let &(xmin, xmax, ymin, ymax) = r.path_bboxes.last().expect("axis label path");
        let width = xmax - xmin;
        let height = ymax - ymin;
        assert!(
            height > width * 2.0,
            "expected rotated vertical label, bbox was width={width}, height={height}"
        );
        let center_y = (ymin + ymax) / 2.0;
        assert!(
            (center_y - 150.0).abs() < 20.0,
            "expected ylabel centered on axes, bbox center_y={center_y}"
        );
    }

    #[test]
    fn axis_label_offset_includes_wide_tick_label_band() {
        let axis = Axis::new(AxisSide::Left)
            .with_locator(Box::new(FixedLocator::new(vec![1.0])))
            .with_formatter(Box::new(FixedFormatter::new(vec!["1000000".to_string()])))
            .with_label("value");
        let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
        let font = FontSource::dejavu_sans();
        let mut r = CountingRenderer::default();
        axis.draw(&mut r, &bbox, (1.0, 1.0), &font);

        let tick_label = r
            .path_bboxes
            .iter()
            .rev()
            .nth(1)
            .expect("tick label path before axis label");
        let axis_label = r.path_bboxes.last().expect("axis label path");
        assert!(
            axis_label.1 < tick_label.0,
            "axis label should sit left of the measured tick-label band: axis={axis_label:?}, tick={tick_label:?}"
        );
    }
}
