//! Render a 3D helix with `Axes3D` and write `target/helix3d.png`.
//!
//! Run with `cargo run -p rizzma --example helix3d`.

use rizzma::mplot3d::Axes3D;

fn main() {
    // A helix: x = cos(t), y = sin(t), z = t over several turns.
    let n = 400;
    let turns = 4.0;
    let t_max = turns * std::f64::consts::TAU;
    let t: Vec<f64> = (0..=n).map(|i| t_max * i as f64 / n as f64).collect();
    let x: Vec<f64> = t.iter().map(|&t| t.cos()).collect();
    let y: Vec<f64> = t.iter().map(|&t| t.sin()).collect();
    let z: Vec<f64> = t.clone();

    let mut ax = Axes3D::new();
    ax.plot3d(&x, &y, &z);

    // A few scatter markers sampled along the helix for emphasis.
    let step = n / 8;
    let sx: Vec<f64> = (0..=n).step_by(step).map(|i| x[i]).collect();
    let sy: Vec<f64> = (0..=n).step_by(step).map(|i| y[i]).collect();
    let sz: Vec<f64> = (0..=n).step_by(step).map(|i| z[i]).collect();
    ax.scatter3d(&sx, &sy, &sz);

    ax.save_png("target/helix3d.png", 800, 800, 100.0)
        .expect("write target/helix3d.png");
    println!("wrote target/helix3d.png");
}
