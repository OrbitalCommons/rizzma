//! Mathtext on tick labels: a log-log power law whose decade ticks render as
//! real superscripts ($10^{0}, 10^{1}, \dots$), with a math title and labels.
//! Writes `target/mathtext_logaxis.png`.

use rizzma::artist::Line2D;
use rizzma::core::color::Rgba;
use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(6.6, 4.6);
    let ax = fig.add_axes(0.15, 0.15, 0.78, 0.7);

    // Two power laws y = x^k over four decades, drawn on log-log axes so the
    // ticks come out as 10^n superscripts.
    let xs: Vec<f64> = (0..200)
        .map(|i| 10f64.powf(4.0 * i as f64 / 199.0))
        .collect();
    let cube: Vec<f64> = xs.iter().map(|&x| x.powi(3)).collect();
    let sqrt: Vec<f64> = xs.iter().map(|&x| x.powf(0.5)).collect();

    let blue = Rgba::from_hex("#1f77b4").unwrap();
    let green = Rgba::from_hex("#2ca02c").unwrap();
    ax.set_xscale_log(10.0);
    ax.set_yscale_log(10.0);
    ax.add_line(
        Line2D::new(xs.clone(), cube)
            .with_color(blue)
            .with_linewidth(2.0),
    );
    ax.add_line(Line2D::new(xs, sqrt).with_color(green).with_linewidth(2.0));

    ax.set_title(r"Power laws $y = x^{k}$ on log-log axes");
    ax.set_xlabel(r"$x$");
    ax.set_ylabel(r"$y = x^{k}$");
    ax.legend(vec![
        (blue, "k = 3".to_string()),
        (green, "k = 1/2".to_string()),
    ]);

    let path = "target/mathtext_logaxis.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
