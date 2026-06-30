//! Render a 3D "sombrero" surface with `Axes3D` and write `target/surface3d.png`.
//!
//! Run with `cargo run -p rizzma-3d --example surface3d`.

use rizzma_3d::Axes3D;

fn main() {
    // A sombrero: z = sin(r) / r with r = hypot(x, y) over a 40x40 grid.
    let nx = 40;
    let ny = 40;
    let lo = -8.0_f64;
    let hi = 8.0_f64;

    let x: Vec<f64> = (0..nx)
        .map(|i| lo + (hi - lo) * i as f64 / (nx - 1) as f64)
        .collect();
    let y: Vec<f64> = (0..ny)
        .map(|j| lo + (hi - lo) * j as f64 / (ny - 1) as f64)
        .collect();

    let mut z = Vec::with_capacity(nx * ny);
    for &yj in &y {
        for &xi in &x {
            let r = xi.hypot(yj);
            // Guard the r -> 0 singularity: lim sin(r)/r = 1.
            let v = if r.abs() < 1e-9 { 1.0 } else { r.sin() / r };
            z.push(v);
        }
    }

    let mut ax = Axes3D::new();
    ax.plot_surface(&x, &y, &z);

    ax.save_png("target/surface3d.png", 800, 800, 100.0)
        .expect("write target/surface3d.png");
    println!("wrote target/surface3d.png");
}
