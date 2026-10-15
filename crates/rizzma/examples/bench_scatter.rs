//! Render-time benchmark for large uniform scatters (issue #330): `x = i`,
//! `y = sin(0.01 i)` with the default marker on a 1600 x 1200 px figure at
//! 100 DPI, autoscaled, timed without PNG encoding.
//!
//! ```sh
//! cargo run --release -p rizzma --example bench_scatter            # 1e5, 3e5, 1e6
//! cargo run --release -p rizzma --example bench_scatter -- 400000  # any sizes
//! ```
//!
//! Uniform markers are stamped from cached sprites; the `per_point` column
//! draws the same markers one path at a time (a two-entry face-color list
//! forces that route), as every scatter drew before.

use std::time::Instant;

use rizzma::Figure;
use rizzma::core::color::Rgba;

/// Milliseconds to build and render the scatter, best of `runs`.
fn time(x: &[f64], y: &[f64], per_point: bool, runs: usize) -> f64 {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            let mut fig = Figure::new(16.0, 12.0).with_dpi(100.0);
            let markers = fig.add_subplot(1, 1, 1).scatter(x, y);
            if per_point {
                let c0 = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);
                markers.set_facecolors(vec![c0, c0]);
            }
            std::hint::black_box(fig.render());
            start.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::INFINITY, f64::min)
}

fn main() {
    let sizes: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("marker counts"))
        .collect();
    let sizes = if sizes.is_empty() {
        vec![100_000, 300_000, 1_000_000]
    } else {
        sizes
    };
    println!("markers,sprite_ms,per_point_ms");
    for n in sizes {
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| (v * 0.01).sin()).collect();
        println!(
            "{n},{:.1},{:.1}",
            time(&x, &y, false, 3),
            time(&x, &y, true, 1)
        );
    }
}
