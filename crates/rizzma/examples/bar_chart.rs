//! Renders a small vertical bar chart to `target/bar_chart.png` — the eyeball
//! check for the Tier-1 `Axes::bar` plotting helper.

use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    let x = [0.0, 1.0, 2.0, 3.0, 4.0];
    let heights = [3.0, 7.0, 2.0, 5.0, 4.0];
    ax.bar(&x, &heights);

    // A horizontal reference line at the mean height.
    let mean = heights.iter().sum::<f64>() / heights.len() as f64;
    ax.axhline(mean);

    ax.set_title("bar chart");
    ax.set_xlabel("category");
    ax.set_ylabel("value");

    let path = "target/bar_chart.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
