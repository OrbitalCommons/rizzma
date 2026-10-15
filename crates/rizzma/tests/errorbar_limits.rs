//! Errorbar limit carets against matplotlib 3.11
//! (`fixtures/errorbar_limits_mpl.py`): for a series mixing detections,
//! upper limits and a lower limit, every caret's width and tip land within a
//! pixel of matplotlib's at 72, 100 and 200 DPI.

use rizzma::Figure;
use rizzma::figure::ErrorbarOptions;

const X: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
const Y: [f64; 5] = [5.0, 6.0, 5.0, 6.0, 5.0];
const YERR: [f64; 5] = [2.0; 5];
const UPLIMS: [bool; 5] = [false, true, false, true, false];
const LOLIMS: [bool; 5] = [false, false, true, false, false];

/// The caret of point `i` measured as the fixture measures it: inked columns
/// and the tip row inside a window around the bar's open end.
fn caret(fig: &Figure, rgb: &[u8], w: usize, i: usize, dpi: f64) -> (usize, usize, usize) {
    let up = UPLIMS[i];
    let end = if up { Y[i] - YERR[i] } else { Y[i] + YERR[i] };
    let (cx, cy) = fig.data_to_pixel(0, X[i], end).unwrap();
    let pt = dpi / 72.0;
    let rows = if up {
        ((cy - pt) as usize, (cy + 8.0 * pt).ceil() as usize)
    } else {
        ((cy - 8.0 * pt) as usize, (cy + pt).ceil() as usize)
    };
    let cols = ((cx - 8.0 * pt) as usize, (cx + 8.0 * pt).ceil() as usize);
    let (mut x0, mut x1, mut tip) = (usize::MAX, 0, if up { 0 } else { usize::MAX });
    for r in rows.0..=rows.1 {
        for c in cols.0..=cols.1 {
            let p = &rgb[(r * w + c) * 3..][..3];
            if (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3 < 128 {
                x0 = x0.min(c);
                x1 = x1.max(c);
                tip = if up { tip.max(r) } else { tip.min(r) };
            }
        }
    }
    (x0, x1, tip)
}

#[test]
fn limit_carets_match_matplotlib() {
    let mut checked = 0;
    for line in include_str!("fixtures/errorbar_limits_mpl.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        let dpi: f64 = f[0].parse().unwrap();
        let i: usize = f[1].parse().unwrap();
        let want: Vec<usize> = f[3..6].iter().map(|v| v.parse().unwrap()).collect();

        let mut fig = Figure::new(4.0, 3.0).with_dpi(dpi);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.set_xlim(0.0, 6.0).set_ylim(0.0, 10.0);
        ax.set_axis_off();
        ax.errorbar_with(
            &X,
            &Y,
            &YERR,
            &ErrorbarOptions::new().uplims(&UPLIMS).lolims(&LOLIMS),
        );
        let (rgb, w, _) = fig.to_rgb8();
        let got = caret(&fig, &rgb, w as usize, i, dpi);
        let got = [got.0, got.1, got.2];
        for k in 0..3 {
            assert!(
                got[k].abs_diff(want[k]) <= 1,
                "dpi {dpi} point {i} ({}): rizzma {got:?} vs matplotlib {want:?}",
                f[2]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 9);
}
