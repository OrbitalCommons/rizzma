//! Render tests for `Axes::text` and `Axes::annotate`: text lands where the
//! data coordinates say, and the leader arrow paints ink between the label
//! and the annotated point.

use rizzma::Figure;

/// Straight RGBA of the pixel at top-down `(x, y)`.
fn pixel(r: &rizzma::skia::SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

/// True when any pixel in the `(2*radius)`-square around top-down `(cx, cy)`
/// is not the white background.
fn ink_near(r: &rizzma::skia::SkiaRenderer, cx: f64, cy: f64, radius: u32) -> bool {
    let (w, h) = (r.pixmap().width(), r.pixmap().height());
    let (cx, cy) = (cx.round() as i64, cy.round() as i64);
    for dy in -(radius as i64)..=(radius as i64) {
        for dx in -(radius as i64)..=(radius as i64) {
            let (x, y) = (cx + dx, cy + dy);
            if x >= 0
                && y >= 0
                && (x as u32) < w
                && (y as u32) < h
                && pixel(r, x as u32, y as u32) != [255, 255, 255, 255]
            {
                return true;
            }
        }
    }
    false
}

fn base_figure() -> Figure {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    ax.set_xlim(0.0, 10.0);
    ax.set_ylim(0.0, 10.0);
    fig
}

#[test]
fn text_paints_ink_at_its_data_anchor() {
    let mut fig = base_figure();
    fig.axes_mut()[0].text(3.0, 4.0, "hello");
    let r = fig.render();

    // The anchor is baseline-left: ink sits just right of and above the
    // anchor pixel in top-down coordinates.
    let (px, py) = fig.data_to_pixel(0, 3.0, 4.0).unwrap();
    assert!(
        ink_near(&r, px + 8.0, py - 3.0, 8),
        "expected glyph ink near the text anchor"
    );
    // Empty control region far from everything stays clean.
    let (qx, qy) = fig.data_to_pixel(0, 8.0, 2.0).unwrap();
    assert!(!ink_near(&r, qx, qy, 4), "control region must stay blank");
}

#[test]
fn annotate_paints_text_and_a_leader_arrow() {
    let mut fig = base_figure();
    fig.axes_mut()[0].annotate("peak", (2.0, 2.0), (6.0, 7.0));
    let r = fig.render();

    // Text ink near the xytext anchor.
    let (tx, ty) = fig.data_to_pixel(0, 6.0, 7.0).unwrap();
    assert!(
        ink_near(&r, tx + 8.0, ty - 3.0, 8),
        "expected label ink near xytext"
    );
    // Arrow ink near the target (the tip is pulled back a few px).
    let (ax_, ay) = fig.data_to_pixel(0, 2.0, 2.0).unwrap();
    assert!(
        ink_near(&r, ax_ + 4.0, ay - 4.0, 7),
        "expected arrowhead ink near xy"
    );
    // Arrow ink along the way (midpoint of the segment).
    let (mx, my) = ((tx + ax_) / 2.0, (ty + ay) / 2.0);
    assert!(
        ink_near(&r, mx, my, 6),
        "expected shaft ink at the segment midpoint"
    );
}

#[test]
fn annotate_with_mathtext_renders() {
    let mut fig = base_figure();
    fig.axes_mut()[0].annotate("$\\sigma^2$", (5.0, 5.0), (7.0, 8.0));
    let png = fig.encode_png().unwrap();
    assert!(!png.is_empty());
}
