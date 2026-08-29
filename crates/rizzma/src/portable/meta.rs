//! The metadata a host reads before it decides to render anything.
//!
//! These types are shared by the exporter (which writes them into the JSON
//! chunk) and by [`inspect`](super::inspect), so the two cannot drift.

use serde::{Deserialize, Serialize};

/// Provenance of the exporter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorRef {
    /// The rizzma crate version that wrote the artifact.
    pub rizzma: String,
}

/// The renderer an artifact was authored against.
///
/// The digest is **provenance and a lookup key, never an authorization**: an
/// artifact reporting a hash of its own bytes proves nothing, since a hostile
/// artifact can honestly report the hash of a hostile renderer. A host resolves
/// renderers through its own vetted registry; see the crate docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererRef {
    /// rizzma version of the authoring renderer.
    pub version: String,
    /// SHA-256 of the published wasm renderer for that version, when known.
    pub sha256: Option<String>,
}

/// Where a poster lives and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PosterRef {
    /// The chunk tag holding the poster bytes (always `"PSTR"` today).
    pub chunk: String,
    /// The poster's media type (always `"image/png"` today).
    pub mime: String,
    /// Poster length in bytes.
    pub bytes: usize,
}

/// Layout, accessibility, and fallback metadata carried in the JSON chunk.
///
/// Everything here answers a question a host has *before* fetching a renderer:
/// how much space to reserve, what to announce to a screen reader, and what to
/// show if the figure cannot be rendered at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// Exact figure width in pixels (`width_in × dpi`), not an aspect hint, so
    /// a host reserves the right box and never reflows.
    pub width_px: u32,
    /// Exact figure height in pixels (`height_in × dpi`).
    pub height_px: u32,
    /// A text description of the figure for assistive technology and for hosts
    /// that show alt text beside a poster.
    pub alt: Option<String>,
    /// The figure's title, when it has one.
    pub title: Option<String>,
    /// The poster, when the artifact carries one.
    pub poster: Option<PosterRef>,
    /// Whether the artifact carries a timeline that animates.
    pub animated: bool,
    /// Length of the animation in seconds; `0.0` when the figure is static.
    ///
    /// A host reads this to decide whether to show transport controls, without
    /// parsing the timeline itself. Defaulted so schema 1 and 2 artifacts,
    /// which predate it, still load.
    #[serde(default)]
    pub duration: f64,
}

/// One entry of the control manifest, as [`inspect`](super::inspect) reports
/// it: the slider's layout, without its track data.
///
/// This is what a host persists after validating an artifact once — typed, in
/// declaration order, and sufficient to draw the sliders. The keyframes stay
/// in the artifact, where the renderer that samples them lives.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ControlRef {
    /// The label a host shows beside the slider.
    pub label: String,
    /// Smallest position, inclusive.
    pub min: f64,
    /// Largest position, inclusive.
    pub max: f64,
    /// The position the figure was authored at.
    pub default: f64,
    /// Snap increment from `min`; `None` is a continuous slider.
    #[serde(default)]
    pub step: Option<f64>,
}

/// What [`inspect`](super::inspect) returns: everything readable without
/// instantiating a renderer or allocating the bulk payloads.
#[derive(Debug, Clone, PartialEq)]
pub struct Metadata {
    /// Wire-format schema version the artifact declares.
    pub schema: u32,
    /// Whether this build can render that schema (see
    /// [`SCHEMA_MIN`](super::SCHEMA_MIN)/[`SCHEMA_VERSION`](super::SCHEMA_VERSION)).
    pub schema_supported: bool,
    /// Which rizzma wrote it.
    pub generator: GeneratorRef,
    /// Which renderer it was authored against.
    pub renderer: RendererRef,
    /// Layout and fallback metadata, absent in schema 1 artifacts.
    pub meta: Option<Meta>,
    /// The control manifest in declaration order; empty before schema 4.
    pub controls: Vec<ControlRef>,
    /// Every chunk in the container, in file order.
    pub chunks: Vec<super::container::ChunkRef>,
    /// Total artifact size in bytes.
    pub total_bytes: usize,
}

impl Metadata {
    /// The poster bytes, if the artifact carries a poster chunk.
    ///
    /// `bytes` must be the same slice that was inspected.
    #[must_use]
    pub fn poster<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        super::container::chunk(bytes, &self.chunks, super::container::TAG_PSTR)
    }

    /// Whether a host can render this artifact live, or must fall back to the
    /// poster: true only when the schema is supported.
    #[must_use]
    pub fn renderable(&self) -> bool {
        self.schema_supported
    }
}
