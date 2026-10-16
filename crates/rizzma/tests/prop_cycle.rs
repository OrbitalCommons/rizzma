//! Every cycled artist shares the axes' property cycle, as in matplotlib:
//! after `set_prop_cycle`, `plot`, `scatter`, `hist` and `bar` called in
//! sequence take colors 1, 2, 3, 1.

use rizzma::Figure;
use rizzma::core::color::Rgba;

#[test]
fn plot_scatter_hist_and_bar_share_a_custom_cycle() {
    let cycle = [
        Rgba::rgb(1.0, 0.0, 0.0),
        Rgba::rgb(0.0, 0.6, 0.0),
        Rgba::rgb(0.0, 0.0, 1.0),
    ];
    let mut fig = Figure::new(6.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.set_prop_cycle(cycle.to_vec());
    // Each artist in its own column, so its color can be read back.
    ax.plot(&[0.0, 1.0], &[2.0, 2.0]).set_linewidth(8.0);
    ax.scatter(&[2.0], &[2.0]).set_sizes(vec![15.0]);
    ax.hist(&[3.2, 3.4, 3.6, 3.8], 1);
    ax.bar(&[5.0], &[3.0]);
    ax.set_xlim(-0.5, 6.0).set_ylim(0.0, 5.0);

    let (rgb, w, _) = fig.to_rgb8();
    let at = |x: f64, y: f64| {
        let (px, py) = fig.data_to_pixel(0, x, y).expect("axes 0");
        let i = (py as usize * w as usize + px as usize) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    let expected = |c: Rgba| {
        let u = c.to_u8_array();
        [u[0], u[1], u[2]]
    };
    assert_eq!(at(0.5, 2.0), expected(cycle[0]), "plot takes color 1");
    assert_eq!(at(2.0, 2.0), expected(cycle[1]), "scatter takes color 2");
    assert_eq!(at(3.5, 1.0), expected(cycle[2]), "hist takes color 3");
    assert_eq!(at(5.0, 1.5), expected(cycle[0]), "bar wraps to color 1");
}
