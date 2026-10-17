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

pub use crate::core::{Affine2D, Path, color::Rgba};

use crate::core::{Bbox, PathSegment};
use crate::render::{CapStyle, GraphicsContext, JoinStyle, Renderer};
use tiny_skia::{
    Color, FillRule, IntSize, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint,
    Rect, Stroke, StrokeDash, Transform,
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
    /// The most recently built clip [`Mask`], keyed by its source rectangle.
    ///
    /// Every artist inside one axes shares the same clip rectangle, and the
    /// default marker fan-out issues one `draw_path` per point, so rebuilding
    /// a full-canvas mask per call would dominate interactive scatter frames.
    clip_cache: Option<(Bbox, Mask)>,
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
        Self {
            pixmap,
            dpi,
            clip_cache: None,
        }
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

    /// Build a rectangular clip [`Mask`] from `bbox` (in matplotlib device
    /// coordinates), Y-flipped into pixmap space.
    ///
    /// Returns `None` when the rectangle is degenerate.
    fn build_clip_mask(width: u32, height: u32, bbox: &Bbox) -> Option<Mask> {
        // TODO: clip_path — honor gc.clip_path for arbitrary-path clipping.
        let pix_h = f64::from(height);
        // Y-flip the rectangle corners; the band's min/max in y swap.
        let left = bbox.xmin();
        let right = bbox.xmax();
        let top = pix_h - bbox.ymax();
        let bottom = pix_h - bbox.ymin();
        let rect = Rect::from_ltrb(left as f32, top as f32, right as f32, bottom as f32)?;
        let mut mask = Mask::new(width, height)?;
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        let rect_path = pb.finish()?;
        mask.fill_path(&rect_path, FillRule::Winding, true, Transform::identity());
        Some(mask)
    }

    /// The pixmap pixels `[x0, x1) x [y0, y1)` a clip rectangle `bbox` covers
    /// entirely — its mask is 255 there, so drawing inside needs no mask.
    fn covered_interior(&self, bbox: &Bbox) -> [i64; 4] {
        let height = f64::from(self.pixmap.height());
        [
            bbox.xmin().ceil() as i64,
            (height - bbox.ymax()).ceil() as i64,
            bbox.xmax().floor() as i64,
            (height - bbox.ymin()).floor() as i64,
        ]
    }

    /// Ensure [`SkiaRenderer::clip_cache`] holds the mask for `gc.clip_rect`.
    ///
    /// Every artist inside one axes draws with the same clip rectangle (and
    /// the marker fan-out repeats it per point), so the mask is built once
    /// and reused until a different rectangle arrives. After this call the
    /// mask is read directly from the `clip_cache` field so the borrow stays
    /// disjoint from `pixmap`.
    fn ensure_clip_mask(&mut self, gc: &GraphicsContext) {
        let Some(bbox) = gc.clip_rect else {
            self.clip_cache = None;
            return;
        };
        if matches!(&self.clip_cache, Some((cached, _)) if *cached == bbox) {
            return;
        }
        self.clip_cache = Self::build_clip_mask(self.pixmap.width(), self.pixmap.height(), &bbox)
            .map(|mask| (bbox, mask));
    }
}

/// Fill (with `fill`) then stroke (with `gc`'s stroke) `sk_path` on `target`,
/// through `transform` and `mask`, at `dpi` for line widths and dashes.
fn paint_path(
    target: &mut Pixmap,
    sk_path: &tiny_skia::Path,
    gc: &GraphicsContext,
    fill: Option<Rgba>,
    dpi: f64,
    transform: Transform,
    mask: Option<&Mask>,
) {
    let anti_alias = gc.antialiased;
    if let Some(face) = fill {
        let mut paint = Paint {
            anti_alias,
            ..Paint::default()
        };
        paint.set_color(to_color(face, gc.alpha));
        target.fill_path(sk_path, &paint, FillRule::Winding, transform, mask);
    }

    if let Some(stroke_color) = gc.stroke {
        let width = gc.line_width * dpi / 72.0;
        let bounds = sk_path.bounds();
        let has_extent = bounds.width() > 0.0 || bounds.height() > 0.0;
        if width > 0.0 && has_extent {
            let mut paint = Paint {
                anti_alias,
                ..Paint::default()
            };
            paint.set_color(to_color(stroke_color, gc.alpha));
            let dash = gc.dashes.as_ref().and_then(|(offset, pattern)| {
                let scale = dpi / 72.0;
                let array: Vec<f32> = pattern.iter().map(|d| (d * scale) as f32).collect();
                StrokeDash::new(array, (offset * scale) as f32)
            });
            let mut stroke = Stroke {
                width: width as f32,
                line_cap: to_line_cap(gc.cap),
                line_join: to_line_join(gc.join),
                dash,
                ..Stroke::default()
            };
            if let Some(limit) = gc.miter_limit {
                stroke.miter_limit = limit as f32;
            }
            target.stroke_path(sk_path, &paint, &stroke, transform, mask);
        }
    }
}

/// Whether `path` is only horizontal and vertical lines, which Agg snaps to
/// the pixel grid unasked (`SNAP_AUTO`).
fn is_rectilinear(path: &Path) -> bool {
    if path.vertices().len() > 1024 {
        return false;
    }
    let mut pen: Option<[f64; 2]> = None;
    for segment in path.iter_segments() {
        match segment {
            PathSegment::MoveTo(p) => pen = Some(p),
            PathSegment::LineTo(p) => {
                let diagonal =
                    pen.is_some_and(|q| (p[0] - q[0]).abs() >= 1e-4 && (p[1] - q[1]).abs() >= 1e-4);
                if diagonal {
                    return false;
                }
                pen = Some(p);
            }
            PathSegment::Quad(..) | PathSegment::Cubic(..) => return false,
            PathSegment::Close => {}
        }
    }
    true
}

/// Below this many markers [`SkiaRenderer::draw_markers`] draws each one
/// exactly: the sprite would cost about as much as the markers it saves.
const SPRITE_MIN_POINTS: usize = 10_000;

/// A clip mask, with the rectangle of pixels it covers fully.
struct Clip<'a> {
    mask: &'a Mask,
    interior: [i64; 4],
}

/// A marker rasterized once for [`SkiaRenderer::draw_markers`], with the
/// marker's origin at its phase past the corner of pixel `(cx, cy)`: the
/// fill and the stroke as separate premultiplied layers (tiny-skia composites
/// them one after the other, and so must the stamp), each with its rows'
/// inked spans.
struct MarkerSprite {
    layers: Vec<SpriteLayer>,
    cx: i64,
    cy: i64,
    width: i64,
    height: i64,
}

/// One layer of a [`MarkerSprite`].
struct SpriteLayer {
    pixmap: Pixmap,
    /// Per row, the half-open column range holding non-transparent pixels.
    spans: Vec<(i64, i64)>,
}

impl SpriteLayer {
    /// `paint` drawn into a fresh `width` x `height` layer; `None` when it
    /// leaves the layer empty.
    fn paint(width: u32, height: u32, paint: impl FnOnce(&mut Pixmap)) -> Option<Self> {
        let mut pixmap = Pixmap::new(width, height)?;
        paint(&mut pixmap);
        let spans: Vec<(i64, i64)> = pixmap
            .data()
            .chunks_exact(width as usize * 4)
            .map(|row| {
                let inked = |px: &[u8]| px[3] != 0;
                let first = row.chunks_exact(4).position(inked);
                let last = row.chunks_exact(4).rposition(inked);
                match (first, last) {
                    (Some(a), Some(b)) => (a as i64, b as i64 + 1),
                    _ => (0, 0),
                }
            })
            .collect();
        spans
            .iter()
            .any(|(a, b)| a < b)
            .then_some(Self { pixmap, spans })
    }
}

impl MarkerSprite {
    /// Rasterize `marker` (already in pixmap orientation, origin at its
    /// anchor) filled with `fill` and stroked per `gc`, its origin on a
    /// pixel corner. `None` when it draws nothing.
    fn new(
        marker: &tiny_skia::Path,
        gc: &GraphicsContext,
        fill: Option<Rgba>,
        dpi: f64,
    ) -> Option<Self> {
        let bounds = marker.bounds();
        // Room for the stroke, including miter joins, plus antialiasing.
        let stroke = if gc.stroke.is_some() {
            gc.line_width * dpi / 72.0
        } else {
            0.0
        };
        let margin = 2.0 * stroke + 2.0;
        let left = (f64::from(bounds.left()) - margin).floor();
        let top = (f64::from(bounds.top()) - margin).floor();
        let right = (f64::from(bounds.right()) + margin).ceil();
        let bottom = (f64::from(bounds.bottom()) + margin).ceil();
        let (width, height) = ((right - left) as u32 + 2, (bottom - top) as u32 + 2);
        // The marker's origin lands on the corner of sprite pixel (cx, cy).
        let (cx, cy) = (-left, -top);
        let shift = Transform::from_translate(cx as f32, cy as f32);
        let face_only = GraphicsContext {
            stroke: None,
            ..gc.clone()
        };
        let layers: Vec<SpriteLayer> = [
            SpriteLayer::paint(width, height, |p| {
                paint_path(p, marker, &face_only, fill, dpi, shift, None);
            }),
            SpriteLayer::paint(width, height, |p| {
                paint_path(p, marker, gc, None, dpi, shift, None);
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!layers.is_empty()).then_some(Self {
            layers,
            cx: cx as i64,
            cy: cy as i64,
            width: i64::from(width),
            height: i64::from(height),
        })
    }

    /// Composite the sprite's layers source-over onto `target` with the
    /// marker origin at its phase past the corner of pixel `(px, py)`,
    /// through `clip` (skipped where the sprite lies wholly inside the
    /// mask's fully covered interior).
    fn stamp(&self, target: &mut Pixmap, px: i64, py: i64, clip: Option<&Clip<'_>>) {
        let (tw, th) = (i64::from(target.width()), i64::from(target.height()));
        let (sw, sh) = (self.width, self.height);
        let (ox, oy) = (px - self.cx, py - self.cy);
        let mask = clip
            .filter(|c| {
                let [ix0, iy0, ix1, iy1] = c.interior;
                !(ox >= ix0 && oy >= iy0 && ox + sw <= ix1 && oy + sh <= iy1)
            })
            .map(|c| c.mask.data());
        let dst = target.data_mut();
        for layer in &self.layers {
            let src = layer.pixmap.data();
            for row in oy.max(0)..(oy + sh).min(th) {
                let (a, b) = layer.spans[(row - oy) as usize];
                let (x0, x1) = ((ox + a).max(0), (ox + b).min(tw));
                for x in x0..x1 {
                    let s = (((row - oy) * sw + (x - ox)) * 4) as usize;
                    let d = ((row * tw + x) * 4) as usize;
                    let pixel = &src[s..s + 4];
                    match mask {
                        // Scale by the clip's coverage as tiny-skia's mask
                        // stage does.
                        Some(cov) => {
                            let c = u32::from(cov[(row * tw + x) as usize]);
                            if c != 0 {
                                let scaled =
                                    [0, 1, 2, 3].map(|k| div255(u32::from(pixel[k]) * c) as u8);
                                source_over(&mut dst[d..d + 4], &scaled);
                            }
                        }
                        None => source_over(&mut dst[d..d + 4], pixel),
                    }
                }
            }
        }
    }
}

/// Premultiplied source-over of `src` onto `dst`, in tiny-skia's arithmetic,
/// so a sprite pixel lands exactly as the same fill drawn in place would.
fn source_over(dst: &mut [u8], src: &[u8]) {
    match src[3] {
        0 => {}
        255 => dst.copy_from_slice(src),
        alpha => {
            let keep = 255 - u32::from(alpha);
            for k in 0..4 {
                dst[k] = (u32::from(src[k]) + div255(u32::from(dst[k]) * keep)) as u8;
            }
        }
    }
}

/// tiny-skia's `div255`: `(x + 255) >> 8`.
fn div255(x: u32) -> u32 {
    (x + 255) >> 8
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

/// Convert a straight (non-premultiplied) RGBA8 buffer into the premultiplied
/// RGBA8 layout that [`tiny_skia::Pixmap::from_vec`] requires.
///
/// `rgba` must hold exactly `width * height * 4` bytes in row-major order. Each
/// pixel's color channels are scaled by its alpha (`c * a / 255`, rounded),
/// producing the premultiplied form tiny-skia stores internally.
fn premultiply_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    debug_assert_eq!(rgba.len(), width * height * 4);
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let [r, g, b, a] = [px[0], px[1], px[2], px[3]];
        // Premultiply with rounding: (channel * alpha + 127) / 255.
        let mul = |c: u8| -> u8 { ((u16::from(c) * u16::from(a) + 127) / 255) as u8 };
        out.push(mul(r));
        out.push(mul(g));
        out.push(mul(b));
        out.push(a);
    }
    out
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
        self.ensure_clip_mask(gc);
        let mask_ref = self.clip_cache.as_ref().map(|(_, mask)| mask);
        paint_path(
            &mut self.pixmap,
            &sk_path,
            gc,
            fill,
            self.dpi,
            Transform::identity(),
            mask_ref,
        );
    }

    /// Stamp one marker at many points, placed as matplotlib Agg's
    /// `draw_markers` places them.
    ///
    /// Each point's origin is the pixel corner `floor(p + 0.5)` of the y-down
    /// pixel grid. A snapping marker (`gc.snap`, or when unset one drawn only
    /// of horizontal and vertical lines) has its vertices snapped to
    /// `floor(v + 0.5)` about that origin, plus half a pixel when the edge is
    /// an odd whole number of pixels wide; any other marker moves half a
    /// pixel, onto the pixel center. Every marker then sits at the same
    /// sub-pixel phase, so many markers share one sprite, rasterized once, as
    /// Agg shares the marker's scanlines; fewer than 10 000 draw one path
    /// each.
    fn draw_markers(
        &mut self,
        gc: &GraphicsContext,
        marker_path: &Path,
        marker_transform: &Affine2D,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        // The marker in y-down pixels about its origin.
        let marker =
            marker_path.transformed(&marker_transform.then(&Affine2D::from_scale(1.0, -1.0)));
        let snap = gc.snap.unwrap_or_else(|| is_rectilinear(&marker));
        let (marker, offset) = if snap {
            let edge = self.points_to_pixels(gc.line_width);
            let half = if (edge + 0.5).floor() as i64 % 2 == 1 {
                0.5
            } else {
                0.0
            };
            let snapped = marker
                .vertices()
                .iter()
                .map(|&[x, y]| [(x + 0.5).floor() + half, (y + 0.5).floor() + half])
                .collect();
            (Path::new(snapped, marker.codes().map(<[_]>::to_vec)), 0.0)
        } else {
            (marker, 0.5)
        };
        let marker = marker.transformed(&Affine2D::from_translation(offset, offset));
        let device = transform.then(&self.flip_transform());
        let origins: Vec<(f64, f64)> = path
            .vertices()
            .iter()
            .map(|&[vx, vy]| device.transform_point((vx, vy)))
            .filter(|(x, y)| x.abs() < 1e9 && y.abs() < 1e9)
            .map(|(x, y)| ((x + 0.5).floor(), (y + 0.5).floor()))
            .collect();
        // Few markers draw exactly, one path each, which costs about what
        // building the sprite would.
        if origins.len() < SPRITE_MIN_POINTS {
            let height = f64::from(self.pixmap.height());
            for (x, y) in origins {
                let place = Affine2D::new(1.0, 0.0, 0.0, -1.0, x, height - y);
                self.draw_path(gc, &marker, &place, fill);
            }
            return;
        }
        let Some(sk_marker) = Self::build_device_path(&marker, &Affine2D::identity()) else {
            return;
        };
        let Some(sprite) = MarkerSprite::new(&sk_marker, gc, fill, self.dpi) else {
            return;
        };
        self.ensure_clip_mask(gc);
        let clip = self.clip_cache.as_ref().map(|(bbox, mask)| Clip {
            mask,
            interior: self.covered_interior(bbox),
        });
        for (x, y) in origins {
            sprite.stamp(&mut self.pixmap, x as i64, y as i64, clip.as_ref());
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

    fn decoration_scale(&self) -> f64 {
        self.dpi / 100.0
    }

    fn flipy(&self) -> bool {
        true
    }

    /// Blit a straight-RGBA8 image with its lower-left corner at device
    /// `(x, y)`.
    ///
    /// `rgba` is a row-major `width * height` buffer (4 bytes per pixel,
    /// top-row-first). The bytes are premultiplied (tiny-skia's required
    /// layout) into a source [`Pixmap`], then composited into the destination.
    ///
    /// # Y-flip
    ///
    /// matplotlib's device origin is bottom-left, so `(x, y)` is the image's
    /// *lower-left* corner and the image spans device y in `[y, y + height]`.
    /// The pixmap is top-down, so the image's top edge (device `y + height`)
    /// lands at pixmap row `pixmap_height - y - height`. Because the input rows
    /// are already top-first, no row reversal is needed: the source pixmap is
    /// blitted at pixmap coordinate `(x, pixmap_height - y - height)`.
    ///
    /// `gc.alpha` is honored as a global opacity via [`PixmapPaint::opacity`],
    /// and `gc.clip_rect` clips the blit. (`gc.clip_path` is not yet honored;
    /// see the private `clip_mask` helper.)
    fn draw_image(
        &mut self,
        gc: &GraphicsContext,
        x: f64,
        y: f64,
        rgba: &[u8],
        width: usize,
        height: usize,
    ) {
        // Reject empty images or buffers that do not match the stated shape.
        if width == 0 || height == 0 || rgba.len() != width * height * 4 {
            return;
        }
        let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
            return;
        };
        let Some(size) = IntSize::from_wh(w, h) else {
            return;
        };
        let premultiplied = premultiply_rgba(rgba, width, height);
        let Some(src) = Pixmap::from_vec(premultiplied, size) else {
            return;
        };

        // Destination top-left in pixmap (top-down) space, applying the Y-flip.
        let pix_h = f64::from(self.pixmap.height());
        let dest_x = x.round() as i32;
        let dest_y = (pix_h - y - height as f64).round() as i32;

        let opacity = gc.alpha.map_or(1.0, |a| a.clamp(0.0, 1.0) as f32);
        let paint = PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        };

        self.ensure_clip_mask(gc);
        self.pixmap.draw_pixmap(
            dest_x,
            dest_y,
            src.as_ref(),
            &paint,
            Transform::identity(),
            self.clip_cache.as_ref().map(|(_, mask)| mask),
        );
    }

    // draw_text uses the trait default for now.
    // TODO: implement text shaping.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bbox;

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
    fn degenerate_strokes_are_ignored() {
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(4.0);

        for points in [&[[50.0, 50.0]][..], &[[50.0, 50.0], [50.0, 50.0]][..]] {
            let mut r = SkiaRenderer::new(100, 100, 72.0);
            r.draw_path(
                &gc,
                &Path::from_polyline(points),
                &Affine2D::identity(),
                None,
            );
            assert_eq!(pixel(&r, 50, 50), [0, 0, 0, 0]);
        }
    }

    #[test]
    fn zero_width_vertical_line_is_not_treated_as_degenerate() {
        let mut r = SkiaRenderer::new(100, 100, 72.0);
        let line = Path::from_polyline(&[[50.0, 10.0], [50.0, 90.0]]);
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(4.0);
        r.draw_path(&gc, &line, &Affine2D::identity(), None);

        assert!(pixel(&r, 50, 50)[3] > 0);
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

    /// Build a `width * height` straight-RGBA8 buffer where every pixel is the
    /// given color.
    fn solid_rgba(width: usize, height: usize, rgba: [u8; 4]) -> Vec<u8> {
        rgba.iter()
            .copied()
            .cycle()
            .take(width * height * 4)
            .collect()
    }

    #[test]
    fn draw_image_places_solid_block_with_y_flip() {
        // 100x100 canvas; blit a 10x10 red block with its lower-left corner at
        // device (20, 20). The block spans device x in [20, 30] and device
        // y in [20, 30]. With the Y-flip its top edge (device y=30) lands at
        // pixmap row 100 - 30 = 70, so the block occupies pixmap rows [70, 80)
        // and columns [20, 30).
        let mut r = SkiaRenderer::new(100, 100, 72.0);
        let img = solid_rgba(10, 10, [255, 0, 0, 255]);
        r.draw_image(&GraphicsContext::new(), 20.0, 20.0, &img, 10, 10);

        // Inside the blitted region: opaque red.
        assert_eq!(pixel(&r, 25, 75), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 20, 70), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 29, 79), [255, 0, 0, 255]);

        // Outside the region (and where the un-flipped block would be): still
        // transparent.
        assert_eq!(pixel(&r, 5, 5), [0, 0, 0, 0]);
        assert_eq!(pixel(&r, 25, 25), [0, 0, 0, 0]);
        assert_eq!(pixel(&r, 50, 50), [0, 0, 0, 0]);
    }

    #[test]
    fn draw_image_half_transparent_blends_over_background() {
        // Paint the whole canvas opaque blue, then blit a half-transparent red
        // (alpha 128) image over part of it. SourceOver compositing yields a
        // purple-ish blend: out = src + dst * (1 - src_a).
        let mut r = SkiaRenderer::new(20, 20, 72.0);
        let bg = Affine2D::from_scale(20.0, 20.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &bg,
            Some(Rgba::BLUE),
        );

        let img = solid_rgba(8, 8, [255, 0, 0, 128]);
        // Lower-left at (5, 5); pixmap rows [20-13, 20-5) = [7, 13).
        r.draw_image(&GraphicsContext::new(), 5.0, 5.0, &img, 8, 8);

        let [red, green, blue, alpha] = pixel(&r, 8, 10);
        // src red premultiplied = 128, plus dst blue * (1 - 128/255) ~= 127.
        assert!(red > 110 && red < 145, "red {red}");
        assert_eq!(green, 0);
        assert!(blue > 110 && blue < 145, "blue {blue}");
        assert_eq!(alpha, 255);

        // A background pixel outside the blit stays pure blue.
        assert_eq!(pixel(&r, 0, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn draw_image_non_square_places_corner() {
        // A wide image: width 30, height 10. Lower-left at (0, 0) on a 40x40
        // canvas. Pixmap rows [40-10, 40) = [30, 40), columns [0, 30).
        let mut r = SkiaRenderer::new(40, 40, 72.0);
        let img = solid_rgba(30, 10, [0, 255, 0, 255]);
        r.draw_image(&GraphicsContext::new(), 0.0, 0.0, &img, 30, 10);

        // Far-right column of the image (x=29) is filled green.
        assert_eq!(pixel(&r, 29, 39), [0, 255, 0, 255]);
        // The bottom-left pixmap corner (device top) is filled too.
        assert_eq!(pixel(&r, 0, 30), [0, 255, 0, 255]);
        // Just past the image width stays transparent.
        assert_eq!(pixel(&r, 30, 39), [0, 0, 0, 0]);
        // Above the image (smaller pixmap row) stays transparent.
        assert_eq!(pixel(&r, 0, 29), [0, 0, 0, 0]);
    }

    #[test]
    fn draw_image_global_alpha_scales_opacity() {
        // Blit an opaque red image with gc.alpha = 0.5: the result should be a
        // half-opaque red over transparent.
        let mut r = SkiaRenderer::new(20, 20, 72.0);
        let img = solid_rgba(6, 6, [255, 0, 0, 255]);
        let gc = GraphicsContext::new().with_alpha(0.5);
        r.draw_image(&gc, 2.0, 2.0, &img, 6, 6);

        let [_, _, _, alpha] = pixel(&r, 4, 14);
        assert!((120..=135).contains(&alpha), "alpha {alpha}");
    }

    #[test]
    fn draw_image_ignores_mismatched_buffer() {
        // A buffer that does not match width*height*4 is rejected (no-op).
        let mut r = SkiaRenderer::new(10, 10, 72.0);
        let img = vec![255u8; 7]; // not 4*W*H for any 2x2 etc.
        r.draw_image(&GraphicsContext::new(), 0.0, 0.0, &img, 2, 2);
        assert_eq!(pixel(&r, 0, 9), [0, 0, 0, 0]);
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
