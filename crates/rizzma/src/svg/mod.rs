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

use crate::core::{PathSegment, SvgFontType};
use crate::render::{CapStyle, GraphicsContext, JoinStyle, PlainText, Renderer, TextAnchor};

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
    /// Deferred `<clipPath>` and marker definitions, emitted inside a
    /// `<defs>` block by [`SvgRenderer::finish`].
    defs: String,
    /// Canvas width in device pixels.
    width: f64,
    /// Canvas height in device pixels.
    height: f64,
    /// Dots per inch, used to convert points to pixels.
    dpi: f64,
    /// The rectangle of each `clipPath` def, as written; `clipN` is entry `N`,
    /// so artists sharing a clip rectangle share one def.
    clips: Vec<String>,
    /// The attributes of each marker `<path>` def; `mN` is entry `N`.
    markers: Vec<String>,
    /// The font family text runs are written in as native `<text>`, or
    /// `None` to draw them as glyph outlines.
    text_family: Option<String>,
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
            clips: Vec::new(),
            markers: Vec::new(),
            text_family: None,
        }
    }

    /// Write text as `fonttype` says: glyph outlines (the default), or native
    /// `<text>` in `font_family`.
    #[must_use]
    pub fn with_fonttype(mut self, fonttype: SvgFontType, font_family: &str) -> Self {
        self.text_family = match fonttype {
            SvgFontType::Path => None,
            SvgFontType::None => Some(font_family.to_owned()),
        };
        self
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

    /// The `clip-path` attribute, with a leading space, clipping an element
    /// to `gc.clip_rect` (in matplotlib device coordinates, Y-flipped into SVG
    /// user space); empty when there is no clip rectangle.
    ///
    /// A rectangle already seen reuses its `<clipPath>` def. Each element
    /// carries its own reference rather than sharing a clipped `<g>`, so it
    /// is composited exactly as a lone clipped element is.
    fn clip_attr(&mut self, gc: &GraphicsContext) -> String {
        // TODO: clip_path — honor gc.clip_path for arbitrary-path clipping.
        let Some(bbox) = gc.clip_rect else {
            return String::new();
        };
        // Y-flip the rectangle: its top edge in SVG space is height - ymax.
        let rect = format!(
            "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            fmt_f(bbox.xmin()),
            fmt_f(self.height - bbox.ymax()),
            fmt_f(bbox.width()),
            fmt_f(bbox.height())
        );
        let id = self
            .clips
            .iter()
            .position(|r| *r == rect)
            .unwrap_or_else(|| {
                let id = self.clips.len();
                let _ = write!(
                    self.defs,
                    "<clipPath id=\"clip{id}\"><rect {rect}/></clipPath>"
                );
                self.clips.push(rect);
                id
            });
        format!(" clip-path=\"url(#clip{id})\"")
    }

    /// The `fill` and `stroke` attributes painting a path with `gc` and the
    /// face color `fill`, each with a leading space.
    fn paint_attrs(&self, gc: &GraphicsContext, fill: Option<Rgba>) -> String {
        let mut attrs = String::new();
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

        let width = self.points_to_pixels(gc.line_width);
        if let Some(stroke) = gc.stroke.filter(|_| width > 0.0) {
            let _ = write!(attrs, " stroke=\"{}\"", rgb_func(stroke));
            let opacity = effective_opacity(stroke, gc.alpha);
            if opacity < 1.0 {
                let _ = write!(attrs, " stroke-opacity=\"{}\"", fmt_f(opacity));
            }
            let _ = write!(attrs, " stroke-width=\"{}\"", fmt_f(width));
            if let Some((offset, pattern)) = gc
                .dashes
                .as_ref()
                .filter(|(_, pattern)| !pattern.is_empty())
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
            if let Some(limit) = gc.miter_limit {
                let _ = write!(attrs, " stroke-miterlimit=\"{}\"", fmt_f(limit));
            }
        }
        attrs
    }

    /// Close the SVG document and return the full markup as a string.
    ///
    /// Any deferred `<clipPath>` and marker definitions are emitted in a
    /// `<defs>` block before the body, and the root `</svg>` tag is appended.
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

/// Escape `text` for XML character data and double-quoted attribute values.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
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

/// Encode `bytes` as standard-alphabet, padded base64 (RFC 4648), for a
/// `data:` URI.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

        let attrs = self.paint_attrs(gc, fill);
        let clip = self.clip_attr(gc);
        let _ = write!(self.body, "<path d=\"{d}\"{attrs}{clip}/>");
    }

    /// Write the run as a native `<text>` element when the renderer writes
    /// text natively, or fill its outline otherwise.
    fn draw_text_run(
        &mut self,
        gc: &GraphicsContext,
        outline: &Path,
        transform: &Affine2D,
        color: Rgba,
        run: &PlainText<'_>,
    ) {
        let Some(family) = &self.text_family else {
            self.draw_path(gc, outline, transform, Some(color));
            return;
        };
        if run.text.is_empty() {
            return;
        }
        let mut attrs = String::new();
        // The run's frame into SVG space; SVG text runs y down from its
        // baseline, where the frame's y runs up.
        let frame = Affine2D::from_scale(1.0, -1.0)
            .then(&run.frame)
            .then(&self.flip_transform());
        let x = match run.anchor {
            TextAnchor::Start => 0.0,
            TextAnchor::Middle => run.width / 2.0,
            TextAnchor::End => run.width,
        };
        let translated = matches!(frame.matrix(), [1.0, 0.0, 0.0, 1.0, _, _]);
        match frame.matrix() {
            [_, _, _, _, e, f] if translated => {
                let _ = write!(attrs, "x=\"{}\" y=\"{}\"", fmt_f(e + x), fmt_f(f));
            }
            [a, b, c, d, e, f] => {
                if x != 0.0 {
                    let _ = write!(attrs, "x=\"{}\" ", fmt_f(x));
                }
                let _ = write!(
                    attrs,
                    "transform=\"matrix({} {} {} {} {} {})\"",
                    fmt_f(a),
                    fmt_f(b),
                    fmt_f(c),
                    fmt_f(d),
                    fmt_f(e),
                    fmt_f(f)
                );
            }
        }
        let _ = write!(
            attrs,
            " font-family=\"{}\" font-size=\"{}\" fill=\"{}\"",
            xml_escape(family),
            fmt_f(run.font_size_px),
            rgb_func(color)
        );
        let opacity = effective_opacity(color, gc.alpha);
        if opacity < 1.0 {
            let _ = write!(attrs, " fill-opacity=\"{}\"", fmt_f(opacity));
        }
        match run.anchor {
            TextAnchor::Start => {}
            TextAnchor::Middle => attrs.push_str(" text-anchor=\"middle\""),
            TextAnchor::End => attrs.push_str(" text-anchor=\"end\""),
        }
        if run.text.starts_with(' ') || run.text.ends_with(' ') || run.text.contains("  ") {
            attrs.push_str(" xml:space=\"preserve\"");
        }
        let text = xml_escape(run.text);
        let clip = self.clip_attr(gc);
        // A clip on an element with a `transform` is read in the transformed
        // space, so a transformed run is clipped by a wrapping group.
        if clip.is_empty() || translated {
            let _ = write!(self.body, "<text {attrs}{clip}>{text}</text>");
        } else {
            let _ = write!(self.body, "<g{clip}><text {attrs}>{text}</text></g>");
        }
    }

    /// Write an unstroked marker once as a `<path>` def and place it at each
    /// vertex with a `<use>`, as matplotlib's SVG backend does. A stroked
    /// marker is written out at each vertex: renderers stroke a translated
    /// `<use>` differently from the same outline at its final position, so
    /// sharing it would change the rasterized image.
    fn draw_markers(
        &mut self,
        gc: &GraphicsContext,
        marker_path: &Path,
        marker_transform: &Affine2D,
        path: &Path,
        transform: &Affine2D,
        fill: Option<Rgba>,
    ) {
        if gc.stroke.is_some() && gc.line_width > 0.0 {
            for &[vx, vy] in path.vertices() {
                let (dx, dy) = transform.transform_point((vx, vy));
                let placed = marker_transform.then(&Affine2D::from_translation(dx, dy));
                self.draw_path(gc, marker_path, &placed, fill);
            }
            return;
        }
        // The marker in SVG space about its own origin: y flips, and each
        // `<use>` translates it to its flipped vertex.
        let local = marker_transform.then(&Affine2D::from_scale(1.0, -1.0));
        let Some(d) = Self::build_path_data(marker_path, &local) else {
            return;
        };
        let def = format!("d=\"{d}\"{}", self.paint_attrs(gc, fill));
        let id = self
            .markers
            .iter()
            .position(|m| *m == def)
            .unwrap_or_else(|| {
                let id = self.markers.len();
                let _ = write!(self.defs, "<path id=\"m{id}\" {def}/>");
                self.markers.push(def);
                id
            });
        // A `<use>` translates its own clip along with the marker, so clipped
        // markers sit in clipped `<g>`s. A group is composited as one layer,
        // which matches compositing its markers one by one only where no two
        // of them share a pixel: a marker whose ink could touch another's in
        // the open group starts a new group. Groups are capped so the check
        // stays linear.
        const GROUP_CAP: usize = 64;
        let reach = marker_path
            .transformed(marker_transform)
            .get_extents()
            .padded(1.0);
        let clip = self.clip_attr(gc);
        let mut group: Vec<[f64; 4]> = Vec::new();
        for &[vx, vy] in path.vertices() {
            let (x, y) = transform.transform_point((vx, vy));
            if !clip.is_empty() {
                let ink = [
                    x + reach.xmin(),
                    y + reach.ymin(),
                    x + reach.xmax(),
                    y + reach.ymax(),
                ];
                let touches =
                    |b: &[f64; 4]| b[0] < ink[2] && ink[0] < b[2] && b[1] < ink[3] && ink[1] < b[3];
                if group.len() == GROUP_CAP || group.iter().any(touches) {
                    self.body.push_str("</g>");
                    group.clear();
                }
                if group.is_empty() {
                    let _ = write!(self.body, "<g{clip}>");
                }
                group.push(ink);
            }
            let _ = write!(
                self.body,
                "<use href=\"#m{id}\" x=\"{}\" y=\"{}\"/>",
                fmt_f(x),
                fmt_f(self.height - y)
            );
        }
        if !group.is_empty() {
            self.body.push_str("</g>");
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
        let encoded = base64_encode(&png);
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

        let clip = self.clip_attr(gc);
        let _ = write!(self.body, "<image {attrs}{clip}/>");
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
    fn nan_in_line_data_lifts_the_pen() {
        // Through the full figure pipeline: a NaN sample must yield a second
        // `M` (moveto) in the line's path data, not a segment across the gap.
        use crate::artist::Line2D;
        use crate::figure::Figure;
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.set_axis_off();
        ax.add_line(Line2D::new(
            vec![0.0, 1.0, 2.0, 3.0, 4.0],
            vec![0.0, 1.0, f64::NAN, 3.0, 4.0],
        ));
        let svg = fig.to_svg();
        let line_path = svg
            .split("<path ")
            .find(|p| p.contains("fill=\"none\"") && p.contains("stroke=\""))
            .expect("a stroked line path");
        let d = line_path
            .split("d=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("path data");
        assert_eq!(d.matches('M').count(), 2, "expected two subpaths in {d}");
        assert_eq!(
            d.matches('L').count(),
            2,
            "expected one lineto per run in {d}"
        );
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
    fn clip_rect_references_a_def() {
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
            "missing clip reference: {svg}"
        );
        assert!(
            svg.contains("<defs>"),
            "clip def must live in <defs>: {svg}"
        );
    }

    #[test]
    fn identical_clip_rects_share_one_def() {
        let mut r = SvgRenderer::new(100.0, 100.0, 72.0);
        let clip = |x0: f64| GraphicsContext {
            clip_rect: Some(Bbox::from_extents(x0, 10.0, 90.0, 90.0)),
            ..GraphicsContext::new()
        };
        for gc in [clip(10.0), clip(10.0), clip(20.0), clip(10.0)] {
            r.draw_path(
                &gc,
                &Path::unit_rectangle(),
                &Affine2D::from_scale(50.0, 50.0),
                Some(Rgba::RED),
            );
        }
        let svg = r.finish();
        assert_eq!(svg.matches("<clipPath ").count(), 2, "{svg}");
        assert_eq!(svg.matches("url(#clip0)").count(), 3, "{svg}");
        assert_eq!(svg.matches("url(#clip1)").count(), 1, "{svg}");
    }

    /// Translucent scattered markers, some overlapping and some crossing the
    /// clip edge, edged with `stroke` if given.
    fn draw_scatter(r: &mut dyn Renderer, stroke: Option<Rgba>) {
        let gc = GraphicsContext {
            clip_rect: Some(Bbox::from_extents(10.5, 12.25, 110.0, 70.0)),
            line_width: 1.5,
            stroke,
            ..GraphicsContext::new()
        };
        let points: Vec<[f64; 2]> = (0..40)
            .map(|i| {
                let t = f64::from(i);
                [
                    8.0 + t * 2.7 + (t * 1.3).sin() * 9.0,
                    40.0 + (t * 0.7).cos() * 33.0,
                ]
            })
            .collect();
        r.draw_markers(
            &gc,
            &Path::unit_circle(),
            &Affine2D::from_scale(4.5, 4.5),
            &Path::from_polyline(&points),
            &Affine2D::identity(),
            Some(Rgba::new(0.1, 0.4, 0.9, 0.6)),
        );
    }

    /// Draws as [`SvgRenderer`] did before markers became `<use>`s: the
    /// trait's per-point `draw_path` fallback.
    struct PerPoint(SvgRenderer);

    impl Renderer for PerPoint {
        fn draw_path(
            &mut self,
            gc: &GraphicsContext,
            path: &Path,
            transform: &Affine2D,
            fill: Option<Rgba>,
        ) {
            self.0.draw_path(gc, path, transform, fill);
        }

        fn canvas_size(&self) -> (f64, f64) {
            self.0.canvas_size()
        }
    }

    #[test]
    fn unstroked_markers_are_one_def_placed_by_use() {
        let mut r = SvgRenderer::new(120.0, 80.0, 72.0);
        draw_scatter(&mut r, None);
        draw_scatter(&mut r, None);
        let svg = r.finish();
        assert_eq!(svg.matches("<path id=\"m0\"").count(), 1, "{svg}");
        assert!(!svg.contains("id=\"m1\""), "same marker, same def: {svg}");
        assert_eq!(svg.matches("<use href=\"#m0\"").count(), 80, "{svg}");
        assert_eq!(svg.matches("<clipPath ").count(), 1, "{svg}");
    }

    #[test]
    fn stroked_markers_are_written_per_point() {
        let mut r = SvgRenderer::new(120.0, 80.0, 72.0);
        draw_scatter(&mut r, Some(Rgba::BLACK));
        let svg = r.finish();
        assert!(!svg.contains("<use "), "{svg}");
        assert_eq!(svg.matches("<path ").count(), 40, "{svg}");
    }

    #[test]
    fn markers_rasterize_exactly_as_per_point_paths() {
        for stroke in [None, Some(Rgba::new(0.0, 0.0, 0.0, 0.7))] {
            let mut ours = SvgRenderer::new(120.0, 80.0, 72.0);
            draw_scatter(&mut ours, stroke);
            let mut paths = PerPoint(SvgRenderer::new(120.0, 80.0, 72.0));
            draw_scatter(&mut paths, stroke);
            let (a, b) = (
                render_svg(&ours.finish(), 120, 80),
                render_svg(&paths.0.finish(), 120, 80),
            );
            assert!(a.data() == b.data(), "stroke {stroke:?}: rasters differ");
        }
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (input, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input.as_bytes()), encoded);
        }
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[cfg(feature = "portable")]
    #[test]
    fn base64_matches_the_base64_crate() {
        use base64::Engine as _;
        let bytes: Vec<u8> = (0..=255).chain((0..=255).rev()).collect();
        for len in 0..bytes.len() {
            assert_eq!(
                base64_encode(&bytes[..len]),
                base64::engine::general_purpose::STANDARD.encode(&bytes[..len])
            );
        }
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
            svg.contains("<image ") && svg.contains("clip-path=\"url(#clip0)\"/>"),
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
