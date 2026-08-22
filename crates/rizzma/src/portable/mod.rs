//! Portable figures: a self-describing artifact that renders itself anywhere.
//!
//! This module implements the `.riz` artifact of `design/10-portable-figure.md`
//! (issue #287): a chunked binary container holding a strict, versioned wire
//! model of the semantic figure — `Figure → Axes → Artists`, data, and rcparams
//! — from which an identical [`Figure`](crate::figure::Figure) is
//! reconstructed and rendered through the ordinary pipeline. The promise is
//! **pixel identity**: `Figure::from_portable(fig.to_portable()?)` renders
//! byte-for-byte the same PNG as `fig` itself.
//!
//! - `spec` — the wire model: plain serde mirrors of the live types, every
//!   struct `deny_unknown_fields`, every enum closed. Unknown content is a hard
//!   error, never silently dropped: a figure missing an artist is a
//!   scientifically wrong figure.
//! - `data` — typed binary accessors: bulk arrays live in one binary chunk,
//!   referenced by index from the JSON spec.
//! - `container` — the `RZFG` chunked container framing JSON + binary.
//!
//! The public entry points live on `Figure`:
//! [`to_portable`](crate::figure::Figure::to_portable),
//! [`save_portable`](crate::figure::Figure::save_portable), and
//! [`from_portable`](crate::figure::Figure::from_portable).

// The wire types (`spec`, and the accessor descriptors in `data`) are always
// compiled: the `Scale`/`Locator`/`Formatter` hooks on the public axis traits
// name them regardless of feature. The machinery that *uses* them — container
// framing and the data bank — is feature-gated along with the JSON codec.
#[cfg(feature = "portable")]
pub(crate) mod container;
#[cfg(feature = "portable")]
pub(crate) mod data;
pub(crate) mod spec;

#[cfg(all(test, feature = "portable"))]
mod tests;

/// The portable wire form of a built-in [`Scale`](crate::axis::scale::Scale).
///
/// Opaque by design. The wire model is rizzma's own schema, so only the
/// built-in scales can produce one: a third-party [`Scale`](crate::axis::scale::Scale)
/// returns `None` from [`Scale::portable_spec`](crate::axis::scale::Scale::portable_spec),
/// and exporting a figure that uses it fails loudly rather than substituting a
/// different scale.
#[derive(Debug, Clone, PartialEq)]
pub struct PortableScale(pub(crate) spec::ScaleWire);

/// The portable wire form of a built-in [`Locator`](crate::axis::ticker::Locator).
///
/// Opaque by design; see [`PortableScale`].
#[derive(Debug, Clone, PartialEq)]
pub struct PortableLocator(pub(crate) spec::LocatorSpec);

/// The portable wire form of a built-in [`Formatter`](crate::axis::ticker::Formatter).
///
/// Opaque by design; see [`PortableScale`]. Notably
/// [`FuncFormatter`](crate::axis::ticker::FuncFormatter) has none — a Rust
/// closure cannot cross the wire — so exporting an axis that uses one fails
/// with [`PortableError::Unsupported`].
#[derive(Debug, Clone, PartialEq)]
pub struct PortableFormatter(pub(crate) spec::FormatterSpec);

/// Wire-format schema version written into every artifact.
///
/// Bumped whenever anything changes that would alter how an existing artifact
/// renders. Loaders support the contiguous range
/// [`SCHEMA_MIN`]`..=`[`SCHEMA_VERSION`] and reject anything newer with a hard
/// error rather than render a figure with content missing.
pub const SCHEMA_VERSION: u32 = 1;

/// Oldest schema version this build can still load.
pub const SCHEMA_MIN: u32 = 1;

/// Errors from portable-figure export and import.
///
/// The import path is strict by design: malformed containers, out-of-bounds
/// accessors, unknown fields or enum variants, and future schema versions all
/// fail loudly instead of best-effort rendering.
#[derive(Debug)]
pub enum PortableError {
    /// The figure holds state the wire format cannot represent (for example a
    /// [`FuncFormatter`](crate::axis::ticker::FuncFormatter) closure or a
    /// custom registered font face). Exporting it would change how the figure
    /// renders, so export refuses instead.
    Unsupported(String),
    /// The artifact bytes are structurally invalid: bad magic, truncated or
    /// duplicate chunks, out-of-bounds accessors, inconsistent array lengths.
    Malformed(String),
    /// The artifact's schema version is outside this build's supported range.
    Schema {
        /// The version the artifact declares.
        found: u32,
        /// Oldest version this build supports.
        min: u32,
        /// Newest version this build supports.
        max: u32,
    },
    /// The JSON spec chunk failed to serialize or deserialize (including
    /// unknown fields and unknown enum variants, which are rejected).
    Json(String),
    /// Filesystem I/O failed in [`save_portable`](crate::figure::Figure::save_portable).
    Io(std::io::Error),
}

impl std::fmt::Display for PortableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortableError::Unsupported(msg) => {
                write!(f, "figure cannot be exported portably: {msg}")
            }
            PortableError::Malformed(msg) => write!(f, "malformed portable figure: {msg}"),
            PortableError::Schema { found, min, max } => write!(
                f,
                "artifact is schema {found}; this build supports {min}..={max} — \
                 rendering it would drop content"
            ),
            PortableError::Json(msg) => write!(f, "portable figure spec error: {msg}"),
            PortableError::Io(err) => write!(f, "portable figure i/o error: {err}"),
        }
    }
}

impl std::error::Error for PortableError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PortableError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PortableError {
    fn from(err: std::io::Error) -> Self {
        PortableError::Io(err)
    }
}

#[cfg(feature = "portable")]
impl From<serde_json::Error> for PortableError {
    fn from(err: serde_json::Error) -> Self {
        PortableError::Json(err.to_string())
    }
}
