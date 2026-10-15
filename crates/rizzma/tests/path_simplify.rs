//! Line simplification (`path.simplify`): long noisy lines render the same
//! pixels with or without it, short lines are never touched, and turning it
//! off restores the exact unsimplified drawing.

use rizzma::Figure;
use rizzma::core::rcparams::RcParams;

/// `y = sin(0.3 x) + U(-0.2, 0.2)` over `x ∈ [0, 100]` at `n` points.
fn curve(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let x: Vec<f64> = (0..n).map(|i| i as f64 / n as f64 * 100.0).collect();
    let y = x
        .iter()
        .map(|t| (t * 0.3).sin() + 0.4 * (uniform() - 0.5))
        .collect();
    (x, y)
}

fn render(x: &[f64], y: &[f64], simplify: bool, size: (f64, f64)) -> Vec<u8> {
    let rc = RcParams {
        path_simplify: simplify,
        ..RcParams::default()
    };
    let mut fig = Figure::new(size.0, size.1).with_rcparams(rc);
    let ax = fig.add_subplot(1, 1, 1);
    ax.set_xlim(0.0, 100.0).set_ylim(-1.5, 1.5);
    ax.plot(x, y);
    fig.to_rgb8().0
}

#[test]
fn simplified_lines_cover_the_same_pixels() {
    let (x, y) = curve(100_000);
    let on = render(&x, &y, true, (16.0, 12.0));
    let off = render(&x, &y, false, (16.0, 12.0));
    let pixels = on.len() / 3;
    let differing = on
        .chunks(3)
        .zip(off.chunks(3))
        .filter(|(a, b)| a.iter().zip(*b).any(|(p, q)| p.abs_diff(*q) > 1))
        .count();
    let share = differing as f64 / pixels as f64;
    assert!(
        share <= 0.001,
        "{differing} of {pixels} pixels differ by more than 1"
    );
}

#[test]
fn short_lines_are_drawn_as_given() {
    let (x, y) = curve(999);
    assert_eq!(
        render(&x, &y, true, (4.0, 3.0)),
        render(&x, &y, false, (4.0, 3.0))
    );
}
