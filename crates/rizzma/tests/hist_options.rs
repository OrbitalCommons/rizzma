//! `hist_with` matches matplotlib 3.11's `Axes.hist` exactly: counts and
//! edges for counted and explicit bins, a range, weights, density and
//! cumulative counts, and the outline of `histtype="step"` and
//! `"stepfilled"`. References come from `fixtures/hist_mpl.py`.

use rizzma::Figure;
use rizzma::core::color::Rgba;
use rizzma::figure::{Bins, HistOptions, HistType};

const DATA: [f64; 14] = [
    0.3, 1.7, 2.2, 2.2, 2.9, 3.4, 3.4, 3.4, 4.1, 4.8, 5.5, 6.0, 7.25, 9.9,
];
const EDGES: [f64; 6] = [0.0, 1.0, 2.5, 3.4, 5.0, 10.0];
const REFERENCE: &str = include_str!("fixtures/hist_mpl.txt");

/// The weights the fixture uses, computed as `numpy.linspace(0.5, 2.0, 14)`
/// does (`i * step + start`, the last pinned to the stop).
fn weights() -> Vec<f64> {
    let step = 1.5 / 13.0;
    let mut w: Vec<f64> = (0..14).map(|i| f64::from(i) * step + 0.5).collect();
    w[13] = 2.0;
    w
}

/// The reference row `kind name` as numbers.
fn row(kind: &str, name: &str) -> Vec<f64> {
    let prefix = format!("{kind} {name} ");
    let line = REFERENCE
        .lines()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no {kind} row for {name}"));
    line[prefix.len()..]
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

/// matplotlib's vertices for the `histtype` outline.
fn path(histtype: &str) -> Vec<[f64; 2]> {
    let mut lines = REFERENCE
        .lines()
        .skip_while(|l| !l.starts_with(&format!("path {histtype} ")));
    let n: usize = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse()
        .unwrap();
    lines
        .take(n)
        .map(|l| {
            let v: Vec<f64> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            [v[0], v[1]]
        })
        .collect()
}

#[test]
fn counts_and_edges_match_matplotlib_exactly() {
    let w = weights();
    let cases: Vec<(&str, HistOptions)> = vec![
        ("count8", HistOptions::new().bins(Bins::Count(8))),
        (
            "edges",
            HistOptions::new().bins(Bins::Edges(EDGES.to_vec())),
        ),
        (
            "range",
            HistOptions::new().bins(Bins::Count(5)).range(2.0, 7.0),
        ),
        (
            "density",
            HistOptions::new().bins(Bins::Count(8)).density(true),
        ),
        (
            "cumulative",
            HistOptions::new().bins(Bins::Count(8)).cumulative(true),
        ),
        (
            "density_cumulative",
            HistOptions::new()
                .bins(Bins::Edges(EDGES.to_vec()))
                .density(true)
                .cumulative(true),
        ),
        (
            "weights",
            HistOptions::new().bins(Bins::Count(6)).weights(&w),
        ),
        (
            "weights_density",
            HistOptions::new()
                .bins(Bins::Edges(EDGES.to_vec()))
                .weights(&w)
                .density(true),
        ),
    ];
    for (name, options) in cases {
        let mut fig = Figure::new(4.0, 3.0);
        let result = fig.add_subplot(1, 1, 1).hist_with(&DATA, &options);
        assert_eq!(result.counts, row("counts", name), "{name} counts");
        assert_eq!(result.edges, row("edges", name), "{name} edges");
    }
}

#[test]
fn step_outlines_match_matplotlib() {
    for (histtype, kind) in [
        ("step", HistType::Step),
        ("stepfilled", HistType::StepFilled),
    ] {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_subplot(1, 1, 1);
        let result = ax.hist_with(
            &DATA,
            &HistOptions::new()
                .bins(Bins::Edges(EDGES.to_vec()))
                .histtype(kind),
        );
        assert_eq!(result.patches.len(), 1, "{histtype}: one outline");
        let patch = &ax.patches()[result.patches.start];
        assert_eq!(
            patch.path().vertices(),
            path(histtype).as_slice(),
            "{histtype}"
        );
        assert_eq!(
            patch.face().is_some(),
            kind == HistType::StepFilled,
            "{histtype} fill"
        );
    }
}

#[test]
fn a_labelled_histogram_joins_the_legend_and_bars_restyle() {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let result = ax.hist_with(
        &DATA,
        &HistOptions::new()
            .bins(Bins::Count(4))
            .color(Rgba::BLUE)
            .label("GDIST"),
    );
    assert_eq!(result.patches.len(), 4);
    for patch in &mut ax.patches_mut()[result.patches.clone()] {
        patch.set_edgecolor(None);
    }
    assert!(
        ax.patches()[result.patches.clone()]
            .iter()
            .all(|p| p.edge().is_none())
    );
    assert_eq!(ax.patches()[result.patches.start].face(), Some(Rgba::BLUE));
    ax.legend_auto();
    let plain = {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.hist_with(
            &DATA,
            &HistOptions::new().bins(Bins::Count(4)).color(Rgba::BLUE),
        );
        ax.legend_auto();
        fig.encode_png().unwrap()
    };
    assert_ne!(fig.encode_png().unwrap(), plain, "the label adds a legend");
}

#[test]
fn plain_hist_is_hist_with_defaults() {
    let draw = |with: bool| {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_subplot(1, 1, 1);
        if with {
            ax.hist_with(&DATA, &HistOptions::new().bins(Bins::Count(7)));
        } else {
            ax.hist(&DATA, 7);
        }
        fig.encode_png().unwrap()
    };
    assert_eq!(draw(true), draw(false));
}
