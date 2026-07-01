//! Typography A/B probe: a titled, labeled plot whose strings stress
//! kerning-sensitive pairs (Wa, Ta, Vo, AV, To, ff, rt). Renders to
//! `target/typography.png` so title/label rendering can be eyeballed against
//! matplotlib.

use std::f64::consts::TAU;

use rizzma::artist::Line2D;
use rizzma::core::color::Rgba;
use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(6.4, 4.8);
    let ax = fig.add_axes(0.14, 0.14, 0.78, 0.74);

    let n = 200;
    let xs: Vec<f64> = (0..n).map(|i| TAU * i as f64 / (n as f64 - 1.0)).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| x.sin()).collect();

    let blue = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);
    ax.add_line(Line2D::new(xs, ys).with_color(blue).with_linewidth(1.5));

    ax.set_title("Wave Amplitude versus Travel Time (AVTo)");
    ax.set_xlabel("Travel Time / offset (seconds)");
    ax.set_ylabel("Wave Amplitude (Volts)");

    let path = "target/typography.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
