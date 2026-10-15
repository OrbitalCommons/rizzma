//! Render-time benchmark for long noisy lines (issue #329): a light curve,
//! `y = sin(0.3 x) + U(-0.2, 0.2)` over `x ∈ [0, 100]`, drawn as one `plot`
//! on a 1600 x 1200 px figure at 100 DPI, timed without PNG encoding.
//!
//! ```sh
//! cargo run --release -p rizzma --example bench_noisy_line            # 1e5 and 1e6 points
//! cargo run --release -p rizzma --example bench_noisy_line -- 300000  # any sizes
//! ```
//!
//! For comparison, matplotlib 3.11 (Agg, `path.simplify` on) takes about
//! 0.3 s for 1e6 points of the same curve.

use std::time::Instant;

use rizzma::Figure;
use rizzma::core::rcparams::RcParams;

/// The noisy curve at `n` points, from a fixed-seed generator so every run
/// draws the same line.
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

/// Milliseconds to build and render the figure, best of `runs`.
fn time(x: &[f64], y: &[f64], simplify: bool, runs: usize) -> f64 {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            let rc = RcParams {
                path_simplify: simplify,
                ..RcParams::default()
            };
            let mut fig = Figure::new(16.0, 12.0).with_dpi(100.0).with_rcparams(rc);
            let ax = fig.add_subplot(1, 1, 1);
            ax.set_xlim(0.0, 100.0).set_ylim(-1.5, 1.5);
            ax.plot(x, y);
            std::hint::black_box(fig.render());
            start.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::INFINITY, f64::min)
}

fn main() {
    let sizes: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("point counts"))
        .collect();
    let sizes = if sizes.is_empty() {
        vec![100_000, 1_000_000]
    } else {
        sizes
    };
    println!("points,simplified_ms,unsimplified_ms");
    for n in sizes {
        let (x, y) = curve(n);
        let simplified = time(&x, &y, true, 3);
        // The unsimplified path grows superlinearly; one run is plenty.
        let unsimplified = if n <= 100_000 {
            format!("{:.1}", time(&x, &y, false, 1))
        } else {
            "skipped".to_owned()
        };
        println!("{n},{simplified:.1},{unsimplified}");
    }
}
