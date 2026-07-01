//! Render a 4x4 grid of 3D bars whose heights form a smooth bump, saving the
//! result to `target/bar3d.png` for visual inspection.

use rizzma::mplot3d::Axes3D;

fn main() {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut zs = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            let x = i as f64;
            let y = j as f64;
            let z = (-((x - 1.5).powi(2) + (y - 1.5).powi(2)) / 2.0).exp() * 5.0 + 0.5;
            xs.push(x);
            ys.push(y);
            zs.push(z);
        }
    }

    let mut ax = Axes3D::new();
    ax.bar3d(&xs, &ys, &zs, 0.8, 0.8);

    ax.save_png("target/bar3d.png", 800, 800, 100.0)
        .expect("failed to save bar3d.png");
    println!("wrote target/bar3d.png");
}
