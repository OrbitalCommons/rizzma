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
//! The container framing, the host-facing metadata, and the renderer-free
//! [`inspect`](rizzma_portable::inspect) entry point live in the companion
//! [`rizzma_portable`] crate, which links no rasterizer: a host that only needs
//! to size a card and show a poster should not have to compile a font stack.
//! This module is the half that needs the renderer.
//!
//! - `spec` — the wire model: plain serde mirrors of the live types, every
//!   struct `deny_unknown_fields`, every enum closed. Unknown content is a hard
//!   error, never silently dropped: a figure missing an artist is a
//!   scientifically wrong figure.
//! - `data` — typed binary accessors: bulk arrays live in one binary chunk,
//!   referenced by index from the JSON spec.
//!
//! The public entry points live on `Figure`:
//! [`to_portable`](crate::figure::Figure::to_portable),
//! [`to_portable_with`](crate::figure::Figure::to_portable_with),
//! [`save_portable`](crate::figure::Figure::save_portable), and
//! [`from_portable`](crate::figure::Figure::from_portable).

#[cfg(feature = "portable")]
pub(crate) mod data;
pub(crate) mod spec;

#[cfg(all(test, feature = "portable"))]
mod tests;

#[cfg(feature = "portable")]
pub use rizzma_portable::{
    ChunkRef, GeneratorRef, Limits, Meta, Metadata, PortableError, PosterRef, RendererRef,
    SCHEMA_MIN, SCHEMA_VERSION, inspect,
};

// Without the `portable` feature the artifact machinery is absent, but the
// public `Scale`/`Locator`/`Formatter` traits still name the wire forms below,
// so those stay compiled either way.
#[cfg(not(feature = "portable"))]
pub use no_portable::PortableError;

#[cfg(not(feature = "portable"))]
mod no_portable {
    /// Placeholder for the error type of the disabled `portable` feature.
    ///
    /// Enable the feature to get the real
    /// [`rizzma_portable::PortableError`](https://docs.rs/rizzma-portable).
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
