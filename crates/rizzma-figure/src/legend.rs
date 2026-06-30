//! Axes legend: a small keyed box of color samples and labels.
//!
//! [`Axes::legend`] stores a list of `(color, label)` entries; [`Axes::draw`]
//! renders them as a boxed key in the upper-right corner inside the axes. Each
//! row pairs a short colored line sample with its label, drawn via
//! [`FontSource::text_to_path`].

use rizzma_core::{Affine2D, Bbox, Path, color::Rgba};
use rizzma_render::{GraphicsContext, Renderer};
use rizzma_text::FontSource;

use crate::axes::Axes;

/// A single legend row: a color swatch paired with a label.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegendEntry {
    /// The color of the line sample drawn beside the label.
    pub(crate) color: Rgba,
    /// The label text drawn to the right of the sample.
    pub(crate) label: String,
}

/// Geometry constants for legend layout, all in pixels.
mod layout {
    /// Font size of legend labels.
    pub(super) const FONT_SIZE: f64 = 10.0;
    /// Inner padding between the legend border and its contents.
    pub(super) const PAD: f64 = 6.0;
    /// Length of the colored line sample.
    pub(super) const SAMPLE_LEN: f64 = 20.0;
    /// Gap between the line sample and the label text.
    pub(super) const SAMPLE_GAP: f64 = 6.0;
    /// Height allotted to each row.
    pub(super) const ROW_HEIGHT: f64 = 16.0;
    /// Offset of the legend box from the axes' upper-right corner.
    pub(super) const MARGIN: f64 = 8.0;
    /// Stroke width of the colored line sample.
    pub(super) const SAMPLE_WIDTH: f64 = 2.0;
    /// Stroke width of the legend box border.
    pub(super) const BORDER_WIDTH: f64 = 0.8;
}

impl Axes {
    /// Add a legend keyed by explicit `(color, label)` entries.
    ///
    /// The legend is drawn as a boxed key in the upper-right corner inside the
    /// axes, one row per entry. Calling this replaces any previously set legend.
    ///
    // TODO: auto-collect from artist labels + best-location search.
    pub fn legend(&mut self, entries: Vec<(Rgba, String)>) -> &mut Self {
        self.legend = entries
            .into_iter()
            .map(|(color, label)| LegendEntry { color, label })
            .collect();
        self
    }

    /// Draw the legend box in the upper-right corner of `axes_px`, if any
    /// entries are set.
    pub(crate) fn draw_legend(
        &self,
        renderer: &mut dyn Renderer,
        axes_px: &Bbox,
        font: &FontSource,
    ) {
        if self.legend.is_empty() {
            return;
        }

        // Size the box from the longest label and the row count.
        let label_w = self
            .legend
            .iter()
            .map(|e| font.measure(&e.label, layout::FONT_SIZE).width)
            .fold(0.0_f64, f64::max);
        let content_w = layout::SAMPLE_LEN + layout::SAMPLE_GAP + label_w;
        let box_w = content_w + 2.0 * layout::PAD;
        let box_h = self.legend.len() as f64 * layout::ROW_HEIGHT + 2.0 * layout::PAD;

        // Position the box just inside the upper-right corner.
        let x1 = axes_px.xmax() - layout::MARGIN;
        let y1 = axes_px.ymax() - layout::MARGIN;
        let x0 = x1 - box_w;
        let y0 = y1 - box_h;
        let box_bbox = Bbox::from_extents(x0, y0, x1, y1);

        // White background with a thin gray border.
        let rect = rect_path(&box_bbox);
        let id = Affine2D::identity();
        renderer.draw_path(&GraphicsContext::new(), &rect, &id, Some(Rgba::WHITE));
        let border_gc = GraphicsContext::new()
            .with_stroke(Rgba::rgb(0.5, 0.5, 0.5))
            .with_line_width(layout::BORDER_WIDTH);
        renderer.draw_path(&border_gc, &rect, &id, None);

        // Rows are laid out top-to-bottom; the y-axis is y-up so the first row
        // sits at the largest y.
        for (i, entry) in self.legend.iter().enumerate() {
            let row_top = y1 - layout::PAD - i as f64 * layout::ROW_HEIGHT;
            let row_mid = row_top - layout::ROW_HEIGHT / 2.0;

            // Colored line sample.
            let sx0 = x0 + layout::PAD;
            let sx1 = sx0 + layout::SAMPLE_LEN;
            let sample = Path::from_polyline(&[[sx0, row_mid], [sx1, row_mid]]);
            let sample_gc = GraphicsContext::new()
                .with_stroke(entry.color)
                .with_line_width(layout::SAMPLE_WIDTH);
            renderer.draw_path(&sample_gc, &sample, &id, None);

            // Label text, baseline centered on the row.
            let tx = sx1 + layout::SAMPLE_GAP;
            let ty = row_mid - layout::FONT_SIZE / 3.0;
            let text = font.text_to_path(&entry.label, layout::FONT_SIZE, [tx, ty]);
            renderer.draw_path(&GraphicsContext::new(), &text, &id, Some(Rgba::BLACK));
        }
    }
}

/// A closed-rectangle [`Path`] tracing `bbox`'s four corners.
fn rect_path(bbox: &Bbox) -> Path {
    let (x0, y0) = (bbox.xmin(), bbox.ymin());
    let (x1, y1) = (bbox.xmax(), bbox.ymax());
    Path::from_polyline(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]])
}

#[cfg(test)]
mod tests {
    use crate::Figure;
    use rizzma_core::color::Rgba;
    use rizzma_skia::SkiaRenderer;

    /// Straight RGBA bytes of the pixel at `(x, y)` (top-left origin).
    fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
        let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn legend_puts_ink_in_upper_right() {
        let mut fig = Figure::new(4.0, 4.0).with_dpi(100.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
        ax.legend(vec![
            (Rgba::RED, "alpha".to_string()),
            (Rgba::BLUE, "beta".to_string()),
        ]);

        let r = fig.render();
        let (w, h) = fig.size_px();
        let (w, h) = (w as u32, h as u32);

        // Scan the upper-right quadrant for non-background ink from the legend.
        let mut found = false;
        'scan: for y in 0..(h / 2) {
            for x in (w / 2)..w {
                let p = pixel(&r, x, y);
                if p != [255, 255, 255, 255] {
                    found = true;
                    break 'scan;
                }
            }
        }
        assert!(found, "expected legend ink in the upper-right region");
    }
}
