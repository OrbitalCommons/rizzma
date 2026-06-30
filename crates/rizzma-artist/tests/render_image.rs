//! End-to-end render test: an [`AxesImage`] rasterized through the real
//! [`SkiaRenderer`].
//!
//! Renders a small vertical gradient (data row `0` = min, last row = max) into
//! a pixmap with an identity transform and asserts the colors vary across the
//! image and match the viridis endpoints approximately.
//!
//! Y-flip + origin: [`SkiaRenderer`] flips `(x, y) -> (height - y)` internally,
//! and the image uses `origin="upper"` so data row `0` is drawn at the *top* of
//! the device extent (the smallest pixmap row).

use rizzma_artist::{Affine2D, Artist, AxesImage};
use rizzma_core::color::{Colormap, viridis};
use rizzma_skia::SkiaRenderer;

/// Read the straight RGBA bytes of the pixel at `(x, y)` in pixmap space.
fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

/// Largest absolute channel difference between two RGBA quads.
fn max_diff(a: [u8; 4], b: [u8; 4]) -> i32 {
    (0..4)
        .map(|i| (i32::from(a[i]) - i32::from(b[i])).abs())
        .max()
        .unwrap_or(0)
}

#[test]
fn gradient_image_varies_and_matches_viridis_endpoints() {
    let mut renderer = SkiaRenderer::new(100, 100, 72.0);

    // A 4-row, 1-column vertical gradient: row 0 = 0.0 (min) .. row 3 = 3.0.
    let data = vec![0.0, 1.0, 2.0, 3.0];
    let img = AxesImage::new(data, 4, 1)
        // Draw across the whole 100x100 device area.
        .with_extent([0.0, 100.0, 0.0, 100.0]);
    img.draw(&mut renderer, &Affine2D::identity());

    // origin="upper": data row 0 (the minimum) is at the device top, which the
    // Y-flip maps to the *smallest* pixmap row. So the top pixmap rows are the
    // viridis low end (dark purple) and the bottom rows the high end (yellow).
    let top = pixel(&renderer, 50, 2);
    let bottom = pixel(&renderer, 50, 97);

    // Both opaque.
    assert!(top[3] > 200 && bottom[3] > 200, "image should be opaque");

    // Top and bottom differ substantially (the gradient is visible).
    assert!(
        max_diff(top, bottom) > 60,
        "gradient should vary: top {top:?} vs bottom {bottom:?}"
    );

    // Endpoints approximately match viridis(0) and viridis(1).
    let lo = viridis().sample(0.0).to_u8_array();
    let hi = viridis().sample(1.0).to_u8_array();
    assert!(
        max_diff(top, lo) < 40,
        "top should be ~viridis(0) {lo:?}, got {top:?}"
    );
    assert!(
        max_diff(bottom, hi) < 40,
        "bottom should be ~viridis(1) {hi:?}, got {bottom:?}"
    );
}
