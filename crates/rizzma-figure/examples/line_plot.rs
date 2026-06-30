//! Renders `y = sin(x)` over `x ∈ [0, 2π]` as a blue line on labeled axes and
//! writes the result to `target/line_plot.png` — the eyeball check for the M2
//! "a line on labeled axes → PNG" milestone.

use std::f64::consts::TAU;

use rizzma_artist::Line2D;
use rizzma_core::color::Rgba;
use rizzma_figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    // 200 samples of sin(x) over [0, 2π].
    let n = 200;
    let xs: Vec<f64> = (0..n).map(|i| TAU * i as f64 / (n as f64 - 1.0)).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| x.sin()).collect();

    // matplotlib's default blue ("C0"), at 1.5-point width.
    let blue = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);
    ax.add_line(Line2D::new(xs, ys).with_color(blue).with_linewidth(1.5));
    ax.set_title("sin(x)");
    ax.set_xlabel("x");
    ax.set_ylabel("y");

    let path = "target/line_plot.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
