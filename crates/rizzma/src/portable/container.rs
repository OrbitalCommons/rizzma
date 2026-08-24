//! The `RZFG` chunked binary container (`.riz`).
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header:  magic "RZFG" | u32 container_version | u32 total_len
//! chunks:  u32 len | 4-byte tag | payload | zero-pad to 8-byte alignment
//! ```
//!
//! The framing is deliberately trivial so an independent implementation is a
//! short function in any language. Writers emit `JSON` and `PSTR` **before**
//! the large `BIN`/`WASM` chunks, so a consumer can read metadata and paint a
//! poster from a partial read; readers must tolerate any order, because a
//! parser that depends on layout breaks on the first writer that differs.

use super::Limits;
use super::PortableError;

/// The container magic bytes.
pub const MAGIC: [u8; 4] = *b"RZFG";
/// Byte-layout version of the container framing itself.
pub const CONTAINER_VERSION: u32 = 1;
/// Chunk payloads start on this alignment so `f64` accessors can be read
/// aligned when mapped in place.
pub const ALIGN: usize = 8;

/// The spec document (required).
pub const TAG_JSON: [u8; 4] = *b"JSON";
/// Typed binary arrays referenced by accessor index.
pub const TAG_BIN: [u8; 4] = *b"BIN ";
/// A PNG poster of the figure as authored.
pub const TAG_PSTR: [u8; 4] = *b"PSTR";
/// Subsetted font face(s) for this artifact.
pub const TAG_FONT: [u8; 4] = *b"FONT";
/// The renderer, inlined (self-contained profile).
pub const TAG_WASM: [u8; 4] = *b"WASM";

/// One chunk located within an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRef {
    /// The 4-byte tag.
    pub tag: [u8; 4],
    /// Byte offset of the payload within the artifact.
    pub offset: usize,
    /// Payload length in bytes.
    pub len: usize,
}

impl ChunkRef {
    /// Printable form of the tag, for error messages and metadata.
    #[must_use]
    pub fn name(&self) -> String {
        tag_name(self.tag)
    }
}

/// Frame chunks into a `.riz` byte vector.
///
/// `chunks` is written in the given order; callers put `JSON` and `PSTR` ahead
/// of bulk payloads.
///
/// # Panics
///
/// Panics if the artifact or any chunk would exceed 4 GiB.
#[must_use]
pub fn write(chunks: &[([u8; 4], &[u8])]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|(_, p)| p.len() + 8 + ALIGN).sum();
    let mut out = Vec::with_capacity(12 + total);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // total_len, patched below
    for (tag, payload) in chunks {
        if payload.is_empty() && *tag == TAG_BIN {
            continue;
        }
        let len = u32::try_from(payload.len()).expect("portable chunk exceeds 4 GiB");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(payload);
        while !out.len().is_multiple_of(ALIGN) {
            out.push(0);
        }
    }
    let total = u32::try_from(out.len()).expect("portable figure exceeds 4 GiB");
    out[8..12].copy_from_slice(&total.to_le_bytes());
    out
}

/// Walk the chunk directory of `bytes`, enforcing `limits`.
///
/// This is the cheap structural pass: it validates the header and every chunk
/// extent without copying payloads, so a host can learn what an artifact
/// contains before deciding to spend memory on it.
///
/// # Errors
///
/// [`PortableError::Malformed`] for a bad header, truncated or duplicate
/// chunks, or non-zero padding; [`PortableError::Budget`] if the artifact or
/// its chunk count exceeds `limits`.
pub fn directory(bytes: &[u8], limits: &Limits) -> Result<Vec<ChunkRef>, PortableError> {
    if bytes.len() > limits.max_total_bytes {
        return Err(PortableError::Budget(format!(
            "artifact is {} bytes, over the {} byte limit",
            bytes.len(),
            limits.max_total_bytes
        )));
    }
    let header = bytes
        .get(..12)
        .ok_or_else(|| malformed("shorter than the 12-byte header"))?;
    if header[..4] != MAGIC {
        return Err(malformed("bad magic (not an RZFG container)"));
    }
    let version = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(PortableError::Malformed(format!(
            "container version {version}; this build reads version {CONTAINER_VERSION}"
        )));
    }
    let declared = u32::from_le_bytes(header[8..12].try_into().expect("4 bytes")) as usize;
    if declared != bytes.len() {
        return Err(PortableError::Malformed(format!(
            "declared length {declared} but got {} bytes",
            bytes.len()
        )));
    }

    let mut out: Vec<ChunkRef> = Vec::new();
    let mut pos = 12;
    while pos < bytes.len() {
        if out.len() >= limits.max_chunks {
            return Err(PortableError::Budget(format!(
                "more than {} chunks",
                limits.max_chunks
            )));
        }
        let head = bytes
            .get(pos..pos + 8)
            .ok_or_else(|| malformed("truncated chunk header"))?;
        let len = u32::from_le_bytes(head[..4].try_into().expect("4 bytes")) as usize;
        let tag: [u8; 4] = head[4..8].try_into().expect("4 bytes");
        let end = pos
            .checked_add(8)
            .and_then(|s| s.checked_add(len))
            .ok_or_else(|| malformed("chunk extent overflows"))?;
        if end > bytes.len() {
            return Err(malformed("truncated chunk payload"));
        }
        if out.iter().any(|c| c.tag == tag) {
            return Err(PortableError::Malformed(format!(
                "duplicate {:?} chunk",
                tag_name(tag)
            )));
        }
        out.push(ChunkRef {
            tag,
            offset: pos + 8,
            len,
        });
        pos = end;
        while !pos.is_multiple_of(ALIGN) {
            match bytes.get(pos) {
                Some(0) => pos += 1,
                Some(_) => return Err(malformed("non-zero chunk padding")),
                None => break,
            }
        }
    }
    if !out.iter().any(|c| c.tag == TAG_JSON) {
        return Err(malformed("missing JSON chunk"));
    }
    Ok(out)
}

/// The payload of the chunk tagged `tag`, if present.
#[must_use]
pub fn chunk<'a>(bytes: &'a [u8], dir: &[ChunkRef], tag: [u8; 4]) -> Option<&'a [u8]> {
    dir.iter()
        .find(|c| c.tag == tag)
        .map(|c| &bytes[c.offset..c.offset + c.len])
}

fn malformed(msg: &str) -> PortableError {
    PortableError::Malformed(msg.to_string())
}

/// Printable form of a chunk tag for error messages.
#[must_use]
pub fn tag_name(tag: [u8; 4]) -> String {
    tag.iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect()
}
