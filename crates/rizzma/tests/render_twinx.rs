//! Render tests for `Figure::twinx` and `Axes::secondary_xaxis_linear`: the
//! twin shares its source's x pixel mapping (tracking later limit changes)
//! while keeping an independent right-hand y axis, and the secondary axis
//! decorates the top edge.

use rizzma::Figure;

fn pixel(r: &rizzma::skia::SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

/// Count non-white pixels in the top-down pixel rect `[x0, x1) x [y0, y1)`.
fn ink_in(r: &rizzma::skia::SkiaRenderer, x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if pixel(r, x, y) != [255, 255, 255, 255] {
                n += 1;
            }
        }
    }
    n
}

fn twin_figure() -> Figure {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.15, 0.15, 0.70, 0.75);
    ax.plot(&[0.0, 5.0, 10.0], &[0.0, 4.0, 2.0]);
    ax.set_xlim(0.0, 10.0);
    ax.set_ylim(0.0, 5.0);
    let twin = fig.twinx(0);
    let tw = &mut fig.axes_mut()[twin];
    tw.plot(&[0.0, 5.0, 10.0], &[100.0, 900.0, 400.0]);
    tw.set_ylim(0.0, 1000.0);
    fig
}

#[test]
fn twin_shares_the_source_x_pixel_mapping() {
    let fig = twin_figure();
    for x in [0.0, 2.5, 5.0, 10.0] {
        let (px_src, _) = fig.data_to_pixel(0, x, 1.0).expect("primary maps");
        let (px_twin, _) = fig.data_to_pixel(1, x, 500.0).expect("twin maps");
        assert!(
            (px_src - px_twin).abs() < 1e-9,
            "x = {x}: primary px {px_src} != twin px {px_twin}"
        );
    }
}

#[test]
fn twin_tracks_later_source_limit_changes() {
    let mut fig = twin_figure();
    // Zoom the primary after the twin exists; the twin must follow.
    fig.axes_mut()[0].set_xlim(2.0, 4.0);
    let (px_src, _) = fig.data_to_pixel(0, 3.0, 1.0).expect("primary maps");
    let (px_twin, _) = fig.data_to_pixel(1, 3.0, 500.0).expect("twin maps");
    assert!(
        (px_src - px_twin).abs() < 1e-9,
        "after set_xlim: primary px {px_src} != twin px {px_twin}"
    );
    // And the twin's own (stale) x data no longer defines its mapping: x = 10
    // now sits right of the axes rect for both.
    let (edge_src, _) = fig.data_to_pixel(0, 4.0, 1.0).unwrap();
    let (far_twin, _) = fig.data_to_pixel(1, 10.0, 500.0).unwrap();
    assert!(far_twin > edge_src, "twin x mapping must be the zoomed one");
}

#[test]
fn twin_keeps_an_independent_y_mapping() {
    let fig = twin_figure();
    // The same pixel row means different data y on each axes: y = 2.5 on the
    // primary (mid-scale of 0..5) coincides with y = 500 on the twin
    // (mid-scale of 0..1000).
    let (_, py_src) = fig.data_to_pixel(0, 5.0, 2.5).unwrap();
    let (_, py_twin) = fig.data_to_pixel(1, 5.0, 500.0).unwrap();
    assert!(
        (py_src - py_twin).abs() < 1e-9,
        "mid-scale rows must coincide: {py_src} vs {py_twin}"
    );
}

#[test]
fn twin_draws_y_decoration_on_the_right() {
    let fig = twin_figure();
    let r = fig.render();
    // Axes rect: x in [60, 340] px, y-up [45, 270] -> top-down rows [30, 255].
    // Right-side tick labels live right of x = 340.
    let right_ink = ink_in(&r, 345, 400, 30, 255);
    assert!(
        right_ink > 20,
        "expected right-hand y tick labels, got {right_ink} ink px"
    );
}

#[test]
fn secondary_xaxis_decorates_the_top_edge() {
    let mut plain = Figure::new(4.0, 3.0);
    let ax = plain.add_axes(0.15, 0.18, 0.70, 0.60);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
    let baseline = plain.render();

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.15, 0.18, 0.70, 0.60);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
    ax.secondary_xaxis_linear(25.4, 0.0, Some("mm"));
    let r = fig.render();

    // Above the top spine (y-up 0.78 * 300 = 234 -> top-down row 66): the
    // converted tick labels and axis label add ink the plain figure lacks.
    let with = ink_in(&r, 60, 340, 10, 64);
    let without = ink_in(&baseline, 60, 340, 10, 64);
    assert!(
        with > without + 20,
        "expected secondary-axis ink above the spine: {with} vs {without}"
    );
}
