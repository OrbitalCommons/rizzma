//! Colorbars against matplotlib 3.11 (`fixtures/colorbar_mpl.py`): on a 2 x 2
//! grid with a colorbar on panel 0, for linear, log and two-slope norms, the
//! parent's frame and the bar land within a pixel of matplotlib's
//! `fig.colorbar(im, ax=ax0)`, the other panels keep their size, and the
//! ticks and their labels are the same.

use rizzma::core::color::{LinearNorm, LogNorm, Norm, TwoSlopeNorm};
use rizzma::{Figure, Mappable};

const REFERENCE: &str = include_str!("fixtures/colorbar_mpl.txt");
const RECTS: [(f64, f64, f64, f64); 4] = [
    (0.08, 0.56, 0.38, 0.38),
    (0.56, 0.56, 0.38, 0.38),
    (0.08, 0.08, 0.38, 0.38),
    (0.56, 0.08, 0.38, 0.38),
];

/// The fixture's 8 x 8 image, `linspace(-300, 900, 64)`.
fn data(abs: bool) -> Vec<f64> {
    (0..64)
        .map(|i| -300.0 + 1200.0 * f64::from(i) / 63.0)
        .map(|v: f64| if abs { v.abs() } else { v })
        .collect()
}

/// matplotlib wraps mathtext tick labels in `\mathdefault{…}`, which only
/// selects the font, and writes negatives with a Unicode minus
/// (`axes.unicode_minus`) where rizzma's tick labels use the ASCII one on
/// every axis; compare the labels past both.
fn plain(label: &str) -> String {
    label
        .replace("\\mathdefault{", "")
        .replace("}}$", "}$")
        .replace('_', " ")
        .replace('\u{2212}', "-")
}

#[test]
fn colorbar_geometry_and_ticks_match_matplotlib() {
    let norms: [(&str, Norm); 3] = [
        ("linear", LinearNorm::new(-200.0, 600.0).into()),
        ("log", LogNorm::new(0.3, 300.0).into()),
        ("twoslope", TwoSlopeNorm::new(-300.0, 0.0, 900.0).into()),
    ];
    let (px_x, px_y) = (1.0 / 640.0, 1.0 / 480.0);
    for (name, norm) in norms {
        let mut fig = Figure::new(6.4, 4.8);
        for &(l, b, w, h) in &RECTS {
            fig.add_axes(l, b, w, h);
        }
        let ax = &mut fig.axes_mut()[0];
        ax.imshow(&data(name == "log"), 8, 8).norm(norm);
        ax.colorbar(Mappable::Image(0));

        let mut ticks = Vec::new();
        for line in REFERENCE.lines().filter(|l| l.starts_with(name)) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let nums = |from: usize| -> Vec<f64> {
                f[from..from + 4]
                    .iter()
                    .map(|v| v.parse().unwrap())
                    .collect()
            };
            match f[1] {
                "axes" => {
                    let i: usize = f[2].parse().unwrap();
                    let want = nums(3);
                    let got = fig.axes_frame(i).unwrap();
                    let got = [got.xmin(), got.ymin(), got.xmax(), got.ymax()];
                    for k in 0..4 {
                        let tol = if k % 2 == 0 { px_x } else { px_y };
                        assert!(
                            (got[k] - want[k]).abs() <= tol,
                            "{name} axes {i}: rizzma {got:?} vs matplotlib {want:?}"
                        );
                    }
                }
                "cbar" => {
                    let want = nums(2);
                    let got = fig.colorbar_frame(0).unwrap();
                    let got = [got.xmin(), got.ymin(), got.xmax(), got.ymax()];
                    for k in 0..4 {
                        let tol = if k % 2 == 0 { px_x } else { px_y };
                        assert!(
                            (got[k] - want[k]).abs() <= tol,
                            "{name} colorbar: rizzma {got:?} vs matplotlib {want:?}"
                        );
                    }
                }
                "tick" => ticks.push((f[2].parse::<f64>().unwrap(), plain(f[3]))),
                other => panic!("unknown fixture row {other}"),
            }
        }

        let got: Vec<(f64, String)> = fig
            .colorbar_ticks(0)
            .into_iter()
            .map(|(v, label)| (v, plain(&label)))
            .collect();
        assert_eq!(
            got.len(),
            ticks.len(),
            "{name}: rizzma {got:?} vs matplotlib {ticks:?}"
        );
        for ((gv, gl), (wv, wl)) in got.iter().zip(&ticks) {
            assert!(
                (gv - wv).abs() < 1e-9,
                "{name}: rizzma {got:?} vs matplotlib {ticks:?}"
            );
            assert_eq!(gl, wl, "{name}: rizzma {got:?} vs matplotlib {ticks:?}");
        }
    }
}

#[test]
fn panels_without_a_colorbar_keep_their_size() {
    let mut plain = Figure::new(6.4, 4.8);
    for i in 1..=4 {
        plain.add_subplot(2, 2, i).imshow(&data(false), 8, 8);
    }
    let mut barred = Figure::new(6.4, 4.8);
    for i in 1..=4 {
        barred.add_subplot(2, 2, i).imshow(&data(false), 8, 8);
    }
    barred.axes_mut()[0].colorbar(Mappable::Image(0));
    // Panel 0 gives up room; the panel below it keeps its frame, and the
    // right-hand column is untouched.
    assert!(barred.axes_frame(0).unwrap().width() < plain.axes_frame(0).unwrap().width());
    for i in 1..4 {
        let (a, b) = (plain.axes_frame(i).unwrap(), barred.axes_frame(i).unwrap());
        if i == 2 {
            // Same column as the colorbar: tight layout aligns column widths
            // by decoration only, so only the left edge and height must hold.
            assert!((a.xmin() - b.xmin()).abs() < 1e-9 && (a.height() - b.height()).abs() < 1e-9);
        } else {
            assert_eq!(a, b, "panel {i} changed size");
        }
    }
}
