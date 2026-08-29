//! The reversible HTML wrapper: one file a browser opens and a host ingests.
//!
//! A `.riz.html` is an ordinary HTML page whose variable content is base64 —
//! inert by construction, since the base64 alphabet contains no `<` or `>` and
//! so cannot close a tag early. The canonical artifact travels in exactly one
//! element, byte-for-byte recoverable by [`unwrap_html`]:
//!
//! ```html
//! <script type="application/vnd.rizzma.figure+base64" id="riz">…</script>
//! ```
//!
//! The shapes this deliberately is **not** (`design/10` §3.1): a true polyglot
//! with `RZFG` at byte 0 would force strict readers to sniff, and raw binary in
//! markup would let attacker-chosen *plot values* spell `-->` or `<script` and
//! break out. Encoding is the security property, not a transport detail.
//!
//! Unwrapping is an explicit step, and the core `.riz` validator never sniffs:
//! one parser never accepts both forms. [`unwrap_html`]'s output goes through
//! the full ordinary validation as if it had arrived raw.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use super::{Limits, PortableError, container, inspect};

/// The exact prefix of the element carrying the canonical artifact.
const OPEN: &str = r#"<script type="application/vnd.rizzma.figure+base64" id="riz">"#;
/// The element terminator.
const CLOSE: &str = "</script>";

/// Runtime assets for a live-tier wrapper, supplied by the caller.
///
/// The embedder chooses the renderer, exactly as a mounting host does — the
/// exporter never fetches one. These are the three published runtime assets a
/// caller has vetted (`runtime.json` pins their digests).
#[derive(Debug, Clone, Copy)]
pub struct HtmlRuntime<'a> {
    /// The wasm renderer (`rizzma_bg.wasm`).
    pub wasm: &'a [u8],
    /// The wasm-bindgen glue module source (`rizzma.js`).
    pub glue: &'a str,
    /// The mount loader module source (`rizzma-mount.js`).
    pub loader: &'a str,
}

/// Escape a string for HTML text and attribute-value contexts.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Title and escaped alt text for a wrapper, read from the artifact itself.
fn wrapper_text(bytes: &[u8], limits: &Limits) -> Result<(String, String), PortableError> {
    let info = inspect(bytes, limits)?;
    let meta = info.meta;
    let title = meta
        .as_ref()
        .and_then(|m| m.title.clone())
        .unwrap_or_else(|| "portable figure".to_string());
    let alt = meta
        .as_ref()
        .and_then(|m| m.alt.clone())
        .unwrap_or_else(|| title.clone());
    Ok((escape(&title), escape(&alt)))
}

/// The shared page skeleton: styles, the canonical artifact element, and the
/// poster fallback that every tier renders first.
fn page(title: &str, alt: &str, artifact_b64: &str, extra: &str) -> String {
    format!(
        r#"<!doctype html>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} — rizzma portable figure</title>
<style>body{{margin:0;background:#1a1b26;color:#c0caf5;font:14px system-ui;display:grid;place-items:center;min-height:100vh;gap:8px}}img,canvas{{max-width:95vw;height:auto}}p{{opacity:.7;margin:4px}}</style>
{OPEN}{artifact_b64}{CLOSE}
<img id="riz-poster" alt="{alt}">
<p id="riz-note">{title} — a rizzma portable figure. This preview is its embedded
poster; a rizzma-aware host renders it with pan, zoom, and animation.</p>
<script>
// Exporter-authored constant code; the only variable inputs are base64 (inert)
// and pre-escaped text. Nothing artifact-derived is assigned via innerHTML.
(() => {{
  const b64 = document.getElementById("riz").textContent;
  const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
  const v = new DataView(bytes.buffer);
  let pos = 12;
  while (pos < bytes.length) {{
    const len = v.getUint32(pos, true);
    const tag = String.fromCharCode(...bytes.subarray(pos + 4, pos + 8));
    if (tag === "PSTR") {{
      document.getElementById("riz-poster").src =
        URL.createObjectURL(new Blob([bytes.subarray(pos + 8, pos + 8 + len)], {{type: "image/png"}}));
      break;
    }}
    pos += 8 + len;
    while (pos % 8) pos += 1;
  }}
}})();
</script>
{extra}"#
    )
}

/// Wrap artifact `bytes` in a poster-tier `.riz.html` page.
///
/// The page shows the artifact's embedded poster and alt text, entirely
/// offline with no wasm. The artifact itself is carried base64-encoded and is
/// recoverable byte-for-byte with [`unwrap_html`].
///
/// # Errors
///
/// As [`inspect`]: the bytes must already be a structurally valid artifact
/// within `limits` — wrapping does not launder a malformed one.
pub fn wrap_html(bytes: &[u8], limits: &Limits) -> Result<String, PortableError> {
    let (title, alt) = wrapper_text(bytes, limits)?;
    Ok(page(&title, &alt, &B64.encode(bytes), ""))
}

/// Wrap artifact `bytes` in a live-tier `.riz.html` page with an embedded,
/// caller-supplied runtime.
///
/// The page renders the poster first and then upgrades to the interactive
/// figure, so an environment where the runtime cannot start (no
/// `crypto.subtle`, wasm disabled) degrades to the poster rather than a blank
/// page. The glue and loader are materialised as blob-URL modules inside the
/// page — the same pattern as the mount protocol's bootstrap.
///
/// # Errors
///
/// As [`wrap_html`].
pub fn wrap_html_live(
    bytes: &[u8],
    limits: &Limits,
    rt: &HtmlRuntime<'_>,
) -> Result<String, PortableError> {
    let (title, alt) = wrapper_text(bytes, limits)?;
    let extra = format!(
        r#"<script type="application/octet-stream" id="riz-rt-wasm">{wasm}</script>
<script type="text/plain" id="riz-rt-glue">{glue}</script>
<script type="text/plain" id="riz-rt-loader">{loader}</script>
<canvas id="riz-live" hidden></canvas>
<script type="module">
// Poster-first upgrade: any failure below leaves the poster showing.
try {{
  const un64 = (id) => Uint8Array.from(atob(document.getElementById(id).textContent), c => c.charCodeAt(0));
  const src = (id) => new TextDecoder().decode(un64(id));
  const blobUrl = (code) => URL.createObjectURL(new Blob([code], {{type: "text/javascript"}}));
  const loader = await import(blobUrl(src("riz-rt-loader")));
  const canvas = document.getElementById("riz-live");
  canvas.hidden = false;
  const handle = await loader.mount(canvas, un64("riz"), {{
    renderer: {{ wasm: un64("riz-rt-wasm"), glue: blobUrl(src("riz-rt-glue")) }},
    autoplay: true,
  }});
  document.getElementById("riz-poster").hidden = true;
  document.getElementById("riz-note").textContent =
    "drag to pan, wheel to zoom, double-click to reset";
  void handle;
}} catch (err) {{
  document.getElementById("riz-live").hidden = true;
  console.warn("riz: live upgrade failed, poster stands:", err);
}}
</script>"#,
        wasm = B64.encode(rt.wasm),
        glue = B64.encode(rt.glue.as_bytes()),
        loader = B64.encode(rt.loader.as_bytes()),
    );
    Ok(page(&title, &alt, &B64.encode(bytes), &extra))
}

/// Recover the canonical `.riz` bytes from a `.riz.html` page.
///
/// Strict by design: the carrier element must appear **exactly once**, and its
/// content must be valid standard-alphabet base64 with no whitespace. The
/// returned bytes have received **no artifact validation** — pass them through
/// [`inspect`]/import exactly as if they had arrived raw. That separation is
/// what keeps the core validator sniff-free: one parser never accepts both the
/// wrapped and the raw form.
///
/// `limits` bounds the allocation, like every other entry point that parses
/// attacker-influenced bytes: the payload's decoded size is computed from its
/// encoded length and checked against `limits.max_total_bytes` **before any
/// decode allocates**, so an oversized carrier is refused for the price of a
/// subtraction, not an out-of-memory. Nothing else in this function allocates
/// proportionally to the input — the scans borrow.
///
/// # Errors
///
/// [`PortableError::Budget`] when the decoded artifact would exceed
/// `limits.max_total_bytes`; [`PortableError::Malformed`] when the marker is
/// missing or duplicated, the element is unterminated, or the base64 does not
/// decode.
pub fn unwrap_html(html: &[u8], limits: &Limits) -> Result<Vec<u8>, PortableError> {
    let text = std::str::from_utf8(html)
        .map_err(|_| PortableError::Malformed("wrapper is not valid UTF-8".to_string()))?;
    let Some(start) = text.find(OPEN) else {
        return Err(PortableError::Malformed(
            "no portable-figure carrier element in this HTML".to_string(),
        ));
    };
    if text[start + OPEN.len()..].contains(OPEN) {
        return Err(PortableError::Malformed(
            "more than one portable-figure carrier element; refusing to guess".to_string(),
        ));
    }
    let payload_start = start + OPEN.len();
    let Some(rel_end) = text[payload_start..].find(CLOSE) else {
        return Err(PortableError::Malformed(
            "unterminated portable-figure carrier element".to_string(),
        ));
    };
    let payload = &text[payload_start..payload_start + rel_end];

    // Validate the base64 *shape* allocation-free before trusting any size
    // arithmetic. An earlier revision subtracted the padding count with a
    // saturating sub — defensive-looking, and a bypass: a payload of nothing
    // but `=` made the computed size zero and sailed past every budget while
    // the decoder could still reserve proportionally to the encoded input.
    // Shape first, then the subtraction is plain arithmetic on known-good
    // numbers.
    if !payload.len().is_multiple_of(4) {
        return Err(PortableError::Malformed(format!(
            "carrier base64 length {} is not a multiple of 4",
            payload.len()
        )));
    }
    let padding = payload.bytes().rev().take_while(|&b| b == b'=').count();
    if padding > 2 {
        return Err(PortableError::Malformed(
            "carrier base64 has more than two padding characters".to_string(),
        ));
    }
    if payload[..payload.len() - padding].contains('=') {
        return Err(PortableError::Malformed(
            "carrier base64 has padding before the end".to_string(),
        ));
    }
    // Alphabet validity is the decoder's to reject: with the shape proven, its
    // allocation is bounded by the budget below, so a bad character costs a
    // bounded buffer, never an unbounded one.
    let decoded_exact = payload.len() / 4 * 3 - padding;
    if decoded_exact > limits.max_total_bytes {
        return Err(PortableError::Budget(format!(
            "carrier decodes to {decoded_exact} bytes, over the {} byte limit",
            limits.max_total_bytes
        )));
    }

    let bytes = B64
        .decode(payload)
        .map_err(|e| PortableError::Malformed(format!("carrier base64 does not decode: {e}")))?;
    if bytes.len() > limits.max_total_bytes {
        return Err(PortableError::Budget(format!(
            "carrier decoded to {} bytes, over the {} byte limit",
            bytes.len(),
            limits.max_total_bytes
        )));
    }
    Ok(bytes)
}

/// Whether `bytes` look like a raw artifact rather than a wrapper.
///
/// A convenience for tools that accept either file: exact magic at byte 0,
/// nothing fuzzier. Hosts should still branch on declared type, not sniffing.
#[must_use]
pub fn is_raw_riz(bytes: &[u8]) -> bool {
    bytes.get(..4) == Some(&container::MAGIC)
}
