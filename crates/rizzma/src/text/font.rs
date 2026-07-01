//! Embedded-only font sourcing.
//!
//! [`FontSource`] holds one or more font faces parsed from in-memory byte slices.
//! It performs **no** system font discovery, so it builds and runs unchanged on
//! `wasm32-unknown-unknown`. The default source is DejaVu Sans, vendored from
//! matplotlib's `mpl-data` (see `fonts/LICENSE_DEJAVU`).

use ttf_parser::{Face, GlyphId};

/// Raw bytes of the vendored DejaVu Sans face, embedded at compile time.
///
/// This is the same TTF matplotlib ships as its default sans-serif font.
pub static DEJAVU_SANS_TTF: &[u8] = include_bytes!("fonts/DejaVuSans.ttf");

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

    /// Returns `true` when the primary face has an outline glyph for `ch`.
    ///
    /// Callers can use this to detect missing codepoints and fall back to an
    /// alternative character before laying out geometry. Returns `false` when no
    /// face is registered or the primary face lacks a glyph for `ch`.
    #[must_use]
    pub fn has_glyph(&self, ch: char) -> bool {
        self.primary_face()
            .and_then(|face| face.glyph_index(ch))
            .is_some()
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

/// Horizontal kerning adjustment, in font design units, between two adjacent
/// glyphs on a baseline.
///
/// Reads the legacy `kern` table — the same source FreeType consults for its
/// default kerning, which is what matplotlib's Agg text rendering uses. GPOS
/// kerning is intentionally ignored: resolving it correctly needs a shaping
/// engine, and DejaVu Sans (like most core fonts) ships the same pairs in the
/// legacy `kern` table that FreeType reads. Returns `0.0` when the font has no
/// `kern` table or no pair value for `(left, right)`.
///
/// The value is in the same font design units as `glyph_hor_advance`, so callers
/// scale it by `font_size_px / units_per_em` alongside the advance.
#[must_use]
pub(crate) fn horizontal_kerning(face: &Face<'_>, left: GlyphId, right: GlyphId) -> f64 {
    let Some(kern) = face.tables().kern else {
        return 0.0;
    };
    // Sum the horizontal, non-state-machine subtables that cover this pair.
    // DejaVu Sans has a single format-0 horizontal subtable, so this reduces to
    // one lookup in practice, matching FreeType's behavior.
    let mut units: i32 = 0;
    for subtable in kern.subtables {
        if !subtable.horizontal || subtable.variable {
            continue;
        }
        if let Some(value) = subtable.glyphs_kerning(left, right) {
            units += i32::from(value);
        }
    }
    f64::from(units)
}
