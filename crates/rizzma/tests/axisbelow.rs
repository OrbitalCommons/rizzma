//! The grid draws where matplotlib 3.11 draws it (`axes.axisbelow`): a black
//! grid line at x = 2 crosses a bar (zorder 1) and a thick line (zorder 2),
//! and the pixel where they meet shows what matplotlib shows there —
//! checked against matplotlib with the same figure:
//!
//! | axisbelow | bar under the grid | line under the grid |
//! |---|---|---|
//! | `True` (Below) | bar | line |
//! | `"line"` (Line, default) | grid | line |
//! | `False` (Above) | grid | grid |

use rizzma::Figure;
use rizzma::core::AxisBelow;
use rizzma::core::color::Rgba;

const BAR: [u8; 3] = [0, 255, 0];
const LINE: [u8; 3] = [255, 0, 0];
const GRID: [u8; 3] = [0, 0, 0];

/// What the grid crossing shows over the bar and over the line.
fn crossings(axisbelow: Option<AxisBelow>) -> ([u8; 3], [u8; 3]) {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
    ax.set_prop_cycle(vec![Rgba::rgb(0.0, 1.0, 0.0)]);
    ax.bar(&[2.0], &[3.0]);
    ax.plot(&[0.5, 3.5], &[4.0, 4.0])
        .set_color(Rgba::rgb(1.0, 0.0, 0.0))
        .set_linewidth(8.0);
    ax.set_xlim(0.0, 4.0).set_ylim(0.0, 5.0);
    ax.set_xticks(&[2.0]).set_yticks(&[]);
    ax.grid_with(Rgba::BLACK, 2.0, 1.0);
    if let Some(axisbelow) = axisbelow {
        ax.set_axisbelow(axisbelow);
    }
    let (rgb, w, _) = fig.to_rgb8();
    let at = |x: f64, y: f64| {
        let (px, py) = fig.data_to_pixel(0, x, y).expect("axes 0");
        let i = (py as usize * w as usize + px as usize) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    (at(2.0, 1.0), at(2.0, 4.0))
}

#[test]
fn below_puts_the_grid_under_everything() {
    assert_eq!(crossings(Some(AxisBelow::Below)), (BAR, LINE));
}

#[test]
fn line_puts_the_grid_over_bars_and_under_lines() {
    assert_eq!(crossings(Some(AxisBelow::Line)), (GRID, LINE));
    assert_eq!(crossings(None), (GRID, LINE), "Line is the default");
}

#[test]
fn above_puts_the_grid_over_everything() {
    assert_eq!(crossings(Some(AxisBelow::Above)), (GRID, GRID));
}

#[test]
fn rc_sets_the_default() {
    use rizzma::core::RcParams;
    let rc = RcParams {
        axes_axisbelow: AxisBelow::Below,
        ..RcParams::default()
    };
    let mut fig = Figure::new(4.0, 3.0).with_rcparams(rc);
    let ax = fig.add_subplot(1, 1, 1);
    ax.bar(&[1.0], &[1.0]);
    ax.grid(true);
    let mut explicit = Figure::new(4.0, 3.0);
    let ax = explicit.add_subplot(1, 1, 1);
    ax.bar(&[1.0], &[1.0]);
    ax.grid(true).set_axisbelow(AxisBelow::Below);
    assert_eq!(fig.encode_png().unwrap(), explicit.encode_png().unwrap());
}
