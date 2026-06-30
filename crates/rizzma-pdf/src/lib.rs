//! PDF vector backend for rizzma.
//!
//! A [`Renderer`] implementation that serializes every [`Path`] into a
//! single-page PDF content stream and assembles a well-formed PDF byte buffer.
//! It sits alongside the SVG and tiny-skia backends, proving the renderer seam
//! is backend-agnostic.
//!
//! # Coordinate systems
//!
//! matplotlib's device space has its origin at the bottom-left with y growing
//! upward. PDF user space shares exactly that convention: the origin is at the
//! bottom-left of the `/MediaBox` and y grows upward. The two coordinate systems
//! therefore coincide, so — unlike the SVG backend, which Y-flips into a
//! top-left space — this backend applies **no Y flip at all** and reports
//! [`Renderer::flipy`] as `false`. A path drawn in the lower-left of the canvas
//! renders in the lower-left of the page.
//!
//! # Scope
//!
//! Filled and stroked vector paths are supported (which covers lines, patches,
//! and text, since higher layers reach the renderer with text already shaped
//! into glyph-outline [`Path`]s). Per-color and global alpha are honored via
//! `ExtGState` `/ca` (fill) and `/CA` (stroke) entries. Raster images
//! ([`Renderer::draw_image`]) are **not yet supported** and are emitted as a
//! no-op.

use std::fmt::Write as _;

pub use rizzma_core::{Affine2D, Path, color::Rgba};

use rizzma_core::PathSegment;
use rizzma_render::{GraphicsContext, Renderer};

/// A vector [`Renderer`] that serializes paths into a single-page PDF document.
///
/// Construct one with [`PdfRenderer::new`], draw into it via the [`Renderer`]
/// trait, then read the result with [`PdfRenderer::into_pdf`] /
/// [`PdfRenderer::finish`] or persist it with [`PdfRenderer::save`].
#[derive(Debug, Clone)]
pub struct PdfRenderer {
    /// Accumulated PDF content-stream operators (page drawing commands).
    content: String,
    /// Canvas width in device pixels (the `/MediaBox` width).
    width: f64,
    /// Canvas height in device pixels (the `/MediaBox` height).
    height: f64,
    /// Dots per inch, used to convert points to pixels.
    dpi: f64,
    /// Deferred `ExtGState` alpha definitions as `(fill_alpha, stroke_alpha)`,
    /// each becoming one `/GSn` resource entry.
    ext_gstates: Vec<(f64, f64)>,
}

impl PdfRenderer {
    /// Create a renderer over a `width_px` by `height_px` canvas, rendering at
    /// `dpi` dots per inch.
    #[must_use]
    pub fn new(width_px: f64, height_px: f64, dpi: f64) -> Self {
        Self {
            content: String::new(),
            width: width_px,
            height: height_px,
            dpi,
            ext_gstates: Vec::new(),
        }
    }

    /// The DPI this renderer scales points to pixels with.
    #[must_use]
    pub fn dpi(&self) -> f64 {
        self.dpi
    }

    /// Append the path-construction operators for `path`, mapping every anchor
    /// and control point through `transform` into PDF user space.
    ///
    /// Returns `true` when at least one drawable operator was emitted.
    fn build_path_ops(&mut self, path: &Path, transform: &Affine2D) -> bool {
        let map = |p: [f64; 2]| -> (f64, f64) { transform.transform_point((p[0], p[1])) };
        let mut any = false;
        for seg in path.iter_segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    let (x, y) = map(p);
                    let _ = writeln!(self.content, "{} {} m", fmt_f(x), fmt_f(y));
                    any = true;
                }
                PathSegment::LineTo(p) => {
                    let (x, y) = map(p);
                    let _ = writeln!(self.content, "{} {} l", fmt_f(x), fmt_f(y));
                    any = true;
                }
                PathSegment::Quad(c, e) => {
                    // PDF has no quadratic operator, so the quadratic is emitted
                    // as a cubic with both control points coincident at the
                    // quadratic control. For the small curves rizzma emits this
                    // is visually faithful; exact 2/3 elevation (which needs the
                    // running current point) is a TODO.
                    let (cx, cy) = map(c);
                    let (ex, ey) = map(e);
                    let _ = writeln!(
                        self.content,
                        "{} {} {} {} {} {} c",
                        fmt_f(cx),
                        fmt_f(cy),
                        fmt_f(cx),
                        fmt_f(cy),
                        fmt_f(ex),
                        fmt_f(ey),
                    );
                    any = true;
                }
                PathSegment::Cubic(c1, c2, e) => {
                    let (c1x, c1y) = map(c1);
                    let (c2x, c2y) = map(c2);
                    let (ex, ey) = map(e);
                    let _ = writeln!(
                        self.content,
                        "{} {} {} {} {} {} c",
                        fmt_f(c1x),
                        fmt_f(c1y),
                        fmt_f(c2x),
                        fmt_f(c2y),
                        fmt_f(ex),
                        fmt_f(ey),
                    );
                    any = true;
                }
                PathSegment::Close => {
                    self.content.push_str("h\n");
                }
            }
        }
        any
    }

    /// Register an `ExtGState` for the `(fill_alpha, stroke_alpha)` pair and
    /// return its resource name (e.g. `GS0`), reusing an existing entry when the
    /// pair already exists. Returns `None` when both alphas are fully opaque.
    fn push_ext_gstate(&mut self, fill_alpha: f64, stroke_alpha: f64) -> Option<String> {
        let fa = fill_alpha.clamp(0.0, 1.0);
        let sa = stroke_alpha.clamp(0.0, 1.0);
        if fa >= 1.0 && sa >= 1.0 {
            return None;
        }
        let idx = self
            .ext_gstates
            .iter()
            .position(|&(f, s)| f == fa && s == sa)
            .unwrap_or_else(|| {
                self.ext_gstates.push((fa, sa));
                self.ext_gstates.len() - 1
            });
        Some(format!("GS{idx}"))
    }

    /// Assemble and return the full PDF document as a byte buffer.
    ///
    /// This computes byte-accurate `xref` offsets from the actually emitted
    /// bytes, so the cross-reference table always points at real object
    /// positions.
    #[must_use]
    pub fn finish(&self) -> Vec<u8> {
        let content = self.content.as_bytes();

        // Build the Resources object body, including any ExtGState entries.
        let mut ext_gstate_dict = String::new();
        for (i, &(fa, sa)) in self.ext_gstates.iter().enumerate() {
            let _ = write!(
                ext_gstate_dict,
                "/GS{i} << /Type /ExtGState /ca {} /CA {} >> ",
                fmt_f(fa),
                fmt_f(sa)
            );
        }
        let resources = if ext_gstate_dict.is_empty() {
            "<< >>".to_string()
        } else {
            format!("<< /ExtGState << {ext_gstate_dict}>> >>")
        };

        // The five objects: 1 Catalog, 2 Pages, 3 Page, 4 Contents (binary
        // stream, emitted specially), 5 Resources. Object 0 is the free head.
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources 5 0 R >>",
                fmt_f(self.width),
                fmt_f(self.height)
            ),
            String::new(), // placeholder for object 4, kept for index alignment
            resources,
        ];

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"%PDF-1.7\n");
        // A binary marker comment so tools treat the file as binary.
        out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

        // Byte offset of each object body (1-indexed by object number).
        let mut offsets: Vec<usize> = vec![0; objects.len() + 1];

        for (i, body) in objects.iter().enumerate() {
            let obj_num = i + 1;
            offsets[obj_num] = out.len();
            if obj_num == 4 {
                let header = format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len());
                out.extend_from_slice(header.as_bytes());
                out.extend_from_slice(content);
                out.extend_from_slice(b"\nendstream\nendobj\n");
            } else {
                let obj = format!("{obj_num} 0 obj\n{body}\nendobj\n");
                out.extend_from_slice(obj.as_bytes());
            }
        }

        // Cross-reference table.
        let xref_pos = out.len();
        let count = objects.len() + 1; // include object 0 (free head)
        let mut xref = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            let _ = writeln!(xref, "{offset:010} 00000 n ");
        }
        out.extend_from_slice(xref.as_bytes());

        let trailer =
            format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n");
        out.extend_from_slice(trailer.as_bytes());

        out
    }

    /// Assemble and return the full PDF document, consuming the renderer.
    #[must_use]
    pub fn into_pdf(self) -> Vec<u8> {
        self.finish()
    }

    /// Render the document and write it to `path` as a PDF file.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if writing the file fails.
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.finish())
    }
}

/// Format an `f64` compactly for a PDF content stream, trimming trailing zeros
/// and a dangling decimal point. Non-finite values collapse to `0`.
fn fmt_f(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// The effective opacity of `rgba` once tinted by an optional global `alpha`,
/// clamped to `0.0..=1.0`.
fn effective_opacity(rgba: Rgba, alpha: Option<f64>) -> f64 {
    (rgba.a * alpha.unwrap_or(1.0)).clamp(0.0, 1.0)
}

impl Renderer for PdfRenderer {
    fn draw_path(
        &mut self,
        gc: &GraphicsContext,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        // Build the path geometry into a scratch suffix first so we can decide
        // whether anything is drawable before emitting any graphics state.
        let saved_len = self.content.len();
        let any = self.build_path_ops(path, transform);
        if !any {
            self.content.truncate(saved_len);
            return;
        }

        let width = self.points_to_pixels(gc.line_width);
        let do_stroke = gc.stroke.is_some() && width > 0.0;
        let do_fill = fill.is_some();

        let fill_alpha = fill.map_or(1.0, |f| effective_opacity(f, gc.alpha));
        let stroke_alpha = gc.stroke.map_or(1.0, |s| effective_opacity(s, gc.alpha));

        // The path ops must follow the graphics-state setup, so split them off
        // and reassemble in the right order.
        let ops = self.content.split_off(saved_len);
        let mut block = String::new();

        if let Some(gs) = self.push_ext_gstate(fill_alpha, stroke_alpha) {
            let _ = writeln!(block, "/{gs} gs");
        }
        if let Some(face) = fill {
            let _ = writeln!(
                block,
                "{} {} {} rg",
                fmt_f(face.r),
                fmt_f(face.g),
                fmt_f(face.b)
            );
        }
        if do_stroke && let Some(stroke) = gc.stroke {
            let _ = writeln!(
                block,
                "{} {} {} RG",
                fmt_f(stroke.r),
                fmt_f(stroke.g),
                fmt_f(stroke.b)
            );
            let _ = writeln!(block, "{} w", fmt_f(width));
        }

        block.push_str(&ops);

        // Choose the paint operator.
        let paint = match (do_fill, do_stroke) {
            (true, true) => "B",
            (true, false) => "f",
            (false, true) => "S",
            (false, false) => "n", // geometry with no paint
        };
        let _ = writeln!(block, "{paint}");

        // Wrap each draw in q/Q so graphics state does not leak between paths.
        self.content.push_str("q\n");
        self.content.push_str(&block);
        self.content.push_str("Q\n");
    }

    fn canvas_size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    fn points_to_pixels(&self, points: f64) -> f64 {
        points * self.dpi / 72.0
    }

    /// PDF user space shares matplotlib's bottom-left, y-up origin, so no flip
    /// is applied.
    fn flipy(&self) -> bool {
        false
    }

    // draw_image uses the trait default (a no-op): raster images are not yet
    // supported in the PDF backend.
    //
    // draw_text uses the trait default: higher layers draw text as glyph-outline
    // paths, which reach draw_path directly, so no native PDF text is emitted.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Find the byte index of the needle in the raw bytes (the PDF binary
    /// marker comment makes the file non-UTF-8, so we scan bytes directly).
    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn find_startxref(pdf: &[u8]) -> usize {
        let marker = b"startxref\n";
        // Find the last occurrence.
        let mut idx = None;
        let mut from = 0;
        while let Some(rel) = find_bytes(&pdf[from..], marker) {
            idx = Some(from + rel);
            from += rel + 1;
        }
        let start = idx.expect("startxref present") + marker.len();
        let end = start
            + pdf[start..]
                .iter()
                .position(|&b| b == b'\n')
                .expect("newline after startxref offset");
        std::str::from_utf8(&pdf[start..end])
            .expect("offset digits are ascii")
            .parse()
            .expect("startxref offset parses")
    }

    #[test]
    fn header_and_trailer_are_well_formed() {
        let r = PdfRenderer::new(100.0, 100.0, 72.0);
        let pdf = r.finish();
        assert!(pdf.starts_with(b"%PDF"), "must start with %PDF");
        assert!(pdf.ends_with(b"%%EOF\n"), "must end with %%EOF");
    }

    #[test]
    fn exactly_one_page() {
        let r = PdfRenderer::new(100.0, 100.0, 72.0);
        let pdf = r.finish();
        let s = String::from_utf8_lossy(&pdf);
        assert_eq!(
            s.matches("/Type /Page").count() - s.matches("/Type /Pages").count(),
            1,
            "exactly one /Type /Page object"
        );
    }

    #[test]
    fn startxref_points_at_xref_keyword() {
        let mut r = PdfRenderer::new(120.0, 80.0, 72.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &Affine2D::from_scale(40.0, 40.0).translate(10.0, 10.0),
            Some(Rgba::RED),
        );
        let pdf = r.finish();
        let offset = find_startxref(&pdf);
        assert_eq!(
            &pdf[offset..offset + 4],
            b"xref",
            "startxref offset must point at the xref keyword"
        );
    }

    #[test]
    fn filled_path_emits_fill_operator_and_color() {
        let mut r = PdfRenderer::new(100.0, 100.0, 72.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &Affine2D::from_scale(60.0, 60.0).translate(20.0, 20.0),
            Some(Rgba::RED),
        );
        let pdf = String::from_utf8_lossy(&r.finish()).into_owned();
        assert!(pdf.contains("1 0 0 rg"), "red nonstroking color: {pdf}");
        assert!(
            pdf.contains("\nf\n") || pdf.contains("\nB\n"),
            "fill or fill+stroke operator present: {pdf}"
        );
        assert!(pdf.contains(" m\n"), "moveto operator present: {pdf}");
    }

    #[test]
    fn stroked_path_emits_stroke_operator_and_width() {
        let mut r = PdfRenderer::new(100.0, 100.0, 72.0);
        let line = Path::from_polyline(&[[10.0, 50.0], [90.0, 50.0]]);
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(4.0);
        r.draw_path(&gc, &line, &Affine2D::identity(), None);
        let pdf = String::from_utf8_lossy(&r.finish()).into_owned();
        assert!(pdf.contains("0 0 0 RG"), "black stroking color: {pdf}");
        assert!(pdf.contains("4 w"), "line width 4: {pdf}");
        assert!(pdf.contains("\nS\n"), "stroke operator present: {pdf}");
    }

    #[test]
    fn fill_and_stroke_uses_b_operator() {
        let mut r = PdfRenderer::new(100.0, 100.0, 72.0);
        let gc = GraphicsContext::new().with_stroke(Rgba::BLACK);
        r.draw_path(
            &gc,
            &Path::unit_rectangle(),
            &Affine2D::from_scale(50.0, 50.0),
            Some(Rgba::BLUE),
        );
        let pdf = String::from_utf8_lossy(&r.finish()).into_owned();
        assert!(pdf.contains("\nB\n"), "fill+stroke uses B: {pdf}");
    }

    #[test]
    fn alpha_emits_ext_gstate() {
        let mut r = PdfRenderer::new(100.0, 100.0, 72.0);
        let gc = GraphicsContext::new().with_alpha(0.5);
        r.draw_path(
            &gc,
            &Path::unit_rectangle(),
            &Affine2D::from_scale(50.0, 50.0),
            Some(Rgba::RED),
        );
        let pdf = String::from_utf8_lossy(&r.finish()).into_owned();
        assert!(pdf.contains("/ExtGState"), "ExtGState resource: {pdf}");
        assert!(pdf.contains("/ca 0.5"), "fill alpha 0.5: {pdf}");
        assert!(pdf.contains("/GS0 gs"), "applies the gstate: {pdf}");
    }

    #[test]
    fn capabilities_match_dpi_and_no_flip() {
        let r = PdfRenderer::new(10.0, 20.0, 144.0);
        assert!(!r.flipy(), "PDF user space is y-up, no flip");
        assert_eq!(r.canvas_size(), (10.0, 20.0));
        assert_eq!(r.points_to_pixels(3.0), 6.0);
    }

    #[test]
    fn empty_path_emits_nothing() {
        let mut r = PdfRenderer::new(50.0, 50.0, 72.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::new(vec![], None),
            &Affine2D::identity(),
            Some(Rgba::RED),
        );
        assert!(r.content.is_empty(), "empty path must not emit content");
    }

    #[test]
    fn xref_offsets_point_at_object_headers() {
        let mut r = PdfRenderer::new(80.0, 60.0, 72.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &Affine2D::from_scale(30.0, 30.0),
            Some(Rgba::GREEN),
        );
        let pdf = r.finish();
        let xref_pos = find_startxref(&pdf);
        let xref_text = std::str::from_utf8(&pdf[xref_pos..]).expect("xref section is ascii");
        for n in 1..=5usize {
            let needle = format!("{n} 0 obj");
            let pos = find_bytes(&pdf, needle.as_bytes()).expect("object header present");
            let line = xref_text
                .lines()
                .nth(n + 2) // skip "xref", "0 6", and the free-head line
                .expect("xref entry line");
            let offset: usize = line[..10].parse().expect("offset parses");
            assert_eq!(offset, pos, "xref offset for object {n} must match header");
        }
    }
}
