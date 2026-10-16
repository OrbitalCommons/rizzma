//! Lines draw markers where matplotlib 3.11 draws them, at its size. For
//! `plot(x, y).set_marker(code)` against `plot(x, y, marker=code)` at 100
//! DPI, each marker's ink centroid lies within 0.1 px of matplotlib's and its
//! inked bounding box is identical (`fixtures/line_markers_mpl.py`). The ink
//! itself differs as rizzma's anti-aliasing differs from Agg's for every
//! primitive: the `'x'`, whose vertices snap onto pixel centers as Agg's do,
//! puts its stroke edges on tiny-skia's sample rows and inks 0.075 px high. A
//! marked line's legend sample shows one marker centered on the sample line.

use rizzma::Figure;
use rizzma::core::color::Rgba;

const W: usize = 400;
const X: [f64; 7] = [0.3, 1.1, 2.0, 2.7, 3.6, 4.4, 5.2];
const Y: [f64; 7] = [1.0, 2.6, 1.9, 3.4, 2.2, 3.9, 3.1];

/// matplotlib's `(cx, cy, xmin, xmax, ymin, ymax)` for each point's marker.
fn reference(code: char) -> Vec<(f64, f64, usize, usize, usize, usize)> {
    include_str!("fixtures/line_markers_mpl.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && l.starts_with(code))
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
                f[5].parse().unwrap(),
                f[6].parse().unwrap(),
                f[7].parse().unwrap(),
            )
        })
        .collect()
}

/// The canvas with only `code` markers inked (a white line under C0
/// markers), as the fixture draws it.
fn markers_only(code: &str) -> (Figure, Vec<u8>) {
    let c0 = Rgba::from_hex("#1f77b4").unwrap();
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    ax.set_axis_off();
    ax.plot(&X, &Y)
        .set_color(Rgba::WHITE)
        .set_marker(code)
        .set_markerfacecolor(c0)
        .set_markeredgecolor(c0);
    ax.set_xlim(0.0, 5.5).set_ylim(0.5, 4.5);
    let (rgb, _, _) = fig.to_rgb8();
    (fig, rgb)
}

#[test]
fn markers_sit_where_matplotlib_puts_them_at_its_size() {
    for code in ['o', 's', 'x'] {
        let (fig, rgb) = markers_only(&code.to_string());
        let dark = |x: usize, y: usize| {
            let i = (y * W + x) * 3;
            f64::from(765 - (u16::from(rgb[i]) + u16::from(rgb[i + 1]) + u16::from(rgb[i + 2])))
        };
        for (i, want) in reference(code).into_iter().enumerate() {
            let (px, py) = fig.data_to_pixel(0, X[i], Y[i]).expect("axes 0");
            let (cx, cy) = (px as usize, py as usize);
            let (mut sum, mut sx, mut sy) = (0.0, 0.0, 0.0);
            let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
            for y in cy - 9..=cy + 9 {
                for x in cx - 9..=cx + 9 {
                    let d = dark(x, y);
                    if d > 0.0 {
                        sum += d;
                        sx += d * (x as f64 + 0.5);
                        sy += d * (y as f64 + 0.5);
                        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
                    }
                }
            }
            let (gx, gy) = (sx / sum, sy / sum);
            assert!(
                (gx - want.0).abs() < 0.1 && (gy - want.1).abs() < 0.1,
                "'{code}' marker {i}: centroid ({gx:.3}, {gy:.3}) vs matplotlib ({}, {})",
                want.0,
                want.1
            );
            assert_eq!(
                (x0, x1, y0, y1),
                (want.2, want.3, want.4, want.5),
                "'{code}' marker {i}: inked bounding box"
            );
        }
    }
}

#[test]
fn a_marked_line_shows_one_marker_on_its_legend_sample() {
    let legend_png = |marker: bool| {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        let line = ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
        line.set_label("series");
        if marker {
            line.set_marker("o");
        }
        ax.legend_auto();
        // Hide the data so only the legend differs between the two renders.
        ax.set_xlim(5.0, 6.0).set_ylim(5.0, 6.0);
        fig.to_rgb8().0
    };
    let (plain, marked) = (legend_png(false), legend_png(true));
    let (mut n, mut sx, mut sy) = (0usize, 0.0, 0.0);
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for i in 0..plain.len() / 3 {
        if plain[i * 3..i * 3 + 3] != marked[i * 3..i * 3 + 3] {
            let (x, y) = (i % W, i / W);
            n += 1;
            sx += x as f64;
            sy += y as f64;
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
        }
    }
    // One 6 pt marker (11 px across with its edge), centered on the sample.
    assert!(n > 40, "the marker changes {n} pixels");
    assert_eq!((x1 - x0 + 1, y1 - y0 + 1), (11, 11), "one marker's extent");
    let (cx, cy) = (sx / n as f64, sy / n as f64);
    let sample_row = (0..W)
        .find_map(|x| {
            (0..300).find(|&y| {
                let i = (y * W + x) * 3;
                plain[i..i + 3] == [31, 119, 180]
            })
        })
        .expect("a C0 sample line in the legend");
    assert!(
        (cy - sample_row as f64).abs() < 1.5,
        "marker row {cy} vs sample {sample_row}"
    );
    assert!((x0 + x1) as f64 / 2.0 - cx < 1.0);
}

#[test]
fn markevery_and_marker_colors() {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    ax.set_axis_off();
    ax.plot(&X, &Y)
        .set_color(Rgba::WHITE)
        .set_marker("s")
        .set_markersize(10.0)
        .set_markerfacecolor(Rgba::RED)
        .set_markeredgecolor(Rgba::BLUE)
        .set_markevery(3);
    ax.set_xlim(0.0, 5.5).set_ylim(0.5, 4.5);
    let (rgb, w, _) = fig.to_rgb8();
    let at = |i: usize| {
        let (px, py) = fig.data_to_pixel(0, X[i], Y[i]).expect("axes 0");
        let k = (py as usize * w as usize + px as usize) * 3;
        [rgb[k], rgb[k + 1], rgb[k + 2]]
    };
    // Points 0, 3 and 6 are marked, in the face color; the rest are not.
    for i in 0..7 {
        let want = if i % 3 == 0 {
            [255, 0, 0]
        } else {
            [255, 255, 255]
        };
        assert_eq!(at(i), want, "point {i}");
    }
}
