//! An unknown colormap name still draws with the default map, exactly as
//! before, but is reported once per distinct name with what to use instead.
//! The warning goes to stderr, so the reporting test re-runs this test
//! binary as a child and reads the child's stderr.

use rizzma::Figure;
use rizzma::core::color::try_colormap;

/// The figure every test draws: a gradient through colormap `name`.
fn gradient(name: &str) -> Figure {
    let data: Vec<f64> = (0..64).map(f64::from).collect();
    let mut fig = Figure::new(2.0, 1.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.imshow(&data, 1, 64).cmap(name);
    fig
}

/// Set in the child: draw with the unknown names, warn, and exit.
const CHILD: &str = "RIZZMA_UNKNOWN_COLORMAP_CHILD";

#[test]
fn unknown_names_draw_the_default_byte_identically() {
    assert_eq!(
        gradient("jet").encode_png().unwrap(),
        gradient("bgyw").encode_png().unwrap()
    );
}

#[test]
fn try_colormap_fails_hard_on_unknown_names() {
    assert!(try_colormap("jet").is_err());
    assert!(try_colormap("misleading:jet").is_ok());
}

/// The child half of [`each_unknown_name_is_reported_once`].
#[test]
fn child_draws_with_unknown_names() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    for _ in 0..3 {
        gradient("jet").encode_png().unwrap();
        gradient("viridis").encode_png().unwrap();
        gradient("virdis").encode_png().unwrap();
    }
}

#[test]
fn each_unknown_name_is_reported_once() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_draws_with_unknown_names", "--nocapture"])
        .env(CHILD, "1")
        .output()
        .expect("the test binary re-runs");
    assert!(out.status.success(), "child failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let warnings: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("rizzma: unknown colormap"))
        .collect();
    assert_eq!(
        warnings,
        [
            r#"rizzma: unknown colormap "jet"; did you mean "misleading:jet" or "cet_r2"? (drawing with the default "bgyw")"#,
            r#"rizzma: unknown colormap "virdis"; did you mean "viridis"? (drawing with the default "bgyw")"#,
        ],
        "stderr:\n{stderr}"
    );
}
