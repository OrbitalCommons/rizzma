//! End-to-end render test: a scatter [`Collection`] rasterized through the real
//! [`SkiaRenderer`].
//!
//! Renders three large, differently-colored markers into a 100x100 pixmap with
//! an identity transform and asserts that ink of the expected color appears near
//! each offset.
//!
//! Y-flip: [`SkiaRenderer`] flips `(x, y) -> (x, height - y)` internally, so a
//! data offset `(x, y)` lands at pixmap row `height - y`.

use rizzma_artist::{Affine2D, Artist, Collection, Rgba};
use rizzma_skia::SkiaRenderer;

/// Read the straight RGBA bytes of the pixel at `(x, y)` in pixmap space.
fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn renders_three_colored_points() {
    let mut renderer = SkiaRenderer::new(100, 100, 72.0);

    // Three offsets in device space, drawn with an identity transform. Large
    // markers (size 20) ensure ink lands squarely on the sampled center pixel.
    let offsets = vec![[25.0, 25.0], [50.0, 50.0], [75.0, 75.0]];
    let coll = Collection::scatter(offsets.clone())
        .with_sizes(vec![20.0])
        .with_facecolors(vec![Rgba::RED, Rgba::GREEN, Rgba::BLUE]);

    coll.draw(&mut renderer, &Affine2D::identity());

    let expected = [Rgba::RED, Rgba::GREEN, Rgba::BLUE];
    for (&[x, y], color) in offsets.iter().zip(expected) {
        // Y-flip into pixmap space.
        let px = x as u32;
        let py = (100.0 - y) as u32;
        let [r, g, b, a] = pixel(&renderer, px, py);
        assert!(
            a > 200,
            "marker at ({x}, {y}) should be opaque, got alpha {a}"
        );
        let [er, eg, eb, _] = color.to_u8_array();
        // The dominant channel should match the expected color's dominant
        // channel; allow slack for antialiasing.
        let close = |got: u8, want: u8| (i32::from(got) - i32::from(want)).abs() < 60;
        assert!(
            close(r, er) && close(g, eg) && close(b, eb),
            "marker at ({x}, {y}) expected ~[{er}, {eg}, {eb}], got [{r}, {g}, {b}]"
        );
    }

    // A far corner outside every marker stays transparent.
    assert_eq!(
        pixel(&renderer, 2, 2),
        [0, 0, 0, 0],
        "corner pixel should be untouched"
    );
}
