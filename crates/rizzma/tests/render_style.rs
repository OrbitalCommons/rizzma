//! Style / `RcParams` wiring: the default look is unchanged, and a figure-level
//! `RcParams` (the dark preset especially), grid toggles, custom color cycles,
//! and tick direction flow from the figure into every axes it creates.

use rizzma::core::color::Rgba;
use rizzma::{Figure, RcParams, TickDirection};

fn pixel(r: &rizzma::skia::SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
    let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

/// Count pixels in the top-down rect `[x0, x1) x [y0, y1)` for which `pred`
/// holds.
fn count_in(
    r: &rizzma::skia::SkiaRenderer,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    pred: impl Fn([u8; 4]) -> bool,
) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if pred(pixel(r, x, y)) {
                n += 1;
            }
        }
    }
    n
}

fn is_white(p: [u8; 4]) -> bool {
    p == [255, 255, 255, 255]
}

/// Applying the default `RcParams` explicitly must be byte-for-byte identical to
/// the implicit default path — the guard that wiring rc through changed nothing.
#[test]
fn default_rcparams_matches_implicit_default() {
    let build = |rc: Option<RcParams>| {
        let mut fig = Figure::new(4.0, 3.0);
        if let Some(rc) = rc {
            fig.set_rcparams(rc);
        }
        let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
        ax.plot(&[0.0, 5.0, 10.0], &[0.0, 8.0, 3.0]);
        ax.plot(&[0.0, 5.0, 10.0], &[2.0, 1.0, 9.0]);
        ax.set_title("styled");
        ax.legend(vec![
            (Rgba::from_u8(31, 119, 180, 255), "a".to_string()),
            (Rgba::from_u8(255, 127, 14, 255), "b".to_string()),
        ]);
        ax.grid(true);
        fig.encode_png().expect("encode")
    };
    assert_eq!(
        build(None),
        build(Some(RcParams::default())),
        "explicit default rc must render identically to the implicit default"
    );
}

/// The dark preset darkens the canvas and lightens the spine ink, where the
/// default keeps a white canvas and black spine.
#[test]
fn dark_preset_darkens_canvas_and_lightens_ink() {
    let render = |rc: RcParams| {
        let mut fig = Figure::new(4.0, 3.0).with_rcparams(rc);
        let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        fig.render()
    };

    let light = render(RcParams::default());
    let dark = render(RcParams::dark());

    // Canvas corner = figure facecolor.
    let light_bg = pixel(&light, 3, 3);
    let dark_bg = pixel(&dark, 3, 3);
    assert!(is_white(light_bg), "default canvas white, got {light_bg:?}");
    assert!(
        dark_bg[0] < 60 && dark_bg[1] < 60 && dark_bg[2] < 60,
        "dark canvas should be near-black, got {dark_bg:?}"
    );

    // Left spine sits at x = 0.15 * 400 = 60. A thin column just left of it is
    // dark (spine ink) under the default and light under the dark preset.
    let light_spine = count_in(&light, 58, 61, 45, 255, |p| {
        p[0] < 40 && p[1] < 40 && p[2] < 40
    });
    let dark_spine = count_in(&dark, 58, 61, 45, 255, |p| {
        p[0] > 180 && p[1] > 180 && p[2] > 180
    });
    assert!(
        light_spine > 20,
        "default spine ink (dark), got {light_spine}"
    );
    assert!(
        dark_spine > 20,
        "dark-preset spine ink (light), got {dark_spine}"
    );
}

/// Grid is off by default and adds interior ink once enabled; a custom grid
/// color comes through.
#[test]
fn grid_toggle_and_custom_color() {
    let interior_ink = |setup: &dyn Fn(&mut rizzma::Axes)| {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        setup(ax);
        let r = fig.render();
        // Inset well inside the spines so only grid lines add non-white ink.
        count_in(&r, 70, 330, 55, 245, |p| !is_white(p))
    };

    let off = interior_ink(&|_ax| {});
    let on = interior_ink(&|ax| {
        ax.grid(true);
    });
    assert_eq!(off, 0, "no grid, no data => clean interior, got {off}");
    assert!(on > 100, "grid should add interior ink, got {on}");

    // A saturated red grid must leave clearly red pixels.
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
    ax.set_xlim(0.0, 10.0);
    ax.set_ylim(0.0, 10.0);
    ax.grid_with(Rgba::from_u8(220, 20, 20, 255), 1.5, 1.0);
    let r = fig.render();
    let red = count_in(&r, 70, 330, 55, 245, |p| {
        p[0] > 150 && p[1] < 90 && p[2] < 90
    });
    assert!(red > 50, "custom red grid should show, got {red}");
}

/// A custom property cycle drives `cycle_color`, wrapping modulo its length.
#[test]
fn custom_prop_cycle_is_used() {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    let red = Rgba::from_u8(200, 0, 0, 255);
    let green = Rgba::from_u8(0, 160, 0, 255);
    ax.set_prop_cycle(vec![red, green]);
    assert_eq!(ax.cycle_color(0), red);
    assert_eq!(ax.cycle_color(1), green);
    assert_eq!(ax.cycle_color(2), red, "cycle wraps");
}

/// Tick direction flows from the axis into the render (inward vs outward ticks
/// produce different pixels).
#[test]
fn tick_direction_changes_the_render() {
    let render = |dir: TickDirection| {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.2, 0.15, 0.65, 0.7);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        ax.xaxis_mut().set_tick_direction(dir);
        ax.yaxis_mut().set_tick_direction(dir);
        fig.encode_png().expect("encode")
    };
    assert_ne!(
        render(TickDirection::Out),
        render(TickDirection::In),
        "inward ticks must render differently from outward ticks"
    );
}
