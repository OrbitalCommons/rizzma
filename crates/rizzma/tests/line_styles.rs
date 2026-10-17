//! Line caps and joins against matplotlib 3.11 (`fixtures/line_styles_mpl.py`):
//! a 4 pt zig-zag, solid (projecting caps, round joins) and dashed (butt caps,
//! round joins), inks within a pixel of Agg's extents and centroid.

use rizzma::Figure;
use rizzma::core::color::Rgba;

const X: [f64; 4] = [0.2, 0.4, 0.6, 0.8];
const Y: [f64; 4] = [0.3, 0.7, 0.3, 0.7];

/// Sub-pixel ink extents `[x0, y0, x1, y1]` and the ink-weighted centroid,
/// measured as the fixture measures them.
fn ink(fig: &Figure) -> ([f64; 4], [f64; 2]) {
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
fn zigzag_caps_and_joins_match_matplotlib() {
    for line in include_str!("fixtures/line_styles_mpl.txt").lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let case = fields[0];
        let want: Vec<f64> = fields[1..].iter().map(|v| v.parse().unwrap()).collect();

        let mut fig = Figure::new(2.0, 2.0);
        let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
        ax.set_axis_off();
        let zigzag = ax.plot(&X, &Y);
        zigzag.set_color(Rgba::BLACK).set_linewidth(4.0);
        if case == "dashed" {
            // matplotlib's `--`, scaled by the line width.
            zigzag.set_dashes(Some((0.0, vec![3.7 * 4.0, 1.6 * 4.0])));
        }
        ax.set_xlim(0.0, 1.0).set_ylim(0.0, 1.0);
        let (extents, center) = ink(&fig);
        let ours: Vec<f64> = extents.iter().chain(&center).copied().collect();
        for (k, (a, b)) in ours.iter().zip(&want).enumerate() {
            assert!(
                (a - b).abs() <= 1.0,
                "{case} value {k}: {ours:.2?} vs matplotlib {want:?}"
            );
        }
    }
}
