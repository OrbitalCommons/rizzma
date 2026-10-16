//! Every line marker code against matplotlib 3.11: one black 12 pt marker at
//! the center of a 100x100 px figure inks the same pixels, centered the same
//! way, as Agg's (`fixtures/markers_mpl.txt`).

use rizzma::Figure;
use rizzma::artist::MarkerStyle;
use rizzma::core::color::Rgba;

/// The sub-pixel ink extents `[x0, y0, x1, y1]` and the ink-weighted
/// centroid, in pixels from the top-left, of `code` drawn as matplotlib's
/// fixture draws it. A pixel with ink `c` puts a left edge at `x + 1 - c` and
/// a right edge at `x + c`, likewise vertically; the extents are the
/// outermost over all inked pixels.
fn measure(code: char) -> ([f64; 4], [f64; 2]) {
    let mut fig = Figure::new(1.0, 1.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.plot(&[0.0], &[0.0])
        .set_color(Rgba::BLACK)
        .set_marker(&code.to_string())
        .set_markersize(12.0);
    ax.set_xlim(-1.0, 1.0).set_ylim(-1.0, 1.0);
    let (rgb, w, _) = fig.to_rgb8();
    let mut extents = [f64::INFINITY, f64::INFINITY, 0.0, 0.0];
    let (mut total, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for (i, px) in rgb.chunks(3).enumerate() {
        let ink = 1.0 - px.iter().map(|&c| f64::from(c)).sum::<f64>() / (3.0 * 255.0);
        if ink <= 0.0 {
            continue;
        }
        let (x, y) = ((i % w as usize) as f64, (i / w as usize) as f64);
        extents = [
            extents[0].min(x + 1.0 - ink),
            extents[1].min(y + 1.0 - ink),
            extents[2].max(x + ink),
            extents[3].max(y + ink),
        ];
        total += ink;
        sx += ink * (x + 0.5);
        sy += ink * (y + 0.5);
    }
    (extents, [sx / total, sy / total])
}

#[test]
fn markers_match_matplotlib() {
    let reference = include_str!("fixtures/markers_mpl.txt");
    let mut mismatches = Vec::new();
    for line in reference.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let code = fields[0].chars().next().expect("code");
        let filled = fields[1] == "1";
        let values: Vec<f64> = fields[2..8].iter().map(|v| v.parse().unwrap()).collect();
        let (extents, centroid) = (&values[..4], &values[4..]);

        let style = MarkerStyle::from_char(code).expect("known code");
        let (ours, center) = measure(code);
        let off = |a: &[f64], b: &[f64]| a.iter().zip(b).any(|(p, q)| (p - q).abs() > 0.5);
        if style.is_filled() != filled || off(&ours, extents) || off(&center, centroid) {
            mismatches.push(format!(
                "{code:?}: filled {} extents {ours:.2?} centroid {center:.2?}; \
                 matplotlib filled {filled} extents {extents:?} centroid {centroid:?}",
                style.is_filled()
            ));
        }
    }
    assert!(mismatches.is_empty(), "\n{}", mismatches.join("\n"));
}
