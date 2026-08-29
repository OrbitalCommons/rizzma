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
use crate::portable::{Limits, PortableConfig, PortableError, SCHEMA_VERSION};

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
    let (json, bin) = split(&bytes);
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

/// The `JSON` and `BIN ` payloads of an artifact, for tests that rewrite one
/// and reframe the other unchanged.
fn split(bytes: &[u8]) -> (&[u8], &[u8]) {
    use crate::portable::container;
    let dir = container::directory(bytes, &Limits::default()).expect("valid container");
    (
        container::chunk(bytes, &dir, container::TAG_JSON).expect("json chunk"),
        container::chunk(bytes, &dir, container::TAG_BIN).unwrap_or(&[]),
    )
}

/// Reframe a (possibly doctored) spec and binary payload into an artifact.
fn reframe(json: &[u8], bin: &[u8]) -> Vec<u8> {
    use crate::portable::container;
    container::write(&[(container::TAG_JSON, json), (container::TAG_BIN, bin)])
}

/// Import `bytes` and discard the figure, so a wrongly-accepted artifact
/// formats in a panic message (`Figure` is deliberately not `Debug`).
fn import(bytes: &[u8]) -> Result<(), PortableError> {
    Figure::from_portable(bytes).map(|_| ())
}

#[test]
fn the_poster_is_what_the_renderer_would_draw() {
    // The poster is a byproduct of a figure the exporter already holds, so it
    // must be exactly the PNG that figure renders — not an approximation.
    let mut fig = Figure::new(5.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&linspace(0.0, 6.0, 50), &linspace(0.0, 1.0, 50));
    ax.set_title("decay");

    let bytes = fig.to_portable().expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");
    let poster = info.poster(&bytes).expect("a poster by default");

    assert_eq!(&poster[..8], b"\x89PNG\r\n\x1a\n", "poster must be a PNG");
    assert!(
        poster == fig.encode_png().expect("png").as_slice(),
        "poster differs from the figure's own render"
    );
    let meta = info.meta.expect("schema 2 carries meta");
    assert_eq!(meta.poster.expect("poster ref").bytes, poster.len());
}

#[test]
fn inspect_sizes_a_card_without_a_renderer() {
    // What a host reads before deciding to fetch 370 KB of wasm: the exact box
    // to reserve, the text to announce, and something to show meanwhile.
    let mut fig = Figure::new(6.4, 4.8).with_dpi(100.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    ax.set_title("step response");

    let cfg = PortableConfig::default().with_alt("step response rising to one");
    let bytes = fig.to_portable_with(&cfg).expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");

    assert!(info.renderable(), "this build must render its own schema");
    let meta = info.meta.expect("meta");
    assert_eq!((meta.width_px, meta.height_px), (640, 480));
    assert_eq!(meta.alt.as_deref(), Some("step response rising to one"));
    assert_eq!(meta.title.as_deref(), Some("step response"));
    assert!(!meta.animated);
    assert_eq!(info.generator.rizzma, env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_poster_can_be_turned_off() {
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[1.0, 0.0]);

    let with = fig.to_portable().expect("export");
    let without = fig
        .to_portable_with(&PortableConfig::default().with_poster(false))
        .expect("export");
    assert!(
        without.len() < with.len(),
        "dropping the poster should shrink the artifact"
    );

    let info = crate::portable::inspect(&without, &Limits::default()).expect("inspect");
    assert!(info.poster(&without).is_none());
    assert!(info.meta.expect("meta").poster.is_none());
    // Still a complete figure: the poster is a fallback, not the content.
    let restored = Figure::from_portable(&without).expect("import");
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}

#[test]
fn schema_1_artifacts_still_load() {
    // Artifacts written by 1.7.x carry no `meta` and no poster. Refusing them
    // would break every figure already archived, so the field is optional for
    // as long as SCHEMA_MIN is 1.
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1)
        .plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0]);
    let current = fig.to_portable().expect("export");

    // Rewrite it as a schema-1 document: drop `meta` and the poster chunk.
    let (json, bin) = split(&current);
    let mut spec: serde_json::Value = serde_json::from_slice(json).expect("json");
    spec["schema"] = serde_json::json!(1);
    spec.as_object_mut().expect("object").remove("meta");
    let legacy = reframe(spec.to_string().as_bytes(), bin);

    let info = crate::portable::inspect(&legacy, &Limits::default()).expect("inspect");
    assert_eq!(info.schema, 1);
    assert!(info.schema_supported, "1 is still within the range");
    assert!(info.meta.is_none());

    let restored = Figure::from_portable(&legacy).expect("schema 1 must still import");
    assert!(restored.encode_png().unwrap() == fig.encode_png().unwrap());
}

#[test]
fn import_enforces_caller_budgets() {
    let bytes = sample_bytes();
    let tight = Limits::default().with_max_total_bytes(128);
    match Figure::from_portable_limited(&bytes, &tight).map(|_| ()) {
        Err(PortableError::Budget(msg)) => assert!(msg.contains("128"), "{msg}"),
        other => panic!("expected Budget, got {other:?}"),
    }
    // The default budget admits an ordinary figure.
    assert!(Figure::from_portable_limited(&bytes, &Limits::default()).is_ok());
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
    reframe(json.as_bytes(), &[])
}

/// The artifact's spec JSON as a mutable `serde_json` value.
fn sample_spec() -> serde_json::Value {
    let bytes = sample_bytes();
    let (json, _) = split(&bytes);
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
    let (_, bin) = split(&bytes);
    let bin = bin.to_vec();
    let reframed = reframe(spec.to_string().as_bytes(), &bin);
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
    let (_, bin) = split(&bytes);
    let reframed = reframe(spec.to_string().as_bytes(), bin);
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

    let (json, bin) = split(&bytes);
    let mut spec: serde_json::Value = serde_json::from_slice(json).unwrap();
    // Claim a 5x4 grid over the same 12 samples.
    spec["figure"]["axes"][0]["images"][0]["nrows"] = serde_json::json!(5);
    let reframed = reframe(spec.to_string().as_bytes(), bin);
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
    let (_, bin) = split(&bytes);
    let reframed = reframe(spec.to_string().as_bytes(), bin);
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
    let (json, _) = split(&bytes);
    let mut framed = reframe(json, &[]);
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
    let (json, _) = split(&bytes);
    let mut framed = reframe(json, &[]);
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
    let (json, bin) = split(&bytes);
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

    let reframed = reframe(json, &bin);
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
    let (json, _) = split(&bytes);
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

// ---------------------------------------------------------------------------
// Inspection: reading an artifact without building a figure from it.
//
// These build artifacts by hand rather than through the exporter, so a
// malformed or future-schema document can be exercised directly — cases the
// exporter cannot produce.
// ---------------------------------------------------------------------------

use crate::portable::container::{self, TAG_BIN, TAG_JSON, TAG_PSTR};
use crate::portable::inspect;

/// A minimal but valid schema-2 spec document.
fn spec_json(poster_bytes: usize) -> String {
    let poster = if poster_bytes > 0 {
        format!(r#","poster":{{"chunk":"PSTR","mime":"image/png","bytes":{poster_bytes}}}"#)
    } else {
        r#","poster":null"#.to_string()
    };
    format!(
        r#"{{"schema":{SCHEMA_VERSION},
            "generator":{{"rizzma":"1.8.0"}},
            "renderer":{{"version":"1.8.0","sha256":"9f2c"}},
            "meta":{{"width_px":640,"height_px":480,"alt":"a figure",
                     "title":"demo"{poster},"animated":false}},
            "figure":{{"ignored":true}},
            "accessors":[]}}"#
    )
}

fn artifact(poster: &[u8], bin: &[u8]) -> Vec<u8> {
    let json = spec_json(poster.len());
    let mut chunks: Vec<([u8; 4], &[u8])> = vec![(TAG_JSON, json.as_bytes())];
    if !poster.is_empty() {
        chunks.push((TAG_PSTR, poster));
    }
    if !bin.is_empty() {
        chunks.push((TAG_BIN, bin));
    }
    container::write(&chunks)
}

#[test]
fn reads_metadata_without_the_bulk_payload() {
    // The point of the crate: learn the figure's size, alt text, and poster
    // without touching a megabyte of samples.
    let bin = vec![7u8; 1_000_000];
    let bytes = artifact(b"fake-png", &bin);
    let info = inspect(&bytes, &Limits::default()).expect("inspect");

    assert_eq!(info.schema, SCHEMA_VERSION);
    assert!(info.schema_supported && info.renderable());
    assert_eq!(info.generator.rizzma, "1.8.0");
    assert_eq!(info.renderer.version, "1.8.0");
    let meta = info.meta.as_ref().expect("schema 2 carries meta");
    assert_eq!((meta.width_px, meta.height_px), (640, 480));
    assert_eq!(meta.alt.as_deref(), Some("a figure"));
    assert_eq!(info.poster(&bytes), Some(&b"fake-png"[..]));
    assert_eq!(info.total_bytes, bytes.len());
    assert_eq!(info.chunks.len(), 3);
}

#[test]
fn a_schema_1_artifact_still_inspects() {
    // Artifacts written before the meta block must not become unreadable:
    // no size, no poster, but provenance and renderability still resolve.
    let json = r#"{"schema":1,"generator":{"rizzma":"1.7.0"},
                   "renderer":{"version":"1.7.0","sha256":null},
                   "figure":{},"accessors":[]}"#;
    let bytes = container::write(&[(TAG_JSON, json.as_bytes())]);
    let info = inspect(&bytes, &Limits::default()).expect("inspect");
    assert_eq!(info.schema, 1);
    assert!(info.schema_supported);
    assert!(info.meta.is_none());
    assert!(info.poster(&bytes).is_none());
}

#[test]
fn an_unsupported_schema_is_reported_not_refused() {
    // A host still wants the size and poster of a figure it cannot draw, so a
    // future schema is a flag rather than an error.
    let json = format!(
        r#"{{"schema":{},"generator":{{"rizzma":"9.9.9"}},
             "renderer":{{"version":"9.9.9","sha256":null}},
             "meta":{{"width_px":640,"height_px":480,"alt":null,"title":null,
                      "poster":null,"animated":true}}}}"#,
        SCHEMA_VERSION + 1
    );
    let bytes = container::write(&[(TAG_JSON, json.as_bytes())]);
    let info = inspect(&bytes, &Limits::default()).expect("inspect");
    assert_eq!(info.schema, SCHEMA_VERSION + 1);
    assert!(!info.schema_supported, "must not claim it can render this");
    assert!(!info.renderable());
    assert!(info.meta.expect("meta").animated);
}

#[test]
fn budgets_are_enforced_before_allocating() {
    let bytes = artifact(b"fake-png", &vec![0u8; 4096]);

    let tiny = Limits::default().with_max_total_bytes(64);
    match inspect(&bytes, &tiny) {
        Err(PortableError::Budget(msg)) => assert!(msg.contains("over the 64 byte limit"), "{msg}"),
        other => panic!("expected Budget, got {other:?}"),
    }

    let json_capped = Limits::default().with_max_json_bytes(10);
    assert!(matches!(
        inspect(&bytes, &json_capped),
        Err(PortableError::Budget(_))
    ));

    let pixel_capped = Limits::default().with_max_canvas_pixels(1000);
    match inspect(&bytes, &pixel_capped) {
        Err(PortableError::Budget(msg)) => assert!(msg.contains("640x480"), "{msg}"),
        other => panic!("expected Budget, got {other:?}"),
    }
}

#[test]
fn a_lying_poster_length_is_malformed() {
    // The spec says 8 bytes; the chunk holds 4. Trusting the spec would hand a
    // truncated image to a decoder.
    let json = spec_json(8);
    let bytes = container::write(&[(TAG_JSON, json.as_bytes()), (TAG_PSTR, b"abcd")]);
    match inspect(&bytes, &Limits::default()) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("poster"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn structural_corruption_is_rejected() {
    let good = artifact(b"png", b"bin");

    let mut bad_magic = good.clone();
    bad_magic[0] = b'X';
    assert!(matches!(
        inspect(&bad_magic, &Limits::default()),
        Err(PortableError::Malformed(_))
    ));

    for cut in [0, 8, 11, good.len() / 2, good.len() - 1] {
        assert!(
            inspect(&good[..cut], &Limits::default()).is_err(),
            "truncating to {cut} bytes must not inspect"
        );
    }

    // A duplicate chunk is ambiguous, so it is refused rather than guessed.
    let json = spec_json(0);
    let mut dup = container::write(&[(TAG_JSON, json.as_bytes()), (TAG_JSON, json.as_bytes())]);
    let total = dup.len() as u32;
    dup[8..12].copy_from_slice(&total.to_le_bytes());
    match inspect(&dup, &Limits::default()) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("duplicate"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn json_and_poster_precede_bulk_payloads() {
    // Writers order chunks so a partial read can paint a card; readers must
    // still tolerate any order, which the duplicate/lookup tests exercise.
    let bytes = artifact(b"png", &vec![0u8; 2048]);
    let info = inspect(&bytes, &Limits::default()).expect("inspect");
    let tags: Vec<String> = info.chunks.iter().map(container::ChunkRef::name).collect();
    assert_eq!(tags, vec!["JSON", "PSTR", "BIN "]);
}

#[test]
fn missing_json_chunk_is_refused() {
    let bytes = container::write(&[(TAG_PSTR, b"png")]);
    match inspect(&bytes, &Limits::default()) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("JSON"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Animation: a timeline is a pure function of t, and it survives the wire.
// ---------------------------------------------------------------------------

use crate::portable::{Interp, Target, Timeline, Track};

/// A figure whose single line ramps from flat to a slope over two seconds.
fn animated_figure() -> Figure {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0]);
    let mut timeline = Timeline::new(2.0);
    timeline.push(
        Track::line_y(
            0,
            0,
            vec![0.0, 2.0],
            vec![vec![0.0, 0.0, 0.0], vec![0.0, 1.0, 2.0]],
        )
        .expect("track"),
    );
    fig.set_timeline(timeline);
    fig
}

#[test]
fn seeking_is_a_pure_function_of_time() {
    // The property the whole design rests on: same t, same figure. Without it
    // scrubbing is approximate and archived replay is a different picture.
    let mut a = animated_figure();
    let mut b = animated_figure();

    a.seek(0.75).expect("seek");
    b.seek(0.25).expect("seek");
    b.seek(1.9).expect("seek");
    b.seek(0.75).expect("seek"); // arrives from elsewhere, same destination
    assert_eq!(a.axes()[0].line_data(0), b.axes()[0].line_data(0));
    assert!(a.encode_png().unwrap() == b.encode_png().unwrap());
}

#[test]
fn interpolation_is_element_wise_and_endpoints_hold() {
    let mut fig = animated_figure();

    fig.seek(0.0).expect("seek");
    assert_eq!(fig.axes()[0].line_data(0).unwrap().1, vec![0.0, 0.0, 0.0]);

    fig.seek(1.0).expect("seek");
    assert_eq!(fig.axes()[0].line_data(0).unwrap().1, vec![0.0, 0.5, 1.0]);

    fig.seek(2.0).expect("seek");
    assert_eq!(fig.axes()[0].line_data(0).unwrap().1, vec![0.0, 1.0, 2.0]);
}

#[test]
fn time_outside_the_timeline_wraps_or_clamps() {
    // Looping wraps so a caller can pass a monotonic clock; non-looping clamps
    // so the end state holds rather than extrapolating data nobody drew.
    let mut looping = animated_figure();
    looping.seek(3.0).expect("seek"); // 3.0 wraps to 1.0
    assert_eq!(
        looping.axes()[0].line_data(0).unwrap().1,
        vec![0.0, 0.5, 1.0]
    );

    // Exactly `duration` is the end, not the start: seeking to the end of an
    // animation should show its end. Wrapping starts strictly past duration.
    let mut at_end = animated_figure();
    at_end.seek(2.0).expect("seek");
    assert_eq!(
        at_end.axes()[0].line_data(0).unwrap().1,
        vec![0.0, 1.0, 2.0]
    );
    let mut past_end = animated_figure();
    past_end.seek(2.5).expect("seek");
    assert_eq!(
        past_end.axes()[0].line_data(0).unwrap().1,
        vec![0.0, 0.25, 0.5]
    );

    let mut once = animated_figure();
    let held = once
        .timeline()
        .expect("timeline")
        .clone()
        .with_looping(false);
    once.set_timeline(held);
    once.seek(99.0).expect("seek");
    assert_eq!(once.axes()[0].line_data(0).unwrap().1, vec![0.0, 1.0, 2.0]);

    // A nonsense clock must not produce a nonsense figure.
    let mut nan = animated_figure();
    nan.seek(f64::NAN).expect("seek");
    assert_eq!(nan.axes()[0].line_data(0).unwrap().1, vec![0.0, 0.0, 0.0]);
}

#[test]
fn step_interpolation_holds_the_earlier_keyframe() {
    let track = Track::new(
        Target::LineY { axes: 0, index: 0 },
        vec![0.0, 1.0],
        vec![vec![1.0], vec![9.0]],
        Interp::Step,
    )
    .expect("track");
    assert_eq!(track.sample(0.0), vec![1.0]);
    assert_eq!(track.sample(0.99), vec![1.0], "step must not ramp");
    assert_eq!(track.sample(1.0), vec![9.0]);
}

#[test]
fn a_scrolling_window_is_two_keyframes_not_many_frames() {
    // The oscilloscope case: animating the view, not the data. If this needed a
    // frame per sample the artifact would be a video with extra steps.
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let x: Vec<f64> = (0..500).map(|i| i as f64 * 0.02).collect();
    let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
    ax.plot(&x, &y);

    let mut timeline = Timeline::new(10.0);
    timeline.push(Track::xlim_sweep(0, 10.0, (0.0, 2.0), (8.0, 10.0)).expect("sweep"));
    fig.set_timeline(timeline);

    fig.seek(5.0).expect("seek");
    let ((lo, hi), _) = fig.axes()[0].effective_limits();
    assert!(
        (lo - 4.0).abs() < 1e-9 && (hi - 6.0).abs() < 1e-9,
        "window should be halfway across: got ({lo}, {hi})"
    );
    assert_eq!(fig.timeline().expect("timeline").tracks[0].len(), 2);
}

#[test]
fn an_animation_survives_the_round_trip() {
    let fig = animated_figure();
    let bytes = fig.to_portable().expect("export");
    let restored = Figure::from_portable(&bytes).expect("import");

    assert_eq!(restored.timeline(), fig.timeline());

    // And it still evaluates identically after the trip.
    let (mut a, mut b) = (fig, restored);
    a.seek(1.3).expect("seek");
    b.seek(1.3).expect("seek");
    assert!(a.encode_png().unwrap() == b.encode_png().unwrap());
}

#[test]
fn an_animated_artifact_declares_itself_to_a_host() {
    // A host decides whether to show transport controls from `inspect` alone,
    // without parsing the timeline.
    let bytes = animated_figure().to_portable().expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");
    let meta = info.meta.expect("meta");
    assert!(meta.animated);
    assert_eq!(meta.duration, 2.0);
    assert_eq!(info.schema, SCHEMA_VERSION);

    // A static figure says so, and reports no duration.
    let mut still = Figure::new(4.0, 3.0);
    still.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    let info = crate::portable::inspect(&still.to_portable().unwrap(), &Limits::default()).unwrap();
    let meta = info.meta.expect("meta");
    assert!(!meta.animated);
    assert_eq!(meta.duration, 0.0);
}

#[test]
fn ill_formed_tracks_are_refused_at_construction() {
    let target = Target::LineY { axes: 0, index: 0 };
    // Ragged frames would make the element-wise lerp undefined.
    assert!(
        Track::new(
            target,
            vec![0.0, 1.0],
            vec![vec![0.0, 0.0], vec![1.0]],
            Interp::Linear
        )
        .is_err()
    );
    // Times must be strictly increasing, or bracketing a time is ambiguous.
    assert!(
        Track::new(
            target,
            vec![1.0, 0.0],
            vec![vec![0.0], vec![1.0]],
            Interp::Linear
        )
        .is_err()
    );
    // Counts must agree.
    assert!(Track::new(target, vec![0.0], vec![], Interp::Linear).is_err());
    assert!(Track::new(target, vec![], vec![], Interp::Linear).is_err());
}

#[test]
fn a_track_pointing_at_nothing_fails_loudly() {
    // Silently skipping a track would animate a figure differently than the
    // author wrote it, with nothing to indicate anything was dropped.
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);

    let mut timeline = Timeline::new(1.0);
    timeline.push(Track::line_y(0, 7, vec![0.0], vec![vec![0.0, 0.0]]).expect("track"));
    fig.set_timeline(timeline);
    match fig.seek(0.5) {
        Err(PortableError::Malformed(msg)) => assert!(msg.contains("line 7"), "{msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    let mut timeline = Timeline::new(1.0);
    timeline.push(Track::line_y(9, 0, vec![0.0], vec![vec![0.0, 0.0]]).expect("track"));
    fig.set_timeline(timeline);
    assert!(matches!(fig.seek(0.5), Err(PortableError::Malformed(_))));
}

// ---------------------------------------------------------------------------
// Seeking a bound, interactive figure: the animation and the user share one
// view, and the user wins until they hand it back.
// ---------------------------------------------------------------------------

/// A figure whose timeline sweeps the x window AND ramps the line's y data.
fn swept_figure() -> Figure {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    let x: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
    let y = vec![0.0; 100];
    ax.plot(&x, &y);
    ax.set_xlim(0.0, 2.0);
    ax.set_ylim(-1.0, 1.0);

    let mut tl = Timeline::new(10.0);
    tl.push(Track::xlim_sweep(0, 10.0, (0.0, 2.0), (8.0, 10.0)).expect("sweep"));
    tl.push(
        Track::line_y(0, 0, vec![0.0, 10.0], vec![vec![0.0; 100], vec![1.0; 100]]).expect("ramp"),
    );
    fig.set_timeline(tl);
    fig
}

/// Wheel-zoom at the axes center so the interactor takes the view.
fn zoom_center(it: &mut Interactor) {
    let (w, h) = it.figure().size_px();
    let out = it.handle(Event::Wheel {
        x: w / 2.0,
        y: h / 2.0,
        dy: -3.0,
    });
    assert_eq!(
        out,
        crate::figure::Outcome::NeedsRedraw,
        "zoom must land on the axes"
    );
}

#[test]
fn a_user_held_view_suspends_the_camera_but_not_the_data() {
    let mut it = Interactor::new(swept_figure());

    // Untouched: the sweep owns the camera.
    it.seek(5.0).expect("seek");
    let ((lo, hi), _) = it.figure().axes()[0].effective_limits();
    assert!(
        (lo - 4.0).abs() < 1e-9 && (hi - 6.0).abs() < 1e-9,
        "sweep should place the window"
    );

    // The user zooms: they now own the view.
    zoom_center(&mut it);
    let (held_x, held_y) = it.figure().axes()[0].effective_limits();

    it.seek(8.0).expect("seek");
    let (after_x, after_y) = it.figure().axes()[0].effective_limits();
    assert_eq!(
        (after_x, after_y),
        (held_x, held_y),
        "the sweep must not fight the user"
    );

    // …but the data track still advanced: the animation's content continues.
    let y = it.figure().axes()[0].line_data(0).expect("line").1;
    assert!(
        (y[0] - 0.8).abs() < 1e-9,
        "data should be at t=8 of the ramp, got {}",
        y[0]
    );
}

#[test]
fn double_click_hands_the_view_back_to_the_timeline() {
    let mut it = Interactor::new(swept_figure());
    it.seek(2.0).expect("seek");
    zoom_center(&mut it);

    // Held: the camera stays put.
    it.seek(6.0).expect("seek");
    let held = it.figure().axes()[0].effective_limits();

    // Double-click is the explicit "yours again".
    let (w, h) = it.figure().size_px();
    it.handle(Event::DoubleClick {
        x: w / 2.0,
        y: h / 2.0,
    });
    it.seek(6.0).expect("seek");
    let ((lo, hi), _) = it.figure().axes()[0].effective_limits();
    assert!(
        (lo - 4.8).abs() < 1e-9 && (hi - 6.8).abs() < 1e-9,
        "after home the sweep resumes: got ({lo}, {hi}), held was {held:?}"
    );
}

#[test]
fn seeking_without_interaction_never_marks_the_view_held() {
    // Seeks alone — however many — must not accumulate into a state where the
    // timeline blocks itself.
    let mut it = Interactor::new(swept_figure());
    for i in 0..50 {
        it.seek(f64::from(i) * 0.2).expect("seek");
    }
    it.seek(5.0).expect("seek");
    let ((lo, hi), _) = it.figure().axes()[0].effective_limits();
    assert!((lo - 4.0).abs() < 1e-9 && (hi - 6.0).abs() < 1e-9);
}

#[test]
fn a_static_figure_ignores_interactor_seek() {
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    let before = fig.encode_png().expect("png");
    let mut it = Interactor::new(fig);
    it.seek(3.0).expect("seek is a no-op without a timeline");
    assert!(it.figure().encode_png().unwrap() == before);
}

// ---------------------------------------------------------------------------
// The reversible HTML wrapper: one file a browser opens and a host ingests.
// ---------------------------------------------------------------------------

use crate::portable::{HtmlRuntime, is_raw_riz, unwrap_html, wrap_html, wrap_html_live};

#[test]
fn wrap_then_unwrap_is_byte_identical() {
    // The property the whole feature rests on: the HTML file IS the carrier of
    // the canonical artifact, not a second format that can drift from it.
    let riz = animated_figure().to_portable().expect("export");
    let html = wrap_html(&riz, &Limits::default()).expect("wrap");
    let recovered = unwrap_html(html.as_bytes(), &Limits::default()).expect("unwrap");
    assert!(
        recovered == riz,
        "strip must recover the exact artifact bytes"
    );

    // And the recovered bytes pass full validation as if they arrived raw.
    let restored = Figure::from_portable(&recovered).expect("import after unwrap");
    assert!(restored.encode_png().unwrap() == animated_figure().encode_png().unwrap());
}

#[test]
fn the_live_tier_strips_identically_and_embeds_the_runtime() {
    let riz = animated_figure().to_portable().expect("export");
    let rt = HtmlRuntime {
        wasm: b"\0asm-fake-renderer-bytes",
        glue: "export default function init() {}",
        loader: "export function mount() {}",
    };
    let html = wrap_html_live(&riz, &Limits::default(), &rt).expect("wrap");

    assert!(unwrap_html(html.as_bytes(), &Limits::default()).expect("unwrap") == riz);
    // Runtime assets travel under their own ids, base64'd, and are therefore
    // not part of the canonical carrier element.
    assert!(html.contains(r#"id="riz-rt-wasm""#));
    assert!(html.contains(r#"id="riz-rt-glue""#));
    assert!(html.contains(r#"id="riz-rt-loader""#));
    assert!(
        !html.contains("export default function init"),
        "runtime source must be base64, never raw in markup"
    );
}

#[test]
fn artifact_text_cannot_inject_into_the_wrapper() {
    // Title and alt come from the artifact and could be attacker-influenced in
    // a re-wrapping tool; they must land escaped, never as live markup.
    let mut fig = Figure::new(3.0, 2.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
    ax.set_title(r#"</script><script>alert(1)</script>"#);
    let cfg = PortableConfig::default().with_alt(r#""><img onerror=alert(2) src=x>"#);
    let riz = fig.to_portable_with(&cfg).expect("export");

    let html = wrap_html(&riz, &Limits::default()).expect("wrap");
    assert!(
        !html.contains("<script>alert"),
        "title must not close the carrier and open a script"
    );
    // `onerror=alert` may appear *inside* the quoted attribute value — that is
    // inert. What must not survive is the closing quote itself: the sequence
    // that would end the attribute and start a real element.
    assert!(
        !html.contains(r#""><img"#),
        "alt's quote must be escaped so the attribute cannot be closed"
    );
    assert!(
        html.contains("&quot;&gt;&lt;img"),
        "escaped alt should be present as text"
    );
    assert!(
        html.contains("&lt;/script&gt;"),
        "escaped title should be present as text"
    );

    // Hostile text changes nothing about the strip.
    assert!(unwrap_html(html.as_bytes(), &Limits::default()).expect("unwrap") == riz);
}

#[test]
fn base64_data_cannot_terminate_the_carrier_early() {
    // The reason encoding is a security property: whatever bytes the artifact
    // holds, the encoded payload contains no '<' and cannot close the tag.
    let riz = animated_figure().to_portable().expect("export");
    let html = wrap_html(&riz, &Limits::default()).expect("wrap");
    let open = r#"<script type="application/vnd.rizzma.figure+base64" id="riz">"#;
    let start = html.find(open).expect("carrier") + open.len();
    let end = html[start..].find("</script>").expect("terminator") + start;
    assert!(
        html[start..end]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)),
        "carrier payload must be pure base64"
    );
}

#[test]
fn unwrap_rejects_what_it_should() {
    let riz = animated_figure().to_portable().expect("export");
    let html = wrap_html(&riz, &Limits::default()).expect("wrap");
    let open = r#"<script type="application/vnd.rizzma.figure+base64" id="riz">"#;

    // No carrier at all.
    match unwrap_html(b"<!doctype html><p>just a page</p>", &Limits::default()) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("no portable-figure"), "{m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
    // Two carriers is ambiguous: refuse rather than guess which is canonical.
    let doubled = format!("{html}{open}AAAA</script>");
    match unwrap_html(doubled.as_bytes(), &Limits::default()) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("more than one"), "{m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
    // Unterminated element.
    let cut = &html[..html.find(open).unwrap() + open.len() + 10];
    assert!(matches!(
        unwrap_html(cut.as_bytes(), &Limits::default()),
        Err(PortableError::Malformed(_))
    ));
    // Corrupt base64.
    let broken = html.replacen(open, &format!("{open}!!"), 1);
    match unwrap_html(broken.as_bytes(), &Limits::default()) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("base64"), "{m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
    // Not UTF-8 at all.
    assert!(matches!(
        unwrap_html(&[0xff, 0xfe, 0x00, 0x01], &Limits::default()),
        Err(PortableError::Malformed(_))
    ));
}

#[test]
fn raw_vs_wrapped_is_a_declared_branch_not_a_sniff() {
    let riz = animated_figure().to_portable().expect("export");
    let html = wrap_html(&riz, &Limits::default()).expect("wrap");
    assert!(is_raw_riz(&riz));
    assert!(!is_raw_riz(html.as_bytes()));
    // And the strict reader refuses the wrapped form outright: one parser
    // never accepts both, which is what keeps polyglot smuggling closed.
    assert!(matches!(
        crate::portable::inspect(html.as_bytes(), &Limits::default()),
        Err(PortableError::Malformed(_))
    ));
}

#[test]
fn wrapping_does_not_launder_a_malformed_artifact() {
    match wrap_html(b"RZFGjunk", &Limits::default()) {
        Err(PortableError::Malformed(_)) => {}
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn unwrap_is_budgeted_before_it_allocates() {
    let riz = animated_figure().to_portable().expect("export");
    let html = wrap_html(&riz, &Limits::default()).expect("wrap");

    // Exactly at the boundary: allowed.
    let exact = Limits::default().with_max_total_bytes(riz.len());
    assert!(unwrap_html(html.as_bytes(), &exact).expect("exact fit") == riz);

    // One byte under: refused as Budget, not Malformed.
    let tight = Limits::default().with_max_total_bytes(riz.len() - 1);
    match unwrap_html(html.as_bytes(), &tight) {
        Err(PortableError::Budget(m)) => assert!(m.contains("over the"), "{m}"),
        other => panic!("expected Budget, got {other:?}"),
    }

    // The proof the budget check precedes the decode: a carrier whose payload
    // is over-budget, valid in SHAPE (length % 4 == 0, sane padding) but
    // invalid in alphabet, must fail with Budget — a decoder that ran first
    // would have reported Malformed instead.
    let open = r#"<script type="application/vnd.rizzma.figure+base64" id="riz">"#;
    let junk = "!".repeat(4_096);
    let page = format!("<!doctype html>{open}{junk}</script>");
    let tiny = Limits::default().with_max_total_bytes(64);
    match unwrap_html(page.as_bytes(), &tiny) {
        Err(PortableError::Budget(_)) => {}
        other => panic!("size must be checked before decoding; got {other:?}"),
    }
}

#[test]
fn padding_cannot_bypass_the_budget() {
    // The bypass a review caught in the first budgeted version: a payload of
    // nothing but '=' made padding == len, the saturating subtraction computed
    // a decoded size of zero, and any budget passed — while the decoder could
    // still reserve proportionally to the encoded input. Shape validation now
    // refuses it outright, allocation-free.
    let open = r#"<script type="application/vnd.rizzma.figure+base64" id="riz">"#;
    let tiny = Limits::default().with_max_total_bytes(64);

    // All padding, length divisible by 4: refused for its shape.
    let all_eq = format!("<!doctype html>{open}{}</script>", "=".repeat(4_096));
    match unwrap_html(all_eq.as_bytes(), &tiny) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("padding"), "{m}"),
        other => panic!("all-padding payload must be Malformed pre-decode, got {other:?}"),
    }

    // Padding buried mid-payload: also a shape violation.
    let mid_eq = format!("<!doctype html>{open}AA==AAAA</script>");
    match unwrap_html(mid_eq.as_bytes(), &tiny) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("padding"), "{m}"),
        other => panic!("mid-payload padding must be Malformed, got {other:?}"),
    }

    // Length not divisible by 4: refused before any size arithmetic.
    let ragged = format!("<!doctype html>{open}AAAAA</script>");
    match unwrap_html(ragged.as_bytes(), &tiny) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("multiple of 4"), "{m}"),
        other => panic!("ragged length must be Malformed, got {other:?}"),
    }

    // Three trailing '=' with valid length: more padding than base64 allows.
    let over_pad = format!("<!doctype html>{open}AAAAA===</script>");
    match unwrap_html(over_pad.as_bytes(), &tiny) {
        Err(PortableError::Malformed(m)) => assert!(m.contains("padding"), "{m}"),
        other => panic!("triple padding must be Malformed, got {other:?}"),
    }
}

/// A figure with one line driven by a `(time, position)` grid — the wave whose
/// wavelength is a slider — plus a second line driven by a plain control track.
fn controlled_figure() -> Figure {
    use crate::portable::{Control, Grid, Interp, Target};

    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0]);
    ax.plot(&[0.0, 1.0, 2.0], &[1.0, 1.0, 1.0]);
    fig.set_timeline(Timeline::new(2.0));

    // Grid: value = t * 10 + p, per element — easy to predict at any (t, p).
    let mut shape = Control::new("shape", 0.0, 4.0, 2.0).expect("control");
    shape.push_grid(
        Grid::new(
            Target::LineY { axes: 0, index: 0 },
            vec![0.0, 2.0],
            vec![0.0, 4.0],
            vec![
                vec![vec![0.0; 3], vec![4.0; 3]],
                vec![vec![20.0; 3], vec![24.0; 3]],
            ],
            Interp::Linear,
        )
        .expect("grid"),
    );
    fig.add_control(shape);

    // Track: line 1 is flat at the control's value.
    let mut level = Control::new("level", 0.0, 10.0, 1.0).expect("control");
    level.push_track(
        Track::new(
            Target::LineY { axes: 0, index: 1 },
            vec![0.0, 10.0],
            vec![vec![0.0; 3], vec![10.0; 3]],
            Interp::Linear,
        )
        .expect("track"),
    );
    fig.add_control(level);
    fig
}

#[test]
fn controls_and_the_clock_compose_as_one_pure_function() {
    // The property design/12 §3 rests on: displayed state is a function of
    // (t, control values), so the order the user reached them in cannot show.
    let mut a = controlled_figure();
    let mut b = controlled_figure();

    a.seek(1.0).expect("seek");
    a.set_control(0, 2.0).expect("control");
    a.set_control(1, 7.0).expect("control");

    b.set_control(1, 3.0).expect("control");
    b.set_control(0, 0.5).expect("control");
    b.seek(0.25).expect("seek");
    b.set_control(1, 7.0).expect("control");
    b.set_control(0, 2.0).expect("control");
    b.seek(1.0).expect("seek");

    assert_eq!(a.axes()[0].line_data(0), b.axes()[0].line_data(0));
    assert_eq!(a.axes()[0].line_data(1), b.axes()[0].line_data(1));
    // t = 1.0, shape = 2.0: bilinear centre of the lattice, 10 + 2.
    assert_eq!(a.axes()[0].line_data(0).unwrap().1, vec![12.0; 3]);
    assert_eq!(a.axes()[0].line_data(1).unwrap().1, vec![7.0; 3]);
}

#[test]
fn grid_samples_bilinearly_and_clamps_at_the_lattice_edge() {
    use crate::portable::{Grid, Interp, Target};

    let grid = Grid::new(
        Target::LineY { axes: 0, index: 0 },
        vec![0.0, 2.0],
        vec![1.0, 3.0],
        vec![
            vec![vec![0.0, 100.0], vec![4.0, 100.0]],
            vec![vec![20.0, 100.0], vec![24.0, 100.0]],
        ],
        Interp::Linear,
    )
    .expect("grid");

    assert_eq!(grid.sample(1.0, 2.0), vec![12.0, 100.0]); // dead centre
    assert_eq!(grid.sample(0.0, 1.0), vec![0.0, 100.0]); // corner
    assert_eq!(grid.sample(-5.0, 0.0), vec![0.0, 100.0]); // clamped low
    assert_eq!(grid.sample(99.0, 99.0), vec![24.0, 100.0]); // clamped high
    assert_eq!(grid.sample(0.5, 3.0), vec![9.0, 100.0]); // one axis interior
}

#[test]
fn grid_step_interp_holds_the_earlier_lattice_cell() {
    use crate::portable::{Grid, Interp, Target};

    let grid = Grid::new(
        Target::LineY { axes: 0, index: 0 },
        vec![0.0, 2.0],
        vec![0.0, 4.0],
        vec![vec![vec![1.0], vec![2.0]], vec![vec![3.0], vec![4.0]]],
        Interp::Step,
    )
    .expect("grid");

    assert_eq!(grid.sample(1.9, 3.9), vec![1.0]); // holds (0, 0) until crossed
    assert_eq!(grid.sample(2.0, 3.9), vec![3.0]);
    assert_eq!(grid.sample(2.0, 4.0), vec![4.0]);
}

#[test]
fn malformed_grids_are_refused() {
    use crate::portable::{Grid, Interp, Target};
    let target = Target::LineY { axes: 0, index: 0 };

    for (times, positions, frames) in [
        // Ragged frame.
        (
            vec![0.0, 1.0],
            vec![0.0],
            vec![vec![vec![1.0, 2.0]], vec![vec![1.0]]],
        ),
        // Row count disagrees with the time axis.
        (vec![0.0, 1.0], vec![0.0], vec![vec![vec![1.0]]]),
        // Row width disagrees with the position axis.
        (vec![0.0], vec![0.0, 1.0], vec![vec![vec![1.0]]]),
        // Non-increasing axis.
        (
            vec![1.0, 1.0],
            vec![0.0],
            vec![vec![vec![1.0]], vec![vec![1.0]]],
        ),
        // Empty axis.
        (vec![], vec![0.0], vec![]),
    ] {
        assert!(matches!(
            Grid::new(target, times, positions, frames, Interp::Linear),
            Err(PortableError::Unsupported(_))
        ));
    }
}

#[test]
fn control_positions_normalize_to_something_drawable() {
    use crate::portable::Control;

    let c = Control::new("λ", 1.0, 3.0, 2.0).expect("control");
    assert_eq!(c.normalize(2.5), 2.5);
    assert_eq!(c.normalize(-10.0), 1.0);
    assert_eq!(c.normalize(10.0), 3.0);
    // A nonsense slider should not produce a nonsense figure.
    assert_eq!(c.normalize(f64::NAN), 2.0);

    let stepped = Control::new("n", 0.0, 10.0, 0.0)
        .expect("control")
        .with_step(2.0)
        .expect("step");
    assert_eq!(stepped.normalize(3.1), 4.0);
    assert_eq!(stepped.normalize(2.9), 2.0);

    assert!(Control::new("bad", 3.0, 1.0, 2.0).is_err());
    assert!(Control::new("out", 0.0, 1.0, 5.0).is_err());
    assert!(
        Control::new("s", 0.0, 1.0, 0.0)
            .expect("control")
            .with_step(-1.0)
            .is_err()
    );
}

#[test]
fn controls_round_trip_and_start_at_their_defaults() {
    let mut fig = controlled_figure();
    fig.set_control(0, 4.0).expect("control");
    fig.set_control(1, 9.0).expect("control");

    let bytes = fig.to_portable().expect("export");
    let restored = Figure::from_portable(&bytes).expect("import");

    // The declaration travels; the live positions are session state and do
    // not — a fresh import starts at the authored defaults.
    assert_eq!(restored.controls(), fig.controls());
    assert_eq!(restored.control_values(), &[2.0, 1.0]);

    // And the restored controls drive the figure identically.
    let mut a = fig;
    let mut b = restored;
    for f in [&mut a, &mut b] {
        f.set_control(0, 1.0).expect("control");
        f.set_control(1, 5.0).expect("control");
        f.seek(0.5).expect("seek");
    }
    assert_eq!(a.axes()[0].line_data(0), b.axes()[0].line_data(0));
    assert_eq!(a.axes()[0].line_data(1), b.axes()[0].line_data(1));
}

#[test]
fn a_grid_only_figure_still_animates() {
    // The wave whose only time-dependence is inside a control's grid: the
    // timeline carries no tracks, but the clock still moves the figure, so
    // every "should this play" gate must say yes.
    let fig = controlled_figure();
    assert!(fig.timeline().is_some_and(Timeline::is_empty));
    assert!(fig.is_animated());

    let bytes = fig.to_portable().expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");
    assert!(
        info.meta.expect("meta").animated,
        "inspect must report a grid-only figure animated"
    );

    // And the grid actually moves under the clock at a fixed control value.
    let mut fig = Figure::from_portable(&bytes).expect("import");
    fig.seek(0.0).expect("seek");
    let at0 = fig.axes()[0].line_data(0).unwrap().1;
    fig.seek(1.0).expect("seek");
    assert_ne!(fig.axes()[0].line_data(0).unwrap().1, at0);
}

#[test]
fn bad_control_indices_and_targets_fail_loudly() {
    use crate::portable::{Control, Interp, Target, Track};

    let mut fig = controlled_figure();
    assert!(matches!(
        fig.set_control(5, 1.0),
        Err(PortableError::Malformed(_))
    ));

    // A control track addressing a line that does not exist.
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 0.0]);
    let mut c = Control::new("ghost", 0.0, 1.0, 0.0).expect("control");
    c.push_track(
        Track::new(
            Target::LineY { axes: 0, index: 7 },
            vec![0.0],
            vec![vec![0.0, 0.0]],
            Interp::Linear,
        )
        .expect("track"),
    );
    fig.add_control(c);
    let err = fig.set_control(0, 0.5).unwrap_err();
    assert!(err.to_string().contains("control 0 track 0"), "{err}");
}

#[test]
fn the_live_wrapper_emits_host_side_sliders() {
    let mut fig = controlled_figure();
    fig.seek(0.0).expect("seek");
    let riz = fig.to_portable().expect("export");
    let limits = Limits::default();
    let rt = crate::portable::HtmlRuntime {
        wasm: b"\0asm fake",
        glue: "// glue",
        loader: "// loader",
    };
    let page = crate::portable::wrap_html_live(&riz, &limits, &rt).expect("wrap");
    assert!(page.contains("riz-controls"));
    assert!(page.contains("setControl"));
    // The poster tier carries no slider scaffolding beyond the shared style.
    let poster = crate::portable::wrap_html(&riz, &limits).expect("wrap");
    assert!(!poster.contains("setControl"));
}

#[test]
fn forged_tracks_and_grids_are_an_import_error_not_a_panic() {
    use crate::portable::{Control, Grid, Interp, Target, Track};

    // The constructors refuse these shapes, but a deserialized artifact never
    // met a constructor — the pub fields let this test forge exactly what a
    // hand-crafted JSON chunk could carry. Before import validation, each of
    // these panicked in sample(): indexing values[] past its end, or times[0]
    // of an empty axis.
    let target = Target::LineY { axes: 0, index: 0 };
    let base = |fig: &mut Figure| {
        fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 0.0]);
    };

    // A stride that lies about the value count.
    let mut fig = Figure::new(4.0, 3.0);
    base(&mut fig);
    let mut tl = Timeline::new(1.0);
    tl.push(Track {
        target,
        times: vec![0.0, 1.0],
        values: vec![0.0, 0.0], // 2 values, claims 2 keyframes of stride 2
        stride: 2,
        interp: Interp::Linear,
    });
    fig.set_timeline(tl);
    let bytes = fig.to_portable().expect("export");
    let Err(err) = Figure::from_portable(&bytes) else {
        panic!("a lying stride must not import");
    };
    assert!(
        matches!(&err, PortableError::Malformed(m) if m.contains("stride")),
        "{err}"
    );

    // An empty keyframe axis.
    let mut fig = Figure::new(4.0, 3.0);
    base(&mut fig);
    let mut tl = Timeline::new(1.0);
    tl.push(Track {
        target,
        times: vec![],
        values: vec![],
        stride: 0,
        interp: Interp::Linear,
    });
    fig.set_timeline(tl);
    let bytes = fig.to_portable().expect("export");
    assert!(Figure::from_portable(&bytes).is_err());

    // A grid lattice that lies about its value count, inside a control.
    let mut fig = Figure::new(4.0, 3.0);
    base(&mut fig);
    fig.set_timeline(Timeline::new(1.0));
    let mut c = Control::new("forged", 0.0, 1.0, 0.0).expect("control");
    c.push_grid(Grid {
        target,
        times: vec![0.0, 1.0],
        positions: vec![0.0, 1.0],
        values: vec![0.0; 3], // claims 2 x 2 x stride 2 = 8
        stride: 2,
        interp: Interp::Linear,
    });
    fig.add_control(c);
    let bytes = fig.to_portable().expect("export");
    let Err(err) = Figure::from_portable(&bytes) else {
        panic!("a lying lattice must not import");
    };
    assert!(
        matches!(&err, PortableError::Malformed(m) if m.contains("lattice")),
        "{err}"
    );

    // A control whose range is nonsense (normalize would return nonsense).
    let mut fig = Figure::new(4.0, 3.0);
    base(&mut fig);
    let mut c = Control::new("ok", 0.0, 1.0, 0.5).expect("control");
    c.min = f64::NAN;
    fig.add_control(c);
    let bytes = fig.to_portable().expect("export");
    assert!(Figure::from_portable(&bytes).is_err());
}

#[test]
fn inspect_reports_the_control_manifest_typed() {
    // A host validates once and persists the manifest without JSON pokes:
    // declaration order, layout fields only, no track data parsed.
    let fig = controlled_figure();
    let bytes = fig.to_portable().expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");

    assert_eq!(info.controls.len(), 2);
    assert_eq!(info.controls[0].label, "shape");
    assert_eq!(
        (
            info.controls[0].min,
            info.controls[0].max,
            info.controls[0].default
        ),
        (0.0, 4.0, 2.0)
    );
    assert_eq!(info.controls[0].step, None);
    assert_eq!(info.controls[1].label, "level");

    // A static artifact reports an empty manifest, not an error.
    let mut plain = Figure::new(4.0, 3.0);
    plain.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    let bytes = plain.to_portable().expect("export");
    let info = crate::portable::inspect(&bytes, &Limits::default()).expect("inspect");
    assert!(info.controls.is_empty());

    // And a forged nonsense range fails at inspect, before a host persists it.
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    let mut c = crate::portable::Control::new("ok", 0.0, 1.0, 0.5).expect("control");
    c.default = 9.0;
    fig.add_control(c);
    let bytes = fig.to_portable().expect("export");
    let err = crate::portable::inspect(&bytes, &Limits::default())
        .expect_err("nonsense range must not inspect");
    assert!(
        matches!(&err, PortableError::Malformed(m) if m.contains("control 0")),
        "{err}"
    );
}

#[test]
fn set_control_echoes_the_position_actually_applied() {
    use crate::portable::{Control, Interp, Target, Track};

    // The echo is the normalization policy's single source of truth: a host
    // shows what came back, never what it sent.
    let mut fig = Figure::new(4.0, 3.0);
    fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 0.0]);
    let mut c = Control::new("n", 0.0, 10.0, 4.0)
        .expect("control")
        .with_step(2.0)
        .expect("step");
    c.push_track(
        Track::new(
            Target::LineY { axes: 0, index: 0 },
            vec![0.0, 10.0],
            vec![vec![0.0; 2], vec![10.0; 2]],
            Interp::Linear,
        )
        .expect("track"),
    );
    fig.add_control(c);

    assert_eq!(fig.set_control(0, 3.1).expect("set"), 4.0); // snapped up
    assert_eq!(fig.set_control(0, 2.9).expect("set"), 2.0); // snapped down
    assert_eq!(fig.set_control(0, -50.0).expect("set"), 0.0); // clamped
    assert_eq!(fig.set_control(0, f64::NAN).expect("set"), 4.0); // defaulted
    // The figure reflects the echoed value, not the requested one.
    assert_eq!(fig.control_values(), &[4.0]);
    assert_eq!(fig.axes()[0].line_data(0).unwrap().1, vec![4.0; 2]);

    // The interactor echoes identically through the user-view wrapper.
    let mut it = Interactor::new(fig);
    assert_eq!(it.set_control(0, 9.9).expect("set"), 10.0);
}
