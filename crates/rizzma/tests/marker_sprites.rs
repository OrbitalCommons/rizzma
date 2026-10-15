//! Uniform scatter markers through `draw_markers`: the raster backend stamps
//! cached sprites, so large scatters must ink the same pixels as drawing each
//! marker as a path, small ones must stay exact, and vector output must not
//! change at all.
//!
//! The per-point reference is the same scatter with its one face color given
//! twice: a non-uniform style list, which draws marker by marker.

use rizzma::Figure;
use rizzma::core::color::Rgba;

/// matplotlib's `C0`, the first scatter's face color.
const C0: Rgba = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);

fn figure(x: &[f64], y: &[f64], per_point: bool) -> Figure {
    let mut fig = Figure::new(16.0, 12.0);
    let markers = fig.add_subplot(1, 1, 1).scatter(x, y);
    if per_point {
        markers.set_facecolors(vec![C0, C0]);
    }
    fig
}

/// `n` uniformly random points in the unit square.
fn random(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n).map(|_| (uniform(), uniform())).unzip()
}

/// The share of inked pixels (non-white in either image) within one unit
/// per channel.
fn inked_agreement(a: &[u8], b: &[u8]) -> f64 {
    let (mut inked, mut close) = (0usize, 0usize);
    for (p, q) in a.chunks(3).zip(b.chunks(3)) {
        if p != [255, 255, 255] || q != [255, 255, 255] {
            inked += 1;
            if p.iter().zip(q).all(|(i, j)| i.abs_diff(*j) <= 1) {
                close += 1;
            }
        }
    }
    close as f64 / inked as f64
}

#[test]
fn large_scatters_ink_the_same_pixels() {
    // The issue's benchmark shape (a dense sine of points) and a uniform
    // random cloud, both well above the sprite threshold.
    let n = 100_000;
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y: Vec<f64> = x.iter().map(|v| (v * 0.01).sin()).collect();
    let cases = [("sine", (x, y)), ("random", random(n))];
    for (name, (x, y)) in cases {
        let sprites = figure(&x, &y, false).to_rgb8().0;
        let exact = figure(&x, &y, true).to_rgb8().0;
        let share = inked_agreement(&sprites, &exact);
        assert!(
            share >= 0.99,
            "{name}: {:.3}% of inked pixels within ±1",
            share * 100.0
        );
    }
}

#[test]
fn small_scatters_draw_exactly() {
    let (x, y) = random(5_000);
    assert_eq!(
        figure(&x, &y, false).to_rgb8().0,
        figure(&x, &y, true).to_rgb8().0
    );
}

#[test]
fn vector_output_is_unchanged() {
    let (x, y) = random(12_000);
    let (uniform, per_point) = (figure(&x, &y, false), figure(&x, &y, true));
    assert_eq!(uniform.to_svg(), per_point.to_svg());
    assert_eq!(uniform.to_pdf(), per_point.to_pdf());
}
