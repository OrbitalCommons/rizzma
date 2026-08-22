//! The `RZFG` chunked binary container (`.riz`).
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header:  magic "RZFG" | u32 container_version | u32 total_len
//! chunks:  u32 len | 4-byte tag | payload | zero-pad to 8-byte alignment
//! ```
//!
//! Container version 1 defines the tags `JSON` (the spec, required), `BIN `
//! (binary accessor data), `FONT`, and `WASM`. This build reads `JSON` and
//! `BIN `; the presence of a `FONT` or `WASM` chunk (the self-contained
//! profile) is rejected until a build that can honor them, per the strict
//! fail-loudly policy — as is any unrecognized tag.

use super::PortableError;

/// The container magic bytes.
const MAGIC: [u8; 4] = *b"RZFG";
/// Byte-layout version of the container framing itself.
const CONTAINER_VERSION: u32 = 1;
/// Chunk payloads start on this alignment so `f64` accessors can be read
/// aligned when mapped in place.
const ALIGN: usize = 8;

const TAG_JSON: [u8; 4] = *b"JSON";
const TAG_BIN: [u8; 4] = *b"BIN ";
const TAG_FONT: [u8; 4] = *b"FONT";
const TAG_WASM: [u8; 4] = *b"WASM";

/// Frame `json` (and `bin`, when non-empty) into a `.riz` byte vector.
pub(crate) fn write(json: &[u8], bin: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + json.len() + bin.len() + 2 * ALIGN);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // total_len, patched below
    write_chunk(&mut out, TAG_JSON, json);
    if !bin.is_empty() {
        write_chunk(&mut out, TAG_BIN, bin);
    }
    let total = u32::try_from(out.len()).expect("portable figure exceeds 4 GiB");
    out[8..12].copy_from_slice(&total.to_le_bytes());
    out
}

fn write_chunk(out: &mut Vec<u8>, tag: [u8; 4], payload: &[u8]) {
    let len = u32::try_from(payload.len()).expect("portable chunk exceeds 4 GiB");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&tag);
    out.extend_from_slice(payload);
    while !out.len().is_multiple_of(ALIGN) {
        out.push(0);
    }
}

/// Parse a `.riz` byte slice into its `(json, bin)` chunk payloads.
///
/// `bin` is empty when the artifact carries no binary chunk.
pub(crate) fn read(bytes: &[u8]) -> Result<(&[u8], &[u8]), PortableError> {
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
    let total = u32::from_le_bytes(header[8..12].try_into().expect("4 bytes")) as usize;
    if total != bytes.len() {
        return Err(PortableError::Malformed(format!(
            "declared length {total} but got {} bytes",
            bytes.len()
        )));
    }

    let mut json: Option<&[u8]> = None;
    let mut bin: Option<&[u8]> = None;
    let mut pos = 12;
    while pos < bytes.len() {
        let head = bytes
            .get(pos..pos + 8)
            .ok_or_else(|| malformed("truncated chunk header"))?;
        let len = u32::from_le_bytes(head[..4].try_into().expect("4 bytes")) as usize;
        let tag: [u8; 4] = head[4..8].try_into().expect("4 bytes");
        let payload = bytes
            .get(pos + 8..pos + 8 + len)
            .ok_or_else(|| malformed("truncated chunk payload"))?;
        let slot = match tag {
            TAG_JSON => &mut json,
            TAG_BIN => &mut bin,
            TAG_FONT | TAG_WASM => {
                return Err(PortableError::Malformed(format!(
                    "chunk {:?} (self-contained profile) is not supported by this build",
                    tag_name(tag)
                )));
            }
            _ => {
                return Err(PortableError::Malformed(format!(
                    "unrecognized chunk tag {:?}",
                    tag_name(tag)
                )));
            }
        };
        if slot.replace(payload).is_some() {
            return Err(PortableError::Malformed(format!(
                "duplicate {:?} chunk",
                tag_name(tag)
            )));
        }
        pos += 8 + len;
        while !pos.is_multiple_of(ALIGN) {
            if bytes.get(pos) != Some(&0) {
                return Err(malformed("non-zero chunk padding"));
            }
            pos += 1;
        }
    }

    let json = json.ok_or_else(|| malformed("missing JSON chunk"))?;
    Ok((json, bin.unwrap_or(&[])))
}

fn malformed(msg: &str) -> PortableError {
    PortableError::Malformed(msg.to_string())
}

/// Printable form of a chunk tag for error messages.
fn tag_name(tag: [u8; 4]) -> String {
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
