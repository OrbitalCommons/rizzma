//! Container framing and renderer-free inspection for rizzma **portable
//! figures** (`.riz`).
//!
//! A portable figure carries the *semantic* model of a plot — axes, artists,
//! data, and style — rather than a rasterized or vectorized picture of one, so
//! a consumer can re-run layout and rasterization from the data and offer pan,
//! zoom, and resolution independence away from the process that produced it.
//! The [`rizzma`](https://docs.rs/rizzma) crate produces and renders them.
//!
//! **This crate deliberately links none of that.** It depends only on serde and
//! a JSON codec — no rasterizer, no font stack — so a host can read what an
//! artifact *is* without paying for the machinery to draw it. That matters
//! because the common case for a host is the one where it never draws: a
//! transcript holding hundreds of figures paints a poster and a card for nearly
//! all of them and renders one or two.
//!
//! ```
//! use rizzma_portable::{inspect, Limits};
//!
//! # fn demo(bytes: &[u8]) -> Result<(), rizzma_portable::PortableError> {
//! let info = inspect(bytes, &Limits::default())?;
//! if let Some(meta) = &info.meta {
//!     // Reserve exactly the right box before fetching a renderer.
//!     println!("{}x{} — {}", meta.width_px, meta.height_px,
//!              meta.alt.as_deref().unwrap_or("figure"));
//! }
//! if !info.renderable() {
//!     // Nothing here can draw this schema: show the poster, not an error.
//!     let _poster: Option<&[u8]> = info.poster(bytes);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Trust
//!
//! An artifact's `renderer.sha256` is an **identity and a lookup key, never an
//! authorization**. A hostile artifact can inline a hostile renderer and
//! honestly report its digest, so verifying artifact bytes against the
//! artifact's own claim is circular and buys nothing. A host must keep its own
//! allowlist mapping schema (and optionally version) to renderers *it* vetted,
//! serve its own copy, and ignore any `WASM` chunk for execution. The
//! self-contained profile is a download and offline convenience, not an
//! execution source for a multi-tenant host.
//!
//! Everything here also treats artifact bytes as attacker-influenced: every
//! entry point takes [`Limits`] supplied by the caller and enforces them before
//! allocating.

pub mod container;
mod error;
mod limits;
mod meta;

pub use container::ChunkRef;
pub use error::PortableError;
pub use limits::Limits;
pub use meta::{GeneratorRef, Meta, Metadata, PosterRef, RendererRef};

use serde::Deserialize;

/// Wire-format schema version written into every artifact.
///
/// Bumped whenever anything changes that would alter how an existing artifact
/// renders. Schema 1 is the original static+interactive model; schema 2 adds
/// the [`Meta`] block and the poster chunk.
pub const SCHEMA_VERSION: u32 = 2;

/// Oldest schema version this build can still load.
pub const SCHEMA_MIN: u32 = 1;

/// The subset of the spec document inspection needs.
///
/// Deliberately **not** `deny_unknown_fields`: the inspector's job is metadata,
/// so it ignores the figure model and the accessor table rather than parsing
/// them. Full validation happens when the figure is actually built.
#[derive(Debug, Deserialize)]
struct Header {
    schema: u32,
    generator: GeneratorRef,
    renderer: RendererRef,
    #[serde(default)]
    meta: Option<Meta>,
}

/// Read an artifact's metadata without instantiating a renderer.
///
/// Walks the chunk directory, parses only the head of the JSON chunk, and
/// never allocates the `BIN` payloads. Everything is checked against `limits`
/// first.
///
/// # Errors
///
/// [`PortableError::Malformed`] for a structurally invalid artifact,
/// [`PortableError::Budget`] when `limits` are exceeded, and
/// [`PortableError::Json`] when the spec head will not parse. Note that an
/// *unsupported schema* is not an error here — it is reported as
/// [`Metadata::schema_supported`] being false, because a host still wants the
/// size and poster of a figure it cannot draw.
pub fn inspect(bytes: &[u8], limits: &Limits) -> Result<Metadata, PortableError> {
    let dir = container::directory(bytes, limits)?;

    let json = container::chunk(bytes, &dir, container::TAG_JSON).expect("directory checked JSON");
    if json.len() > limits.max_json_bytes {
        return Err(PortableError::Budget(format!(
            "JSON chunk is {} bytes, over the {} byte limit",
            json.len(),
            limits.max_json_bytes
        )));
    }
    let header: Header = serde_json::from_slice(json)?;

    if let Some(meta) = &header.meta {
        if let Some(poster) = &meta.poster {
            if poster.bytes > limits.max_poster_bytes {
                return Err(PortableError::Budget(format!(
                    "poster is {} bytes, over the {} byte limit",
                    poster.bytes, limits.max_poster_bytes
                )));
            }
            let actual = container::chunk(bytes, &dir, container::TAG_PSTR).map(<[u8]>::len);
            if actual != Some(poster.bytes) {
                return Err(PortableError::Malformed(format!(
                    "spec declares a {}-byte poster but the PSTR chunk holds {}",
                    poster.bytes,
                    actual.map_or_else(|| "nothing".to_string(), |n| n.to_string())
                )));
            }
        }
        let pixels = (meta.width_px as usize).saturating_mul(meta.height_px as usize);
        if pixels > limits.max_canvas_pixels {
            return Err(PortableError::Budget(format!(
                "figure is {}x{} = {pixels} pixels, over the {} pixel limit",
                meta.width_px, meta.height_px, limits.max_canvas_pixels
            )));
        }
    }

    Ok(Metadata {
        schema: header.schema,
        schema_supported: (SCHEMA_MIN..=SCHEMA_VERSION).contains(&header.schema),
        generator: header.generator,
        renderer: header.renderer,
        meta: header.meta,
        chunks: dir,
        total_bytes: bytes.len(),
    })
}

// Deliberately no `portable_sha256` helper: a host that cares about provenance
// must digest the bytes *it* received, with its own vetted implementation, and
// shipping a hand-rolled hash here would add surface for a convenience nobody
// can safely depend on anyway.

#[cfg(test)]
mod tests;
