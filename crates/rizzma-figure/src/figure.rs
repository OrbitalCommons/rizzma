//! The top-level [`Figure`]: a canvas of [`Axes`] rendered to pixels.
//!
//! [`Figure`] mirrors matplotlib's `Figure`: it owns its size (in inches), a
//! DPI, a background color, a font source, and a list of [`Axes`]. It resolves
//! figure-fraction axes positions to pixels, fills its background, and draws
//! each axes. Convenience wrappers render directly to a [`SkiaRenderer`] and to
//! PNG bytes or files.
//!
//! # Coordinate convention
//!
//! Figure fractions `(fx, fy)` map to the **y-UP** pixel `(fx * W, fy * H)`
//! with `W = width_in * dpi`, `H = height_in * dpi`; the raster backend applies
//! its own Y-flip.

use rizzma_core::{Bbox, color::Rgba};
use rizzma_render::Renderer;
use rizzma_skia::{PngError, SkiaRenderer};
use rizzma_text::FontSource;

use crate::axes::Axes;
use crate::gridspec::GridSpec;

/// The default dots-per-inch for a new [`Figure`].
const DEFAULT_DPI: f64 = 100.0;

/// A figure: a sized canvas holding one or more [`Axes`].
///
/// Construct with [`Figure::new`], add axes with [`Figure::add_axes`] or
/// [`Figure::add_subplot`], then draw to a renderer with [`Figure::draw`] or
/// render straight to pixels/PNG with [`Figure::render`], [`Figure::save_png`],
/// and [`Figure::encode_png`].
pub struct Figure {
    /// Width in inches.
    width_in: f64,
    /// Height in inches.
    height_in: f64,
    /// Dots per inch (pixels per inch).
    dpi: f64,
    /// Background fill color of the whole canvas.
    facecolor: Rgba,
    /// Font source used for all text in the figure.
    font: FontSource,
    /// The axes owned by this figure, drawn in insertion order.
    axes: Vec<Axes>,
}

impl Figure {
    /// Create a `width_in` by `height_in` inch figure at the default DPI
    /// (`100`), a white background, and the embedded DejaVu Sans font.
    #[must_use]
    pub fn new(width_in: f64, height_in: f64) -> Self {
        Self {
            width_in,
            height_in,
            dpi: DEFAULT_DPI,
            facecolor: Rgba::WHITE,
            font: FontSource::dejavu_sans(),
            axes: Vec::new(),
        }
    }

    /// Set the DPI, returning `self` for chaining.
    #[must_use]
    pub fn with_dpi(mut self, dpi: f64) -> Self {
        self.dpi = dpi;
        self
    }

    /// Set the canvas background color, returning `self` for chaining.
    #[must_use]
    pub fn with_facecolor(mut self, facecolor: Rgba) -> Self {
        self.facecolor = facecolor;
        self
    }

    /// The figure size in pixels as `(width, height)` (`size_in * dpi`).
    #[must_use]
    pub fn size_px(&self) -> (f64, f64) {
        (self.width_in * self.dpi, self.height_in * self.dpi)
    }

    /// The DPI this figure renders at.
    #[must_use]
    pub fn dpi(&self) -> f64 {
        self.dpi
    }

    /// Add axes at the figure-fraction rectangle `(left, bottom, width,
    /// height)`, returning a mutable reference to the new [`Axes`].
    pub fn add_axes(&mut self, l: f64, b: f64, w: f64, h: f64) -> &mut Axes {
        self.axes.push(Axes::new(Bbox::from_bounds(l, b, w, h)));
        self.axes.last_mut().expect("just pushed axes")
    }

    /// Add axes for cell `index` of an `nrows` by `ncols` grid, returning a
    /// mutable reference to the new [`Axes`].
    ///
    /// `index` is **1-based** and runs row-major (left to right, top to bottom),
    /// matching matplotlib's `Figure.add_subplot`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is zero or exceeds `nrows * ncols`.
    pub fn add_subplot(&mut self, nrows: usize, ncols: usize, index: usize) -> &mut Axes {
        assert!(index >= 1, "subplot index is 1-based");
        assert!(index <= nrows * ncols, "subplot index out of range");
        let row = (index - 1) / ncols;
        let col = (index - 1) % ncols;
        let gs = GridSpec::new(nrows, ncols);
        let position = gs.subplot(row, col).get_position(&gs);
        self.axes.push(Axes::new(position));
        self.axes.last_mut().expect("just pushed axes")
    }

    /// A shared reference to this figure's axes.
    #[must_use]
    pub fn axes(&self) -> &[Axes] {
        &self.axes
    }

    /// A mutable slice of this figure's axes, for restyling or adding artists to
    /// an existing axes after creation.
    pub fn axes_mut(&mut self) -> &mut [Axes] {
        &mut self.axes
    }

    /// Draw the whole figure into `renderer`: fill the canvas with the
    /// background color, then draw each axes.
    pub fn draw(&self, renderer: &mut dyn Renderer) {
        let (w, h) = self.size_px();
        // Fill the full canvas background.
        let rect =
            rizzma_core::Path::from_polyline(&[[0.0, 0.0], [w, 0.0], [w, h], [0.0, h], [0.0, 0.0]]);
        renderer.draw_path(
            &rizzma_render::GraphicsContext::new(),
            &rect,
            &rizzma_core::Affine2D::identity(),
            Some(self.facecolor),
        );

        for ax in &self.axes {
            ax.draw(renderer, w, h, &self.font);
        }
    }

    /// Render the figure to a fresh [`SkiaRenderer`] and return it.
    #[must_use]
    pub fn render(&self) -> SkiaRenderer {
        let (w, h) = self.size_px();
        let mut renderer = SkiaRenderer::new(w as u32, h as u32, self.dpi);
        self.draw(&mut renderer);
        renderer
    }

    /// Render the figure and save it to `path` as a PNG.
    ///
    /// # Errors
    ///
    /// Returns a [`PngError`] if encoding or writing the file fails.
    pub fn save_png<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), PngError> {
        self.render().save_png(path)
    }

    /// Render the figure and return the encoded PNG bytes.
    ///
    /// # Errors
    ///
    /// Returns a [`PngError`] if encoding fails.
    pub fn encode_png(&self) -> Result<Vec<u8>, PngError> {
        self.render().encode_png()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the straight RGBA bytes of the pixel at `(x, y)` (top-left origin).
    fn pixel(r: &SkiaRenderer, x: u32, y: u32) -> [u8; 4] {
        let p = r.pixmap().pixel(x, y).expect("pixel in bounds");
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn size_px_scales_by_dpi() {
        let fig = Figure::new(6.0, 4.0).with_dpi(100.0);
        assert_eq!(fig.size_px(), (600.0, 400.0));
    }

    #[test]
    fn add_subplot_positions_match_gridspec() {
        let mut fig = Figure::new(4.0, 4.0);
        let ax = fig.add_subplot(2, 2, 1);
        // Cell (row 0, col 0): top-left cell of a 2x2 grid.
        let gs = GridSpec::new(2, 2);
        let expected = gs.subplot(0, 0).get_position(&gs);
        assert_eq!(ax.position(), expected);
    }

    #[test]
    fn one_line_figure_renders_ink_and_png() {
        let mut fig = Figure::new(2.0, 2.0).with_dpi(100.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
        let r = fig.render();
        let (w, h) = fig.size_px();
        let (w, h) = (w as u32, h as u32);

        // (a) A canvas corner equals the white facecolor.
        assert_eq!(pixel(&r, 0, 0), [255, 255, 255, 255]);

        // (b) There is non-background ink somewhere inside the axes region.
        let mut found_ink = false;
        'scan: for y in (h / 4)..(3 * h / 4) {
            for x in (w / 4)..(3 * w / 4) {
                if pixel(&r, x, y) != [255, 255, 255, 255] {
                    found_ink = true;
                    break 'scan;
                }
            }
        }
        assert!(found_ink, "expected non-background ink within the axes");

        // (c) PNG encodes to non-empty bytes.
        assert!(!fig.encode_png().expect("encode succeeds").is_empty());
    }
}
