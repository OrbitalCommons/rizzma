//! Per-frame render budget for interactive (wasm) targets.
//!
//! Interactivity re-renders the whole figure every animation frame, so the
//! render + un-premultiply path has a wall-clock budget: a Tier-1 line plot at
//! 800x600 logical pixels, DPR 2 (1600x1200 backing), must stay comfortably
//! under a frame. The design target is 16 ms (`design/06-wasm-interactive-plan.md`
//! §7); the assertion threshold is deliberately generous for slow shared CI
//! runners, while the measured median is printed for eyeballing regressions
//! (`cargo test --test frame_budget -- --nocapture`).

use std::time::Instant;

use rizzma::Figure;
use rizzma::wasm::figure_to_rgba_scaled;

/// Generous CI ceiling: ~10x the 16 ms design target in optimized builds.
/// Debug builds render ~10x slower still (measured ~165 ms vs ~15 ms release),
/// and CI's `cargo test --workspace` runs unoptimized, so the debug ceiling is
/// scaled up; it still catches order-of-magnitude regressions.
const BUDGET_MS: f64 = if cfg!(debug_assertions) {
    1600.0
} else {
    160.0
};

/// An 8x6 inch (800x600 px @ 100 dpi) Tier-1 line plot.
fn tier1_figure() -> Figure {
    let mut fig = Figure::new(8.0, 6.0);
    let ax = fig.add_axes(0.1, 0.1, 0.85, 0.8);
    let n = 1000;
    let xs: Vec<f64> = (0..n).map(|i| i as f64 / 50.0).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| (x * 1.7).sin() * (-x / 8.0).exp())
        .collect();
    ax.plot(&xs, &ys);
    ax.set_title("frame budget");
    ax.set_xlabel("x");
    ax.set_ylabel("y");
    fig
}

#[test]
fn hidpi_frame_renders_within_budget() {
    let fig = tier1_figure();

    // Warm-up: first render pays one-time font/layout costs.
    let _ = figure_to_rgba_scaled(&fig, 2.0);

    let mut times_ms: Vec<f64> = (0..5)
        .map(|_| {
            let t0 = Instant::now();
            let (rgba, w, h) = figure_to_rgba_scaled(&fig, 2.0);
            assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
            t0.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    times_ms.sort_by(|a, b| a.total_cmp(b));
    let median = times_ms[times_ms.len() / 2];

    println!(
        "frame_budget: median {median:.1} ms over {times_ms:?} (target 16 ms, ceiling {BUDGET_MS} ms)"
    );
    assert!(
        median < BUDGET_MS,
        "1600x1200 frame render median {median:.1} ms exceeds the {BUDGET_MS} ms CI ceiling \
         (design target is 16 ms — see design/06-wasm-interactive-plan.md §7)"
    );
}
