//! Renders a radial `sin(r)` ripple field with `imshow` (viridis) to
//! `target/imshow.png` — the eyeball check for the `Axes::imshow` raster helper.

use rizzma::figure::Figure;

fn main() {
    let mut fig = Figure::new(5.0, 4.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);

    // A 2D ripple: sin(r) where r is the distance from the grid center.
    let nrows = 200;
    let ncols = 200;
    let mut data = Vec::with_capacity(nrows * ncols);
    for row in 0..nrows {
        for col in 0..ncols {
            // Map the grid to [-6, 6] in both axes.
            let x = (col as f64 / (ncols - 1) as f64) * 12.0 - 6.0;
            let y = (row as f64 / (nrows - 1) as f64) * 12.0 - 6.0;
            let r = (x * x + y * y).sqrt();
            data.push(r.sin());
        }
    }

    ax.imshow(&data, nrows, ncols)
        .cmap("viridis")
        .set_extent([-6.0, 6.0, -6.0, 6.0]);

    ax.set_title("imshow: sin(r) ripple (viridis)");
    ax.set_xlabel("x");
    ax.set_ylabel("y");

    let path = "target/imshow.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
