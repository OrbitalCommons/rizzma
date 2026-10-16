//! SVG text written natively (`svg_fonttype = None`): every text run a
//! figure draws, rasterized with resvg in the bundled DejaVu Sans, lands
//! within a pixel of its glyph-outline rendering, and the characters reach
//! the document escaped.

use rizzma::core::color::Rgba;
use rizzma::core::{Affine2D, Path, RcParams, SvgFontType};
use rizzma::figure::HorizontalAlignment;
use rizzma::render::{GraphicsContext, PlainText, Renderer};
use rizzma::svg::SvgRenderer;
use rizzma::{Figure, text::font::dejavu_sans_ttf};

const LABEL: &str = "Δv [m/s] <5% & ±1σ";

/// A figure exercising every kind of text run: tick labels on both axes, a
/// rotated y label, a title, a legend, a suptitle, and free text at each
/// horizontal alignment and rotated.
fn figure() -> Figure {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[3.0, 1.0, 2.0]).set_label(LABEL);
    ax.set_title("title & <tag>")
        .set_xlabel(LABEL)
        .set_ylabel("rate (Hz)");
    ax.legend_auto();
    fig.suptitle("suptitle");
    for (x, ha) in [
        (0.2, HorizontalAlignment::Left),
        (0.5, HorizontalAlignment::Center),
        (0.8, HorizontalAlignment::Right),
    ] {
        fig.text(x, 0.6, "aligned").set_ha(ha);
    }
    fig.text(0.5, 0.4, "rotated").set_rotation(30.0);
    fig
}

/// Rasterize `svg` with resvg, resolving fonts to the bundled DejaVu Sans.
fn rasterize(svg: &str, width: u32, height: u32) -> resvg::tiny_skia::Pixmap {
    let mut options = resvg::usvg::Options::default();
    options
        .fontdb_mut()
        .load_font_data(dejavu_sans_ttf().to_vec());
    let tree = resvg::usvg::Tree::from_str(svg, &options).expect("svg parses");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).expect("pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap
}

/// The bounding box `[x0, y0, x1, y1]` of pixels with any ink, or `None`.
fn ink_box(pixmap: &resvg::tiny_skia::Pixmap) -> Option<[u32; 4]> {
    let w = pixmap.width();
    pixmap
        .pixels()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.alpha() > 64)
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .fold(None, |b, (x, y)| {
            let [x0, y0, x1, y1] = b.unwrap_or([x, y, x, y]);
            Some([x0.min(x), y0.min(y), x1.max(x), y1.max(y)])
        })
}

/// Renders each text run on its own, as outline and as native text, and
/// records both ink boxes. Everything else is dropped.
struct RunChecker {
    size: (f64, f64),
    dpi: f64,
    runs: Vec<(String, [u32; 4], [u32; 4])>,
}

impl Renderer for RunChecker {
    fn draw_path(&mut self, _: &GraphicsContext, _: &Path, _: &Affine2D, _: Option<Rgba>) {}

    fn draw_text_run(
        &mut self,
        gc: &GraphicsContext,
        outline: &Path,
        transform: &Affine2D,
        color: Rgba,
        run: &PlainText<'_>,
    ) {
        let (w, h) = self.size;
        let mut boxes = [SvgFontType::Path, SvgFontType::None].map(|fonttype| {
            let mut svg = SvgRenderer::new(w, h, self.dpi).with_fonttype(fonttype, "DejaVu Sans");
            svg.draw_text_run(gc, outline, transform, color, run);
            ink_box(&rasterize(&svg.finish(), w as u32, h as u32))
        });
        if let [Some(path), Some(native)] = boxes.each_mut().map(Option::take) {
            self.runs.push((run.text.to_owned(), path, native));
        }
    }

    fn canvas_size(&self) -> (f64, f64) {
        self.size
    }

    fn points_to_pixels(&self, points: f64) -> f64 {
        points * self.dpi / 72.0
    }

    fn decoration_scale(&self) -> f64 {
        self.dpi / 100.0
    }
}

#[test]
fn native_text_lands_within_a_pixel_of_its_outline() {
    let fig = figure();
    let mut checker = RunChecker {
        size: fig.size_px(),
        dpi: fig.dpi(),
        runs: Vec::new(),
    };
    fig.draw(&mut checker);
    let texts: Vec<&str> = checker.runs.iter().map(|(t, ..)| t.as_str()).collect();
    for expected in [
        LABEL,
        "rate (Hz)",
        "title & <tag>",
        "suptitle",
        "aligned",
        "rotated",
    ] {
        assert!(
            texts.contains(&expected),
            "no run {expected:?} in {texts:?}"
        );
    }
    for (text, path, native) in &checker.runs {
        for k in 0..4 {
            assert!(
                path[k].abs_diff(native[k]) <= 1,
                "{text:?}: outline ink {path:?}, native ink {native:?}"
            );
        }
    }
}

#[test]
fn native_text_is_escaped_and_searchable() {
    let mut fig = figure();
    let rc = RcParams {
        svg_fonttype: SvgFontType::None,
        ..RcParams::default()
    };
    fig.set_rcparams(rc);
    let svg = fig.to_svg();
    assert!(
        svg.contains(">Δv [m/s] &lt;5% &amp; ±1σ</text>"),
        "label not written as escaped text: {svg}"
    );
    assert!(svg.contains(">title &amp; &lt;tag&gt;</text>"), "{svg}");
    assert!(svg.contains("font-family=\"DejaVu Sans\""), "{svg}");
    assert!(svg.contains("text-anchor=\"middle\""), "{svg}");
    assert!(svg.contains("text-anchor=\"end\""), "{svg}");
    assert!(svg.contains("transform=\"matrix("), "rotated runs: {svg}");
    // The document still parses.
    let (w, h) = fig.size_px();
    rasterize(&svg, w as u32, h as u32);

    let paths = figure().to_svg();
    assert!(!paths.contains("<text"), "the default draws outlines");
    assert!(svg.len() < paths.len());
}
