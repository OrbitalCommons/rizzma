//! tiny-skia raster backend for rizzma.
//!
//! The reference [`Renderer`] implementation: rasterizes paths into an RGBA
//! [`tiny_skia::Pixmap`] and encodes PNG. The yardstick the other backends are
//! diffed against.
//!
//! # Coordinate systems
//!
//! matplotlib's device space has its origin at the bottom-left with y growing
//! upward, while a [`tiny_skia::Pixmap`] is a top-down buffer with the origin at
//! the top-left. Every path drawn here is therefore mapped through a *composite
//! device transform*: first the caller's `transform` into matplotlib device
//! space, then a Y-flip `(x, y) -> (x, height - y)` into pixmap space. The flip
//! is `Affine2D::from_scale(1, -1)` followed by a translation of `height` in y.
//!
//! Build-order home: Phase 3 of `design/04-implementation-plan.md`.

use std::fmt;

pub use rizzma_core::{Affine2D, Path, color::Rgba};

use rizzma_core::PathSegment;
use rizzma_render::{CapStyle, GraphicsContext, JoinStyle, Renderer};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Rect, Stroke, StrokeDash,
    Transform,
};

/// An error encoding the pixmap to PNG, or writing it to disk.
///
/// Wraps the failure surfaced by [`Pixmap::encode_png`]/[`Pixmap::save_png`]
/// without leaking tiny-skia's private `png` dependency into this crate's API.
#[derive(Debug)]
pub struct PngError(String);

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PNG encoding failed: {}", self.0)
    }
}

impl std::error::Error for PngError {}

/// A raster [`Renderer`] backed by a [`tiny_skia::Pixmap`].
///
/// Construct one with [`SkiaRenderer::new`], draw into it via the [`Renderer`]
/// trait, then read the result with [`SkiaRenderer::encode_png`],
/// [`SkiaRenderer::save_png`], [`SkiaRenderer::pixmap`], or
/// [`SkiaRenderer::into_pixmap`].
#[derive(Debug, Clone)]
pub struct SkiaRenderer {
    /// The target pixmap, top-down RGBA.
    pixmap: Pixmap,
    /// Dots per inch, used to convert points to pixels.
    dpi: f64,
}

impl SkiaRenderer {
    /// Create a renderer over a `width_px` by `height_px` pixmap cleared to
    /// transparent, rendering at `dpi` dots per inch.
    ///
    /// # Panics
    ///
    /// Panics if `width_px` or `height_px` is zero (a pixmap cannot be empty).
    #[must_use]
    pub fn new(width_px: u32, height_px: u32, dpi: f64) -> Self {
        let pixmap = Pixmap::new(width_px, height_px)
            .expect("SkiaRenderer: pixmap dimensions must be non-zero");
        Self { pixmap, dpi }
    }

    /// The DPI this renderer scales points to pixels with.
    #[must_use]
    pub fn dpi(&self) -> f64 {
        self.dpi
    }

    /// A shared reference to the underlying pixmap.
    #[must_use]
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Consume the renderer, returning the underlying pixmap.
    #[must_use]
    pub fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    /// Encode the current pixmap contents as a PNG byte buffer.
    ///
    /// # Errors
    ///
    /// Returns a [`PngError`] if encoding fails, which for an in-memory buffer
    /// indicates an allocation failure.
    pub fn encode_png(&self) -> Result<Vec<u8>, PngError> {
        self.pixmap
            .encode_png()
            .map_err(|e| PngError(e.to_string()))
    }

    /// Save the current pixmap contents to `path` as a PNG file.
    ///
    /// # Errors
    ///
    /// Returns a [`PngError`] if encoding or writing the file fails.
    pub fn save_png<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), PngError> {
        self.pixmap
            .save_png(path)
            .map_err(|e| PngError(e.to_string()))
    }

    /// The Y-flip taking matplotlib device space (origin bottom-left) into
    /// pixmap space (origin top-left): `(x, y) -> (x, height - y)`.
    fn flip_transform(&self) -> Affine2D {
        let height = f64::from(self.pixmap.height());
        Affine2D::from_scale(1.0, -1.0).translate(0.0, height)
    }

    /// Build a [`tiny_skia::Path`] from `path`, mapping every anchor and control
    /// point through `device` (the composite transform into pixmap space).
    ///
    /// Returns `None` for paths that produce no drawable geometry (e.g. empty or
    /// degenerate), which tiny-skia's [`PathBuilder::finish`] also rejects.
    fn build_device_path(path: &Path, device: &Affine2D) -> Option<tiny_skia::Path> {
        let map = |p: [f64; 2]| -> (f32, f32) {
            let (x, y) = device.transform_point((p[0], p[1]));
            (x as f32, y as f32)
        };
        let mut pb = PathBuilder::new();
        for seg in path.iter_segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    let (x, y) = map(p);
                    pb.move_to(x, y);
                }
                PathSegment::LineTo(p) => {
                    let (x, y) = map(p);
                    pb.line_to(x, y);
                }
                PathSegment::Quad(c, e) => {
                    let (cx, cy) = map(c);
                    let (ex, ey) = map(e);
                    pb.quad_to(cx, cy, ex, ey);
                }
                PathSegment::Cubic(c1, c2, e) => {
                    let (c1x, c1y) = map(c1);
                    let (c2x, c2y) = map(c2);
                    let (ex, ey) = map(e);
                    pb.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
                }
                PathSegment::Close => pb.close(),
            }
        }
        pb.finish()
    }

    /// Build a rectangular clip [`Mask`] from `gc.clip_rect` (in matplotlib
    /// device coordinates), Y-flipped into pixmap space.
    ///
    /// Returns `None` when there is no clip rectangle or it is degenerate.
    fn clip_mask(&self, gc: &GraphicsContext) -> Option<Mask> {
        // TODO: clip_path — honor gc.clip_path for arbitrary-path clipping.
        let bbox = gc.clip_rect?;
        let height = f64::from(self.pixmap.height());
        // Y-flip the rectangle corners; the band's min/max in y swap.
        let left = bbox.xmin();
        let right = bbox.xmax();
        let top = height - bbox.ymax();
        let bottom = height - bbox.ymin();
        let rect = Rect::from_ltrb(left as f32, top as f32, right as f32, bottom as f32)?;
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        let rect_path = pb.finish()?;
        mask.fill_path(&rect_path, FillRule::Winding, true, Transform::identity());
        Some(mask)
    }
}

/// Convert an [`Rgba`] (optionally tinted by `gc.alpha`) into a tiny-skia
/// [`Color`], clamping channels to `0.0..=1.0`.
fn to_color(rgba: Rgba, alpha: Option<f64>) -> Color {
    let a = alpha.map_or(rgba.a, |g| rgba.a * g);
    Color::from_rgba(
        rgba.r.clamp(0.0, 1.0) as f32,
        rgba.g.clamp(0.0, 1.0) as f32,
        rgba.b.clamp(0.0, 1.0) as f32,
        a.clamp(0.0, 1.0) as f32,
    )
    .unwrap_or(Color::TRANSPARENT)
}

/// Map a seam [`CapStyle`] to tiny-skia's [`LineCap`].
fn to_line_cap(cap: CapStyle) -> LineCap {
    match cap {
        CapStyle::Butt => LineCap::Butt,
        CapStyle::Round => LineCap::Round,
        CapStyle::Projecting => LineCap::Square,
    }
}

/// Map a seam [`JoinStyle`] to tiny-skia's [`LineJoin`].
fn to_line_join(join: JoinStyle) -> LineJoin {
    match join {
        JoinStyle::Miter => LineJoin::Miter,
        JoinStyle::Round => LineJoin::Round,
        JoinStyle::Bevel => LineJoin::Bevel,
    }
}

impl Renderer for SkiaRenderer {
    fn draw_path(
        &mut self,
        gc: &GraphicsContext,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        let device = transform.then(&self.flip_transform());
        let Some(sk_path) = Self::build_device_path(path, &device) else {
            return;
        };
        let mask = self.clip_mask(gc);
        let mask_ref = mask.as_ref();
        let anti_alias = gc.antialiased;

        if let Some(face) = fill {
            let mut paint = Paint {
                anti_alias,
                ..Paint::default()
            };
            paint.set_color(to_color(face, gc.alpha));
            self.pixmap.fill_path(
                &sk_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask_ref,
            );
        }

        if let Some(stroke_color) = gc.stroke {
            let width = self.points_to_pixels(gc.line_width);
            if width > 0.0 {
                let mut paint = Paint {
                    anti_alias,
                    ..Paint::default()
                };
                paint.set_color(to_color(stroke_color, gc.alpha));
                let dash = gc.dashes.as_ref().and_then(|(offset, pattern)| {
                    let scale = self.dpi / 72.0;
                    let array: Vec<f32> = pattern.iter().map(|d| (d * scale) as f32).collect();
                    StrokeDash::new(array, (offset * scale) as f32)
                });
                let stroke = Stroke {
                    width: width as f32,
                    line_cap: to_line_cap(gc.cap),
                    line_join: to_line_join(gc.join),
                    dash,
                    ..Stroke::default()
                };
                self.pixmap
                    .stroke_path(&sk_path, &paint, &stroke, Transform::identity(), mask_ref);
            }
        }
    }

    fn canvas_size(&self) -> (f64, f64) {
        (
            f64::from(self.pixmap.width()),
            f64::from(self.pixmap.height()),
        )
    }

    fn points_to_pixels(&self, points: f64) -> f64 {
        points * self.dpi / 72.0
    }

    fn flipy(&self) -> bool {
        true
    }

    // draw_image / draw_text use the trait defaults for now.
    // TODO: implement raster image blit and text shaping.
}

#[cfg(test)]
mod tests {
    use super::*;
    use rizzma_core::Bbox;

    /// Read the RGBA bytes of the pixel at `(x, y)` (top-left origin). The bytes
    /// are premultiplied, but for the opaque colors used here that equals the
    /// straight RGBA.
    fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
        let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn filled_rectangle_covers_center_not_corner() {
        // 100x100 canvas. Map the unit rectangle to device x in [20, 80] and
        // y in [20, 80]. Device y is bottom-left while the pixmap is top-down,
        // but the band [20, 80] is symmetric about the vertical center, so it
        // lands on pixmap rows [20, 80] either way: the center pixel (50, 50)
        // is inside and the corner (5, 5) is outside.
        let mut r = SkiaRenderer::new(100, 100, 72.0);
        let transform = Affine2D::from_scale(60.0, 60.0).translate(20.0, 20.0);
        let gc = GraphicsContext::new();
        r.draw_path(&gc, &Path::unit_rectangle(), &transform, Some(Rgba::RED));

        assert_eq!(pixel(&r, 50, 50), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn stroked_polyline_sets_pixels_on_path() {
        let mut r = SkiaRenderer::new(100, 100, 72.0);
        // Horizontal line across the middle of the canvas (device y = 50).
        let line = Path::from_polyline(&[[10.0, 50.0], [90.0, 50.0]]);
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(4.0);
        r.draw_path(&gc, &line, &Affine2D::identity(), None);

        // The flip maps device y=50 to pixmap row 50 on a 100px canvas.
        let [_, _, _, a] = pixel(&r, 50, 50);
        assert!(a > 0, "stroke should mark the path center, got alpha {a}");
    }

    #[test]
    fn clip_rect_limits_drawing() {
        let mut r = SkiaRenderer::new(100, 100, 72.0);
        // Fill the whole canvas red, but clip to device rect x,y in [40, 60].
        let transform = Affine2D::from_scale(100.0, 100.0);
        let gc = GraphicsContext {
            clip_rect: Some(Bbox::from_extents(40.0, 40.0, 60.0, 60.0)),
            ..GraphicsContext::new()
        };
        r.draw_path(&gc, &Path::unit_rectangle(), &transform, Some(Rgba::RED));

        // Center of the clip band is painted; well outside it is not.
        assert_eq!(pixel(&r, 50, 50), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn encode_png_returns_non_empty() {
        let mut r = SkiaRenderer::new(16, 16, 72.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &Affine2D::from_scale(16.0, 16.0),
            Some(Rgba::BLUE),
        );
        assert!(!r.encode_png().expect("encode succeeds").is_empty());
    }

    #[test]
    fn capabilities_match_dpi() {
        let r = SkiaRenderer::new(10, 20, 144.0);
        assert!(r.flipy());
        assert_eq!(r.canvas_size(), (10.0, 20.0));
        // 144 dpi -> 2 px per point.
        assert_eq!(r.points_to_pixels(3.0), 6.0);
    }
}
