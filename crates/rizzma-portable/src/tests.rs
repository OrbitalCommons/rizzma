//! Inspection tests.
//!
//! These build artifacts by hand rather than through rizzma, so the crate is
//! testable without the renderer it deliberately does not depend on.

use crate::container::{self, TAG_BIN, TAG_JSON, TAG_PSTR};
use crate::{Limits, PortableError, SCHEMA_VERSION, inspect};

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
