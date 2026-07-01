//! Single-line text measurement.
//!
//! Computes the advance width and vertical metrics of a single line of text at a
//! given pixel size. This is the rizzma analog of matplotlib's
//! `RendererBase.get_text_width_height_descent`.
//!
//! Scope: a single line, left-to-right, with no shaping, kerning, ligatures, or
//! bidi. Each character contributes its horizontal advance from the font's `hmtx`
//! table; widths therefore sum per-glyph advances. Multiline layout, complex
//! script shaping, and bidi reordering are explicitly out of scope for now.

use crate::text::font::FontSource;

/// The measured extent of a single line of text, in **pixels at the requested
/// font size**.
///
/// All fields are non-negative. `height` equals `ascent + descent` and gives the
/// font's vertical line extent (it is a font-level quantity, independent of which
/// glyphs are present).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextExtent {
    /// Total advance width of the line, in pixels.
    pub width: f64,
    /// Vertical extent of the line (`ascent + descent`), in pixels.
    pub height: f64,
    /// Distance from the baseline to the top of the line, in pixels. Positive.
    pub ascent: f64,
    /// Distance from the baseline to the bottom of the line, in pixels.
    /// Non-negative (zero for fonts that declare no descent).
    pub descent: f64,
}

impl FontSource {
    /// Measures a single line of `text` rendered at `font_size_px` pixels.
    ///
    /// The returned [`TextExtent`] is expressed in pixels at the given size. The
    /// width is the sum of per-character horizontal advances from the primary
    /// face, scaled from font design units by `font_size_px / units_per_em`. The
    /// vertical metrics come from the face's horizontal header (`hhea`):
    /// `ascent` is positive-up, `descent` is reported as a non-negative
    /// magnitude.
    ///
    /// An empty string yields a zero-width extent (the vertical metrics still
    /// reflect the font). If no face is registered or the primary face fails to
    /// parse, an all-zero extent is returned.
    ///
    /// Shaping, kerning, ligatures, bidi, and multiline layout are out of scope;
    /// see the module docs.
    #[must_use]
    pub fn measure(&self, text: &str, font_size_px: f64) -> TextExtent {
        let Some(face) = self.primary_face() else {
            return TextExtent {
                width: 0.0,
                height: 0.0,
                ascent: 0.0,
                descent: 0.0,
            };
        };

        let units_per_em = f64::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return TextExtent {
                width: 0.0,
                height: 0.0,
                ascent: 0.0,
                descent: 0.0,
            };
        }
        let scale = font_size_px / units_per_em;

        let mut advance_units: f64 = 0.0;
        for ch in text.chars() {
            if let Some(glyph_id) = face.glyph_index(ch)
                && let Some(advance) = face.glyph_hor_advance(glyph_id)
            {
                advance_units += f64::from(advance);
            }
        }

        // `hhea` descent is stored negative-down; report it as a magnitude.
        let ascent = f64::from(face.ascender()) * scale;
        let descent = (f64::from(face.descender()) * scale).abs();

        TextExtent {
            width: advance_units * scale,
            height: ascent + descent,
            ascent,
            descent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> FontSource {
        FontSource::dejavu_sans()
    }

    #[test]
    fn empty_string_has_zero_width() {
        let extent = source().measure("", 12.0);
        assert_eq!(extent.width, 0.0);
    }

    #[test]
    fn non_empty_has_positive_width() {
        let extent = source().measure("hello", 12.0);
        assert!(extent.width > 0.0, "width was {}", extent.width);
    }

    #[test]
    fn width_scales_monotonically_with_size() {
        let small = source().measure("rizzma", 10.0).width;
        let medium = source().measure("rizzma", 20.0).width;
        let large = source().measure("rizzma", 40.0).width;
        assert!(small < medium, "{small} !< {medium}");
        assert!(medium < large, "{medium} !< {large}");
    }

    #[test]
    fn width_scales_linearly_with_size() {
        let base = source().measure("rizzma", 10.0).width;
        let doubled = source().measure("rizzma", 20.0).width;
        assert!((doubled - 2.0 * base).abs() < 1e-9);
    }

    #[test]
    fn wide_glyphs_exceed_narrow_glyphs() {
        let wide = source().measure("WW", 12.0).width;
        let narrow = source().measure("ii", 12.0).width;
        assert!(wide > narrow, "WW ({wide}) should exceed ii ({narrow})");
    }

    #[test]
    fn vertical_metrics_are_sane() {
        let extent = source().measure("Ag", 12.0);
        assert!(extent.ascent > 0.0, "ascent was {}", extent.ascent);
        assert!(extent.descent >= 0.0, "descent was {}", extent.descent);
        assert!((extent.height - (extent.ascent + extent.descent)).abs() < 1e-9);
    }

    #[test]
    fn empty_source_returns_zeroes() {
        let extent = FontSource::new().measure("hello", 12.0);
        assert_eq!(
            extent,
            TextExtent {
                width: 0.0,
                height: 0.0,
                ascent: 0.0,
                descent: 0.0,
            }
        );
    }
}
