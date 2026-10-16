//! Marker codes against matplotlib 3.11: one black 12 pt marker at the center
//! of a 100x100 px figure inks the same pixels, centered the same way, as
//! Agg's, drawn on a line (`fixtures/markers_mpl.txt`) and scattered with an
//! edge (`fixtures/scatter_markers_mpl.txt`).

use rizzma::Figure;
use rizzma::artist::MarkerStyle;
use rizzma::core::color::Rgba;

/// A 100x100 px figure whose axes fill it, limits -1..1, axis off.
fn canvas() -> Figure {
    let mut fig = Figure::new(1.0, 1.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.set_xlim(-1.0, 1.0).set_ylim(-1.0, 1.0);
    fig
}

/// The sub-pixel ink extents `[x0, y0, x1, y1]` and the ink-weighted
/// centroid, in pixels from the top-left, of `fig`. A pixel with ink `c` puts
/// a left edge at `x + 1 - c` and a right edge at `x + c`, likewise
/// vertically; the extents are the outermost over all inked pixels.
fn ink(fig: &Figure) -> ([f64; 4], [f64; 2]) {
    let (rgb, w, _) = fig.to_rgb8();
    let mut extents = [f64::INFINITY, f64::INFINITY, 0.0, 0.0];
    let (mut total, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for (i, px) in rgb.chunks(3).enumerate() {
        let ink = 1.0 - px.iter().map(|&c| f64::from(c)).sum::<f64>() / (3.0 * 255.0);
        if ink <= 0.0 {
            continue;
        }
        let (x, y) = ((i % w as usize) as f64, (i / w as usize) as f64);
        extents = [
            extents[0].min(x + 1.0 - ink),
            extents[1].min(y + 1.0 - ink),
            extents[2].max(x + ink),
            extents[3].max(y + ink),
        ];
        total += ink;
        sx += ink * (x + 0.5);
        sy += ink * (y + 0.5);
    }
    (extents, [sx / total, sy / total])
}

/// Whether any of `a` lies more than half a pixel from its match in `b`.
fn off(a: &[f64], b: &[f64]) -> bool {
    a.iter().zip(b).any(|(p, q)| (p - q).abs() > 0.5)
}

/// `(code, values)` for each line of a fixture, skipping `skip` fields after
/// the code.
fn rows(fixture: &str, skip: usize) -> Vec<(char, Vec<f64>)> {
    fixture
        .lines()
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let code = fields[0].chars().next().expect("code");
            let values = fields[1 + skip..]
                .iter()
                .map(|v| v.parse().unwrap())
                .collect();
            (code, values)
        })
        .collect()
}

#[test]
fn line_markers_match_matplotlib() {
    let fixture = include_str!("fixtures/markers_mpl.txt");
    let fills: Vec<bool> = fixture
        .lines()
        .map(|l| l.split_whitespace().nth(1) == Some("1"))
        .collect();
    let mut mismatches = Vec::new();
    for ((code, values), filled) in rows(fixture, 1).into_iter().zip(fills) {
        let style = MarkerStyle::from_char(code).expect("known code");
        let mut fig = canvas();
        fig.axes_mut()[0]
            .plot(&[0.0], &[0.0])
            .set_color(Rgba::BLACK)
            .set_marker(&code.to_string())
            .set_markersize(12.0);
        let (extents, center) = ink(&fig);
        if style.is_filled() != filled || off(&extents, &values[..4]) || off(&center, &values[4..])
        {
            mismatches.push(format!(
                "{code:?}: filled {} extents {extents:.2?} centroid {center:.2?}; \
                 matplotlib filled {filled} {values:?}",
                style.is_filled()
            ));
        }
    }
    assert!(mismatches.is_empty(), "\n{}", mismatches.join("\n"));
}

/// Scatter edges join round, as a matplotlib collection's do whatever the
/// marker: each edged marker's ink spans what Agg's does. (Where a scatter's
/// markers sit, and the square's snapping, are compared separately.)
#[test]
fn edged_scatter_markers_span_what_matplotlib_spans() {
    let mut mismatches = Vec::new();
    for (code, values) in rows(include_str!("fixtures/scatter_markers_mpl.txt"), 0) {
        if code == 's' {
            continue;
        }
        let marker = MarkerStyle::from_char(code).expect("known code");
        let mut fig = canvas();
        fig.axes_mut()[0]
            .scatter(&[0.0], &[0.0])
            .set_marker(marker.path().clone())
            .set_sizes(vec![12.0])
            .set_facecolors(vec![Rgba::BLACK])
            .set_edgecolors(vec![Rgba::BLACK])
            .set_linewidth(1.0);
        let (extents, _) = ink(&fig);
        let spans = [extents[2] - extents[0], extents[3] - extents[1]];
        let theirs = [values[2] - values[0], values[3] - values[1]];
        if off(&spans, &theirs) {
            mismatches.push(format!(
                "{code:?}: spans {spans:.2?}; matplotlib {theirs:.2?}"
            ));
        }
    }
    assert!(mismatches.is_empty(), "\n{}", mismatches.join("\n"));
}
