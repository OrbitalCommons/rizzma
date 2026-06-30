//! Integration test: render a Bottom axis with the skia backend and check ink.

use rizzma_axis::axis::{Axis, AxisSide};
use rizzma_core::Bbox;
use rizzma_skia::SkiaRenderer;
use rizzma_text::FontSource;

/// Whether any pixel in the region `[x0, x1) x [y0, y1)` (top-left pixmap
/// coordinates) is non-transparent.
fn has_ink(renderer: &SkiaRenderer, x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
    let pixmap = renderer.pixmap();
    let w = pixmap.width();
    // `data()` is row-major premultiplied RGBA; the alpha byte is index 3 of 4.
    let data = pixmap.data();
    for y in y0..y1 {
        for x in x0..x1 {
            let idx = ((y * w + x) * 4 + 3) as usize;
            if data[idx] != 0 {
                return true;
            }
        }
    }
    false
}

#[test]
fn bottom_axis_inks_bottom_edge_not_interior() {
    let mut renderer = SkiaRenderer::new(300, 300, 72.0);
    let font = FontSource::dejavu_sans();
    // Axes rectangle in y-UP space; the bottom spine sits at y = 50 (display),
    // which after the renderer's Y-flip lands near the bottom of the 300px tall
    // pixmap (pixmap row ~250).
    let bbox = Bbox::from_extents(50.0, 50.0, 250.0, 250.0);
    let axis = Axis::new(AxisSide::Bottom);
    axis.draw(&mut renderer, &bbox, (0.0, 10.0), &font);

    // The spine (display y = 50) maps to pixmap row 250; ticks and labels lie
    // below it (pixmap rows 250..300). Expect ink in that band.
    assert!(
        has_ink(&renderer, 50, 240, 250, 300),
        "expected ink near the bottom edge (ticks/labels)"
    );

    // The upper-middle interior should be empty: no grid by default.
    assert!(
        !has_ink(&renderer, 100, 80, 200, 160),
        "unexpected ink in the upper-middle interior"
    );
}
