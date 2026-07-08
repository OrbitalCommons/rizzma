//! Renders two labeled lines with a legend and a viridis colorbar, then writes
//! the same scene to both `target/legend_colorbar.png` (via skia) and
//! `target/legend_colorbar.svg` (via the SVG backend) — the eyeball check that
//! the figure is backend-agnostic.

use std::f64::consts::TAU;

use rizzma::artist::Line2D;
use rizzma::core::color::Rgba;
use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    // Leave room on the right for the colorbar.
    let ax = fig.add_axes(0.12, 0.12, 0.72, 0.8);

    let n = 200;
    let xs: Vec<f64> = (0..n).map(|i| TAU * i as f64 / (n as f64 - 1.0)).collect();
    let sin: Vec<f64> = xs.iter().map(|&x| x.sin()).collect();
    let cos: Vec<f64> = xs.iter().map(|&x| x.cos()).collect();

    let blue = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);
    let orange = Rgba::new(1.0, 0.498_039_22, 0.054_901_96, 1.0);

    ax.add_line(
        Line2D::new(xs.clone(), sin)
            .with_color(blue)
            .with_linewidth(1.5),
    );
    ax.add_line(Line2D::new(xs, cos).with_color(orange).with_linewidth(1.5));
    ax.set_title("legend + colorbar");
    ax.set_xlabel("x");
    ax.set_ylabel("y");
    ax.legend(vec![
        (blue, "sin(x)".to_string()),
        (orange, "cos(x)".to_string()),
    ]);

    fig.colorbar("bgyw", 0.0, 1.0);

    let png = "target/legend_colorbar.png";
    let svg = "target/legend_colorbar.svg";
    fig.save_png(png).expect("save PNG");
    fig.save_svg(svg).expect("save SVG");
    println!("wrote {png}");
    println!("wrote {svg}");
}
