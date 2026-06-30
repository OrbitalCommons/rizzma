//! The [`Line2D`] artist: a stroked polyline through `(x, y)` data points.
//!
//! Mirrors matplotlib's `Line2D` for the stroke-only case (markers are deferred
//! to a follow-up). Drawing zips the `x`/`y` data into points, builds a
//! [`Path::from_polyline`], and strokes it through the [`Renderer`].

use rizzma_core::{Affine2D, Bbox, Path, color::Rgba};
use rizzma_render::{CapStyle, GraphicsContext, JoinStyle, Renderer};

use crate::Artist;

/// A 2D line: a stroked polyline through paired `x`/`y` data.
///
/// Constructed with [`Line2D::new`], then customized with the builder-style
/// setters. The defaults mirror matplotlib: opaque black, `1.5`-point width,
/// butt caps, miter joins, visible, and zorder `2.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Line2D {
    /// X coordinates of the data points, in data space.
    xdata: Vec<f64>,
    /// Y coordinates of the data points, in data space.
    ydata: Vec<f64>,
    /// Stroke color.
    color: Rgba,
    /// Stroke width in points.
    linewidth: f64,
    /// Optional dash pattern as `(offset, on_off_lengths)` in points.
    dashes: Option<(f64, Vec<f64>)>,
    /// Line cap style for open ends.
    cap: CapStyle,
    /// Line join style for corners.
    join: JoinStyle,
    /// Whether the line is drawn.
    visible: bool,
    /// Stacking order; higher draws on top.
    zorder: f64,
}

impl Line2D {
    /// Construct a [`Line2D`] from `xdata` and `ydata` with matplotlib-ish
    /// defaults: opaque black, `1.5`-point width, butt caps, miter joins,
    /// visible, and zorder `2.0`.
    ///
    /// The `x` and `y` vectors are paired index-wise at draw time; only the
    /// common prefix is drawn if they differ in length.
    #[must_use]
    pub fn new(xdata: Vec<f64>, ydata: Vec<f64>) -> Self {
        Self {
            xdata,
            ydata,
            color: Rgba::BLACK,
            linewidth: 1.5,
            dashes: None,
            cap: CapStyle::Butt,
            join: JoinStyle::Miter,
            visible: true,
            zorder: 2.0,
        }
    }

    /// Set the stroke color, returning `self` for chaining.
    #[must_use]
    pub fn with_color(mut self, color: Rgba) -> Self {
        self.color = color;
        self
    }

    /// Set the stroke width in points, returning `self` for chaining.
    #[must_use]
    pub fn with_linewidth(mut self, linewidth: f64) -> Self {
        self.linewidth = linewidth;
        self
    }

    /// Set the dash pattern as `(offset, on_off_lengths)` in points, returning
    /// `self` for chaining.
    #[must_use]
    pub fn with_dashes(mut self, dashes: Option<(f64, Vec<f64>)>) -> Self {
        self.dashes = dashes;
        self
    }

    /// Set the line cap style, returning `self` for chaining.
    #[must_use]
    pub fn with_cap(mut self, cap: CapStyle) -> Self {
        self.cap = cap;
        self
    }

    /// Set the line join style, returning `self` for chaining.
    #[must_use]
    pub fn with_join(mut self, join: JoinStyle) -> Self {
        self.join = join;
        self
    }

    /// Set whether the line is drawn, returning `self` for chaining.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the stacking order, returning `self` for chaining.
    #[must_use]
    pub fn with_zorder(mut self, zorder: f64) -> Self {
        self.zorder = zorder;
        self
    }

    /// The `(x, y)` data points zipped into a `Vec<[f64; 2]>`.
    #[must_use]
    pub fn points(&self) -> Vec<[f64; 2]> {
        self.xdata
            .iter()
            .zip(self.ydata.iter())
            .map(|(&x, &y)| [x, y])
            .collect()
    }

    /// Draw this line's stroke style against an already-built data-space path.
    ///
    /// This lets an owning axes pre-transform nonlinear-scale geometry while
    /// reusing the line's color, width, dash, cap, and join settings.
    pub fn draw_path(&self, renderer: &mut dyn Renderer, path: &Path, transform: &Affine2D) {
        if !self.visible || path.vertices().len() < 2 {
            return;
        }
        let gc = GraphicsContext {
            line_width: self.linewidth,
            dashes: self.dashes.clone(),
            cap: self.cap,
            join: self.join,
            stroke: Some(self.color),
            ..GraphicsContext::new()
        };
        renderer.draw_path(&gc, path, transform, None);
    }
}

impl Artist for Line2D {
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        if !self.visible {
            return;
        }
        let points = self.points();
        if points.len() < 2 {
            return;
        }
        let path = Path::from_polyline(&points);
        self.draw_path(renderer, &path, transform);
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        let mut any = false;
        for (&x, &y) in self.xdata.iter().zip(self.ydata.iter()) {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x);
            ymax = ymax.max(y);
            any = true;
        }
        if any {
            Some(Bbox::from_extents(xmin, ymin, xmax, ymax))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that records, per `draw_path` call, the path's vertex
    /// count and the stroke color from the [`GraphicsContext`].
    #[derive(Default)]
    struct MockRenderer {
        calls: Vec<(usize, Option<Rgba>)>,
    }

    impl Renderer for MockRenderer {
        fn draw_path(
            &mut self,
            gc: &GraphicsContext,
            path: &Path,
            _transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            self.calls.push((path.vertices().len(), gc.stroke));
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    #[test]
    fn three_point_line_draws_once_with_color() {
        let line = Line2D::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.0]).with_color(Rgba::RED);
        let mut r = MockRenderer::default();
        line.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls, vec![(3, Some(Rgba::RED))]);
    }

    #[test]
    fn invisible_line_draws_nothing() {
        let line = Line2D::new(vec![0.0, 1.0], vec![0.0, 1.0]).with_visible(false);
        let mut r = MockRenderer::default();
        line.draw(&mut r, &Affine2D::identity());
        assert!(r.calls.is_empty());
    }

    #[test]
    fn single_point_line_draws_nothing() {
        let line = Line2D::new(vec![0.0], vec![0.0]);
        let mut r = MockRenderer::default();
        line.draw(&mut r, &Affine2D::identity());
        assert!(r.calls.is_empty());
    }

    #[test]
    fn data_extents_bounds_finite_points() {
        let line = Line2D::new(vec![1.0, 3.0, 2.0], vec![-1.0, 4.0, 0.0]);
        let e = line.data_extents().expect("non-empty");
        assert_eq!(e.xmin(), 1.0);
        assert_eq!(e.xmax(), 3.0);
        assert_eq!(e.ymin(), -1.0);
        assert_eq!(e.ymax(), 4.0);
    }

    #[test]
    fn data_extents_ignores_nan_point() {
        let line = Line2D::new(vec![1.0, f64::NAN, 2.0], vec![1.0, 5.0, 2.0]);
        let e = line.data_extents().expect("non-empty");
        // The NaN x-point is skipped, so x stays in [1, 2] and y in [1, 2].
        assert_eq!(e.xmin(), 1.0);
        assert_eq!(e.xmax(), 2.0);
        assert_eq!(e.ymin(), 1.0);
        assert_eq!(e.ymax(), 2.0);
    }

    #[test]
    fn data_extents_empty_is_none() {
        let line = Line2D::new(vec![], vec![]);
        assert!(line.data_extents().is_none());
    }

    #[test]
    fn default_zorder_is_two() {
        let line = Line2D::new(vec![0.0, 1.0], vec![0.0, 1.0]);
        assert_eq!(line.zorder(), 2.0);
    }
}
