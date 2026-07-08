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

use crate::core::{Bbox, color::Rgba};
use crate::render::Renderer;
use crate::skia::{PngError, SkiaRenderer};
use crate::text::FontSource;

use crate::figure::axes::Axes;
use crate::figure::gridspec::GridSpec;

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
    /// Colorbars registered on this figure, drawn after the axes (see
    /// [`Figure::colorbar`]).
    pub(crate) colorbars: Vec<crate::figure::colorbar::Colorbar>,
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
            colorbars: Vec::new(),
        }
    }

    /// A shared reference to this figure's font source (used by colorbars and
    /// other figure-level decorations).
    pub(crate) fn font_source(&self) -> &FontSource {
        &self.font
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

    /// Add a **twin** of axes `source` sharing its x mapping with an
    /// independent right-hand y axis (matplotlib's `twinx()`), returning the
    /// new axes' index.
    ///
    /// The twin sits at the same position with a transparent background, no
    /// frame, and no x decoration of its own; its x-limits mirror the
    /// source's *effective* limits at draw time (so later `set_xlim` or
    /// autoscale changes on the source track automatically). Plot right-unit
    /// series on the twin and style its y axis as usual.
    ///
    /// Interaction (pan/zoom) drives each axes' own stored limits; on a twin
    /// the shared x always re-resolves from the source.
    ///
    /// ![twinx](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_twinx.png)
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(4.0, 3.0);
    /// let ax = fig.add_axes(0.12, 0.12, 0.76, 0.76);
    /// ax.plot(&[0.0, 1.0, 2.0], &[0.0, 5.0, 3.0]);
    /// ax.set_ylabel("mm");
    /// let twin = fig.twinx(0);
    /// fig.axes_mut()[twin].plot(&[0.0, 1.0, 2.0], &[0.0, 250.0, 150.0]);
    /// fig.axes_mut()[twin].set_ylabel("µrad");
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `source` is out of range.
    pub fn twinx(&mut self, source: usize) -> usize {
        assert!(source < self.axes.len(), "twinx: axes index out of range");
        let mut twin = Axes::new(self.axes[source].position());
        twin.configure_as_twinx(source);
        self.axes.push(twin);
        self.axes.len() - 1
    }

    /// The shared x-limits a twin axes mirrors, or `None` for ordinary axes
    /// (or a dangling/self link).
    pub(crate) fn xlim_override_for(&self, idx: usize) -> Option<(f64, f64)> {
        let src = self.axes.get(idx)?.xlim_link?;
        if src == idx {
            return None;
        }
        Some(self.axes.get(src)?.effective_limits().0)
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
        self.draw_sized(renderer, w, h);
    }

    /// Draw the figure at an explicit pixel size `(w, h)`.
    ///
    /// Everything downstream of the figure (axes rects, tick/text layout,
    /// colorbars) is positioned from the pixel size alone, so rendering at
    /// `size_px() * s` into a renderer whose DPI is `dpi * s` produces the same
    /// figure uniformly scaled — the basis of [`Figure::render_scaled`].
    fn draw_sized(&self, renderer: &mut dyn Renderer, w: f64, h: f64) {
        // Fill the full canvas background.
        let rect =
            crate::core::Path::from_polyline(&[[0.0, 0.0], [w, 0.0], [w, h], [0.0, h], [0.0, 0.0]]);
        renderer.draw_path(
            &crate::render::GraphicsContext::new(),
            &rect,
            &crate::core::Affine2D::identity(),
            Some(self.facecolor),
        );

        for (i, ax) in self.axes.iter().enumerate() {
            ax.draw_with(renderer, w, h, &self.font, self.xlim_override_for(i));
        }

        // Draw figure-level colorbars on top of the axes.
        self.draw_colorbars(renderer, w, h);
    }

    /// Render the figure to a fresh [`SkiaRenderer`] and return it.
    #[must_use]
    pub fn render(&self) -> SkiaRenderer {
        self.render_scaled(1.0)
    }

    /// Render the figure at `scale` × its size and DPI (for HiDPI targets).
    ///
    /// The output is `size_px() * scale` pixels with line widths, fonts, and
    /// markers scaled together — identical to rendering a figure built with
    /// `with_dpi(dpi * scale)`. Pixel-space APIs ([`Figure::pixel_to_data`],
    /// [`Figure::data_to_pixel`], [`Figure::axes_at`]) stay in *logical*
    /// (unscaled) pixels; callers presenting at `scale` divide device pixels by
    /// `scale` first.
    ///
    /// # Panics
    ///
    /// Panics if `scale` is not finite and positive.
    #[must_use]
    pub fn render_scaled(&self, scale: f64) -> SkiaRenderer {
        assert!(
            scale.is_finite() && scale > 0.0,
            "render scale must be finite and positive, got {scale}"
        );
        let (w, h) = self.size_px();
        let (w, h) = (w * scale, h * scale);
        let mut renderer = SkiaRenderer::new(w as u32, h as u32, self.dpi * scale);
        self.draw_sized(&mut renderer, w, h);
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

    /// Render the figure to an SVG document and return it as a string.
    ///
    /// This drives the *same* [`Figure::draw`] path used for PNG output, but
    /// against an [`crate::svg::SvgRenderer`] instead of the raster backend, proving the
    /// figure is backend-agnostic (one scene → PNG via skia, SVG via svg).
    #[must_use]
    pub fn to_svg(&self) -> String {
        let (w, h) = self.size_px();
        let mut renderer = crate::svg::SvgRenderer::new(w, h, self.dpi);
        self.draw(&mut renderer);
        renderer.finish()
    }

    /// Render the figure and write it to `path` as an SVG file.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if writing the file fails.
    pub fn save_svg<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.to_svg())
    }

    /// Render the figure to a PDF document and return the encoded bytes.
    ///
    /// Drives the *same* [`Figure::draw`] path used for PNG and SVG output, but
    /// against an [`crate::pdf::PdfRenderer`], so one scene renders to PNG (skia),
    /// SVG, or PDF unchanged.
    #[must_use]
    pub fn to_pdf(&self) -> Vec<u8> {
        let (w, h) = self.size_px();
        let mut renderer = crate::pdf::PdfRenderer::new(w, h, self.dpi);
        self.draw(&mut renderer);
        renderer.finish()
    }

    /// Render the figure and write it to `path` as a PDF file.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if writing the file fails.
    pub fn save_pdf<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.to_pdf())
    }

    /// Forward-map a data point in axes `axes_index` to a **top-down canvas
    /// pixel** `(px, py)` (the same pixel space [`Figure::render`] produces, with
    /// `py` measured from the top-left corner).
    ///
    /// This runs the exact forward transform used at draw time: the axes'
    /// effective (resolved) `(xlim, ylim)` and its `trans_data` affine map the
    /// data point into **y-up** display pixels, then the backend **Y-flip** is
    /// applied (`py = fig_h_px - display_y`) so the result is a top-down canvas
    /// pixel. "Effective" limits mean explicit [`set_xlim`](crate::figure::Axes::set_xlim)
    /// /[`set_ylim`](crate::figure::Axes::set_ylim) when set, else the autoscaled
    /// data extents expanded by the axes margins.
    ///
    /// Returns `None` if `axes_index` is out of range.
    ///
    /// This is the exact inverse of [`Figure::pixel_to_data`].
    #[must_use]
    pub fn data_to_pixel(&self, axes_index: usize, data_x: f64, data_y: f64) -> Option<(f64, f64)> {
        let ax = self.axes.get(axes_index)?;
        let (fig_w_px, fig_h_px) = self.size_px();
        let (_axes_px, td) = ax.pixel_rect_and_trans_data_with(
            fig_w_px,
            fig_h_px,
            self.xlim_override_for(axes_index),
        );
        let [scaled_x, scaled_y] = ax.data_to_scaled().map_point(data_x, data_y);
        let (px, display_y) = td.transform_point((scaled_x, scaled_y));
        // Y-flip: matplotlib's display space is y-up (origin bottom-left), but
        // the canvas pixmap is top-down, so a display height of `display_y`
        // corresponds to a top-down row `fig_h_px - display_y`.
        Some((px, fig_h_px - display_y))
    }

    /// Inverse-map a **top-down canvas pixel** `(px, py)` (as produced by
    /// [`Figure::render`], `py` from the top-left corner) to data coordinates in
    /// axes `axes_index`.
    ///
    /// This inverts the exact forward transform of [`Figure::data_to_pixel`]:
    /// the backend **Y-flip** is undone (`display_y = fig_h_px - py`) to recover
    /// the y-up display point, which is then pushed through the inverse of the
    /// axes' `trans_data` affine. The limits used are the effective/resolved ones
    /// (explicit [`set_xlim`](crate::figure::Axes::set_xlim)/[`set_ylim`](crate::figure::Axes::set_ylim)
    /// when set, else autoscaled-with-margins) — identical to draw time.
    ///
    /// Returns `None` if:
    /// - `axes_index` is out of range, or
    /// - the pixel lies **outside** that axes' pixel rectangle (so a hover
    ///   readout can tell the cursor isn't over the axes), or
    /// - the data transform is singular and cannot be inverted.
    ///
    /// This is the exact inverse of [`Figure::data_to_pixel`].
    #[must_use]
    pub fn pixel_to_data(&self, axes_index: usize, px: f64, py: f64) -> Option<(f64, f64)> {
        let ax = self.axes.get(axes_index)?;
        let (fig_w_px, fig_h_px) = self.size_px();
        let (axes_px, td) = ax.pixel_rect_and_trans_data_with(
            fig_w_px,
            fig_h_px,
            self.xlim_override_for(axes_index),
        );
        // Undo the backend Y-flip to recover the y-up display point that
        // `trans_data` operates in.
        let display_y = fig_h_px - py;
        // Reject pixels outside the axes rectangle. `axes_px` is in y-up display
        // pixels, so compare against the un-flipped `display_y`.
        if !axes_px.contains_point(px, display_y) {
            return None;
        }
        let inv = td.inverted()?;
        let (scaled_x, scaled_y) = inv.transform_point((px, display_y));
        let [data_x, data_y] = ax.data_to_scaled().inverse_point(scaled_x, scaled_y);
        Some((data_x, data_y))
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

    #[test]
    fn to_svg_contains_svg_and_path() {
        let mut fig = Figure::new(2.0, 2.0).with_dpi(100.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);

        let svg = fig.to_svg();
        assert!(svg.contains("<svg"), "missing <svg root: {svg}");
        assert!(svg.contains("</svg>"), "missing </svg> close");
        assert!(svg.contains("<path"), "missing at least one <path");
    }

    #[test]
    fn to_pdf_emits_valid_document() {
        let mut fig = Figure::new(2.0, 2.0).with_dpi(100.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);

        let pdf = fig.to_pdf();
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
        assert!(
            pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"),
            "missing %%EOF"
        );
        // The same scene that yields SVG <path>s must produce a non-empty PDF.
        assert!(
            pdf.len() > 200,
            "PDF unexpectedly small: {} bytes",
            pdf.len()
        );
    }

    #[test]
    fn data_pixel_round_trip_honors_log_scale() {
        let mut fig = Figure::new(2.0, 2.0).with_dpi(100.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.set_xscale_log(10.0)
            .set_xlim(1.0, 1000.0)
            .set_ylim(0.0, 10.0);
        let (px, py) = fig
            .data_to_pixel(0, 10.0, 4.0)
            .expect("axes index is valid");
        let (x, y) = fig.pixel_to_data(0, px, py).expect("pixel is inside axes");

        assert!((x - 10.0).abs() < 1e-9, "expected x=10, got {x}");
        assert!((y - 4.0).abs() < 1e-9, "expected y=4, got {y}");
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }

    /// A figure with one axes at a known fractional position and explicit
    /// limits, so its pixel rect and data extents are fully determined.
    fn fixture() -> Figure {
        // 4x2 inch at 100 dpi -> 400x200 px canvas.
        let mut fig = Figure::new(4.0, 2.0).with_dpi(100.0);
        let ax = fig.add_axes(0.25, 0.5, 0.5, 0.25);
        // axes_px (y-up): x in [100, 300], y in [100, 150].
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(-5.0, 5.0);
        fig
    }

    #[test]
    fn data_to_pixel_maps_lower_left_to_bottom_left_pixel() {
        let fig = fixture();
        // Lower-left data corner (xmin, ymin) -> axes-rect lower-left in y-up
        // display, which is the *larger* py after the Y-flip.
        // axes_px y-up lower-left = (100, 100); canvas py = 200 - 100 = 100.
        let (px, py) = fig.data_to_pixel(0, 0.0, -5.0).expect("in range");
        approx(px, 100.0);
        approx(py, 100.0);

        // Data center (5, 0) -> axes-rect pixel center.
        // y-up center = (200, 125); canvas py = 200 - 125 = 75.
        let (cx, cy) = fig.data_to_pixel(0, 5.0, 0.0).expect("in range");
        approx(cx, 200.0);
        approx(cy, 75.0);

        // Upper-right data corner (xmax, ymax) -> axes-rect upper-right in
        // y-up display = (300, 150); canvas py = 200 - 150 = 50.
        let (ux, uy) = fig.data_to_pixel(0, 10.0, 5.0).expect("in range");
        approx(ux, 300.0);
        approx(uy, 50.0);
    }

    #[test]
    fn pixel_to_data_round_trips() {
        let fig = fixture();
        for &(x, y) in &[
            (0.0, -5.0),
            (10.0, 5.0),
            (5.0, 0.0),
            (2.5, -1.25),
            (7.3, 3.1),
        ] {
            let (px, py) = fig.data_to_pixel(0, x, y).expect("in range");
            let (rx, ry) = fig.pixel_to_data(0, px, py).expect("inside axes");
            approx(rx, x);
            approx(ry, y);
        }
    }

    #[test]
    fn pixel_to_data_returns_none_outside_and_out_of_range() {
        let fig = fixture();
        // Well outside the axes pixel rect (canvas is 400x200; this is past it).
        assert!(fig.pixel_to_data(0, 5.0, 5.0).is_none());
        assert!(fig.pixel_to_data(0, 399.0, 199.0).is_none());
        // Out-of-range axes index.
        assert!(fig.pixel_to_data(1, 200.0, 100.0).is_none());
        assert!(fig.data_to_pixel(1, 0.0, 0.0).is_none());
    }

    #[test]
    fn imshow_figure_svg_embeds_image_data() {
        let mut fig = Figure::new(2.0, 2.0).with_dpi(50.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.imshow(&[0.0, 1.0, 2.0, 3.0], 2, 2);

        let svg = fig.to_svg();
        assert!(
            svg.contains("<image "),
            "imshow should emit SVG image: {svg}"
        );
        assert!(
            svg.contains("href=\"data:image/png;base64,"),
            "imshow should embed PNG data URI: {svg}"
        );
    }
}
