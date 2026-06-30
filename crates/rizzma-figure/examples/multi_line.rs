//! Plots three phase-shifted sine waves with no explicit colors, exercising the
//! [`Axes`](rizzma_figure::Axes) property color cycle: successive
//! [`plot`](rizzma_figure::Axes::plot) calls come out blue (C0), orange (C1),
//! and green (C2). Writes the result to `target/multi_line.png`.

use std::f64::consts::TAU;

use rizzma_figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    // 200 samples over [0, 2π].
    let n = 200;
    let xs: Vec<f64> = (0..n).map(|i| TAU * i as f64 / (n as f64 - 1.0)).collect();

    // Three phase-shifted sines, each plotted without a color so the property
    // cycle assigns C0 (blue), C1 (orange), C2 (green) in turn.
    for k in 0..3 {
        let phase = TAU * f64::from(k) / 3.0;
        let ys: Vec<f64> = xs.iter().map(|&x| (x + phase).sin()).collect();
        ax.plot(&xs, &ys);
    }

    ax.set_title("phase-shifted sines");
    ax.set_xlabel("x");
    ax.set_ylabel("y");

    let path = "target/multi_line.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
