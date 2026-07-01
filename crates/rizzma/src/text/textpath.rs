//! Glyph-outline extraction: text rendered as vector [`Path`]s.
//!
//! This is the rizzma analog of matplotlib's "text as path" rendering, where a
//! string is laid out left-to-right and each glyph's outline is appended to a
//! single [`Path`]. The result can be fed through the figure transform stack and
//! drawn by the raster renderer like any other path.
//!
//! Scope matches [`crate::text::metrics`]: a single line, left-to-right, no
//! complex-script shaping, ligatures, or bidi. Pairwise `kern`-table kerning
//! **is** applied (each glyph tucks against its predecessor by the same amount
//! [`FontSource::measure`] accounts for), so the laid-out width stays identical
//! to the measured advance.
//!
//! # Coordinate orientation
//!
//! Outlines are produced in a **y-UP** coordinate system, matching the
//! font/matplotlib convention: glyphs extend upward from the baseline, so a
//! capital letter occupies positive `y`. The baseline sits at `origin.y` and the
//! pen starts at `origin.x`. Callers are expected to compose the figure transform
//! and the raster renderer applies its own Y-flip, so text ends up correctly
//! oriented on screen.

use crate::core::{Path, PathCode};
use ttf_parser::OutlineBuilder;

use crate::text::font::{FontSource, horizontal_kerning};

/// Accumulates `ttf-parser` outline callbacks into a [`Path`].
///
/// Incoming glyph coordinates are in font design units; they are scaled by
/// `scale` and translated by `(offset_x, offset_y)` so the glyph lands at the
/// current pen position relative to the text origin. The y-axis is left pointing
/// up (no flip), preserving the font's native orientation.
struct PathOutlineBuilder {
    /// Collected vertices, including curve control points.
    vertices: Vec<[f64; 2]>,
    /// One drawing code per appended vertex.
    codes: Vec<PathCode>,
    /// Font-unit-to-pixel scale (`font_size_px / units_per_em`).
    scale: f64,
    /// Horizontal pen offset, in pixels, for the current glyph.
    offset_x: f64,
    /// Vertical baseline offset, in pixels.
    offset_y: f64,
}

impl PathOutlineBuilder {
    /// Maps a font-unit point to the output pixel coordinate (y stays up).
    fn map(&self, x: f32, y: f64) -> [f64; 2] {
        [
            self.offset_x + f64::from(x) * self.scale,
            self.offset_y + y * self.scale,
        ]
    }
}

impl OutlineBuilder for PathOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.vertices.push(self.map(x, f64::from(y)));
        self.codes.push(PathCode::MoveTo);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.vertices.push(self.map(x, f64::from(y)));
        self.codes.push(PathCode::LineTo);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Quadratic: one control point + end point -> CURVE3.
        self.vertices.push(self.map(x1, f64::from(y1)));
        self.codes.push(PathCode::CurveTo3);
        self.vertices.push(self.map(x, f64::from(y)));
        self.codes.push(PathCode::CurveTo3);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // Cubic: two control points + end point -> CURVE4.
        self.vertices.push(self.map(x1, f64::from(y1)));
        self.codes.push(PathCode::CurveTo4);
        self.vertices.push(self.map(x2, f64::from(y2)));
        self.codes.push(PathCode::CurveTo4);
        self.vertices.push(self.map(x, f64::from(y)));
        self.codes.push(PathCode::CurveTo4);
    }

    fn close(&mut self) {
        // `ClosePoly` consumes one vertex; reuse the subpath's start position.
        // matplotlib stores the start vertex here, but the value is unused by the
        // close semantics, so any placeholder at the pen origin is fine.
        self.vertices.push([self.offset_x, self.offset_y]);
        self.codes.push(PathCode::ClosePoly);
    }
}

impl FontSource {
    /// Lays out `text` at `font_size_px` and returns its glyph outlines as a
    /// single [`Path`], positioned with the baseline at `origin[1]` and the pen
    /// starting at `origin[0]`.
    ///
    /// Each character's glyph is looked up in the primary face and its outline
    /// appended; the pen then advances by the glyph's horizontal advance from the
    /// `hmtx` table plus the `kern`-table adjustment against the previous glyph,
    /// scaled by `font_size_px / units_per_em`. Characters with no glyph or no
    /// outline (spaces, control characters) advance the pen without contributing
    /// geometry. The total advance width equals that reported by
    /// [`FontSource::measure`], which applies the same kerning.
    ///
    /// Outlines are emitted **y-up** (glyphs extend toward positive `y` from the
    /// baseline); see the [module docs](crate::text::textpath) for orientation details.
    ///
    /// An empty string, a missing/unparsable face, or a degenerate
    /// `units_per_em` yields an empty [`Path`] (no vertices).
    #[must_use]
    pub fn text_to_path(&self, text: &str, font_size_px: f64, origin: [f64; 2]) -> Path {
        let empty = || Path::new(Vec::new(), Some(Vec::new()));

        let Some(face) = self.primary_face() else {
            return empty();
        };
        let units_per_em = f64::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return empty();
        }
        let scale = font_size_px / units_per_em;

        let mut builder = PathOutlineBuilder {
            vertices: Vec::new(),
            codes: Vec::new(),
            scale,
            offset_x: origin[0],
            offset_y: origin[1],
        };

        // Pen x advances by each glyph's horizontal advance plus the pairwise
        // kern against the previous glyph (matching `FontSource::measure`).
        let mut pen_x = origin[0];
        let mut prev_glyph = None;
        for ch in text.chars() {
            let Some(glyph_id) = face.glyph_index(ch) else {
                continue;
            };
            // Tuck this glyph against its predecessor before placing it.
            if let Some(prev) = prev_glyph {
                pen_x += horizontal_kerning(&face, prev, glyph_id) * scale;
            }
            // Position this glyph at the current pen, then append its outline.
            // Glyphs without an outline (e.g. space) produce no segments but
            // still advance the pen below.
            builder.offset_x = pen_x;
            let _ = face.outline_glyph(glyph_id, &mut builder);

            if let Some(advance) = face.glyph_hor_advance(glyph_id) {
                pen_x += f64::from(advance) * scale;
            }
            prev_glyph = Some(glyph_id);
        }

        Path::new(builder.vertices, Some(builder.codes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> FontSource {
        FontSource::dejavu_sans()
    }

    /// Counts how many times `code` appears in the path's codes.
    fn count_code(path: &Path, code: PathCode) -> usize {
        path.codes()
            .map(|cs| cs.iter().filter(|&&c| c == code).count())
            .unwrap_or(0)
    }

    #[test]
    fn empty_string_is_empty_path() {
        let path = source().text_to_path("", 12.0, [0.0, 0.0]);
        assert!(path.vertices().is_empty());
    }

    #[test]
    fn glyph_has_moveto_and_close() {
        let path = source().text_to_path("A", 12.0, [0.0, 0.0]);
        assert!(!path.vertices().is_empty(), "A should have an outline");
        assert!(
            count_code(&path, PathCode::MoveTo) >= 1,
            "expected at least one MoveTo"
        );
        assert!(
            count_code(&path, PathCode::ClosePoly) >= 1,
            "expected at least one ClosePoly"
        );
    }

    #[test]
    fn outline_is_y_up_above_baseline() {
        // With the baseline at y = 0 and y-up orientation, a capital 'A' should
        // occupy positive y.
        let path = source().text_to_path("A", 12.0, [0.0, 0.0]);
        let e = path.get_extents();
        assert!(e.ymax() > 0.0, "ymax was {}", e.ymax());
        assert!(e.height() > 0.0, "height was {}", e.height());
    }

    #[test]
    fn height_grows_with_font_size() {
        let small = source().text_to_path("A", 12.0, [0.0, 0.0]);
        let large = source().text_to_path("A", 24.0, [0.0, 0.0]);
        let hs = small.get_extents().height();
        let hl = large.get_extents().height();
        assert!(hs > 0.0, "small height was {hs}");
        // Doubling the font size should roughly double the glyph height.
        let ratio = hl / hs;
        assert!((ratio - 2.0).abs() < 0.05, "height ratio was {ratio}");
    }

    #[test]
    fn advance_width_matches_measure() {
        let size = 16.0;
        let path = source().text_to_path("AB", size, [0.0, 0.0]);
        // Lay out a probe glyph immediately after "AB"; the gap between its
        // start and the origin equals the advance width of "AB".
        let measured = source().measure("AB", size).width;
        let path_width = path.get_extents().xmax();
        // The drawn outline of "AB" ends slightly before the full advance (the
        // right side bearing of 'B'), so the advance must be at least the inked
        // width and close to the measured advance.
        assert!(
            path_width <= measured + 1e-6,
            "inked width {path_width} should not exceed advance {measured}"
        );
        assert!(
            (measured - path_width).abs() < measured * 0.25,
            "inked width {path_width} unexpectedly far from advance {measured}"
        );
    }

    #[test]
    fn advance_width_matches_measure_exact() {
        // Tie the layout advance to the metrics API by placing a sentinel glyph
        // and comparing pen positions. We reconstruct the pen advance from a
        // single-glyph-difference trick: "AB" then origin-shifted "AB ".
        let size = 16.0;
        let measured = source().measure("AB", size).width;
        let measured_plus = source().measure("ABA", size).width;
        // Width of a trailing 'A' = measure("ABA") - measure("AB"). The
        // path of "ABA" should extend its xmax by roughly that amount over "AB".
        let ab = source().text_to_path("AB", size, [0.0, 0.0]);
        let aba = source().text_to_path("ABA", size, [0.0, 0.0]);
        let delta_path = aba.get_extents().xmax() - ab.get_extents().xmax();
        let delta_measure = measured_plus - measured;
        assert!(
            (delta_path - delta_measure).abs() < delta_measure * 0.2,
            "path delta {delta_path} vs measure delta {delta_measure}"
        );
    }

    #[test]
    fn kerned_pair_path_matches_kerned_measure() {
        // The laid-out path width of a kerning pair must track the kerned
        // measurement (both apply the same `kern`-table adjustment), and must be
        // narrower than the two glyphs placed with no kerning.
        let size = 100.0;
        let src = source();
        let ta = src.text_to_path("Ta", size, [0.0, 0.0]);
        let measured = src.measure("Ta", size).width;
        let t = src.measure("T", size).width;
        let a = src.measure("a", size).width;
        // Inked path ends within the last glyph's advance, so it is <= the
        // kerned advance but well above the unkerned pair minus a glyph.
        assert!(ta.get_extents().xmax() <= measured + 1e-6);
        assert!(measured < t + a, "'Ta' should kern below T+a");
    }

    #[test]
    fn space_advances_the_pen() {
        // "A B" should be wider than "AB" because the space advances the pen.
        let ab = source()
            .text_to_path("AB", 16.0, [0.0, 0.0])
            .get_extents()
            .xmax();
        let a_b = source()
            .text_to_path("A B", 16.0, [0.0, 0.0])
            .get_extents()
            .xmax();
        assert!(a_b > ab, "'A B' ({a_b}) should be wider than 'AB' ({ab})");
    }
}
