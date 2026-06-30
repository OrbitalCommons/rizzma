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
//! [`rizzma_text`]. The raster backend applies its own Y-flip, so callers do
//! not flip here. `axes_bbox` is the axes rectangle in that space.
//!
//! Ticks point **out of** the axes: for a [`AxisSide::Bottom`] axis the spine
//! lies at `y = ymin` and ticks extend downward (decreasing y); for
//! [`AxisSide::Left`] the spine lies at `x = xmin` and ticks extend leftward
//! (decreasing x). [`AxisSide::Top`] and [`AxisSide::Right`] mirror these.

use rizzma_core::{Affine2D, Bbox, Path, color::Rgba};
use rizzma_render::{GraphicsContext, Renderer};
use rizzma_text::FontSource;

use crate::scale::{LinearScale, Scale};
use crate::ticker::{AutoLocator, Formatter, Locator, ScalarFormatter};

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
/// [`Axis::draw`]. See the [module docs](crate::axis) for the coordinate
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
    /// Font size of tick labels and the axis label, in pixels.
    label_size: f64,
    /// Padding between a tick and its label, in pixels.
    label_pad: f64,
    /// Whether to draw grid lines spanning the axes at each tick.
    grid: bool,
}

impl Axis {
    /// Create an axis for `side` with sensible defaults: a [`LinearScale`], an
    /// [`AutoLocator`], a [`ScalarFormatter`], black ink, `tick_length = 3.5`,
    /// `tick_width = 1.0`, `label_size = 10.0`, `label_pad = 2.0`, and no grid.
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
            label_size: 10.0,
            label_pad: 2.0,
            grid: false,
        }
    }

    /// Set the scale, returning `self` for chaining.
    #[must_use]
    pub fn with_scale(mut self, scale: Box<dyn Scale>) -> Self {
        self.scale = scale;
        self
    }

    /// Set the locator, returning `self` for chaining.
    #[must_use]
    pub fn with_locator(mut self, locator: Box<dyn Locator>) -> Self {
        self.locator = locator;
        self
    }

    /// Set the formatter, returning `self` for chaining.
    #[must_use]
    pub fn with_formatter(mut self, formatter: Box<dyn Formatter>) -> Self {
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

    /// Set the label font size in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_label_size(mut self, label_size: f64) -> Self {
        self.label_size = label_size;
        self
    }

    /// Set the tick-to-label padding in pixels, returning `self` for chaining.
    #[must_use]
    pub fn with_label_pad(mut self, label_pad: f64) -> Self {
        self.label_pad = label_pad;
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

        self.draw_spine(renderer, axes_bbox, &stroke_gc);
        self.draw_ticks(renderer, axes_bbox, &ticks, vmin, vmax, &stroke_gc);
        self.draw_tick_labels(renderer, axes_bbox, &ticks, vmin, vmax, font);
        self.draw_axis_label(renderer, axes_bbox, font);
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
        vmin: f64,
        vmax: f64,
        gc: &GraphicsContext,
    ) {
        let len = self.tick_length;
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
        vmin: f64,
        vmax: f64,
        font: &FontSource,
    ) {
        let gc = GraphicsContext::new();
        for (i, &t) in ticks.iter().enumerate() {
            let text = self.formatter.format(t, Some(i));
            if text.is_empty() {
                continue;
            }
            let extent = font.measure(&text, self.label_size);
            let p = self.data_to_pixel(t, axes_bbox, vmin, vmax);
            let origin = self.label_origin(axes_bbox, p, extent.width, extent.ascent);
            let path = font.text_to_path(&text, self.label_size, origin);
            renderer.draw_path(&gc, &path, &Affine2D::identity(), Some(self.color));
        }
    }

    /// Compute the baseline origin `(x, y)` for a tick label centered on the
    /// tick position `p` and offset out of the axes by `label_pad +
    /// label_size`.
    ///
    /// `text_width` centers the label along the axis; `ascent` is used to
    /// vertically center horizontal-axis labels about their cap region.
    fn label_origin(&self, axes_bbox: &Bbox, p: f64, text_width: f64, ascent: f64) -> [f64; 2] {
        let perp = self.label_pad + self.label_size;
        match self.side {
            AxisSide::Bottom => {
                // Below the tick: baseline drops by the tick length + pad +
                // ascent so the glyph tops clear the spine.
                let x = p - text_width / 2.0;
                let y = axes_bbox.ymin() - self.tick_length - perp;
                [x, y]
            }
            AxisSide::Top => {
                let x = p - text_width / 2.0;
                let y = axes_bbox.ymax() + self.tick_length + self.label_pad;
                [x, y]
            }
            AxisSide::Left => {
                // Left of the tick: right-align the text to the offset point.
                let x = axes_bbox.xmin() - self.tick_length - self.label_pad - text_width;
                let y = p - ascent / 2.0;
                [x, y]
            }
            AxisSide::Right => {
                let x = axes_bbox.xmax() + self.tick_length + self.label_pad;
                let y = p - ascent / 2.0;
                [x, y]
            }
        }
    }

    /// Fill the axis label (if any), centered along the axis and offset further
    /// out than the tick labels.
    fn draw_axis_label(&self, renderer: &mut dyn Renderer, axes_bbox: &Bbox, font: &FontSource) {
        let Some(label) = &self.label else {
            return;
        };
        if label.is_empty() {
            return;
        }
        let gc = GraphicsContext::new();
        let extent = font.measure(label, self.label_size);
        // Offset beyond the tick labels: tick length + two label rows + pads.
        let extra = self.tick_length + 2.0 * (self.label_pad + self.label_size);
        let origin = match self.side {
            AxisSide::Bottom => {
                let x = (axes_bbox.xmin() + axes_bbox.xmax()) / 2.0 - extent.width / 2.0;
                let y = axes_bbox.ymin() - extra;
                [x, y]
            }
            AxisSide::Top => {
                let x = (axes_bbox.xmin() + axes_bbox.xmax()) / 2.0 - extent.width / 2.0;
                let y = axes_bbox.ymax() + extra;
                [x, y]
            }
            AxisSide::Left => {
                let x = axes_bbox.xmin() - extra - extent.width;
                let y = (axes_bbox.ymin() + axes_bbox.ymax()) / 2.0 - extent.ascent / 2.0;
                [x, y]
            }
            AxisSide::Right => {
                let x = axes_bbox.xmax() + extra;
                let y = (axes_bbox.ymin() + axes_bbox.ymax()) / 2.0 - extent.ascent / 2.0;
                [x, y]
            }
        };
        let path = font.text_to_path(label, self.label_size, origin);
        renderer.draw_path(&gc, &path, &Affine2D::identity(), Some(self.color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that counts `draw_path` calls and records the bbox of each
    /// path's vertices (after applying the transform), for assertions.
    #[derive(Default)]
    struct CountingRenderer {
        paths: usize,
        verts: Vec<[f64; 2]>,
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
            for &[x, y] in path.vertices() {
                let (tx, ty) = transform.transform_point((x, y));
                self.verts.push([tx, ty]);
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
}
