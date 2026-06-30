//! Embedded-only font sourcing.
//!
//! [`FontSource`] holds one or more font faces parsed from in-memory byte slices.
//! It performs **no** system font discovery, so it builds and runs unchanged on
//! `wasm32-unknown-unknown`. The default source is DejaVu Sans, vendored from
//! matplotlib's `mpl-data` (see `fonts/LICENSE_DEJAVU`).

use ttf_parser::Face;

/// Raw bytes of the vendored DejaVu Sans face, embedded at compile time.
///
/// This is the same TTF matplotlib ships as its default sans-serif font.
pub static DEJAVU_SANS_TTF: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

/// A set of embedded font faces usable for text measurement.
///
/// Faces are stored as owned byte buffers; a [`Face`] is parsed on demand from a
/// buffer. No filesystem or system font database is touched, which keeps the
/// type usable under `wasm32`.
///
/// Construct the default with [`FontSource::dejavu_sans`], or build a custom set
/// and add faces with [`FontSource::register_face`].
#[derive(Clone, Default)]
pub struct FontSource {
    /// Owned font file buffers, in registration order. The first entry is the
    /// primary face used by measurement.
    faces: Vec<Vec<u8>>,
}

impl FontSource {
    /// Creates an empty font source with no faces registered.
    ///
    /// Most callers want [`FontSource::dejavu_sans`] instead.
    #[must_use]
    pub fn new() -> Self {
        Self { faces: Vec::new() }
    }

    /// Creates a font source backed by the embedded DejaVu Sans face.
    #[must_use]
    pub fn dejavu_sans() -> Self {
        let mut source = Self::new();
        source.register_face(DEJAVU_SANS_TTF);
        source
    }

    /// Registers an additional face from raw TrueType/OpenType bytes.
    ///
    /// The bytes are copied into the source so the caller need not keep them
    /// alive. The newly added face becomes available for measurement; the first
    /// registered face remains the primary one.
    pub fn register_face(&mut self, data: &[u8]) {
        self.faces.push(data.to_vec());
    }

    /// Returns the number of registered faces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Returns `true` if no faces are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Parses and returns the primary (first-registered) face, if any.
    ///
    /// Returns `None` when no face is registered or the primary buffer fails to
    /// parse as a valid font.
    #[must_use]
    pub(crate) fn primary_face(&self) -> Option<Face<'_>> {
        let bytes = self.faces.first()?;
        Face::parse(bytes, 0).ok()
    }
}
