//! SVG vector backend for rizzma.
//!
//! A second [`Renderer`] implementation alongside the tiny-skia raster backend,
//! proving the renderer seam is backend-agnostic. Instead of rasterizing, it
//! serializes every [`Path`] into hand-built SVG `<path>` markup.
//!
//! # Coordinate systems
//!
//! matplotlib's device space has its origin at the bottom-left with y growing
//! upward, while SVG's user space has its origin at the top-left with y growing
//! downward. As with the raster backend, every path is therefore mapped through
//! a *composite device transform*: first the caller's `transform` into
//! matplotlib device space, then a Y-flip `(x, y) -> (x, height - y)` into SVG
//! space. The flip is `Affine2D::from_scale(1, -1)` followed by a translation of
//! `height` in y.
//!
//! Build-order home: Phase 9 of `design/04-implementation-plan.md`.

use std::fmt::Write as _;

pub use crate::core::{Affine2D, Path, color::Rgba};
use base64::Engine as _;

use crate::core::PathSegment;
use crate::render::{CapStyle, GraphicsContext, JoinStyle, Renderer};

/// A vector [`Renderer`] that serializes paths into an SVG document.
///
/// Construct one with [`SvgRenderer::new`], draw into it via the [`Renderer`]
/// trait, then read the result with [`SvgRenderer::finish`] (which closes the
/// document) or persist it with [`SvgRenderer::save_svg`].
#[derive(Debug, Clone)]
pub struct SvgRenderer {
    /// The accumulated SVG body markup (between the opening `<svg>` and the
    /// closing `</svg>`).
    body: String,
    /// Deferred `<clipPath>` definitions, emitted inside a `<defs>` block by
    /// [`SvgRenderer::finish`].
    defs: String,
    /// Canvas width in device pixels.
    width: f64,
    /// Canvas height in device pixels.
    height: f64,
    /// Dots per inch, used to convert points to pixels.
    dpi: f64,
    /// Monotonic counter handing out unique `clipPath` element ids.
    clip_counter: usize,
}

impl SvgRenderer {
    /// Create a renderer over a `width_px` by `height_px` canvas, rendering at
    /// `dpi` dots per inch.
    ///
    /// This opens the SVG document buffer with an `<svg>` root carrying the
    /// canvas dimensions and a matching `viewBox`.
    #[must_use]
    pub fn new(width_px: f64, height_px: f64, dpi: f64) -> Self {
        Self {
            body: String::new(),
            defs: String::new(),
            width: width_px,
            height: height_px,
            dpi,
            clip_counter: 0,
        }
    }

    /// The DPI this renderer scales points to pixels with.
    #[must_use]
    pub fn dpi(&self) -> f64 {
        self.dpi
    }

    /// The Y-flip taking matplotlib device space (origin bottom-left) into SVG
    /// user space (origin top-left): `(x, y) -> (x, height - y)`.
    fn flip_transform(&self) -> Affine2D {
        Affine2D::from_scale(1.0, -1.0).translate(0.0, self.height)
    }

    /// Build the SVG `d` attribute for `path`, mapping every anchor and control
    /// point through `device` (the composite transform into SVG user space).
    ///
    /// Returns `None` when the path yields no drawable segments.
    fn build_path_data(path: &Path, device: &Affine2D) -> Option<String> {
        let map = |p: [f64; 2]| -> (f64, f64) { device.transform_point((p[0], p[1])) };
        let mut d = String::new();
        let mut any = false;
        for seg in path.iter_segments() {
            if !d.is_empty() {
                d.push(' ');
            }
            match seg {
                PathSegment::MoveTo(p) => {
                    let (x, y) = map(p);
                    let _ = write!(d, "M {} {}", fmt_f(x), fmt_f(y));
                    any = true;
                }
                PathSegment::LineTo(p) => {
                    let (x, y) = map(p);
                    let _ = write!(d, "L {} {}", fmt_f(x), fmt_f(y));
                    any = true;
                }
                PathSegment::Quad(c, e) => {
                    let (cx, cy) = map(c);
                    let (ex, ey) = map(e);
                    let _ = write!(
                        d,
                        "Q {} {} {} {}",
                        fmt_f(cx),
                        fmt_f(cy),
                        fmt_f(ex),
                        fmt_f(ey)
                    );
                    any = true;
                }
                PathSegment::Cubic(c1, c2, e) => {
                    let (c1x, c1y) = map(c1);
                    let (c2x, c2y) = map(c2);
                    let (ex, ey) = map(e);
                    let _ = write!(
                        d,
                        "C {} {} {} {} {} {}",
                        fmt_f(c1x),
                        fmt_f(c1y),
                        fmt_f(c2x),
                        fmt_f(c2y),
                        fmt_f(ex),
                        fmt_f(ey)
                    );
                    any = true;
                }
                PathSegment::Close => d.push('Z'),
            }
        }
        if any { Some(d) } else { None }
    }

    /// Register a `<clipPath>` def for `gc.clip_rect` (in matplotlib device
    /// coordinates), Y-flipped into SVG user space, returning its id.
    ///
    /// Returns `None` when there is no clip rectangle.
    fn push_clip_def(&mut self, gc: &GraphicsContext) -> Option<String> {
        // TODO: clip_path — honor gc.clip_path for arbitrary-path clipping.
        let bbox = gc.clip_rect?;
        let left = bbox.xmin();
        let width = bbox.width();
        // Y-flip the rectangle: its top edge in SVG space is height - ymax.
        let top = self.height - bbox.ymax();
        let height = bbox.height();
        let id = format!("clip{}", self.clip_counter);
        self.clip_counter += 1;
        let _ = write!(
            self.defs,
            "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
            fmt_f(left),
            fmt_f(top),
            fmt_f(width),
            fmt_f(height)
        );
        Some(id)
    }

    /// Close the SVG document and return the full markup as a string.
    ///
    /// Any deferred `<clipPath>` definitions are emitted in a `<defs>` block
    /// before the body, and the root `</svg>` tag is appended.
    #[must_use]
    pub fn finish(self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            fmt_f(self.width),
            fmt_f(self.height),
            fmt_f(self.width),
            fmt_f(self.height)
        );
        if !self.defs.is_empty() {
            out.push_str("<defs>");
            out.push_str(&self.defs);
            out.push_str("</defs>");
        }
        out.push_str(&self.body);
        out.push_str("</svg>");
        out
    }

    /// Render the document and write it to `path` as an SVG file.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if writing the file fails.
    pub fn save_svg<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.clone().finish())
    }
}

/// Format an [`Rgba`]'s color channels as an SVG `rgb(r,g,b)` function string.
fn rgb_func(rgba: Rgba) -> String {
    let [r, g, b, _] = rgba.to_u8_array();
    format!("rgb({r},{g},{b})")
}

/// The effective opacity of `rgba` once tinted by an optional global `alpha`,
/// clamped to `0.0..=1.0`.
fn effective_opacity(rgba: Rgba, alpha: Option<f64>) -> f64 {
    (rgba.a * alpha.unwrap_or(1.0)).clamp(0.0, 1.0)
}

/// Map a seam [`CapStyle`] to the SVG `stroke-linecap` keyword.
fn cap_keyword(cap: CapStyle) -> &'static str {
    match cap {
        CapStyle::Butt => "butt",
        CapStyle::Round => "round",
        CapStyle::Projecting => "square",
    }
}

/// Map a seam [`JoinStyle`] to the SVG `stroke-linejoin` keyword.
fn join_keyword(join: JoinStyle) -> &'static str {
    match join {
        JoinStyle::Miter => "miter",
        JoinStyle::Round => "round",
        JoinStyle::Bevel => "bevel",
    }
}

/// Format an `f64` compactly, trimming trailing zeros and a dangling decimal
/// point so coordinates like `12.0` render as `12` and `1.5000` as `1.5`.
///
/// Non-finite values collapse to `0` so the emitted markup stays well-formed.
fn fmt_f(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    // Six decimal places is ample sub-pixel precision for device coordinates.
    let mut s = format!("{v:.6}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    // Normalize a possible "-0" produced by trimming.
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// Encode straight RGBA8 pixels as a PNG byte vector.
///
/// Returns `None` for an invalid buffer shape or an encoder error. The renderer
/// treats that as a no-op so a malformed image cannot corrupt the SVG document.
fn encode_rgba_png(rgba: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        return None;
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(bytes)
}

impl Renderer for SvgRenderer {
    fn draw_path(
        &mut self,
        gc: &GraphicsContext,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        let device = transform.then(&self.flip_transform());
        let Some(d) = Self::build_path_data(path, &device) else {
            return;
        };

        let mut attrs = String::new();
        let _ = write!(attrs, "d=\"{d}\"");

        // Fill.
        match fill {
            Some(face) => {
                let _ = write!(attrs, " fill=\"{}\"", rgb_func(face));
                let opacity = effective_opacity(face, gc.alpha);
                if opacity < 1.0 {
                    let _ = write!(attrs, " fill-opacity=\"{}\"", fmt_f(opacity));
                }
            }
            None => attrs.push_str(" fill=\"none\""),
        }

        // Stroke.
        let width = self.points_to_pixels(gc.line_width);
        if let Some(stroke) = gc.stroke
            && width > 0.0
        {
            let _ = write!(attrs, " stroke=\"{}\"", rgb_func(stroke));
            let opacity = effective_opacity(stroke, gc.alpha);
            if opacity < 1.0 {
                let _ = write!(attrs, " stroke-opacity=\"{}\"", fmt_f(opacity));
            }
            let _ = write!(attrs, " stroke-width=\"{}\"", fmt_f(width));
            if let Some((offset, pattern)) = &gc.dashes
                && !pattern.is_empty()
            {
                let scale = self.dpi / 72.0;
                let array: Vec<String> = pattern.iter().map(|d| fmt_f(d * scale)).collect();
                let _ = write!(attrs, " stroke-dasharray=\"{}\"", array.join(","));
                if *offset != 0.0 {
                    let _ = write!(attrs, " stroke-dashoffset=\"{}\"", fmt_f(offset * scale));
                }
            }
            let _ = write!(attrs, " stroke-linecap=\"{}\"", cap_keyword(gc.cap));
            let _ = write!(attrs, " stroke-linejoin=\"{}\"", join_keyword(gc.join));
        }

        // Wrap in a clip group when a clip rectangle is set.
        if let Some(clip_id) = self.push_clip_def(gc) {
            let _ = write!(
                self.body,
                "<g clip-path=\"url(#{clip_id})\"><path {attrs}/></g>"
            );
        } else {
            let _ = write!(self.body, "<path {attrs}/>");
        }
    }

    fn canvas_size(&self) -> (f64, f64) {
        (self.width, self.height)
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

    fn draw_image(
        &mut self,
        gc: &GraphicsContext,
        x: f64,
        y: f64,
        rgba: &[u8],
        width: usize,
        height: usize,
    ) {
        let Some(png) = encode_rgba_png(rgba, width, height) else {
            return;
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let img_h = height as f64;
        let top = self.height - y - img_h;
        let mut attrs = String::new();
        let _ = write!(
            attrs,
            "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:image/png;base64,{encoded}\"",
            fmt_f(x),
            fmt_f(top),
            fmt_f(width as f64),
            fmt_f(img_h)
        );
        if let Some(alpha) = gc.alpha {
            let opacity = alpha.clamp(0.0, 1.0);
            if opacity < 1.0 {
                let _ = write!(attrs, " opacity=\"{}\"", fmt_f(opacity));
            }
        }

        if let Some(clip_id) = self.push_clip_def(gc) {
            let _ = write!(
                self.body,
                "<g clip-path=\"url(#{clip_id})\"><image {attrs}/></g>"
            );
        } else {
            let _ = write!(self.body, "<image {attrs}/>");
        }
    }

    // draw_text uses the trait default for now: higher layers draw text as
    // paths, so no native SVG `<text>` is emitted yet.
    // TODO: emit optional native `<text>`.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bbox;

    fn render_svg(svg: &str, width: u32, height: u32) -> resvg::tiny_skia::Pixmap {
        let tree =
            resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).expect("svg parses");
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).expect("pixmap allocates");
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    fn pixel(pixmap: &resvg::tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pixmap.pixel(x, y).expect("pixel in bounds");
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn filled_rectangle_emits_path_and_fill() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let transform = Affine2D::from_scale(60.0, 60.0).translate(20.0, 20.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &transform,
            Some(Rgba::RED),
        );
        let svg = r.finish();
        assert!(svg.contains("<svg"), "missing <svg root: {svg}");
        assert!(svg.contains("<path "), "missing <path element: {svg}");
        assert!(
            svg.contains("d=\"M "),
            "path data should start with M: {svg}"
        );
        assert!(svg.contains('Z'), "closed rect should contain Z: {svg}");
        assert!(svg.contains("fill=\"rgb("), "missing rgb fill: {svg}");
        assert!(svg.ends_with("</svg>"), "should close with </svg>: {svg}");
    }

    #[test]
    fn stroked_polyline_emits_stroke_attrs() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let line = Path::from_polyline(&[[10.0, 50.0], [90.0, 50.0]]);
        let gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(4.0);
        r.draw_path(&gc, &line, &Affine2D::identity(), None);
        let svg = r.finish();
        assert!(svg.contains("<path "), "missing <path: {svg}");
        assert!(svg.contains("stroke=\""), "missing stroke: {svg}");
        assert!(
            svg.contains("stroke-width=\""),
            "missing stroke-width: {svg}"
        );
        assert!(svg.contains("fill=\"none\""), "unfilled path: {svg}");
    }

    #[test]
    fn dashed_line_includes_dasharray() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let line = Path::from_polyline(&[[10.0, 50.0], [90.0, 50.0]]);
        let gc = GraphicsContext {
            dashes: Some((0.0, vec![4.0, 2.0])),
            ..GraphicsContext::new().with_stroke(Rgba::BLACK)
        };
        r.draw_path(&gc, &line, &Affine2D::identity(), None);
        let svg = r.finish();
        assert!(
            svg.contains("stroke-dasharray=\""),
            "missing dasharray: {svg}"
        );
    }

    #[test]
    fn document_is_well_formed() {
        let mut r = SvgRenderer::new(64.0, 48.0, 96.0);
        r.draw_path(
            &GraphicsContext::new(),
            &Path::unit_rectangle(),
            &Affine2D::from_scale(64.0, 48.0),
            Some(Rgba::BLUE),
        );
        let svg = r.finish();
        assert_eq!(svg.matches("<svg").count(), 1, "exactly one <svg: {svg}");
        assert_eq!(
            svg.matches("</svg>").count(),
            1,
            "exactly one </svg>: {svg}"
        );
        // The first path's d attribute must start with a moveto.
        let d_start = svg.find("d=\"").expect("a path with d");
        assert_eq!(&svg[d_start + 3..d_start + 4], "M", "d must start with M");
    }

    #[test]
    fn clip_rect_wraps_in_group_with_def() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let gc = GraphicsContext {
            clip_rect: Some(Bbox::from_extents(40.0, 40.0, 60.0, 60.0)),
            ..GraphicsContext::new()
        };
        r.draw_path(
            &gc,
            &Path::unit_rectangle(),
            &Affine2D::from_scale(100.0, 100.0),
            Some(Rgba::RED),
        );
        let svg = r.finish();
        assert!(
            svg.contains("<clipPath id=\"clip0\""),
            "missing clipPath: {svg}"
        );
        assert!(
            svg.contains("clip-path=\"url(#clip0)\""),
            "missing clip group: {svg}"
        );
        assert!(
            svg.contains("<defs>"),
            "clip def must live in <defs>: {svg}"
        );
    }

    #[test]
    fn fmt_f_trims_trailing_zeros() {
        assert_eq!(fmt_f(12.0), "12");
        assert_eq!(fmt_f(1.5), "1.5");
        assert_eq!(fmt_f(-0.0), "0");
        assert_eq!(fmt_f(f64::NAN), "0");
    }

    #[test]
    fn capabilities_match_dpi() {
        let r = SvgRenderer::new(10.0, 20.0, 144.0);
        assert!(r.flipy());
        assert_eq!(r.canvas_size(), (10.0, 20.0));
        assert_eq!(r.points_to_pixels(3.0), 6.0);
    }

    #[test]
    fn draw_image_embeds_png_data_uri_with_y_flip() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, // top row
            0, 0, 255, 255, 255, 255, 0, 255, // bottom row
        ];
        r.draw_image(&GraphicsContext::new(), 10.0, 20.0, &rgba, 2, 2);
        let svg = r.finish();

        assert!(svg.contains("<image "), "missing image: {svg}");
        assert!(
            svg.contains("href=\"data:image/png;base64,"),
            "missing PNG data URI: {svg}"
        );
        assert!(svg.contains("x=\"10\""), "wrong x: {svg}");
        assert!(svg.contains("y=\"78\""), "wrong y-flip: {svg}");
        assert!(svg.contains("width=\"2\""), "wrong width: {svg}");
        assert!(svg.contains("height=\"2\""), "wrong height: {svg}");
    }

    #[test]
    fn draw_image_honors_alpha_and_clip_rect() {
        let mut r = SvgRenderer::new(20.0, 20.0, 72.0);
        let rgba = vec![255, 0, 0, 255];
        let gc = GraphicsContext {
            alpha: Some(0.5),
            clip_rect: Some(Bbox::from_extents(1.0, 2.0, 3.0, 4.0)),
            ..GraphicsContext::new()
        };
        r.draw_image(&gc, 1.0, 2.0, &rgba, 1, 1);
        let svg = r.finish();

        assert!(svg.contains("opacity=\"0.5\""), "missing opacity: {svg}");
        assert!(
            svg.contains("<clipPath id=\"clip0\""),
            "missing clipPath: {svg}"
        );
        assert!(
            svg.contains("<g clip-path=\"url(#clip0)\"><image "),
            "image should be clipped: {svg}"
        );
    }

    #[test]
    fn draw_image_rejects_bad_buffer_shape() {
        let mut r = SvgRenderer::new(20.0, 20.0, 72.0);
        r.draw_image(&GraphicsContext::new(), 0.0, 0.0, &[255, 0, 0], 1, 1);
        let svg = r.finish();

        assert!(
            !svg.contains("<image "),
            "bad buffer should be ignored: {svg}"
        );
    }

    #[test]
    fn draw_image_rasterizes_at_y_flipped_position() {
        let mut r = SvgRenderer::new(6.0, 6.0, 72.0);
        let rgba = vec![255, 0, 0, 255];
        r.draw_image(&GraphicsContext::new(), 2.0, 1.0, &rgba, 1, 1);

        let pixmap = render_svg(&r.finish(), 6, 6);

        assert_eq!(pixel(&pixmap, 2, 4), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 2, 1), [0, 0, 0, 0]);
    }
}
