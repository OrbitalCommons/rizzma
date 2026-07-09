//! Artists must clip to the axes frame (matplotlib's `clip_on=True` default).
//!
//! A zoomed view — explicit limits tighter than the data — used to spill
//! lines across the whole canvas: over the margins, the title, and the tick
//! labels. These tests pin the fix on every backend: colored artist ink stays
//! inside the frame in the raster output, and the vector backends carry a
//! real clip (an SVG `<clipPath>`, a PDF `W n` operator) for every artist.

use rizzma::Figure;
use rizzma::pdf::PdfRenderer;
use rizzma::svg::SvgRenderer;

/// The demo-site damped oscillation, zoomed into a subrange so both lines
/// extend well beyond the limits on every side.
fn zoomed_figure() -> Figure {
    let n = 400;
    let mut xs = vec![0.0; n];
    let mut damped = vec![0.0; n];
    let mut envelope = vec![0.0; n];
    for i in 0..n {
        let x = 12.0 * i as f64 / (n - 1) as f64;
        xs[i] = x;
        envelope[i] = (-x / 4.0).exp();
        damped[i] = envelope[i] * (2.0 * x).cos();
    }
    let mut fig = Figure::new(4.8, 3.4);
    // An explicit rect so the frame's pixel extents are known exactly.
    let ax = fig.add_axes(0.15, 0.15, 0.70, 0.70);
    ax.plot(&xs, &damped);
    ax.plot(&xs, &envelope);
    ax.set_title("clipped");
    ax.set_xlim(4.0, 6.0);
    ax.set_ylim(-0.2, 0.2);
    fig
}

/// Whether a straight-RGBA pixel is *colored* (saturated hue) rather than the
/// grayscale ink of text, ticks, and the frame.
fn is_colored(r: u8, g: u8, b: u8, a: u8) -> bool {
    if a < 200 {
        return false;
    }
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    max - min > 40
}

#[test]
fn zoomed_lines_stay_inside_the_frame() {
    let fig = zoomed_figure();
    let renderer = fig.render();
    let px = renderer.pixmap();
    let (w, h) = (px.width(), px.height());
    assert_eq!((w, h), (480, 340));

    // The frame in pixmap (top-down) coordinates: x in [72, 408],
    // y-up [51, 289] flips to rows [51, 289].
    let (fx0, fx1) = (72i64, 408i64);
    let (fy0, fy1) = (51i64, 289i64);
    let margin = 2; // antialiasing halo along the clip edge

    let mut inside = 0u32;
    let mut outside = 0u32;
    for y in 0..h {
        for x in 0..w {
            let p = px.pixel(x, y).unwrap().demultiply();
            if !is_colored(p.red(), p.green(), p.blue(), p.alpha()) {
                continue;
            }
            let (xi, yi) = (i64::from(x), i64::from(y));
            if xi >= fx0 - margin && xi <= fx1 + margin && yi >= fy0 - margin && yi <= fy1 + margin
            {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(
        inside > 500,
        "the zoomed lines must still draw: {inside} px"
    );
    assert_eq!(
        outside, 0,
        "no colored artist ink may escape the axes frame"
    );
}

#[test]
fn svg_backend_clips_artists() {
    let fig = zoomed_figure();
    let mut renderer = SvgRenderer::new(480.0, 340.0, 100.0);
    fig.draw(&mut renderer);
    let svg = renderer.finish();
    assert!(
        svg.contains("<clipPath"),
        "zoomed artists must register an SVG clipPath"
    );
    assert!(
        svg.contains("clip-path=\"url(#"),
        "artist groups must reference the clipPath"
    );
}

#[test]
fn pdf_backend_clips_artists() {
    let fig = zoomed_figure();
    let mut renderer = PdfRenderer::new(480.0, 340.0, 100.0);
    fig.draw(&mut renderer);
    let pdf = renderer.into_pdf();
    // The clip is installed as `x y w h re W n` inside the content stream;
    // the stream is plain (uncompressed) text in this backend.
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("re W n"),
        "zoomed artists must install a PDF clip path"
    );
}
