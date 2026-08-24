//! Reading an artifact's metadata without rendering it.
//!
//! A host has decisions to make *before* it draws anything, or instead of ever
//! drawing: how much space to reserve, what to announce to a screen reader,
//! which runtime to pick, and what to show when it cannot render at all.
//! [`inspect`] answers those from the head of the JSON chunk, never allocating
//! the binary payloads.
//!
//! That is the common case rather than an edge case: a transcript holding
//! hundreds of figures paints a poster and a card for nearly all of them and
//! renders one or two.

use serde::Deserialize;

use super::container;
use super::{GeneratorRef, Limits, Meta, Metadata, PortableError, RendererRef};

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

/// Read an artifact's metadata without building a figure or rendering it.
///
/// Walks the chunk directory, parses only the head of the JSON chunk, and
/// never allocates the `BIN` payloads. Everything is checked against `limits`
/// first, because artifact bytes are attacker-influenced and the host is the
/// one that knows what is reasonable.
///
/// ```
/// use rizzma::Figure;
/// use rizzma::portable::{inspect, Limits};
///
/// let mut fig = Figure::new(6.4, 4.8);
/// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
/// let bytes = fig.to_portable()?;
///
/// let info = inspect(&bytes, &Limits::default())?;
/// let meta = info.meta.as_ref().expect("schema 2 carries meta");
/// assert_eq!((meta.width_px, meta.height_px), (640, 480));
/// assert!(info.renderable());
/// assert!(info.poster(&bytes).is_some());
/// # Ok::<(), rizzma::PortableError>(())
/// ```
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
        schema_supported: (super::SCHEMA_MIN..=super::SCHEMA_VERSION).contains(&header.schema),
        generator: header.generator,
        renderer: header.renderer,
        meta: header.meta,
        chunks: dir,
        total_bytes: bytes.len(),
    })
}
