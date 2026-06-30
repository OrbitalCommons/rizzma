//! End-to-end render test: a [`Patch`] rasterized through the real
//! [`SkiaRenderer`].
//!
//! Renders a blue-filled circle into a 100x100 pixmap with an identity
//! transform and asserts that the center pixel is blue while a corner pixel is
//! transparent.
//!
//! Y-flip: [`SkiaRenderer`] flips `(x, y) -> (x, height - y)` internally. The
//! circle is centered at `(50, 50)` on a 100px canvas, which is symmetric about
//! the vertical center, so the center pixel lands on `(50, 50)` in pixmap space
//! either way.

use rizzma_artist::{Affine2D, Artist, Patch, Rgba};
use rizzma_skia::SkiaRenderer;

/// Read the straight RGBA bytes of the pixel at `(x, y)` in pixmap space.
fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn renders_blue_circle_into_pixmap() {
    let mut renderer = SkiaRenderer::new(100, 100, 72.0);

    // A blue circle of radius 30 centered at (50, 50) in device space, drawn
    // with an identity transform. No edge so only the fill is asserted.
    let patch = Patch::circle([50.0, 50.0], 30.0)
        .facecolor(Some(Rgba::BLUE))
        .edgecolor(None);

    patch.draw(&mut renderer, &Affine2D::identity());

    // The center of the circle is filled blue.
    let [r, g, b, a] = pixel(&renderer, 50, 50);
    assert!(
        r < 60 && g < 60 && b > 200 && a > 200,
        "expected a blue pixel at the center, got [{r}, {g}, {b}, {a}]"
    );

    // A far corner is outside the radius-30 disk and stays transparent.
    assert_eq!(
        pixel(&renderer, 5, 5),
        [0, 0, 0, 0],
        "corner pixel should be untouched"
    );
}
