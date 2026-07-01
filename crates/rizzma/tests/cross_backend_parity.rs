//! Cross-backend pixel-parity guard for the [`Renderer`] seam.
//!
//! This host-only integration test proves that the same scene, drawn purely
//! through the backend-agnostic [`Renderer`] trait, rasterizes to *visually
//! identical* pixels through the tiny-skia raster backend ([`SkiaRenderer`]) and
//! the SVG vector backend ([`SvgRenderer`]). It is a permanent guard against
//! backend drift (part of M4): if one backend silently changes its coordinate
//! mapping, fill rule, stroke geometry, or color handling, the per-channel RMS
//! difference between the two buffers blows past the tolerance and this fails.
//!
//! ## How the comparison works
//!
//! 1. The `scene` function draws an antialiasing-robust, text-free scene (a
//!    solid background, a filled sub-rectangle, a filled circle, and one thick
//!    solid stroked line) using only `Renderer` trait calls, so both backends
//!    receive byte-identical instructions.
//! 2. The skia backend rasterizes directly; we read its premultiplied RGBA8
//!    bytes from the pixmap.
//! 3. The SVG backend serializes markup; we parse it with `resvg`/`usvg` and
//!    render it into a `tiny_skia::Pixmap` of the same size, yielding a second
//!    premultiplied RGBA8 buffer in the same layout.
//! 4. Both backends apply the same internal Y-flip, so the two buffers should
//!    agree modulo antialiasing/rasterizer rounding. We compute the
//!    root-mean-square per-channel difference (0-255 scale) and assert it is
//!    below a small tolerance.
//!
//! ## Tolerance rationale
//!
//! The scene is deliberately built from primitives whose coverage is identical
//! across the two backends. Both rasterize through the *same* tiny-skia engine
//! (the skia backend rasterizes directly; the SVG markup is rasterized by resvg,
//! which also delegates to tiny-skia), and the SVG `<path>` `d` data we emit is
//! the exact same flattened geometry the raster backend draws — so with this
//! scene the two buffers come out **byte-for-byte identical** and the measured
//! RMS is `0.0` on the 0-255 scale.
//!
//! The tolerance is nonetheless set to a small `1.0` rather than `0.0`: an exact
//! match is the *expected* result, but pinning it to literal zero would make the
//! test brittle against a future tiny-skia/resvg point release that nudges
//! sub-pixel antialiasing by a single least-significant bit on an edge pixel.
//! `1.0` still catches any real geometry/color regression (which shifts whole
//! filled regions and pushes the RMS into the tens or hundreds) while tolerating
//! that benign edge jitter. No primitives had to be excluded.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

use rizzma::core::{Affine2D, Path, color::Rgba};
use rizzma::render::{GraphicsContext, Renderer};
use rizzma::skia::SkiaRenderer;
use rizzma::svg::SvgRenderer;

/// Canvas dimensions and DPI shared by both backends.
const W: u32 = 200;
const H: u32 = 200;
const DPI: f64 = 72.0;

/// The RMS-per-channel tolerance (0-255 scale). The measured value for this
/// scene is `0.0` (the buffers are byte-identical); see the module docs for why
/// the bound is a small `1.0` rather than literal zero.
const RMS_TOLERANCE: f64 = 1.0;

/// Draw the parity scene through the [`Renderer`] trait only.
///
/// Everything here is opaque, solid (no dashes), and text-free, and avoids
/// clipping — all of which legitimately differ across rasterizers. The shapes
/// are kept away from the canvas edges so no geometry is clipped by the border.
fn scene(r: &mut dyn Renderer) {
    // Opaque background so both buffers are fully covered and directly
    // comparable (otherwise transparent regions could differ in stored RGB).
    let bg = Rgba::new(0.93, 0.93, 0.95, 1.0);
    r.draw_path(
        &GraphicsContext::new(),
        &Path::unit_rectangle(),
        &Affine2D::from_scale(f64::from(W), f64::from(H)),
        Some(bg),
    );

    // Filled sub-rectangle (solid fill, no stroke), device x,y in [30, 110].
    let rect_tf = Affine2D::from_scale(80.0, 80.0).translate(30.0, 30.0);
    r.draw_path(
        &GraphicsContext::new(),
        &Path::unit_rectangle(),
        &rect_tf,
        Some(Rgba::new(0.20, 0.45, 0.80, 1.0)),
    );

    // Filled circle, radius 35 centered at device (140, 140), no stroke.
    let circle_tf = Affine2D::from_scale(35.0, 35.0).translate(140.0, 140.0);
    r.draw_path(
        &GraphicsContext::new(),
        &Path::unit_circle(),
        &circle_tf,
        Some(Rgba::new(0.85, 0.30, 0.25, 1.0)),
    );

    // One thick solid stroked line (no fill). Horizontal so its edges are axis
    // aligned and the two rasterizers agree on coverage.
    let line = Path::from_polyline(&[[25.0, 165.0], [110.0, 165.0]]);
    let gc = GraphicsContext::new()
        .with_stroke(Rgba::new(0.1, 0.1, 0.1, 1.0))
        .with_line_width(8.0);
    r.draw_path(&gc, &line, &Affine2D::identity(), None);
}

/// Render the scene with the skia backend and return its premultiplied RGBA8
/// bytes.
fn render_skia() -> Vec<u8> {
    let mut r = SkiaRenderer::new(W, H, DPI);
    scene(&mut r);
    r.pixmap().data().to_vec()
}

/// Render the scene with the SVG backend, rasterize the markup via resvg, and
/// return premultiplied RGBA8 bytes in the same layout as the skia pixmap.
fn render_svg() -> Vec<u8> {
    let mut r = SvgRenderer::new(f64::from(W), f64::from(H), DPI);
    scene(&mut r);
    let svg = r.finish();

    // The scene is text-free, so the default (empty) font database is fine.
    let tree = Tree::from_str(&svg, &Options::default()).expect("parse SVG markup");
    let mut pixmap = Pixmap::new(W, H).expect("allocate resvg pixmap");
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    pixmap.data().to_vec()
}

/// Root-mean-square per-channel difference (0-255 scale) between two equal
/// length RGBA8 buffers.
fn rms_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "buffers must be the same length");
    let mut sum_sq = 0.0_f64;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let d = f64::from(x) - f64::from(y);
        sum_sq += d * d;
    }
    (sum_sq / a.len() as f64).sqrt()
}

/// Whether a buffer actually contains drawn ink, i.e. it is not uniformly one
/// color (which would mean nothing was drawn over the background).
fn has_ink(buf: &[u8]) -> bool {
    let Some(first) = buf.first_chunk::<4>() else {
        return false;
    };
    buf.chunks_exact(4).any(|px| px != first)
}

#[test]
fn skia_and_svg_backends_render_matching_pixels() {
    let skia = render_skia();
    let svg = render_svg();

    // Same canvas, same layout: 200x200 RGBA8 = 160_000 bytes each.
    let expected_len = (W * H * 4) as usize;
    assert_eq!(skia.len(), expected_len, "skia buffer size");
    assert_eq!(svg.len(), expected_len, "svg buffer size");

    // Both backends must have actually drawn the scene (more than a flat fill).
    assert!(has_ink(&skia), "skia buffer is uniform — nothing drawn");
    assert!(has_ink(&svg), "svg buffer is uniform — nothing drawn");

    let rms = rms_difference(&skia, &svg);
    assert!(
        rms < RMS_TOLERANCE,
        "cross-backend RMS difference {rms:.4} exceeds tolerance {RMS_TOLERANCE} \
         (per-channel, 0-255); residual should be edge antialiasing only — \
         a larger value signals real backend drift",
    );
}
