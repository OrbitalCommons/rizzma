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
    /// Placement rectangle in figure fractions; `None` uses the legacy
    /// right-hand default strip.
    rect: Option<Bbox>,
    /// Gradient direction: `false` runs bottom→top (ticks right), `true` runs
    /// left→right (ticks below).
    horizontal: bool,
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
            rect: None,
            horizontal: false,
        });
        self
    }

    /// Add a **vertical** colorbar in the figure-fraction rectangle
    /// `(left, bottom, width, height)` — matplotlib's dedicated-axes
    /// `colorbar(cax=…)` form, and the primitive a *shared* colorbar for a
    /// subplot grid is built on: compute one `[vmin, vmax]` across the grid's
    /// data and place one bar in its own column (a [`GridSpec`] cell resolves
    /// to exactly such a rectangle via
    /// [`SubplotSpec::get_position`](crate::figure::SubplotSpec::get_position)).
    ///
    /// The gradient runs bottom→top with ticks and labels on the right.
    ///
    /// [`GridSpec`]: crate::figure::GridSpec
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(5.0, 3.0);
    /// fig.add_axes(0.10, 0.10, 0.36, 0.8);
    /// fig.add_axes(0.50, 0.10, 0.36, 0.8);
    /// // One shared colorbar for both axes, in its own right-hand column.
    /// fig.colorbar_at((0.90, 0.10, 0.03, 0.8), "viridis", 0.0, 1.0);
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn colorbar_at(
        &mut self,
        rect: (f64, f64, f64, f64),
        cmap_name: &str,
        vmin: f64,
        vmax: f64,
    ) -> &mut Self {
        let (left, bottom, width, height) = rect;
        self.colorbars.push(Colorbar {
            cmap_name: cmap_name.to_string(),
            vmin,
            vmax,
            rect: Some(Bbox::from_bounds(left, bottom, width, height)),
            horizontal: false,
        });
        self
    }

    /// Add a **horizontal** colorbar in the figure-fraction rectangle
    /// `(left, bottom, width, height)`: the gradient runs left→right with
    /// ticks and labels below (matplotlib's `orientation="horizontal"`).
    pub fn colorbar_at_horizontal(
        &mut self,
        rect: (f64, f64, f64, f64),
        cmap_name: &str,
        vmin: f64,
        vmax: f64,
    ) -> &mut Self {
        let (left, bottom, width, height) = rect;
        self.colorbars.push(Colorbar {
            cmap_name: cmap_name.to_string(),
            vmin,
            vmax,
            rect: Some(Bbox::from_bounds(left, bottom, width, height)),
            horizontal: true,
        });
        self
    }

    /// Draw every registered colorbar onto `renderer` at figure pixel size
    /// `(w, h)`.
    pub(crate) fn draw_colorbars(&self, renderer: &mut dyn Renderer, w: f64, h: f64) {
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

        let frac = self
            .rect
            .unwrap_or_else(|| Bbox::from_bounds(BAR_LEFT, BAR_BOTTOM, BAR_WIDTH, BAR_HEIGHT));
        let bar = Bbox::from_extents(
            frac.xmin() * fig_w,
            frac.ymin() * fig_h,
            frac.xmax() * fig_w,
            frac.ymax() * fig_h,
        );

        self.draw_gradient(renderer, &bar, cmap.as_ref());
        self.draw_frame(renderer, &bar);
        self.draw_ticks(renderer, &bar, font);
    }

    /// Fill the bar with `N_SLICES` thin rectangles sampled from `cmap`,
    /// running bottom→top (vertical) or left→right (horizontal).
    fn draw_gradient(&self, renderer: &mut dyn Renderer, bar: &Bbox, cmap: &dyn Colormap) {
        let id = Affine2D::identity();
        let along = if self.horizontal {
            bar.width()
        } else {
            bar.height()
        };
        let slice = along / N_SLICES as f64;
        for i in 0..N_SLICES {
            let t = i as f64 / (N_SLICES as f64 - 1.0);
            let a0 = i as f64 * slice;
            // Overlap each slice by a hair to avoid seam gaps from rounding.
            let a1 = a0 + slice + 0.5;
            let rect = if self.horizontal {
                Bbox::from_extents(bar.xmin() + a0, bar.ymin(), bar.xmin() + a1, bar.ymax())
            } else {
                Bbox::from_extents(bar.xmin(), bar.ymin() + a0, bar.xmax(), bar.ymin() + a1)
            };
            renderer.draw_path(
                &GraphicsContext::new(),
                &rect_path(&rect),
                &id,
                Some(cmap.sample(t)),
            );
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
        let s = renderer.decoration_scale();
        let (tick_len, tick_gap, font_size) = (TICK_LEN * s, TICK_GAP * s, TICK_FONT_SIZE * s);
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
            let label = formatter.format(value, Some(i));

            if self.horizontal {
                // Tick mark below the bar, label centered under it.
                let x = bar.xmin() + frac * bar.width();
                let ty0 = bar.ymin();
                let ty1 = bar.ymin() - tick_len;
                let mark = Path::from_polyline(&[[x, ty0], [x, ty1]]);
                renderer.draw_path(&tick_gc, &mark, &id, None);

                let width = font.measure(&label, font_size).width;
                let lx = x - width / 2.0;
                let ly = ty1 - tick_gap - font_size * 0.8;
                let text = font.text_to_path(&label, font_size, [lx, ly]);
                renderer.draw_path(&GraphicsContext::new(), &text, &id, Some(Rgba::BLACK));
            } else {
                let y = bar.ymin() + frac * bar.height();

                // Tick mark on the right edge.
                let tx0 = bar.xmax();
                let tx1 = bar.xmax() + tick_len;
                let mark = Path::from_polyline(&[[tx0, y], [tx1, y]]);
                renderer.draw_path(&tick_gc, &mark, &id, None);

                // Label, vertically centered on the tick.
                let lx = tx1 + tick_gap;
                let ly = y - font_size / 3.0;
                let text = font.text_to_path(&label, font_size, [lx, ly]);
                renderer.draw_path(&GraphicsContext::new(), &text, &id, Some(Rgba::BLACK));
            }
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

    /// Count distinct non-background colors along a pixel column (`vertical`)
    /// or row (`!vertical`) at fraction `at` of the canvas.
    fn gradient_colors(r: &SkiaRenderer, w: u32, h: u32, at: f64, vertical: bool) -> usize {
        let mut colors = std::collections::HashSet::new();
        if vertical {
            let x = (at * w as f64) as u32;
            for y in 0..h {
                let p = pixel(r, x, y);
                if p != [255, 255, 255, 255] {
                    colors.insert([p[0], p[1], p[2]]);
                }
            }
        } else {
            let y = (at * h as f64) as u32;
            for x in 0..w {
                let p = pixel(r, x, y);
                if p != [255, 255, 255, 255] {
                    colors.insert([p[0], p[1], p[2]]);
                }
            }
        }
        colors.len()
    }

    #[test]
    fn colorbar_at_places_the_gradient_in_the_requested_rect() {
        let mut fig = Figure::new(4.0, 4.0).with_dpi(100.0);
        // A bar in the *left* half, well away from the legacy right-side slot.
        fig.colorbar_at((0.20, 0.10, 0.04, 0.80), "viridis", 0.0, 1.0);

        let r = fig.render();
        let (w, h) = fig.size_px();
        let (w, h) = (w as u32, h as u32);

        // Gradient present at the requested column (center of the 0.20..0.24
        // strip)…
        assert!(
            gradient_colors(&r, w, h, 0.22, true) > 5,
            "expected a gradient in the requested rect"
        );
        // …and nothing at the legacy default position.
        assert_eq!(
            gradient_colors(&r, w, h, 0.915, true),
            0,
            "legacy strip position must stay empty"
        );
    }

    #[test]
    fn horizontal_colorbar_runs_left_to_right() {
        let mut fig = Figure::new(4.0, 4.0).with_dpi(100.0);
        fig.colorbar_at_horizontal((0.10, 0.85, 0.80, 0.05), "viridis", 0.0, 1.0);

        let r = fig.render();
        let (w, h) = fig.size_px();
        let (w, h) = (w as u32, h as u32);

        // Note: figure fractions are y-up; row 0.875 in y-up is 0.125 in the
        // top-down pixmap.
        assert!(
            gradient_colors(&r, w, h, 0.125, false) > 5,
            "expected a horizontal gradient across the requested row"
        );
    }
}
