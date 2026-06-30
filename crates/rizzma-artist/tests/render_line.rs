//! End-to-end render test: a [`Line2D`] rasterized through the real
//! [`SkiaRenderer`].
//!
//! Renders a red diagonal line into a 100x100 pixmap and asserts that a pixel
//! along the diagonal is red while a corner away from the line is untouched.
//!
//! Y-flip: the device coordinate space has its origin at the bottom-left, while
//! the pixmap is top-down with the origin at the top-left. [`SkiaRenderer`]
//! performs the flip `(x, y) -> (x, height - y)` internally, so the artist and
//! transform work purely in device coordinates and the test reads pixels in
//! pixmap coordinates. The main diagonal `(0,0) -> (100,100)` in device space
//! maps to the anti-diagonal in pixmap rows, but every point of the form
//! `(t, t)` flips to `(t, 100 - t)`, so the pixmap center `(50, 50)` lies on the
//! drawn line either way.

use rizzma_artist::{Affine2D, Artist, Line2D, Rgba};
use rizzma_skia::SkiaRenderer;

/// Read the straight RGBA bytes of the pixel at `(x, y)` in pixmap space.
fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn renders_red_diagonal_into_pixmap() {
    let mut renderer = SkiaRenderer::new(100, 100, 72.0);

    // Data points (0,0) -> (1,1) mapped to device (0,0) -> (100,100) by a
    // pure scale. The renderer flips Y internally when rasterizing.
    let transform = Affine2D::from_scale(100.0, 100.0);
    let line = Line2D::new(vec![0.0, 1.0], vec![0.0, 1.0])
        .with_color(Rgba::RED)
        .with_linewidth(2.0);

    line.draw(&mut renderer, &transform);

    // The pixmap center sits on the diagonal regardless of the Y-flip.
    let [r, g, b, a] = pixel(&renderer, 50, 50);
    assert!(
        r > 200 && g < 60 && b < 60 && a > 200,
        "expected a red pixel on the diagonal, got [{r}, {g}, {b}, {a}]"
    );

    // In pixmap space the line traces the anti-diagonal `x + y = 100`. The
    // bottom-right corner `(95, 95)` is far from it and must stay untouched.
    assert_eq!(
        pixel(&renderer, 95, 95),
        [0, 0, 0, 0],
        "corner pixel should be untouched"
    );
}
