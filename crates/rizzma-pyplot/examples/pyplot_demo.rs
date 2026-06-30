//! A small plot built purely through the pyplot-style facade.
//!
//! Run with `cargo run -p rizzma-pyplot --example pyplot_demo`.

use rizzma_pyplot as plt;

fn main() {
    let xs: Vec<f64> = (0..=50).map(|i| i as f64 * 0.2).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| x.sin()).collect();

    plt::plot(&xs, &ys);
    plt::title("sine via pyplot facade");
    plt::xlabel("x");
    plt::ylabel("sin(x)");

    let path = "target/pyplot_demo.png";
    plt::savefig(path).expect("save the demo figure");
    println!("wrote {path}");
}
