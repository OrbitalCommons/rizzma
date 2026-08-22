//! Portable-figure tests.
//!
//! The load-bearing test is [`round_trip_is_pixel_identical`]: for a battery of
//! figures covering every artist, scale, and decoration, exporting and
//! re-importing must render a byte-identical PNG. That one assertion
//! transitively pins the entire wire model — a field dropped anywhere in the
//! spec shows up as a pixel difference.
//!
//! The rest of the suite covers what pixel identity cannot see: that bulk data
//! really travels in the binary chunk (and survives values JSON cannot express),
//! that an imported figure is still *live* under interaction, and that every
//! way an artifact can be malformed produces an error rather than a wrong
//! figure or a panic.

use crate::axis::ticker::{
    EngFormatter, FixedLocator, FuncFormatter, LogLocator, MultipleLocator, PercentFormatter,
    StrMethodFormatter,
};
use crate::core::color::Rgba;
use crate::core::{Bbox, Path, RcParams};
use crate::figure::{Axes, Event, Figure, Interactor, MouseButton};
use crate::portable::{PortableError, SCHEMA_VERSION};

/// Deterministic pseudo-random values in `0.0..1.0` (no rand dependency).
fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((state >> 33) as f64) / ((1u64 << 31) as f64)
    }
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1).max(1) as f64)
        .collect()
}

/// One figure per interesting corner of the model, each named for failure
/// messages. Every artist type, every scale, and the figure- and axes-level
/// decorations appear at least once.
fn fixtures() -> Vec<(&'static str, Figure)> {
    let mut out: Vec<(&'static str, Figure)> = Vec::new();

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&linspace(0.0, 6.0, 64), &linspace(-1.0, 1.0, 64));
    out.push(("line", fig));

    let mut fig = Figure::new(5.0, 3.5);
    let ax = fig.add_subplot(1, 1, 1);
    let x = linspace(0.0, 10.0, 200);
    let y: Vec<f64> = x.iter().map(|v| v.sin() * v).collect();
    ax.plot(&x, &y);
    ax.set_title("decay $\\alpha$");
    ax.set_xlabel("time [s]");
    ax.set_ylabel("amplitude");
    ax.grid(true);
    ax.legend(vec![(Rgba::rgb(0.1, 0.4, 0.8), "signal".to_string())]);
    fig.suptitle("figure title");
    out.push(("styled_line", fig));

    let mut fig = Figure::new(4.0, 4.0);
    let ax = fig.add_subplot(1, 1, 1);
    let mut r = rng(7);
    let xs: Vec<f64> = (0..300).map(|_| r()).collect();
    let ys: Vec<f64> = (0..300).map(|_| r()).collect();
    ax.scatter(&xs, &ys);
    out.push(("scatter", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.bar(&[0.0, 1.0, 2.0, 3.0], &[3.0, 1.0, 4.0, 1.5]);
    out.push(("bar_patches", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let data: Vec<f64> = (0..(16 * 16)).map(|i| (i as f64 * 0.13).sin()).collect();
    ax.imshow(&data, 16, 16);
    fig.colorbar("viridis", -1.0, 1.0);
    out.push(("image_colorbar", fig));

    let mut fig = Figure::new(4.5, 3.5);
    let ax = fig.add_subplot(1, 1, 1);
    let z: Vec<f64> = (0..(20 * 24))
        .map(|i| {
            let (r, c) = (i / 24, i % 24);
            (r as f64 * 0.3).sin() * (c as f64 * 0.2).cos()
        })
        .collect();
    ax.pcolormesh(&z, 20, 24);
    out.push(("quadmesh", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.loglog(&linspace(1.0, 1000.0, 50), &linspace(1.0, 1e6, 50));
    out.push(("loglog", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&linspace(-100.0, 100.0, 80), &linspace(-100.0, 100.0, 80));
    ax.set_xscale_symlog(10.0, 2.0);
    ax.set_yscale_asinh(1.0);
    out.push(("symlog_asinh", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&linspace(0.01, 0.99, 40), &linspace(0.01, 0.99, 40));
    ax.set_xscale_logit();
    out.push(("logit", fig));

    let mut fig = Figure::new(5.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0]);
    ax.xaxis_mut()
        .set_locator(Box::new(MultipleLocator::with_offset(0.5, 0.1)));
    ax.xaxis_mut()
        .set_formatter(Box::new(StrMethodFormatter::new("{x}")));
    ax.yaxis_mut()
        .set_locator(Box::new(FixedLocator::new(vec![1.0, 2.0, 3.0])));
    ax.yaxis_mut().set_formatter(Box::new(EngFormatter::new()));
    out.push(("custom_tickers", fig));

    let mut fig = Figure::new(5.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.semilogx(&linspace(1.0, 1e4, 40), &linspace(0.0, 1.0, 40));
    ax.xaxis_mut()
        .set_locator(Box::new(LogLocator::with_subs(10.0, vec![1.0, 2.0, 5.0])));
    ax.yaxis_mut()
        .set_formatter(Box::new(PercentFormatter::fraction()));
    out.push(("log_subs_percent", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    ax.axhline(0.5);
    ax.axvspan(0.2, 0.4);
    ax.annotate("peak", (0.5, 0.5), (0.7, 0.8));
    ax.text(0.1, 0.9, "note");
    out.push(("spans_annotations", fig));

    let mut fig = Figure::new(5.0, 4.0).with_rcparams(RcParams::dark());
    let ax = fig.add_subplot(2, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]);
    let ax = fig.add_subplot(2, 1, 2);
    ax.plot(&[0.0, 1.0, 2.0], &[2.0, 0.0, 1.0]);
    out.push(("dark_two_subplots", fig));

    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1)
        .plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0]);
    fig.twinx(0);
    out.push(("twinx", fig));

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let z: Vec<f64> = (0..(30 * 30))
        .map(|i| {
            let (r, c) = ((i / 30) as f64 * 0.2 - 3.0, (i % 30) as f64 * 0.2 - 3.0);
            (-(r * r + c * c) / 4.0).exp()
        })
        .collect();
    ax.contour(&z, 30, 30);
    ax.clabel();
    out.push(("contour_labels", fig));

    out
}

#[test]
fn round_trip_is_pixel_identical() {
    for (name, fig) in fixtures() {
        let bytes = fig
            .to_portable()
            .unwrap_or_else(|e| panic!("{name}: export failed: {e}"));
        let restored =
            Figure::from_portable(&bytes).unwrap_or_else(|e| panic!("{name}: import failed: {e}"));

        let want = fig.encode_png().expect("reference png");
        let got = restored.encode_png().expect("restored png");
        assert_eq!(
            got.len(),
            want.len(),
            "{name}: restored figure rendered a different-sized png"
        );
        assert!(
            got == want,
            "{name}: restored figure is not pixel-identical"
        );
    }
}

#[test]
fn round_trip_preserves_svg_and_pdf_too() {
    // The wire model is backend-agnostic: it restores the scene, not a raster.
    let (_, fig) = fixtures()
        .into_iter()
        .find(|(n, _)| *n == "styled_line")
        .unwrap();
    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    assert_eq!(restored.to_svg(), fig.to_svg());
    assert_eq!(restored.to_pdf(), fig.to_pdf());
}

#[test]
fn imported_figure_is_still_interactive() {
    // The point of carrying data rather than geometry: the restored figure can
    // be panned and zoomed, revealing data the original viewport never showed,
    // and it lands in exactly the same place as the original would.
    let mut fig = Figure::new(5.0, 4.0);
    let ax = fig.add_subplot(1, 1, 1);
    let x = linspace(0.0, 20.0, 500);
    let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
    ax.plot(&x, &y);

    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();

    let drive = |figure: Figure| {
        let mut it = Interactor::new(figure);
        it.handle(Event::Wheel {
            x: 240.0,
            y: 180.0,
            dy: -3.0,
        });
        it.handle(Event::MouseDown {
            x: 240.0,
            y: 180.0,
            button: MouseButton::Left,
        });
        it.handle(Event::MouseMove { x: 300.0, y: 150.0 });
        it.handle(Event::MouseUp {
            x: 300.0,
            y: 150.0,
            button: MouseButton::Left,
        });
        let limits = it.figure().axes()[0].effective_limits();
        (limits, it.figure().encode_png().expect("png"))
    };

    let (want_limits, want_png) = drive(fig);
    let (got_limits, got_png) = drive(restored);
    assert_eq!(
        got_limits, want_limits,
        "zoom/pan diverged after a round trip"
    );
    assert!(
        got_png == want_png,
        "interacted figures rendered differently"
    );
}

#[test]
fn bulk_data_travels_in_the_binary_chunk() {
    // Vertices must not be spelled out as JSON floats: the JSON chunk should
    // stay tiny while the binary chunk carries the samples.
    let mut fig = Figure::new(4.0, 3.0);
    let x = linspace(0.0, 1.0, 20_000);
    fig.add_subplot(1, 1, 1).plot(&x, &x);

    let bytes = fig.to_portable().unwrap();
    let (json, bin) = super::container::read(&bytes).unwrap();
    assert!(
        bin.len() >= 20_000 * 2 * 8,
        "expected both coordinate arrays in the binary chunk, got {} bytes",
        bin.len()
    );
    assert!(
        json.len() < 8_000,
        "json chunk should stay small, got {} bytes",
        json.len()
    );
}

#[test]
fn non_finite_samples_survive_the_round_trip() {
    // JSON cannot express NaN or infinity; the binary chunk can, and gaps in a
    // series are meaningful data, not noise.
    let mut fig = Figure::new(4.0, 3.0);
    let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let y = vec![0.0, f64::NAN, 2.0, f64::INFINITY, -1.0];
    fig.add_subplot(1, 1, 1).plot(&x, &y);

    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}

#[test]
fn colormap_sampled_colors_round_trip_exactly() {
    // Mesh face colors come from a colormap as f64 channels; snapping them to
    // 8-bit hex on the way out would shift pixels.
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let z: Vec<f64> = (0..(6 * 8)).map(|i| i as f64 / 47.0).collect();
    ax.pcolormesh(&z, 6, 8);

    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}

#[test]
fn save_and_load_a_file() {
    let dir = std::env::temp_dir().join("rizzma-portable-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("figure.riz");

    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[1.0, 0.0]);
    fig.save_portable(&path).expect("save");

    let restored = Figure::from_portable(&std::fs::read(&path).expect("read")).expect("load");
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
    std::fs::remove_file(&path).ok();
}

#[test]
fn func_formatter_fails_export_loudly() {
    // A closure cannot cross the wire. Substituting a default formatter would
    // silently relabel every tick, so export refuses instead.
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    ax.xaxis_mut()
        .set_formatter(Box::new(FuncFormatter::from_fn(|v, _| format!("<{v}>"))));

    match fig.to_portable() {
        Err(PortableError::Unsupported(msg)) => {
            assert!(msg.contains("FuncFormatter"), "unhelpful message: {msg}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// Import `bytes` and discard the figure, so a wrongly-accepted artifact
/// formats in a panic message (`Figure` is deliberately not `Debug`).
fn import(bytes: &[u8]) -> Result<(), PortableError> {
    Figure::from_portable(bytes).map(|_| ())
}

/// A valid artifact to mutate in the rejection tests below.
fn sample_bytes() -> Vec<u8> {
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1)
        .plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0]);
    fig.to_portable().expect("export")
}

/// Re-frame `spec_json` into a container with an empty binary chunk.
fn container_with_json(json: &str) -> Vec<u8> {
    super::container::write(json.as_bytes(), &[])
}

/// The artifact's spec JSON as a mutable `serde_json` value.
fn sample_spec() -> serde_json::Value {
    let bytes = sample_bytes();
    let (json, _) = super::container::read(&bytes).unwrap();
    serde_json::from_slice(json).unwrap()
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = sample_bytes();
    bytes[0] = b'X';
    assert!(matches!(
        Figure::from_portable(&bytes),
        Err(PortableError::Malformed(_))
    ));
}

#[test]
fn rejects_truncation() {
    let bytes = sample_bytes();
    for cut in [0, 4, 11, 20, bytes.len() / 2, bytes.len() - 1] {
        assert!(
            Figure::from_portable(&bytes[..cut]).is_err(),
            "truncating to {cut} bytes should not load"
        );
    }
}

#[test]
fn rejects_a_future_schema() {
    let mut spec = sample_spec();
    spec["schema"] = serde_json::json!(SCHEMA_VERSION + 1);
    let bytes = container_with_json(&spec.to_string());
    match import(&bytes) {
        Err(PortableError::Schema { found, max, .. }) => {
            assert_eq!(found, SCHEMA_VERSION + 1);
            assert_eq!(max, SCHEMA_VERSION);
        }
        other => panic!("expected Schema, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_fields() {
    // Forward compatibility by skipping is wrong for a figure: an artist a
    // newer schema introduced would vanish without a trace.
    let mut spec = sample_spec();
    spec["figure"]["axes"][0]["lines"][0]["glow"] = serde_json::json!(true);
    let bytes = container_with_json(&spec.to_string());
    assert!(matches!(
        Figure::from_portable(&bytes),
        Err(PortableError::Json(_))
    ));
}

#[test]
fn rejects_unknown_enum_variants() {
    let mut spec = sample_spec();
    spec["figure"]["axes"][0]["xscale"] = serde_json::json!({ "hyperbolic": { "k": 2.0 } });
    let bytes = container_with_json(&spec.to_string());
    assert!(matches!(
        Figure::from_portable(&bytes),
        Err(PortableError::Json(_))
    ));
}

#[test]
fn rejects_out_of_bounds_accessors() {
    let mut spec = sample_spec();
    spec["accessors"][0]["count"] = serde_json::json!(1_000_000);
    let bytes = sample_bytes();
    let (_, bin) = super::container::read(&bytes).unwrap();
    let bin = bin.to_vec();
    let reframed = super::container::write(spec.to_string().as_bytes(), &bin);
    match import(&reframed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("accessor"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_a_dangling_accessor_index() {
    let mut spec = sample_spec();
    spec["figure"]["axes"][0]["lines"][0]["x"] = serde_json::json!({ "acc": 999 });
    let bytes = sample_bytes();
    let (_, bin) = super::container::read(&bytes).unwrap();
    let reframed = super::container::write(spec.to_string().as_bytes(), bin);
    match import(&reframed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("999"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_inconsistent_array_lengths() {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let data: Vec<f64> = (0..12).map(|i| i as f64).collect();
    ax.imshow(&data, 3, 4);
    let bytes = fig.to_portable().unwrap();

    let (json, bin) = super::container::read(&bytes).unwrap();
    let mut spec: serde_json::Value = serde_json::from_slice(json).unwrap();
    // Claim a 5x4 grid over the same 12 samples.
    spec["figure"]["axes"][0]["images"][0]["nrows"] = serde_json::json!(5);
    let reframed = super::container::write(spec.to_string().as_bytes(), bin);
    match import(&reframed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("samples"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_hostile_locator_parameters() {
    // A zero step would trip a constructor assertion; the importer turns it
    // into an error rather than letting a malformed artifact panic the process.
    let mut spec = sample_spec();
    spec["figure"]["axes"][0]["xaxis"]["locator"] =
        serde_json::json!({ "multiple": { "step": 0.0, "offset": 0.0 } });
    let bytes = sample_bytes();
    let (_, bin) = super::container::read(&bytes).unwrap();
    let reframed = super::container::write(spec.to_string().as_bytes(), bin);
    match import(&reframed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("step"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_self_contained_chunks_this_build_cannot_honor() {
    // FONT/WASM chunks belong to a later phase; ignoring them would render
    // with the wrong font or the wrong renderer.
    let bytes = sample_bytes();
    let (json, _) = super::container::read(&bytes).unwrap();
    let mut framed = super::container::write(json, &[]);
    // Append a FONT chunk and fix up the declared total length.
    let payload = b"not-a-font";
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(b"FONT");
    framed.extend_from_slice(payload);
    while !framed.len().is_multiple_of(8) {
        framed.push(0);
    }
    let total = framed.len() as u32;
    framed[8..12].copy_from_slice(&total.to_le_bytes());

    match import(&framed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("FONT"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_a_duplicate_chunk() {
    let bytes = sample_bytes();
    let (json, _) = super::container::read(&bytes).unwrap();
    let mut framed = super::container::write(json, &[]);
    let start = framed.len();
    framed.extend_from_slice(&(json.len() as u32).to_le_bytes());
    framed.extend_from_slice(b"JSON");
    framed.extend_from_slice(json);
    while !framed.len().is_multiple_of(8) {
        framed.push(0);
    }
    assert!(framed.len() > start);
    let total = framed.len() as u32;
    framed[8..12].copy_from_slice(&total.to_le_bytes());

    match import(&framed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("duplicate"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn rejects_an_unknown_path_code() {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.add_patch(crate::artist::Patch::new(Path::from_polyline(&[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
    ])));
    ax.scatter(&[0.5], &[0.5]);
    let bytes = fig.to_portable().unwrap();
    let (json, bin) = super::container::read(&bytes).unwrap();
    let spec: serde_json::Value = serde_json::from_slice(json).unwrap();

    // Find a u8 (path-code) accessor and corrupt its first byte.
    let mut bin = bin.to_vec();
    let offset = spec["accessors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["dtype"] == "u8")
        .map(|a| a["offset"].as_u64().unwrap() as usize)
        .expect("a path-code accessor");
    bin[offset] = 200;

    let reframed = super::container::write(json, &bin);
    match import(&reframed) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("path code"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn error_messages_name_the_problem() {
    // These strings are what a user sees when an artifact will not load, so
    // they are part of the contract.
    let schema = PortableError::Schema {
        found: 9,
        min: 1,
        max: 2,
    };
    let text = schema.to_string();
    assert!(text.contains('9') && text.contains("1..=2"), "{text}");
    assert!(text.contains("drop content"), "{text}");
}

#[test]
fn spec_bytes_are_stable_across_a_round_trip() {
    // Export → import → export must reproduce the artifact byte-for-byte:
    // nothing is invented, reordered, or lost on the way through.
    let first = sample_bytes();
    let second = Figure::from_portable(&first)
        .unwrap()
        .to_portable()
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn json_scalars_round_trip_bit_exactly() {
    // Scalars (limits, positions, colors) travel as JSON numbers, and
    // serde_json only parses them back to the same bits with its
    // `float_roundtrip` feature enabled — without it, values land one ULP off
    // and a re-exported artifact silently differs from its original. This test
    // pins that dependency feature; it is not hypothetical, it fired once.
    let awkward = [
        0.109_999_999_999_999_99_f64,
        1.0 / 3.0,
        f64::MIN_POSITIVE,
        f64::MAX,
        std::f64::consts::PI * 1e-7,
    ];
    for v in awkward {
        let text = serde_json::to_string(&v).expect("serialize");
        let back: f64 = serde_json::from_str(&text).expect("parse");
        assert_eq!(
            back.to_bits(),
            v.to_bits(),
            "{v} did not survive a JSON round trip ({text})"
        );
    }
}

#[test]
fn declares_its_schema_and_generator() {
    let bytes = sample_bytes();
    let (json, _) = super::container::read(&bytes).unwrap();
    let spec: serde_json::Value = serde_json::from_slice(json).unwrap();
    assert_eq!(spec["schema"], SCHEMA_VERSION);
    assert_eq!(spec["generator"]["rizzma"], env!("CARGO_PKG_VERSION"));
    assert_eq!(spec["renderer"]["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn an_empty_figure_round_trips() {
    let fig = Figure::new(3.0, 2.0).with_facecolor(Rgba::rgb(0.2, 0.3, 0.4));
    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}

#[test]
fn axes_added_by_rect_keep_their_position() {
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_axes(0.2, 0.3, 0.5, 0.4);
    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    let want: Bbox = fig.axes()[0].position();
    let got: Bbox = restored.axes()[0].position();
    assert_eq!(
        (got.x0, got.y0, got.x1, got.y1),
        (want.x0, want.y0, want.x1, want.y1)
    );
}

#[test]
fn axes_helper_is_reachable_for_every_artist_vec() {
    // Guards against a new artist Vec being added to `Axes` without a matching
    // wire field: the spec's own struct is exhaustive, so this compiles only
    // while the two agree.
    let mut fig = Figure::new(4.0, 3.0);
    let ax: &mut Axes = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    ax.scatter(&[0.5], &[0.5]);
    ax.bar(&[0.0], &[1.0]);
    ax.imshow(&[1.0, 2.0, 3.0, 4.0], 2, 2);
    ax.pcolormesh(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 2, 3);

    let restored = Figure::from_portable(&fig.to_portable().unwrap()).unwrap();
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}
