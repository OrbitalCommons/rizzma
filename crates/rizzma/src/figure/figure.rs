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

/// Tight-layout pad between the figure/cell edge and the outermost
/// decoration, in pixels at the default 100 DPI. Matches matplotlib's
/// `tight_layout(pad=1.08)` at its default 10 pt font:
/// `1.08 x 10 pt / 72 = 0.15 in` = 15 px at 100 DPI.
const LAYOUT_PAD: f64 = 15.0;

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
        let mut ax = Axes::new(position);
        // Subplot-managed axes get tight layout: the margin-less grid cell is
        // the outer envelope and the frame rect is derived from decoration
        // extents at draw time (matplotlib's tight layout). Explicit
        // `add_axes` rects stay literal.
        let envelope_gs = GridSpec::new(nrows, ncols).with_margins(0.0, 1.0, 0.0, 1.0);
        ax.layout_envelope = Some(envelope_gs.subplot(row, col).get_position(&envelope_gs));
        self.axes.push(ax);
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
        twin.layout_envelope = self.axes[source].layout_envelope;
        twin.configure_as_twinx(source);
        self.axes.push(twin);
        self.axes.len() - 1
    }

    /// Link `follower`'s x-limits to `leader`'s (matplotlib's `sharex`).
    ///
    /// The follower keeps its own y axis and decorations but mirrors the
    /// leader's *effective* x-limits at draw time, so `set_xlim`, autoscale
    /// changes, and interactive pan/zoom on the leader move both. Interaction
    /// on the follower writes its x changes through to the leader (see
    /// [`Interactor`](crate::figure::Interactor)), so zooming either axes
    /// keeps the group's x in lockstep while each y stays independent.
    ///
    /// # Panics
    ///
    /// Panics if either index is out of range, the two are equal, or `leader`
    /// itself already follows another axes (chains are not resolved — link
    /// every follower directly to one leader).
    pub fn sharex(&mut self, follower: usize, leader: usize) {
        assert!(follower < self.axes.len(), "sharex: follower out of range");
        assert!(leader < self.axes.len(), "sharex: leader out of range");
        assert!(follower != leader, "sharex: an axes cannot follow itself");
        assert!(
            self.axes[leader].xlim_link.is_none(),
            "sharex: the leader must not itself follow another axes"
        );
        self.axes[follower].xlim_link = Some(leader);
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

    /// The tight-layout frame rectangle (pixels) for axes `idx` at figure
    /// pixel size `(fig_w, fig_h)`, or `None` for literally-placed axes.
    ///
    /// All auto-layout axes sharing one envelope (a twin pair) are laid out
    /// together: their per-side decoration insets are unioned so every frame
    /// in the group coincides.
    pub(crate) fn layout_rect_for(&self, idx: usize, fig_w: f64, fig_h: f64) -> Option<Bbox> {
        let envelope = self.axes.get(idx)?.layout_envelope?;
        // Decoration scale: DPI-relative, times any render_scaled factor
        // (fig_w grows with the scale while size_px() stays logical).
        let (logical_w, _) = self.size_px();
        let s = self.dpi / 100.0 * (fig_w / logical_w);
        let pad = LAYOUT_PAD * s;

        let (mut left, mut right, mut bottom, mut top) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (i, ax) in self.axes.iter().enumerate() {
            if ax.layout_envelope != Some(envelope) {
                continue;
            }
            let (l, r, b, t_) = ax.layout_insets(&self.font, s, self.xlim_override_for(i));
            left = left.max(l);
            right = right.max(r);
            bottom = bottom.max(b);
            top = top.max(t_);
        }

        let rect = Bbox::from_extents(
            envelope.xmin() * fig_w + left + pad,
            envelope.ymin() * fig_h + bottom + pad,
            envelope.xmax() * fig_w - right - pad,
            envelope.ymax() * fig_h - top - pad,
        );
        // Degenerate (decorations larger than the cell): fall back to the
        // literal position rather than an inverted rect.
        (rect.width() > 1.0 && rect.height() > 1.0).then_some(rect)
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
            ax.draw_with(
                renderer,
                w,
                h,
                &self.font,
                self.xlim_override_for(i),
                self.layout_rect_for(i, w, h),
            );
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
        let (_axes_px, td) = ax.pixel_rect_and_trans_data_in(
            fig_w_px,
            fig_h_px,
            self.xlim_override_for(axes_index),
            self.layout_rect_for(axes_index, fig_w_px, fig_h_px),
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
        let (axes_px, td) = ax.pixel_rect_and_trans_data_in(
            fig_w_px,
            fig_h_px,
            self.xlim_override_for(axes_index),
            self.layout_rect_for(axes_index, fig_w_px, fig_h_px),
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
    fn tight_layout_frame_hugs_undecorated_sides() {
        // A subplot with no title and no labels: the right and top frame
        // edges sit one pad plus the end-tick-label overhang inside the
        // figure (the last x tick label spills half its width past the frame
        // corner, the top y tick label half its height); left/bottom leave
        // room for the tick-label bands.
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w, h) = fig.size_px();
        let rect = fig.layout_rect_for(0, w, h).expect("subplot is auto-laid");
        assert!(rect.xmax() <= w - LAYOUT_PAD, "right edge sits inside pad");
        assert!(
            rect.xmax() > w - LAYOUT_PAD - 30.0,
            "right inset is only the pad + a half tick label"
        );
        assert!(rect.ymax() <= h - LAYOUT_PAD, "top edge sits inside pad");
        assert!(
            rect.ymax() > h - LAYOUT_PAD - 15.0,
            "top inset is only the pad + a half tick label"
        );
        assert!(rect.xmin() > LAYOUT_PAD, "left leaves tick-label room");
        assert!(rect.ymin() > LAYOUT_PAD, "bottom leaves tick-label room");
        // The band sides claim more than the overhang sides.
        assert!(rect.xmin() > w - rect.xmax());
        assert!(rect.ymin() > h - rect.ymax());
    }

    #[test]
    fn tight_layout_reserves_end_tick_label_overhang() {
        // The frame must not run all the way to pad distance from the figure
        // edge: the last x tick label (e.g. "1.0") is centered on the frame's
        // right corner and needs half its width beyond it.
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w, h) = fig.size_px();
        let rect = fig.layout_rect_for(0, w, h).unwrap();
        assert!(
            rect.xmax() < w - LAYOUT_PAD - 4.0,
            "right margin exceeds the bare pad by the label overhang, got {}",
            w - rect.xmax()
        );
    }

    #[test]
    fn tight_layout_moves_only_the_decorated_side() {
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w, h) = fig.size_px();
        let before = fig.layout_rect_for(0, w, h).unwrap();

        fig.axes_mut()[0].set_title("a title");
        let with_title = fig.layout_rect_for(0, w, h).unwrap();
        assert!(
            with_title.ymax() < before.ymax(),
            "title lowers the top edge"
        );
        assert!((with_title.xmin() - before.xmin()).abs() < 1e-9);
        assert!((with_title.xmax() - before.xmax()).abs() < 1e-9);

        fig.axes_mut()[0].set_ylabel("volts");
        let with_ylabel = fig.layout_rect_for(0, w, h).unwrap();
        assert!(
            with_ylabel.xmin() > with_title.xmin(),
            "a y label pushes the left edge in"
        );
        assert!((with_ylabel.xmax() - with_title.xmax()).abs() < 1e-9);
    }

    #[test]
    fn add_axes_rects_stay_literal() {
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_axes(0.2, 0.3, 0.5, 0.4)
            .plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w, h) = fig.size_px();
        assert!(
            fig.layout_rect_for(0, w, h).is_none(),
            "explicit rects are never re-laid"
        );
    }

    #[test]
    fn twin_frames_coincide_under_tight_layout() {
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 5.0]);
        fig.axes_mut()[0].set_ylabel("left units");
        let twin = fig.twinx(0);
        fig.axes_mut()[twin].plot(&[0.0, 1.0], &[0.0, 500.0]);
        fig.axes_mut()[twin].set_ylabel("right units");

        let (w, h) = fig.size_px();
        let a = fig.layout_rect_for(0, w, h).unwrap();
        let b = fig.layout_rect_for(twin, w, h).unwrap();
        assert!((a.xmin() - b.xmin()).abs() < 1e-9);
        assert!((a.xmax() - b.xmax()).abs() < 1e-9);
        assert!((a.ymin() - b.ymin()).abs() < 1e-9);
        assert!((a.ymax() - b.ymax()).abs() < 1e-9);
        // The twin's right-side labels claim room: right inset exceeds the
        // pad by more than any end-tick-label overhang could.
        assert!(
            a.xmax() < w - LAYOUT_PAD - 20.0,
            "right labels push the frame in"
        );
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
