//! Mathtext in a real plot: two Gaussian PDFs with the density formula as the
//! title and script-`N` / Greek axis labels. Shows `$...$` math composing with
//! ordinary plotting. Writes `target/mathtext_plot.png`.

use std::f64::consts::PI;

use rizzma::artist::Line2D;
use rizzma::core::color::Rgba;
use rizzma::figure::Figure;

/// Normal PDF with mean `mu` and standard deviation `sigma`.
fn gaussian(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    (-0.5 * z * z).exp() / (sigma * (2.0 * PI).sqrt())
}

fn main() {
    let mut fig = Figure::new(7.5, 4.6);
    let ax = fig.add_axes(0.12, 0.15, 0.82, 0.66);

    let n = 400;
    let xs: Vec<f64> = (0..n)
        .map(|i| -6.0 + 12.0 * i as f64 / (n as f64 - 1.0))
        .collect();

    let blue = Rgba::from_hex("#1f77b4").unwrap();
    let orange = Rgba::from_hex("#ff7f0e").unwrap();

    let y1: Vec<f64> = xs.iter().map(|&x| gaussian(x, 0.0, 1.0)).collect();
    let y2: Vec<f64> = xs.iter().map(|&x| gaussian(x, 0.0, 2.0)).collect();
    ax.add_line(
        Line2D::new(xs.clone(), y1)
            .with_color(blue)
            .with_linewidth(2.0),
    );
    ax.add_line(Line2D::new(xs, y2).with_color(orange).with_linewidth(2.0));

    // Title and axis labels use `$...$` mathtext; the legend uses Unicode
    // (legend labels are plain glyphs, not math-laid-out).
    ax.set_title(
        r"$\mathcal{N}(x \mid \mu,\, \sigma^{2}) = \frac{1}{\sigma\sqrt{2\pi}}\; e^{-(x-\mu)^{2} / 2\sigma^{2}}$",
    );
    ax.set_xlabel(r"$x$");
    ax.set_ylabel(r"$\mathcal{N}(x \mid \mu,\, \sigma^{2})$");
    ax.legend(vec![
        (blue, "σ = 1".to_string()),
        (orange, "σ = 2".to_string()),
    ]);

    let path = "target/mathtext_plot.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
