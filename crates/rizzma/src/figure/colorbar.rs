//! Figure colorbar: a thin vertical gradient strip with tick labels.
//!
//! [`Figure::colorbar`] registers a colorbar mapping a named [`Colormap`] over a
//! data range `[vmin, vmax]`; [`Figure::draw`] renders each registered colorbar
//! as a narrow stack of filled rectangles on the right side of the figure, with
//! a few nicely placed tick labels.

use crate::axis::ticker::{AutoLocator, Formatter, Locator, ScalarFormatter};
use crate::core::{Affine2D, Bbox, Path, color::Colormap, color::Rgba, color::colormap};
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

use crate::figure::figure::Figure;

/// Number of gradient slices sampled along the colorbar.
const N_SLICES: usize = 256;
/// Left edge of the colorbar, in figure fractions.
const BAR_LEFT: f64 = 0.90;
/// Width of the colorbar, in figure fractions.
const BAR_WIDTH: f64 = 0.03;
/// Bottom edge of the colorbar, in figure fractions.
const BAR_BOTTOM: f64 = 0.10;
/// Height of the colorbar, in figure fractions.
const BAR_HEIGHT: f64 = 0.80;
/// Font size of colorbar tick labels, in pixels.
const TICK_FONT_SIZE: f64 = 9.0;
/// Length of a tick mark, in pixels.
const TICK_LEN: f64 = 4.0;
/// Gap between a tick mark and its label, in pixels.
const TICK_GAP: f64 = 3.0;

/// A registered colorbar: a colormap mapped over a data range.
pub(crate) struct Colorbar {
    /// The name of the colormap to sample (e.g. `"viridis"`).
    cmap_name: String,
    /// Lower bound of the mapped data range.
    vmin: f64,
    /// Upper bound of the mapped data range.
    vmax: f64,
}

impl Figure {
    /// Add a vertical colorbar on the right of the figure.
    ///
    /// `cmap_name` is a builtin colormap name (see [`colormap`]); `vmin`/`vmax`
    /// label the bottom and top of the gradient. Unknown colormap names render
    /// no gradient (the strip is skipped at draw time).
    pub fn colorbar(&mut self, cmap_name: &str, vmin: f64, vmax: f64) -> &mut Self {
        self.colorbars.push(Colorbar {
            cmap_name: cmap_name.to_string(),
            vmin,
            vmax,
        });
        self
    }

    /// Draw every registered colorbar onto `renderer`.
    pub(crate) fn draw_colorbars(&self, renderer: &mut dyn Renderer) {
        let (w, h) = self.size_px();
        for cb in &self.colorbars {
            cb.draw(renderer, w, h, self.font_source());
        }
    }
}

impl Colorbar {
    /// Draw this colorbar into `renderer` given the figure size in pixels.
    fn draw(&self, renderer: &mut dyn Renderer, fig_w: f64, fig_h: f64, font: &FontSource) {
        let Some(cmap) = colormap(&self.cmap_name) else {
            return;
        };

        let bar = Bbox::from_extents(
            BAR_LEFT * fig_w,
            BAR_BOTTOM * fig_h,
            (BAR_LEFT + BAR_WIDTH) * fig_w,
            (BAR_BOTTOM + BAR_HEIGHT) * fig_h,
        );

        self.draw_gradient(renderer, &bar, cmap.as_ref());
        self.draw_frame(renderer, &bar);
        self.draw_ticks(renderer, &bar, font);
    }

    /// Fill the bar with `N_SLICES` thin rectangles sampled from `cmap`.
    fn draw_gradient(&self, renderer: &mut dyn Renderer, bar: &Bbox, cmap: &dyn Colormap) {
        let id = Affine2D::identity();
        let slice_h = bar.height() / N_SLICES as f64;
        for i in 0..N_SLICES {
            let t = i as f64 / (N_SLICES as f64 - 1.0);
            let y0 = bar.ymin() + i as f64 * slice_h;
            // Overlap each slice by a hair to avoid seam gaps from rounding.
            let y1 = y0 + slice_h + 0.5;
            let rect = rect_path(&Bbox::from_extents(bar.xmin(), y0, bar.xmax(), y1));
            renderer.draw_path(&GraphicsContext::new(), &rect, &id, Some(cmap.sample(t)));
        }
    }

    /// Stroke a thin black frame around the bar.
    fn draw_frame(&self, renderer: &mut dyn Renderer, bar: &Bbox) {
        let frame_gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(0.8);
        renderer.draw_path(&frame_gc, &rect_path(bar), &Affine2D::identity(), None);
    }

    /// Draw tick marks and labels mapping the gradient onto `[vmin, vmax]`.
    fn draw_ticks(&self, renderer: &mut dyn Renderer, bar: &Bbox, font: &FontSource) {
        let locator = AutoLocator::new();
        let ticks = locator.tick_values(self.vmin, self.vmax);
        let mut formatter = ScalarFormatter::new();
        formatter.set_locs(&ticks);

        let id = Affine2D::identity();
        let tick_gc = GraphicsContext::new()
            .with_stroke(Rgba::BLACK)
            .with_line_width(0.8);
        let span = self.vmax - self.vmin;
        if span.abs() < f64::EPSILON {
            return;
        }

        for (i, &value) in ticks.iter().enumerate() {
            if value < self.vmin.min(self.vmax) || value > self.vmin.max(self.vmax) {
                continue;
            }
            let frac = (value - self.vmin) / span;
            let y = bar.ymin() + frac * bar.height();

            // Tick mark on the right edge.
            let tx0 = bar.xmax();
            let tx1 = bar.xmax() + TICK_LEN;
            let mark = Path::from_polyline(&[[tx0, y], [tx1, y]]);
            renderer.draw_path(&tick_gc, &mark, &id, None);

            // Label, vertically centered on the tick.
            let label = formatter.format(value, Some(i));
            let lx = tx1 + TICK_GAP;
            let ly = y - TICK_FONT_SIZE / 3.0;
            let text = font.text_to_path(&label, TICK_FONT_SIZE, [lx, ly]);
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
    use crate::figure::Figure;
    use crate::skia::SkiaRenderer;

    /// Straight RGBA bytes of the pixel at `(x, y)` (top-left origin).
    fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
        let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn colorbar_strip_has_varied_colors() {
        let mut fig = Figure::new(4.0, 4.0).with_dpi(100.0);
        fig.add_axes(0.1, 0.1, 0.7, 0.8);
        fig.colorbar("viridis", 0.0, 1.0);

        let r = fig.render();
        let (w, h) = fig.size_px();
        let (w, h) = (w as u32, h as u32);

        // Sample a column through the middle of the colorbar strip
        // (left = 0.90, width = 0.03 -> center near 0.915 of the width).
        let x = (0.915 * w as f64) as u32;
        let mut colors = std::collections::HashSet::new();
        let mut non_bg = 0;
        for y in 0..h {
            let p = pixel(&r, x, y);
            if p != [255, 255, 255, 255] {
                non_bg += 1;
                colors.insert([p[0], p[1], p[2]]);
            }
        }
        assert!(non_bg > 0, "expected non-background pixels in the strip");
        assert!(
            colors.len() > 5,
            "expected a varied gradient, got {} distinct colors",
            colors.len()
        );
    }
}
