//! Drive [`PdfRenderer`] directly (not via `Figure`) to produce a small demo
//! page exercising a filled rectangle, a stroked polyline, and a filled
//! triangle at known positions, then write it to `target/pdf_demo.pdf`.
//!
//! Run with `cargo run -p rizzma-pdf --example pdf_demo`, then open the PDF to
//! confirm the shapes are right-side-up: the red rectangle sits in the
//! lower-left, the blue triangle in the upper-right.

use rizzma_pdf::{Affine2D, Path, PdfRenderer, Rgba};
use rizzma_render::{GraphicsContext, Renderer};

fn main() -> std::io::Result<()> {
    // 200 wide, 150 tall canvas at 72 DPI (1 px == 1 pt).
    let mut r = PdfRenderer::new(200.0, 150.0, 72.0);

    // A filled red rectangle in the LOWER-LEFT: device (20,20)..(80,60).
    // unit_rectangle spans (0,0)..(1,1); scale to 60x40 then offset to (20,20).
    let rect_xf = Affine2D::from_scale(60.0, 40.0).translate(20.0, 20.0);
    r.draw_path(
        &GraphicsContext::new(),
        &Path::unit_rectangle(),
        &rect_xf,
        Some(Rgba::RED),
    );

    // A stroked black polyline across the middle, rising left-to-right.
    let line = Path::from_polyline(&[[20.0, 90.0], [90.0, 110.0], [160.0, 80.0]]);
    let line_gc = GraphicsContext::new()
        .with_stroke(Rgba::BLACK)
        .with_line_width(3.0);
    r.draw_path(&line_gc, &line, &Affine2D::identity(), None);

    // A filled blue triangle in the UPPER-RIGHT.
    let triangle = Path::from_polyline(&[
        [130.0, 100.0],
        [185.0, 100.0],
        [157.0, 140.0],
        [130.0, 100.0],
    ]);
    r.draw_path(&line_gc, &triangle, &Affine2D::identity(), Some(Rgba::BLUE));

    std::fs::create_dir_all("target")?;
    r.save("target/pdf_demo.pdf")?;
    println!("wrote target/pdf_demo.pdf ({} bytes)", r.finish().len());
    Ok(())
}
