//! Draw a filled red square and a stroked diagonal, writing `target/red_square.png`.
//!
//! Run with `cargo run -p rizzma-skia --example red_square`.

use rizzma_render::{GraphicsContext, Renderer};
use rizzma_skia::SkiaRenderer;
use rizzma_skia::{Affine2D, Path, Rgba};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = SkiaRenderer::new(200, 200, 72.0);

    // Filled red square covering device x,y in [40, 160].
    let square = Affine2D::from_scale(120.0, 120.0).translate(40.0, 40.0);
    renderer.draw_path(
        &GraphicsContext::new(),
        &Path::unit_rectangle(),
        &square,
        Some(Rgba::RED),
    );

    // Stroked black diagonal across the canvas.
    let diagonal = Path::from_polyline(&[[20.0, 20.0], [180.0, 180.0]]);
    let gc = GraphicsContext::new()
        .with_stroke(Rgba::BLACK)
        .with_line_width(3.0);
    renderer.draw_path(&gc, &diagonal, &Affine2D::identity(), None);

    renderer.save_png("target/red_square.png")?;
    println!("wrote target/red_square.png");
    Ok(())
}
