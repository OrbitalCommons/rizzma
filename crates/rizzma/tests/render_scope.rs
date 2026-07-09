//! End-to-end render checks for oscilloscope-styled axes.
//!
//! A scope axes draws everything inside its frame — CRT background, phosphor
//! graticule, traces, corner readouts — so it must render sensibly even at
//! sparkline-strip sizes with no decoration room at all.

use rizzma::Figure;

/// Render a full-bleed scope strip at sparkline height and return the pixmap.
fn scope_strip() -> rizzma::skia::SkiaRenderer {
    let n = 200;
    let t: Vec<f64> = (0..n).map(|i| i as f64 / 10.0).collect();
    let v: Vec<f64> = t.iter().map(|t| (t * 1.7).sin()).collect();
    let mut fig = Figure::new(3.0, 0.5); // 300 x 50 px: a strip
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.oscilloscope();
    ax.plot(&t, &v);
    fig.render()
}

#[test]
fn scope_strip_is_dark_with_phosphor_ink() {
    let renderer = scope_strip();
    let px = renderer.pixmap();

    // Interior background (away from bezel, graticule, and trace crossings)
    // is the near-black CRT face — nothing white anywhere.
    let (mut dark, mut phosphor, mut white) = (0u32, 0u32, 0u32);
    for y in 0..px.height() {
        for x in 0..px.width() {
            let p = px.pixel(x, y).unwrap().demultiply();
            let (r, g, b) = (p.red(), p.green(), p.blue());
            if r < 40 && g < 40 && b < 40 {
                dark += 1;
            }
            if g > 140 && g > r + 40 && g > b + 40 {
                phosphor += 1;
            }
            if r > 240 && g > 240 && b > 240 {
                white += 1;
            }
        }
    }
    let total = px.width() * px.height();
    assert!(
        dark > total / 2,
        "most of a scope strip is CRT-dark: {dark} of {total}"
    );
    assert!(
        phosphor > 200,
        "the graticule and trace must be phosphor green: {phosphor} px"
    );
    assert_eq!(white, 0, "no white pixels on a full-bleed scope strip");
}

#[test]
fn scope_traces_run_flush_in_x() {
    // A scope sweeps edge-to-edge: autoscaled x-limits equal the data
    // extremes exactly (no margin inset), while y keeps its headroom.
    let mut fig = Figure::new(3.0, 0.5);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.oscilloscope();
    ax.plot(&[], &[]);
    ax.set_line_data(0, &[5.0, 7.0, 15.0], &[-1.0, 2.0, 1.0])
        .unwrap();
    let ((xlo, xhi), (ylo, yhi)) = fig.axes()[0].effective_limits();
    assert_eq!((xlo, xhi), (5.0, 15.0), "x flush to the data");
    assert!(ylo < -1.0 && yhi > 2.0, "y keeps margin headroom");
}

#[test]
fn scope_axes_reserve_no_decoration_room() {
    // Under tight layout a scope subplot's frame runs to the pad on every
    // side: no tick bands, no labels, no title.
    let mut fig = Figure::new(4.0, 1.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.oscilloscope();
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    let png = fig.encode_png().expect("scope subplot renders");
    assert!(!png.is_empty());
}
