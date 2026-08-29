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
//! Everything the format needs lives here, behind the default-on `portable`
//! feature:
//!
//! - [`container`] — the `RZFG` chunk framing, and the only part an
//!   independent implementation has to reimplement.
//! - [`inspect`] — read an artifact's size, title, alt text, poster, and
//!   schema **without building a figure or rendering one**. This is the entry
//!   point a host uses for the common case, which is the case where it never
//!   draws: a transcript holding hundreds of figures paints a card for nearly
//!   all of them and renders one or two.
//! - [`Limits`] — the budgets a caller imposes on artifacts it did not
//!   produce, enforced before allocating.
//! - `spec` — the wire model: plain serde mirrors of the live types, every
//!   struct `deny_unknown_fields`, every enum closed. Unknown content is a hard
//!   error, never silently dropped: a figure missing an artist is a
//!   scientifically wrong figure.
//! - `data` — typed binary accessors: bulk arrays live in one binary chunk,
//!   referenced by index from the JSON spec.
//!
//! The figure-level entry points live on `Figure`:
//! [`to_portable`](crate::figure::Figure::to_portable),
//! [`to_portable_with`](crate::figure::Figure::to_portable_with),
//! [`save_portable`](crate::figure::Figure::save_portable),
//! [`from_portable`](crate::figure::Figure::from_portable), and
//! [`from_portable_limited`](crate::figure::Figure::from_portable_limited).
//!
//! # Trust
//!
//! An artifact's `renderer.sha256` is an **identity and a lookup key, never an
//! authorization**. A hostile artifact can inline a hostile renderer and
//! honestly report its digest, so verifying artifact bytes against the
//! artifact's own claim is circular and buys nothing. A host keeps its own
//! allowlist mapping schema (and optionally version) to renderers *it* vetted,
//! serves its own copy, and ignores any `WASM` chunk for execution.

#[cfg(feature = "portable")]
pub mod container;
#[cfg(feature = "portable")]
mod controls;
#[cfg(feature = "portable")]
pub(crate) mod data;
#[cfg(feature = "portable")]
mod error;
#[cfg(feature = "portable")]
mod html;
#[cfg(feature = "portable")]
mod inspect;
#[cfg(feature = "portable")]
mod limits;
#[cfg(feature = "portable")]
mod meta;
pub(crate) mod spec;
#[cfg(feature = "portable")]
mod timeline;

#[cfg(all(test, feature = "portable"))]
mod tests;

#[cfg(feature = "portable")]
pub use container::ChunkRef;
#[cfg(feature = "portable")]
pub use controls::{Control, Grid};
#[cfg(feature = "portable")]
pub use error::PortableError;
#[cfg(feature = "portable")]
pub use html::{HtmlRuntime, is_raw_riz, unwrap_html, wrap_html, wrap_html_live};
#[cfg(feature = "portable")]
pub use inspect::inspect;
#[cfg(feature = "portable")]
pub use limits::Limits;
#[cfg(feature = "portable")]
pub use meta::{ControlRef, GeneratorRef, Meta, Metadata, PosterRef, RendererRef};
#[cfg(feature = "portable")]
pub use timeline::{Interp, Target, Timeline, Track};

/// Wire-format schema version written into every artifact.
///
/// Bumped whenever anything changes that would alter how an existing artifact
/// renders. Schema 1 is the original static+interactive model; schema 2 adds
/// the [`Meta`] block and the poster chunk; schema 3 the [`Timeline`]; schema
/// 4 user-driven [`Control`]s.
#[cfg(feature = "portable")]
pub const SCHEMA_VERSION: u32 = 4;

/// Oldest schema version this build can still load.
#[cfg(feature = "portable")]
pub const SCHEMA_MIN: u32 = 1;

/// The schema a document's features require it to declare, at minimum.
///
/// Schema is the compatibility decision a host makes *before* parsing the
/// figure — it selects the runtime. A forged document must not be able to
/// under-declare: a "schema 3" artifact carrying controls would render on a
/// schema-4 runtime while a host that correctly selected a schema-3 runtime
/// from the declaration watches it rejected for the unknown field. Both
/// import and inspection refuse a declaration below what the features demand.
#[cfg(feature = "portable")]
pub(crate) fn required_schema(has_meta: bool, has_timeline: bool, has_controls: bool) -> u32 {
    if has_controls {
        4
    } else if has_timeline {
        3
    } else if has_meta {
        2
    } else {
        1
    }
}

// Without the `portable` feature the artifact machinery is absent, but the
// public `Scale`/`Locator`/`Formatter` traits still name the wire forms below,
// so those stay compiled either way.
#[cfg(not(feature = "portable"))]
pub use no_portable::PortableError;

#[cfg(not(feature = "portable"))]
mod no_portable {
    /// Placeholder for the error type of the disabled `portable` feature.
    ///
    /// Enable the `portable` feature for the real one.
    #[derive(Debug)]
    pub enum PortableError {}

    impl std::fmt::Display for PortableError {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match *self {}
        }
    }

    impl std::error::Error for PortableError {}
}

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
/// with `PortableError::Unsupported`.
#[derive(Debug, Clone, PartialEq)]
pub struct PortableFormatter(pub(crate) spec::FormatterSpec);

/// How [`Figure::to_portable_with`](crate::figure::Figure::to_portable_with)
/// writes an artifact.
///
/// [`PortableConfig::default`] writes a poster, because a figure that cannot
/// render and has nothing to show is the one failure mode with no recovery.
#[cfg(feature = "portable")]
#[derive(Debug, Clone, PartialEq)]
pub struct PortableConfig {
    /// Embed a PNG poster of the figure as authored (see [`PosterRef`]).
    ///
    /// Turn this off only when the consumer is known to always render live and
    /// the bytes matter; every non-rendering path — scripting disabled,
    /// unsupported schema, archive preview, the card shown while a runtime
    /// downloads — has nothing to display without it.
    pub poster: bool,
    /// Text description of the figure for assistive technology, carried in
    /// [`Meta::alt`].
    pub alt: Option<String>,
}

#[cfg(feature = "portable")]
impl PortableConfig {
    /// The defaults: a poster, and no alt text beyond the figure's own title.
    #[must_use]
    pub fn new() -> Self {
        Self {
            poster: true,
            alt: None,
        }
    }

    /// Set whether a poster is embedded, returning `self` for chaining.
    #[must_use]
    pub fn with_poster(mut self, poster: bool) -> Self {
        self.poster = poster;
        self
    }

    /// Set the alt text, returning `self` for chaining.
    #[must_use]
    pub fn with_alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }
}

#[cfg(feature = "portable")]
impl Default for PortableConfig {
    fn default() -> Self {
        Self::new()
    }
}
