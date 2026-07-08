//! The renderer seam for rizzma.
//!
//! Defines the [`Renderer`] trait (`draw_path`/`draw_markers`/`draw_image`/
//! `draw_text`) plus the [`GraphicsContext`] state and paint enums every backend
//! consumes. This is the one abstraction that must survive the port: high-level
//! artists describe what to draw in terms of [`Path`]s, [`Affine2D`] transforms,
//! and [`Rgba`] colors, and each backend implements [`Renderer`] to realize them.
//!
//! It is a distillation of matplotlib's `RendererBase`/`GraphicsContextBase`.
//!
//! Build-order home: Phase 3 of `design/04-implementation-plan.md`.

pub use crate::core::{Affine2D, Bbox, Path, color::Rgba};

/// How the ends of an open stroked subpath are drawn.
///
/// Mirrors matplotlib's `_capstyle` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CapStyle {
    /// Stroke ends exactly at the endpoint with a flat edge.
    #[default]
    Butt,
    /// Stroke ends with a semicircle centered on the endpoint.
    Round,
    /// Stroke extends half the line width beyond the endpoint with a flat edge.
    Projecting,
}

/// How two connected stroked segments are joined at a corner.
///
/// Mirrors matplotlib's `_joinstyle` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JoinStyle {
    /// Outer edges are extended to meet at a sharp point.
    #[default]
    Miter,
    /// The corner is filled with a circular arc.
    Round,
    /// The corner is filled with a straight bevel edge.
    Bevel,
}

/// Per-draw graphics state shared by every [`Renderer`] call.
///
/// This is matplotlib's `GraphicsContextBase` distilled to the fields the seam
/// needs. Stroke color and width, dashing, line caps/joins, clipping, and a
/// global alpha all live here; the face (fill) color is passed separately to the
/// individual draw methods, matching matplotlib's `rgbFace` argument.
#[derive(Debug, Clone)]
pub struct GraphicsContext {
    /// Stroke width in points (pre-DPI scaling).
    pub line_width: f64,
    /// Optional dash pattern as `(offset, on_off_lengths)` in points.
    pub dashes: Option<(f64, Vec<f64>)>,
    /// Line cap style for open subpath ends.
    pub cap: CapStyle,
    /// Line join style for connected segments.
    pub join: JoinStyle,
    /// Optional global alpha applied on top of per-color alpha.
    pub alpha: Option<f64>,
    /// Whether drawing should be antialiased.
    pub antialiased: bool,
    /// Optional axis-aligned clip rectangle in device coordinates.
    pub clip_rect: Option<Bbox>,
    /// Optional clip path in device coordinates.
    pub clip_path: Option<Path>,
    /// Stroke (edge) color. `None` means no stroke is drawn.
    pub stroke: Option<Rgba>,
}

impl GraphicsContext {
    /// Construct a [`GraphicsContext`] with the default state: a `1.0`-point
    /// antialiased stroke, no dashes, butt caps, miter joins, no clipping, and no
    /// stroke color set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            line_width: 1.0,
            dashes: None,
            cap: CapStyle::default(),
            join: JoinStyle::default(),
            alpha: None,
            antialiased: true,
            clip_rect: None,
            clip_path: None,
            stroke: None,
        }
    }

    /// Set the stroke width in points, returning `self` for chaining.
    #[must_use]
    pub fn with_line_width(mut self, line_width: f64) -> Self {
        self.line_width = line_width;
        self
    }

    /// Set the stroke color, returning `self` for chaining.
    #[must_use]
    pub fn with_stroke(mut self, stroke: Rgba) -> Self {
        self.stroke = Some(stroke);
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

    /// Set the global alpha, returning `self` for chaining.
    #[must_use]
    pub fn with_alpha(mut self, alpha: f64) -> Self {
        self.alpha = Some(alpha);
        self
    }
}

/// Note: a hand-written [`Default`] is provided rather than `#[derive]` so the
/// documented defaults (`line_width = 1.0`, `antialiased = true`) hold.
impl Default for GraphicsContext {
    fn default() -> Self {
        Self::new()
    }
}

/// The backend-agnostic drawing contract.
///
/// A [`Renderer`] receives geometry already described in figure coordinates plus
/// a transform into device space, and is responsible for rasterizing or
/// serializing it. The trait is deliberately small: only [`Renderer::draw_path`]
/// and [`Renderer::canvas_size`] are required, and everything else has a default
/// implemented in terms of them. This mirrors matplotlib's `RendererBase`, where
/// `draw_markers`/`draw_path_collection` fall back to repeated `draw_path` calls.
pub trait Renderer {
    /// Draw a single [`Path`] transformed by `transform` into device space.
    ///
    /// `fill` is the face color (matplotlib's `rgbFace`); `None` means the path is
    /// not filled. The stroke color and width come from `gc`.
    fn draw_path(
        &mut self,
        gc: &GraphicsContext,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    );

    /// Draw `marker_path` (transformed by `marker_transform`) once at each vertex
    /// of `path` (transformed by `transform`).
    ///
    /// The default implementation mirrors matplotlib's fallback: it transforms
    /// each of `path`'s vertices to device space and, for each, draws the marker
    /// translated to that point via [`Renderer::draw_path`].
    fn draw_markers(
        &mut self,
        gc: &GraphicsContext,
        marker_path: &Path,
        marker_transform: &Affine2D,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        for &[vx, vy] in path.vertices() {
            let (dx, dy) = transform.transform_point((vx, vy));
            let placed = marker_transform.then(&Affine2D::from_translation(dx, dy));
            self.draw_path(gc, marker_path, &placed, fill);
        }
    }

    /// Blit an RGBA image buffer with its lower-left corner at `(x, y)`.
    ///
    /// `rgba` holds `width * height` straight RGBA pixels in row-major order
    /// (4 bytes each). The default implementation is a no-op; raster backends
    /// override it.
    fn draw_image(
        &mut self,
        gc: &GraphicsContext,
        x: f64,
        y: f64,
        rgba: &[u8],
        width: usize,
        height: usize,
    ) {
        let _ = (gc, x, y, rgba, width, height);
    }

    /// Draw `text` anchored at `(x, y)`, rotated `angle_deg` counter-clockwise.
    ///
    /// `font_size_px` is the font size in device pixels. The default
    /// implementation is a no-op until text shaping lands; backends override it.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        gc: &GraphicsContext,
        x: f64,
        y: f64,
        text: &str,
        font_size_px: f64,
        angle_deg: f64,
        color: Rgba,
    ) {
        let _ = (gc, x, y, text, font_size_px, angle_deg, color);
    }

    /// Whether the y-axis is flipped (device origin at top-left).
    ///
    /// Matches matplotlib's `RendererBase.flipy`; raster backends typically
    /// return `true`, vector backends with a bottom-left origin return `false`.
    fn flipy(&self) -> bool {
        true
    }

    /// Convert a length in points to device pixels.
    ///
    /// The default identity mapping assumes 72 DPI; backends scale by their DPI.
    fn points_to_pixels(&self, points: f64) -> f64 {
        points
    }

    /// Scale factor for fixed-size decoration geometry — font sizes, pads,
    /// tick lengths, marker radii, annotation arrows — whose constants are
    /// authored in pixels **at the default 100 DPI**.
    ///
    /// Backends with a DPI return `dpi / 100`, so decorations grow
    /// proportionally with the canvas on high-DPI renders instead of shrinking
    /// relative to it (stroke widths already scale via
    /// [`points_to_pixels`](Renderer::points_to_pixels)). The default is `1.0`
    /// for DPI-less renderers.
    fn decoration_scale(&self) -> f64 {
        1.0
    }

    /// The canvas size in device pixels as `(width, height)`.
    fn canvas_size(&self) -> (f64, f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that records each `draw_path` call's resolved device-space
    /// translation, used to verify the default `draw_markers` fan-out.
    #[derive(Default)]
    struct RecordingRenderer {
        calls: Vec<(f64, f64)>,
    }

    impl Renderer for RecordingRenderer {
        fn draw_path(
            &mut self,
            _gc: &GraphicsContext,
            _path: &Path,
            transform: &Affine2D,
            _fill: Option<Rgba>,
        ) {
            let [.., e, f] = transform.matrix();
            self.calls.push((e, f));
        }

        fn canvas_size(&self) -> (f64, f64) {
            (640.0, 480.0)
        }
    }

    #[test]
    fn default_context_matches_documented_defaults() {
        let gc = GraphicsContext::default();
        assert_eq!(gc.line_width, 1.0);
        assert!(gc.antialiased);
        assert_eq!(gc.cap, CapStyle::Butt);
        assert_eq!(gc.join, JoinStyle::Miter);
        assert!(gc.dashes.is_none());
        assert!(gc.alpha.is_none());
        assert!(gc.clip_rect.is_none());
        assert!(gc.clip_path.is_none());
        assert!(gc.stroke.is_none());
    }

    #[test]
    fn builder_setters() {
        let red = Rgba::RED;
        let gc = GraphicsContext::new()
            .with_line_width(2.5)
            .with_stroke(red)
            .with_cap(CapStyle::Round)
            .with_join(JoinStyle::Bevel)
            .with_alpha(0.5);
        assert_eq!(gc.line_width, 2.5);
        assert_eq!(gc.stroke, Some(red));
        assert_eq!(gc.cap, CapStyle::Round);
        assert_eq!(gc.join, JoinStyle::Bevel);
        assert_eq!(gc.alpha, Some(0.5));
    }

    #[test]
    fn draw_markers_fans_out_one_draw_path_per_point() {
        let mut renderer = RecordingRenderer::default();
        let gc = GraphicsContext::default();
        let marker = Path::unit_rectangle();
        let points = Path::from_polyline(&[[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]);

        renderer.draw_markers(
            &gc,
            &marker,
            &Affine2D::identity(),
            &points,
            &Affine2D::identity(),
            Some(Rgba::BLUE),
        );

        // One draw_path per marker position, each translated to the point.
        assert_eq!(renderer.calls, vec![(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
    }

    #[test]
    fn draw_markers_honors_marker_transform_then_translation() {
        let mut renderer = RecordingRenderer::default();
        let gc = GraphicsContext::default();
        let marker = Path::unit_rectangle();
        let points = Path::from_polyline(&[[10.0, 20.0]]);

        renderer.draw_markers(
            &gc,
            &marker,
            &Affine2D::from_translation(100.0, 200.0),
            &points,
            &Affine2D::from_scale(2.0, 2.0),
            None,
        );

        // Point scaled to (20, 40), then marker's own (100, 200) offset added.
        assert_eq!(renderer.calls, vec![(120.0, 240.0)]);
    }

    #[test]
    fn default_capabilities() {
        let renderer = RecordingRenderer::default();
        assert!(renderer.flipy());
        assert_eq!(renderer.points_to_pixels(12.0), 12.0);
        assert_eq!(renderer.canvas_size(), (640.0, 480.0));
    }
}
