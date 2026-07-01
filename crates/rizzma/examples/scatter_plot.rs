//! Renders a colormapped scatter to `target/scatter_plot.png` — the eyeball
//! check for the `Axes::scatter_mapped` plotting helper.

use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    // A swirl whose color tracks the angle.
    let n = 60;
    let x: Vec<f64> = (0..n).map(|i| (i as f64 * 0.3).cos() * i as f64).collect();
    let y: Vec<f64> = (0..n).map(|i| (i as f64 * 0.3).sin() * i as f64).collect();
    let c: Vec<f64> = (0..n).map(|i| i as f64).collect();
    ax.scatter_mapped(&x, &y, &c, "viridis");

    ax.set_title("scatter (viridis)");
    ax.set_xlabel("x");
    ax.set_ylabel("y");

    let path = "target/scatter_plot.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
