//! Renders a histogram to `target/histogram.png` — the eyeball check for the
//! `Axes::hist` plotting helper.

use rizzma_figure::Figure;

fn main() {
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    // A deterministic, vaguely bell-shaped sample.
    let data: Vec<f64> = (0..200)
        .map(|i| {
            let t = i as f64 * 0.05;
            t.sin() + (t * 0.5).cos() + (i as f64 * 0.013).fract() * 2.0
        })
        .collect();
    let (counts, edges) = ax.hist(&data, 12);
    println!("{} bins, {} edges", counts.len(), edges.len());

    ax.set_title("histogram");
    ax.set_xlabel("value");
    ax.set_ylabel("count");

    let path = "target/histogram.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
