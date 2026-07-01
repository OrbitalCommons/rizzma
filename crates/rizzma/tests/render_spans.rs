//! End-to-end render test for full-span reference lines.
//!
//! Draws an `axhline` at `y = 0.5` on an axes with explicit `[0, 1]` limits and
//! asserts the rendered raster has a horizontal band of ink spanning roughly the
//! full axes width at the expected row.
//!
//! Pixmap coordinates have a top-left origin while the axes data space is y-UP,
//! so a higher `y` value sits at a *smaller* pixmap row.

use rizzma::core::color::Rgba;
use rizzma::figure::Figure;
use rizzma::skia::SkiaRenderer;

/// Whether the pixel at `(x, y)` (pixmap space) has visible (non-near-white,
/// opaque) ink. The axes background is white, so dark ink reads as low RGB.
fn is_ink(r: &SkiaRenderer, x: u32, y: u32) -> bool {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    p.alpha() > 200 && (p.red() as u16 + p.green() as u16 + p.blue() as u16) < 600
}

#[test]
fn axhline_renders_full_width_horizontal_line() {
    let mut fig = Figure::new(4.0, 4.0).with_dpi(100.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    ax.set_xlim(0.0, 1.0);
    ax.set_ylim(0.0, 1.0);
    ax.set_facecolor(Rgba::WHITE);
    ax.axhline(0.5);

    let r = fig.render();
    let (w_px, h_px) = fig.size_px();
    let h = h_px as u32;

    // Axes rectangle in pixmap space. The figure-fraction position is
    // [0.1, 0.9] in both axes; the pixmap row is flipped relative to data y.
    let ax_x0 = (0.1 * w_px) as u32;
    let ax_x1 = (0.9 * w_px) as u32;
    // y = 0.5 sits at the vertical center of the axes; with a symmetric axes
    // rectangle the center row is simply the figure's vertical center.
    let center_row = h / 2;

    // Search a few rows around the expected center for the line (anti-aliasing
    // and line width spread the ink across a couple of rows).
    let mut best_row = center_row;
    let mut best_count = 0u32;
    for row in (center_row.saturating_sub(3))..=(center_row + 3).min(h - 1) {
        let mut count = 0;
        for x in (ax_x0 + 2)..(ax_x1 - 2) {
            if is_ink(&r, x, row) {
                count += 1;
            }
        }
        if count > best_count {
            best_count = count;
            best_row = row;
        }
    }

    let axes_width = ax_x1 - ax_x0;
    assert!(
        best_count as f64 > 0.9 * f64::from(axes_width),
        "axhline should span ~full axes width: covered {best_count} of {axes_width} \
         columns at row {best_row}"
    );
    assert!(
        best_row.abs_diff(center_row) <= 3,
        "line row {best_row} should be near the axes center {center_row}"
    );
}
