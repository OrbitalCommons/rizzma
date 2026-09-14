//! Axes legend: a small keyed box of color samples and labels.
//!
//! [`Axes::legend`] stores a list of `(color, label)` entries and
//! [`Axes::legend_auto`] derives them from the artists' own labels;
//! [`Axes::draw`] renders them as a boxed key in the upper-right corner inside
//! the axes. Each row pairs a short colored line sample with its label, drawn
//! via [`FontSource::text_to_path`].

use crate::core::{Affine2D, Bbox, Path, color::Rgba};
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

use crate::figure::axes::Axes;

/// A single legend row: a color swatch paired with a label.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegendEntry {
    /// The color of the line sample drawn beside the label.
    pub(crate) color: Rgba,
    /// The label text drawn to the right of the sample.
    pub(crate) label: String,
}

/// Corner of an axes in which a legend is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegendLocation {
    /// Top-right corner (the default).
    #[default]
    UpperRight,
    /// Top-left corner.
    UpperLeft,
    /// Bottom-right corner.
    LowerRight,
    /// Bottom-left corner.
    LowerLeft,
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
    /// axes, one row per entry. Calling this replaces any previously set
    /// entries. For a legend that follows the artists themselves, label them
    /// and call [`legend_auto`](Axes::legend_auto) instead.
    ///
    // TODO: best-location search.
    pub fn legend(&mut self, entries: Vec<(Rgba, String)>) -> &mut Self {
        self.legend = entries
            .into_iter()
            .map(|(color, label)| LegendEntry { color, label })
            .collect();
        self
    }

    /// Build the legend from the artists' own labels — matplotlib's bare
    /// `ax.legend()`.
    ///
    /// Walks every [`Line2D`](crate::artist::Line2D), then every
    /// [`Patch`](crate::artist::Patch), in insertion order and adds one row per
    /// labelled artist, keyed by the artist's color: a line's stroke, a patch's
    /// face (or its edge when unfilled). Because the color is read from the
    /// artist, the legend cannot drift from the plot when a series is restyled.
    ///
    /// Skipped, as in matplotlib: unlabelled artists, empty labels, and labels
    /// starting with `_`. A label shared by several artists — a series split
    /// into runs, or one line per interval — yields a single row colored by its
    /// first artist, so callers need not de-duplicate. Replaces any previously
    /// set entries; the title and location are kept.
    ///
    /// ```
    /// use rizzma::artist::Line2D;
    /// use rizzma::core::{Bbox, Rgba};
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.plot(&[0.0, 1.0], &[0.0, 1.0]).set_label("p50");
    /// ax.add_line(Line2D::new(vec![0.0, 1.0], vec![1.0, 2.0])
    ///     .with_color(Rgba::RED)
    ///     .with_label("p95"));
    /// ax.plot(&[0.0, 1.0], &[2.0, 3.0]).set_label("_guide");
    /// ax.legend_auto();
    /// ```
    pub fn legend_auto(&mut self) -> &mut Self {
        let lines = self
            .lines
            .iter()
            .filter_map(|line| line.label().map(|label| (line.color(), label)));
        let patches = self.patches.iter().filter_map(|patch| {
            let color = patch.face().or_else(|| patch.edge())?;
            patch.label().map(|label| (color, label))
        });

        let mut entries: Vec<LegendEntry> = Vec::new();
        for (color, label) in lines.chain(patches) {
            if label.is_empty() || label.starts_with('_') {
                continue;
            }
            if entries.iter().any(|e| e.label == label) {
                continue;
            }
            entries.push(LegendEntry {
                color,
                label: label.to_owned(),
            });
        }
        self.legend = entries;
        self
    }

    /// Add a legend with a heading above its entries.
    ///
    /// Calling this replaces both the current entries and title.
    ///
    /// ![legend title](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_legend_colorbar.png)
    ///
    /// ```no_run
    /// use rizzma::{Figure, core::Rgba};
    /// let mut fig = Figure::new(4.0, 3.0);
    /// fig.add_subplot(1, 1, 1).legend_with_title(
    ///     vec![(Rgba::RED, "signal".to_owned())],
    ///     "Channels",
    /// );
    /// fig.save_png("legend_title.png")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn legend_with_title(
        &mut self,
        entries: Vec<(Rgba, String)>,
        title: impl Into<String>,
    ) -> &mut Self {
        self.legend(entries);
        self.legend_title = Some(title.into());
        self
    }

    /// Add a legend at a selected axes corner.
    ///
    /// ![legend location](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_legend_colorbar.png)
    ///
    /// ```no_run
    /// use rizzma::{Figure, LegendLocation, core::Rgba};
    /// let mut fig = Figure::new(4.0, 3.0);
    /// fig.add_subplot(1, 1, 1).legend_at(
    ///     vec![(Rgba::BLUE, "data".to_owned())],
    ///     LegendLocation::LowerLeft,
    /// );
    /// fig.save_png("legend_location.png")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn legend_at(
        &mut self,
        entries: Vec<(Rgba, String)>,
        location: LegendLocation,
    ) -> &mut Self {
        self.legend(entries);
        self.legend_location = location;
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

        // Layout constants are px at the default 100 DPI; scale with the
        // renderer so the key stays proportionate on high-DPI renders.
        let s = renderer.decoration_scale();
        let font_size = layout::FONT_SIZE * s;
        let (sample_len, sample_gap) = (layout::SAMPLE_LEN * s, layout::SAMPLE_GAP * s);
        let (row_height, margin, pad) =
            (layout::ROW_HEIGHT * s, layout::MARGIN * s, layout::PAD * s);

        // Size the box from the longest label and the row count.
        let label_w = self
            .legend
            .iter()
            .map(|e| font.measure(&e.label, font_size).width)
            .fold(0.0_f64, f64::max);
        let title_w = self
            .legend_title
            .as_deref()
            .filter(|title| !title.is_empty())
            .map_or(0.0, |title| font.measure(title, font_size).width);
        let has_title = title_w > 0.0;
        let content_w = (sample_len + sample_gap + label_w).max(title_w);
        let box_w = content_w + 2.0 * pad;
        let box_h = (self.legend.len() as f64 + f64::from(has_title)) * row_height + 2.0 * pad;

        // Position the box just inside the requested corner.
        let (x0, x1) = match self.legend_location {
            LegendLocation::UpperRight | LegendLocation::LowerRight => {
                let x1 = axes_px.xmax() - margin;
                (x1 - box_w, x1)
            }
            LegendLocation::UpperLeft | LegendLocation::LowerLeft => {
                let x0 = axes_px.xmin() + margin;
                (x0, x0 + box_w)
            }
        };
        let (y0, y1) = match self.legend_location {
            LegendLocation::UpperRight | LegendLocation::UpperLeft => {
                let y1 = axes_px.ymax() - margin;
                (y1 - box_h, y1)
            }
            LegendLocation::LowerRight | LegendLocation::LowerLeft => {
                let y0 = axes_px.ymin() + margin;
                (y0, y0 + box_h)
            }
        };
        let box_bbox = Bbox::from_extents(x0, y0, x1, y1);

        // Background and border colors follow the axes' resolved style.
        let rect = rect_path(&box_bbox);
        let id = Affine2D::identity();
        renderer.draw_path(
            &GraphicsContext::new(),
            &rect,
            &id,
            Some(self.legend_facecolor),
        );
        let border_gc = GraphicsContext::new()
            .with_stroke(self.legend_edgecolor)
            .with_line_width(layout::BORDER_WIDTH);
        renderer.draw_path(&border_gc, &rect, &id, None);

        if let Some(title) = self
            .legend_title
            .as_deref()
            .filter(|title| !title.is_empty())
        {
            let ty = y1 - pad - row_height / 2.0 - font_size / 3.0;
            let text = font.text_to_path(title, font_size, [x0 + pad, ty]);
            renderer.draw_path(
                &GraphicsContext::new(),
                &text,
                &id,
                Some(self.legend_labelcolor),
            );
        }

        // Rows are laid out top-to-bottom; the y-axis is y-up so the first row
        // sits at the largest y.
        for (i, entry) in self.legend.iter().enumerate() {
            let title_rows = f64::from(has_title);
            let row_top = y1 - pad - (i as f64 + title_rows) * row_height;
            let row_mid = row_top - row_height / 2.0;

            // Colored line sample.
            let sx0 = x0 + pad;
            let sx1 = sx0 + sample_len;
            let sample = Path::from_polyline(&[[sx0, row_mid], [sx1, row_mid]]);
            let sample_gc = GraphicsContext::new()
                .with_stroke(entry.color)
                .with_line_width(layout::SAMPLE_WIDTH);
            renderer.draw_path(&sample_gc, &sample, &id, None);

            // Label text, baseline centered on the row.
            let tx = sx1 + sample_gap;
            let ty = row_mid - font_size / 3.0;
            let text = font.text_to_path(&entry.label, font_size, [tx, ty]);
            renderer.draw_path(
                &GraphicsContext::new(),
                &text,
                &id,
                Some(self.legend_labelcolor),
            );
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
    use super::Axes;
    use crate::artist::{Line2D, Patch};
    use crate::core::Bbox;

    #[test]
    fn legend_auto_collects_labelled_artists_in_order() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]).set_label("p50");
        let c0 = ax.lines()[0].color();
        ax.add_line(
            Line2D::new(vec![0.0, 1.0], vec![1.0, 2.0])
                .with_color(Rgba::RED)
                .with_label("p95"),
        );
        // Anonymous, empty, and underscore-prefixed labels are skipped.
        ax.plot(&[0.0, 1.0], &[2.0, 3.0]);
        ax.plot(&[0.0, 1.0], &[2.0, 3.0]).set_label("");
        ax.plot(&[0.0, 1.0], &[2.0, 3.0]).set_label("_guide");
        // Patches follow lines and key on the face color.
        ax.add_patch(
            Patch::rectangle(0.0, 0.0, 1.0, 1.0)
                .facecolor(Some(Rgba::GREEN))
                .with_label("band"),
        );
        ax.add_patch(
            Patch::rectangle(0.0, 0.0, 1.0, 1.0)
                .facecolor(None)
                .edgecolor(Some(Rgba::BLUE))
                .with_label("outline"),
        );

        ax.legend_auto();
        let rows: Vec<(Rgba, &str)> = ax
            .legend
            .iter()
            .map(|e| (e.color, e.label.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (c0, "p50"),
                (Rgba::RED, "p95"),
                (Rgba::GREEN, "band"),
                (Rgba::BLUE, "outline"),
            ]
        );
    }

    #[test]
    fn legend_auto_dedups_by_label_and_tracks_restyling() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // One logical series drawn as three runs.
        for run in 0..3 {
            let x0 = f64::from(run) * 2.0;
            ax.add_line(
                Line2D::new(vec![x0, x0 + 1.0], vec![0.0, 1.0])
                    .with_color(Rgba::RED)
                    .with_label("camera"),
            );
        }
        ax.legend_auto();
        assert_eq!(ax.legend.len(), 1);
        assert_eq!(ax.legend[0].color, Rgba::RED);

        // Recoloring the series and rebuilding follows the artist.
        for line in ax.lines_mut() {
            line.set_color(Rgba::BLUE);
        }
        ax.legend_auto();
        assert_eq!(ax.legend.len(), 1);
        assert_eq!(ax.legend[0].color, Rgba::BLUE);
    }

    #[test]
    fn legend_auto_keeps_title_and_location() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.legend_with_title(vec![(Rgba::RED, "stale".to_owned())], "Channels");
        ax.legend_location = LegendLocation::LowerLeft;
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]).set_label("fresh");
        ax.legend_auto();
        assert_eq!(ax.legend.len(), 1);
        assert_eq!(ax.legend[0].label, "fresh");
        assert_eq!(ax.legend_title.as_deref(), Some("Channels"));
        assert_eq!(ax.legend_location, LegendLocation::LowerLeft);
    }

    use super::LegendLocation;
    use crate::core::color::Rgba;
    use crate::figure::Figure;
    use crate::skia::SkiaRenderer;

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

    #[test]
    fn legend_with_title_stores_heading_and_renders() {
        let mut fig = Figure::new(4.0, 4.0);
        fig.add_axes(0.1, 0.1, 0.8, 0.8)
            .legend_with_title(vec![(Rgba::RED, "alpha".to_owned())], "Series");
        assert_eq!(fig.axes()[0].legend_title.as_deref(), Some("Series"));
        assert!(!fig.encode_png().unwrap().is_empty());
    }

    #[test]
    fn legend_at_records_each_corner_location() {
        for location in [
            LegendLocation::UpperRight,
            LegendLocation::UpperLeft,
            LegendLocation::LowerRight,
            LegendLocation::LowerLeft,
        ] {
            let mut fig = Figure::new(2.0, 2.0);
            fig.add_axes(0.1, 0.1, 0.8, 0.8)
                .legend_at(vec![(Rgba::RED, "item".to_owned())], location);
            assert_eq!(fig.axes()[0].legend_location, location);
            assert!(!fig.encode_png().unwrap().is_empty());
        }
    }
}
