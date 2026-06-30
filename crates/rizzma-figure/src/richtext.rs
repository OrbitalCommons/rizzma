//! Math-aware single-line text layout for figure text artists.
//!
//! This module is the first seam between figure text rendering and the
//! [`rizzma_mathtext`] engine. It parses a label into [`TextRun`] spans, lays
//! out plain spans with [`FontSource::text_to_path`] and math spans with
//! [`layout_math`], and concatenates the resulting glyph/rule [`Path`]s into a
//! single baseline-relative frame so callers can position the whole label like
//! any other path.
//!
//! Coordinates are **y-up** with the baseline at `y = 0` and the pen starting at
//! `x = 0`, matching the axes coordinate system and the raster backend's Y-flip
//! convention.
//! A pure-plain label produces exactly the same geometry and width as
//! [`FontSource::text_to_path`]/[`FontSource::measure`], so non-math titles are
//! unchanged.

use rizzma_core::Path;
use rizzma_mathtext::layout_math;
use rizzma_text::{FontSource, TextRun, TextSpanKind};

/// A laid-out single line of (possibly math) text in a local baseline frame.
///
/// The [`paths`](RichText::paths) are positioned with the line's baseline at
/// `y = 0` and the left edge at `x = 0`, in y-up coordinates. Fill them with a
/// single color to draw the label.
#[derive(Clone, Debug, PartialEq)]
pub struct RichText {
    /// Glyph-outline and rule paths in the local baseline frame.
    pub paths: Vec<Path>,
    /// Total advance width of the line, in pixels.
    pub width: f64,
    /// Distance from the baseline to the top of the line, in pixels.
    pub ascent: f64,
    /// Distance from the baseline to the bottom of the line, in pixels.
    pub descent: f64,
}

/// Lay out `text` as a single line, rendering `$...$` spans as mathtext.
///
/// `text` is parsed with [`TextRun::parse`]: plain spans are drawn with
/// [`FontSource::text_to_path`] and math spans with
/// [`rizzma_mathtext::layout_math`]. Spans are placed left to right, advancing a
/// shared pen, and every glyph/rule path is collected into the returned
/// [`RichText`] in a baseline-relative, y-up frame (baseline at `y = 0`, left
/// edge at `x = 0`).
///
/// A pure-plain string yields a single path whose width equals
/// [`FontSource::measure`]; an empty string yields an empty layout.
///
/// # Examples
///
/// ```
/// use rizzma_figure::richtext::layout_rich_text;
/// use rizzma_text::FontSource;
///
/// let font = FontSource::dejavu_sans();
/// let rich = layout_rich_text(&font, "$x^2$", 12.0);
/// assert!(rich.width > 0.0);
/// assert!(!rich.paths.is_empty());
/// ```
#[must_use]
pub fn layout_rich_text(font: &FontSource, text: &str, font_size_px: f64) -> RichText {
    let run = TextRun::parse(text);

    let mut paths = Vec::new();
    let mut x = 0.0;
    let mut ascent: f64 = 0.0;
    let mut descent: f64 = 0.0;

    for span in run.spans() {
        match span.kind() {
            TextSpanKind::Math(_) => {
                let layout = layout_math(span.content(), font, font_size_px);
                // Read metrics before placing, then collect the run's geometry
                // via the variant-agnostic `MathLayout::translated` +
                // `MathElement::path` accessors. New `MathElement` kinds
                // (accents, future constructs) need no changes here.
                let advance = layout.width;
                ascent = ascent.max(layout.ascent);
                descent = descent.max(layout.descent);
                for element in layout.translated(x, 0.0).elements {
                    paths.push(element.path().clone());
                }
                x += advance;
            }
            // Plain spans and raw-TeX passthrough both fall back to drawing
            // their content as ordinary glyph outlines.
            TextSpanKind::Plain | TextSpanKind::RawTex(_) => {
                let content = span.content();
                let extent = font.measure(content, font_size_px);
                paths.push(font.text_to_path(content, font_size_px, [x, 0.0]));
                x += extent.width;
                ascent = ascent.max(extent.ascent);
                descent = descent.max(extent.descent);
            }
        }
    }

    RichText {
        paths,
        width: x,
        ascent,
        descent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> FontSource {
        FontSource::dejavu_sans()
    }

    #[test]
    fn math_superscript_changes_layout() {
        let scripted = layout_rich_text(&font(), "$x^2$", 12.0);
        assert!(scripted.width > 0.0);
        // The superscript splits the base and exponent into separate glyph
        // paths, so a math layout is non-trivial.
        assert!(scripted.paths.len() > 1);

        // The superscripted form must differ from the plain "x2": both width
        // (the exponent is scaled down) and the layout itself change.
        let plain = layout_rich_text(&font(), "x2", 12.0);
        assert!((scripted.width - plain.width).abs() > f64::EPSILON);
        assert_ne!(scripted.paths, plain.paths);
    }

    #[test]
    fn plain_width_matches_measure() {
        let text = "plain title";
        let rich = layout_rich_text(&font(), text, 12.0);
        let measured = font().measure(text, 12.0);
        assert_eq!(rich.paths.len(), 1);
        assert!((rich.width - measured.width).abs() < 1e-9);
        assert!((rich.ascent - measured.ascent).abs() < 1e-9);
        assert!((rich.descent - measured.descent).abs() < 1e-9);
    }

    #[test]
    fn plain_path_matches_text_to_path() {
        let text = "rizzma";
        let rich = layout_rich_text(&font(), text, 12.0);
        let direct = font().text_to_path(text, 12.0, [0.0, 0.0]);
        // A pure-plain label must reproduce today's single-path geometry.
        assert_eq!(rich.paths, vec![direct]);
    }

    #[test]
    fn empty_string_is_empty_layout() {
        let rich = layout_rich_text(&font(), "", 12.0);
        assert!(rich.paths.is_empty());
        assert_eq!(rich.width, 0.0);
        assert_eq!(rich.ascent, 0.0);
        assert_eq!(rich.descent, 0.0);
    }

    #[test]
    fn mixed_plain_and_math_advances_pen() {
        let rich = layout_rich_text(&font(), "y = $x^2$", 12.0);
        let plain_only = layout_rich_text(&font(), "y = ", 12.0);
        // Adding the math span past the plain prefix must widen the line.
        assert!(rich.width > plain_only.width);
        assert!(rich.paths.len() > plain_only.paths.len());
    }
}
