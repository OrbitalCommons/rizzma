//! Free-standing text: [`Text`], placed in figure fractions by
//! [`Figure::text`](crate::figure::Figure::text), and the alignment rule it
//! shares with matplotlib.
//!
//! matplotlib (3.11) aligns a string by a layout box: horizontally from the pen
//! origin to the right edge of the ink, vertically over the ink widened to at
//! least the font's typographic ascender and descender, so strings with and
//! without ascenders or descenders sit on a common line. [`Text`] reproduces
//! that box, rotates it, and aligns the rotated box's bounds to the anchor, as
//! matplotlib's default `rotation_mode` does.

use crate::core::{Affine2D, Bbox, color::Rgba};
use crate::figure::axes::{TextBoxStyle, rounded_rect_path};
use crate::figure::richtext::layout_rich_text;
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

/// Font size of [`Figure::text`](crate::figure::Figure::text), in px at 100
/// DPI — the size axes text uses.
pub(crate) const DEFAULT_TEXT_SIZE: f64 = 10.0;

/// Horizontal alignment of text about its anchor — matplotlib's `ha`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlignment {
    /// The text starts at the anchor (the default).
    #[default]
    Left,
    /// The text is centered on the anchor.
    Center,
    /// The text ends at the anchor.
    Right,
}

/// Vertical alignment of text about its anchor — matplotlib's `va`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment {
    /// The top of the text sits at the anchor.
    Top,
    /// The text is centered on the anchor.
    Center,
    /// The baseline sits at the anchor (the default).
    #[default]
    Baseline,
    /// The bottom of the text, below any descenders, sits at the anchor.
    Bottom,
}

/// A string placed at an anchor with matplotlib's alignment, rotation and
/// optional background box — what [`Figure::text`](crate::figure::Figure::text)
/// returns for styling.
///
/// `$...$` spans render as mathtext, as in titles and axes text.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    /// The string drawn.
    text: String,
    /// Anchor position, in the owner's coordinates (figure fractions for
    /// figure text).
    x: f64,
    y: f64,
    /// Font size in px at 100 DPI.
    size: f64,
    /// Ink color.
    color: Rgba,
    /// Horizontal alignment about the anchor.
    ha: HorizontalAlignment,
    /// Vertical alignment about the anchor.
    va: VerticalAlignment,
    /// Counter-clockwise rotation in degrees.
    rotation: f64,
    /// Optional background box.
    bbox: Option<TextBoxStyle>,
}

impl Text {
    /// Text `s` anchored at `(x, y)`, left- and baseline-aligned, unrotated,
    /// at `size` px (at 100 DPI) in `color`.
    pub(crate) fn new(x: f64, y: f64, s: impl Into<String>, size: f64, color: Rgba) -> Self {
        Self {
            text: s.into(),
            x,
            y,
            size,
            color,
            ha: HorizontalAlignment::Left,
            va: VerticalAlignment::Baseline,
            rotation: 0.0,
            bbox: None,
        }
    }

    /// The string drawn.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The anchor position, in the owner's coordinates.
    #[must_use]
    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    /// Replace the string.
    pub fn set_text(&mut self, s: impl Into<String>) -> &mut Self {
        self.text = s.into();
        self
    }

    /// Move the anchor, in the owner's coordinates.
    pub fn set_position(&mut self, x: f64, y: f64) -> &mut Self {
        self.x = x;
        self.y = y;
        self
    }

    /// Set the font size, in px at 100 DPI (scaling with the DPI like every
    /// other decoration).
    pub fn set_size(&mut self, size: f64) -> &mut Self {
        self.size = size;
        self
    }

    /// Set the ink color.
    pub fn set_color(&mut self, color: Rgba) -> &mut Self {
        self.color = color;
        self
    }

    /// Set the horizontal alignment about the anchor — matplotlib's `ha`.
    pub fn set_ha(&mut self, ha: HorizontalAlignment) -> &mut Self {
        self.ha = ha;
        self
    }

    /// Set the vertical alignment about the anchor — matplotlib's `va`.
    pub fn set_va(&mut self, va: VerticalAlignment) -> &mut Self {
        self.va = va;
        self
    }

    /// Set the counter-clockwise rotation in degrees. The rotated text's
    /// bounds are aligned to the anchor, as in matplotlib's default
    /// `rotation_mode`.
    pub fn set_rotation(&mut self, degrees: f64) -> &mut Self {
        self.rotation = degrees;
        self
    }

    /// Draw a background box behind the text, or remove it with `None`.
    pub fn set_bbox(&mut self, bbox: Option<TextBoxStyle>) -> &mut Self {
        self.bbox = bbox;
        self
    }

    /// The layout box `(left, bottom, right, top)` in the unrotated baseline
    /// frame at `size_px`, plus the laid-out glyph paths.
    fn layout(&self, font: &FontSource, size_px: f64) -> (Bbox, Vec<crate::core::Path>) {
        let rich = layout_rich_text(font, &self.text, size_px);
        let ink = rich
            .paths
            .iter()
            .filter(|p| !p.vertices().is_empty())
            .map(crate::core::Path::get_extents)
            .reduce(|a, b| a.union(&b));
        let (right, ymin, ymax) = match ink {
            Some(ink) => (ink.xmax(), ink.ymin(), ink.ymax()),
            None => (rich.width, 0.0, 0.0),
        };
        let (ascent, descent) = font.line_box(size_px);
        let layout = Bbox::from_extents(0.0, ymin.min(-descent), right, ymax.max(ascent));
        (layout, rich.paths)
    }

    /// The transform from the baseline frame to device pixels, placing the
    /// rotated layout box against `anchor` per the alignment.
    fn placement(&self, layout: &Bbox, anchor: (f64, f64)) -> Affine2D {
        let rotation = Affine2D::from_rotation_deg(self.rotation);
        let rotated = layout.transformed(&rotation);
        let dx = match self.ha {
            HorizontalAlignment::Left => -rotated.xmin(),
            HorizontalAlignment::Center => -(rotated.xmin() + rotated.xmax()) / 2.0,
            HorizontalAlignment::Right => -rotated.xmax(),
        };
        let dy = match self.va {
            VerticalAlignment::Top => -rotated.ymax(),
            VerticalAlignment::Center => -(rotated.ymin() + rotated.ymax()) / 2.0,
            VerticalAlignment::Baseline => 0.0,
            VerticalAlignment::Bottom => -rotated.ymin(),
        };
        rotation.then(&Affine2D::from_translation(anchor.0 + dx, anchor.1 + dy))
    }

    /// The width and height in pixels of the rotated layout box (background
    /// box included) at decoration scale `s` — what layout reserves for it.
    pub(crate) fn extent(&self, font: &FontSource, s: f64) -> (f64, f64) {
        if self.text.is_empty() {
            return (0.0, 0.0);
        }
        let (layout, _) = self.layout(font, self.size * s);
        let layout = self.padded(&layout, s);
        let rotated = layout.transformed(&Affine2D::from_rotation_deg(self.rotation));
        (rotated.width(), rotated.height())
    }

    /// `layout` grown by the background box's padding, if there is a box.
    fn padded(&self, layout: &Bbox, s: f64) -> Bbox {
        let pad = self.bbox.map_or(0.0, |style| style.pad.max(0.0) * s);
        Bbox::from_extents(
            layout.xmin() - pad,
            layout.ymin() - pad,
            layout.xmax() + pad,
            layout.ymax() + pad,
        )
    }

    /// Draw the text (and its box) aligned to `anchor`, in device pixels with
    /// y up.
    pub(crate) fn draw(&self, renderer: &mut dyn Renderer, font: &FontSource, anchor: (f64, f64)) {
        if self.text.is_empty() {
            return;
        }
        let s = renderer.decoration_scale();
        let (layout, paths) = self.layout(font, self.size * s);
        let place = self.placement(&layout, anchor);
        if let Some(style) = self.bbox {
            let background =
                rounded_rect_path(&self.padded(&layout, s), style.corner_radius.max(0.0) * s);
            renderer.draw_path(
                &GraphicsContext::new(),
                &background,
                &place,
                Some(style.facecolor),
            );
            if let Some(edge) = style.edgecolor {
                let gc = GraphicsContext::new()
                    .with_stroke(edge)
                    .with_line_width(style.linewidth);
                renderer.draw_path(&gc, &background, &place, None);
            }
        }
        for path in &paths {
            renderer.draw_path(&GraphicsContext::new(), path, &place, Some(self.color));
        }
    }
}
