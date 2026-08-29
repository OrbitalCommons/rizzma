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

use crate::core::rcparams::RcParams;
use crate::core::{Bbox, color::Rgba};
#[cfg(feature = "portable")]
use crate::portable::PortableError;
use crate::render::Renderer;
use crate::skia::{PngError, SkiaRenderer};
use crate::text::FontSource;

use crate::figure::axes::Axes;
use crate::figure::gridspec::GridSpec;
use crate::figure::richtext::layout_rich_text;

/// The default dots-per-inch for a new [`Figure`].
const DEFAULT_DPI: f64 = 100.0;

/// Tight-layout pad between the figure/cell edge and the outermost
/// decoration, in pixels at the default 100 DPI. Matches matplotlib's
/// `tight_layout(pad=1.08)` at its default 10 pt font:
/// `1.08 x 10 pt / 72 = 0.15 in` = 15 px at 100 DPI.
const LAYOUT_PAD: f64 = 15.0;
const SUPTITLE_SIZE: f64 = 14.0;
const SUPTITLE_PAD: f64 = 8.0;

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
    /// Resolved style defaults seeded into each axes as it is created.
    rc: RcParams,
    /// Font source used for all text in the figure.
    font: FontSource,
    /// The axes owned by this figure, drawn in insertion order.
    axes: Vec<Axes>,
    /// Optional title centered above the complete subplot grid.
    suptitle: Option<String>,
    /// Figure-title ink color, seeded from the active theme.
    suptitle_color: Rgba,
    /// Colorbars registered on this figure, drawn after the axes (see
    /// [`Figure::colorbar`]).
    pub(crate) colorbars: Vec<crate::figure::colorbar::Colorbar>,
    /// Optional animation, evaluated by [`Figure::seek`] and carried by
    /// portable export.
    #[cfg(feature = "portable")]
    pub(crate) timeline: Option<crate::portable::Timeline>,
    /// User-driven parameters (schema 4), evaluated by [`Figure::set_control`]
    /// and carried by portable export.
    #[cfg(feature = "portable")]
    pub(crate) controls: Vec<crate::portable::Control>,
    /// Live position of each control, parallel to `controls`.
    #[cfg(feature = "portable")]
    pub(crate) control_values: Vec<f64>,
    /// The clock position last sought to, kept so a control change can
    /// re-evaluate its grids at the current instant.
    #[cfg(feature = "portable")]
    pub(crate) time: f64,
}

impl Figure {
    /// Create a `width_in` by `height_in` inch figure at the default DPI
    /// (`100`), a white background, and the embedded DejaVu Sans font.
    #[must_use]
    pub fn new(width_in: f64, height_in: f64) -> Self {
        let rc = RcParams::default();
        Self {
            width_in,
            height_in,
            dpi: DEFAULT_DPI,
            facecolor: rc.figure_facecolor,
            rc,
            font: FontSource::dejavu_sans(),
            axes: Vec::new(),
            suptitle: None,
            suptitle_color: Rgba::BLACK,
            colorbars: Vec::new(),
            #[cfg(feature = "portable")]
            timeline: None,
            #[cfg(feature = "portable")]
            controls: Vec::new(),
            #[cfg(feature = "portable")]
            control_values: Vec::new(),
            #[cfg(feature = "portable")]
            time: 0.0,
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

    /// Set the canvas background color in place (the post-construction
    /// counterpart of [`with_facecolor`](Figure::with_facecolor)).
    pub fn set_facecolor(&mut self, facecolor: Rgba) -> &mut Self {
        self.facecolor = facecolor;
        self
    }

    /// Adopt `rc` as this figure's style defaults, returning `self` for
    /// chaining. Applies the canvas background immediately and seeds every
    /// axes created afterward (and any that already exist). Build a theme with
    /// [`RcParams::dark`] or any custom [`RcParams`], e.g.
    /// `Figure::new(6.0, 4.0).with_rcparams(RcParams::dark())`.
    #[must_use]
    pub fn with_rcparams(mut self, rc: RcParams) -> Self {
        self.set_rcparams(rc);
        self
    }

    /// The post-construction counterpart of
    /// [`with_rcparams`](Figure::with_rcparams): adopt `rc`, recolor the
    /// canvas, and re-seed every existing axes' style.
    pub fn set_rcparams(&mut self, rc: RcParams) -> &mut Self {
        self.facecolor = rc.figure_facecolor;
        self.suptitle_color = rc.text_color;
        for ax in &mut self.axes {
            ax.apply_rcparams(&rc);
        }
        self.rc = rc;
        self
    }

    /// This figure's current style defaults.
    #[must_use]
    pub fn rcparams(&self) -> &RcParams {
        &self.rc
    }

    /// Set a title centered above the entire subplot grid.
    ///
    /// Tight-layout subplots reserve a top band for the title automatically.
    /// Math spans use the same `$...$` syntax as axes titles.
    ///
    /// ![figure supertitle](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_suptitle.png)
    ///
    /// ```no_run
    /// use rizzma::Figure;
    /// let mut fig = Figure::new(6.0, 3.0);
    /// fig.add_subplot(1, 2, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// fig.add_subplot(1, 2, 2).plot(&[0.0, 1.0], &[1.0, 0.0]);
    /// fig.suptitle("Two views");
    /// fig.save_png("suptitle.png")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn suptitle(&mut self, title: impl Into<String>) -> &mut Self {
        self.suptitle = Some(title.into());
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
        let mut ax = Axes::new(Bbox::from_bounds(l, b, w, h));
        ax.apply_rcparams(&self.rc);
        self.axes.push(ax);
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
        ax.apply_rcparams(&self.rc);
        // Subplot-managed axes get tight layout: the margin-less grid cell is
        // the outer envelope and the frame rect is derived from decoration
        // extents at draw time (matplotlib's tight layout). Explicit
        // `add_axes` rects stay literal.
        //
        // The envelope grid drops `wspace`/`hspace` as well as the margins:
        // matplotlib's default gaps exist to hold exactly the decorations this
        // layout measures per-axes, so keeping them would reserve that space
        // twice and leave an empty band between rows and columns.
        let envelope_gs = GridSpec::new(nrows, ncols)
            .with_margins(0.0, 1.0, 0.0, 1.0)
            .with_spacing(0.0, 0.0);
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
        twin.apply_rcparams(&self.rc);
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

        // matplotlib's `label_outer`: when the pair is stacked in one grid
        // column, the upper axes' x tick labels are redundant — hide them
        // (tick marks stay) so the reclaimed band goes to the frames.
        let (f_env, l_env) = (
            self.axes[follower].layout_envelope,
            self.axes[leader].layout_envelope,
        );
        if let (Some(f), Some(l)) = (f_env, l_env)
            && f.xmin() == l.xmin()
            && f.xmax() == l.xmax()
            && f.ymin() != l.ymin()
        {
            let upper = if f.ymin() > l.ymin() {
                follower
            } else {
                leader
            };
            self.axes[upper].set_x_tick_labels_visible(false);
        }
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
    /// in the group coincides. Left/right insets additionally union across
    /// every axes in the same grid *column* (envelopes with equal x-extents),
    /// so vertically stacked subplots get frames of identical width even when
    /// their y tick labels differ (matplotlib aligns columns the same way).
    pub(crate) fn layout_rect_for(&self, idx: usize, fig_w: f64, fig_h: f64) -> Option<Bbox> {
        let envelope = self.axes.get(idx)?.layout_envelope?;
        // Decoration scale: DPI-relative, times any render_scaled factor
        // (fig_w grows with the scale while size_px() stays logical).
        let (logical_w, _) = self.size_px();
        let s = self.dpi / 100.0 * (fig_w / logical_w);
        let pad = LAYOUT_PAD * s;

        let (mut left, mut right, mut bottom, mut top) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (i, ax) in self.axes.iter().enumerate() {
            let Some(env) = ax.layout_envelope else {
                continue;
            };
            let same_cell = env == envelope;
            let same_column = env.xmin() == envelope.xmin() && env.xmax() == envelope.xmax();
            if !same_cell && !same_column {
                continue;
            }
            let (l, r, b, t_) = ax.layout_insets(&self.font, s, self.xlim_override_for(i));
            if same_column {
                left = left.max(l);
                right = right.max(r);
            }
            if same_cell {
                bottom = bottom.max(b);
                top = top.max(t_);
            }
        }

        // Interior cell edges (between adjacent subplots) carry half the pad
        // each, so neighbors sit one full pad apart — matplotlib's tight
        // layout spends a single h_pad/w_pad between subplots, not two.
        const EDGE_EPS: f64 = 1e-9;
        let pad_left = if envelope.xmin() > EDGE_EPS {
            pad / 2.0
        } else {
            pad
        };
        let pad_right = if envelope.xmax() < 1.0 - EDGE_EPS {
            pad / 2.0
        } else {
            pad
        };
        let pad_bottom = if envelope.ymin() > EDGE_EPS {
            pad / 2.0
        } else {
            pad
        };
        let mut pad_top = if envelope.ymax() < 1.0 - EDGE_EPS {
            pad / 2.0
        } else {
            pad
        };
        if envelope.ymax() >= 1.0 - EDGE_EPS
            && let Some(title) = &self.suptitle
            && !title.is_empty()
        {
            let rich = layout_rich_text(&self.font, title, SUPTITLE_SIZE * s);
            pad_top += rich.ascent + rich.descent + SUPTITLE_PAD * 2.0 * s;
        }

        let rect = Bbox::from_extents(
            envelope.xmin() * fig_w + left + pad_left,
            envelope.ymin() * fig_h + bottom + pad_bottom,
            envelope.xmax() * fig_w - right - pad_right,
            envelope.ymax() * fig_h - top - pad_top,
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

        if let Some(title) = &self.suptitle
            && !title.is_empty()
        {
            let s = renderer.decoration_scale();
            let rich = layout_rich_text(&self.font, title, SUPTITLE_SIZE * s);
            let shift = crate::core::Affine2D::from_translation(
                (w - rich.width) / 2.0,
                h - SUPTITLE_PAD * s - rich.ascent,
            );
            for path in &rich.paths {
                renderer.draw_path(
                    &crate::render::GraphicsContext::new(),
                    &path.transformed(&shift),
                    &crate::core::Affine2D::identity(),
                    Some(self.suptitle_color),
                );
            }
        }
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

    /// Attach an animation, replacing any existing one.
    ///
    /// The figure is not moved by attaching a timeline — call
    /// [`Figure::seek`] to evaluate it at a time. Exporting the figure carries
    /// the animation with it (schema 3).
    #[cfg(feature = "portable")]
    pub fn set_timeline(&mut self, timeline: crate::portable::Timeline) -> &mut Self {
        self.timeline = Some(timeline);
        self
    }

    /// This figure's animation, if it has one.
    #[cfg(feature = "portable")]
    #[must_use]
    pub fn timeline(&self) -> Option<&crate::portable::Timeline> {
        self.timeline.as_ref()
    }

    /// Move the figure to time `t` (seconds), applying every track.
    ///
    /// `t` is wrapped or clamped into the timeline first, so a caller can hand
    /// over a monotonically increasing clock without doing modular arithmetic.
    /// This is a pure function of `t`: the same time always produces the same
    /// figure, which is what makes seeking exact and replay honest.
    ///
    /// Does nothing when the figure has neither a timeline nor controls.
    ///
    /// # Errors
    ///
    /// [`PortableError::Malformed`](crate::portable::PortableError::Malformed)
    /// if a track addresses an artist that does not exist or hands it the wrong
    /// number of values — a mismatch that would otherwise show up as a silently
    /// wrong plot.
    #[cfg(feature = "portable")]
    pub fn seek(&mut self, t: f64) -> Result<(), PortableError> {
        self.time = t;
        self.apply_state()
    }

    /// Re-evaluate the displayed state as a pure function of the clock and
    /// every control's position: the timeline's tracks at the current time,
    /// then each control in declaration order — its tracks at its position,
    /// its grids at `(time, position)`. Re-applying the whole function on
    /// every change is what makes target overlap well-defined: the later step
    /// wins *always*, not whichever event fired last (design/12 §3).
    #[cfg(feature = "portable")]
    fn apply_state(&mut self) -> Result<(), PortableError> {
        if let Some(timeline) = self.timeline.take() {
            let result: Result<(), PortableError> = (|| {
                let time = timeline.normalize(self.time);
                for (i, track) in timeline.tracks.iter().enumerate() {
                    self.apply_target(&format!("track {i}"), track.target, track.sample(time))?;
                }
                Ok(())
            })();
            self.timeline = Some(timeline);
            result?;
        }
        if self.controls.is_empty() {
            return Ok(());
        }
        let time = self
            .timeline
            .as_ref()
            .map_or(self.time, |tl| tl.normalize(self.time));
        let controls = std::mem::take(&mut self.controls);
        let result: Result<(), PortableError> = (|| {
            for (ci, control) in controls.iter().enumerate() {
                let v = self.control_values[ci];
                for (i, track) in control.tracks.iter().enumerate() {
                    self.apply_target(
                        &format!("control {ci} track {i}"),
                        track.target,
                        track.sample(v),
                    )?;
                }
                for (i, grid) in control.grids.iter().enumerate() {
                    self.apply_target(
                        &format!("control {ci} grid {i}"),
                        grid.target,
                        grid.sample(time, v),
                    )?;
                }
            }
            Ok(())
        })();
        self.controls = controls;
        result
    }

    #[cfg(feature = "portable")]
    fn apply_target(
        &mut self,
        who: &str,
        target: crate::portable::Target,
        values: Vec<f64>,
    ) -> Result<(), PortableError> {
        use crate::portable::Target;

        let axes_index = target.axes();
        let ax = self.axes.get_mut(axes_index).ok_or_else(|| {
            PortableError::Malformed(format!(
                "{who} animates axes {axes_index}, which does not exist"
            ))
        })?;
        let bad = |what: &str| {
            PortableError::Malformed(format!("{who} animates {what}, which does not exist"))
        };
        match target {
            Target::LineX { index, .. } | Target::LineY { index, .. } => {
                let (x, y) = ax
                    .line_data(index)
                    .ok_or_else(|| bad(&format!("line {index}")))?;
                let is_x = matches!(target, Target::LineX { .. });
                let (new_x, new_y) = if is_x { (values, y) } else { (x, values) };
                ax.set_line_data(index, &new_x, &new_y)
                    .map_err(|e| PortableError::Malformed(format!("{who}: {e}")))?;
            }
            Target::Offsets { index, .. } => {
                if !values.len().is_multiple_of(2) {
                    return Err(PortableError::Malformed(format!(
                        "{who} gives {} values for scatter offsets, which come in pairs",
                        values.len()
                    )));
                }
                let (xs, ys): (Vec<f64>, Vec<f64>) =
                    values.chunks_exact(2).map(|p| (p[0], p[1])).collect();
                ax.set_collection_offsets(index, &xs, &ys)
                    .map_err(|e| PortableError::Malformed(format!("{who}: {e}")))?;
            }
            Target::ImageData { index, .. } => {
                let (rows, cols) = ax
                    .image_shape(index)
                    .ok_or_else(|| bad(&format!("image {index}")))?;
                ax.set_image_data(index, &values, rows, cols, None)
                    .map_err(|e| PortableError::Malformed(format!("{who}: {e}")))?;
            }
            Target::Xlim { .. } | Target::Ylim { .. } => {
                if values.len() != 2 {
                    return Err(PortableError::Malformed(format!(
                        "{who} gives {} values for view limits, which are [min, max]",
                        values.len()
                    )));
                }
                if matches!(target, Target::Xlim { .. }) {
                    ax.set_xlim(values[0], values[1]);
                } else {
                    ax.set_ylim(values[0], values[1]);
                }
            }
        }
        Ok(())
    }

    /// Whether the clock moves anything: the timeline has tracks, or some
    /// control carries a `(time, position)` grid. A figure whose only
    /// time-dependence is a grid still animates — the timeline then
    /// contributes the clock its grids are evaluated against.
    #[cfg(feature = "portable")]
    #[must_use]
    pub fn is_animated(&self) -> bool {
        self.timeline.as_ref().is_some_and(|tl| !tl.is_empty())
            || (self.timeline.is_some() && self.controls.iter().any(|c| !c.grids.is_empty()))
    }

    /// Declare a user-driven parameter (schema 4), returning its index for
    /// [`Figure::set_control`]. The control starts at its default; the base
    /// artist data should be what its tracks produce there, since that is
    /// what a schema-3 host — and the poster — will show.
    #[cfg(feature = "portable")]
    pub fn add_control(&mut self, control: crate::portable::Control) -> usize {
        self.control_values.push(control.default);
        self.controls.push(control);
        self.controls.len() - 1
    }

    /// The controls this figure declares, in declaration order.
    #[cfg(feature = "portable")]
    #[must_use]
    pub fn controls(&self) -> &[crate::portable::Control] {
        &self.controls
    }

    /// The live position of each control, parallel to [`Figure::controls`].
    #[cfg(feature = "portable")]
    #[must_use]
    pub fn control_values(&self) -> &[f64] {
        &self.control_values
    }

    /// Move control `index` to `value` — clamped into its range, snapped to
    /// its step, defaulted when non-finite — and re-evaluate the figure.
    ///
    /// This is a pure function of the clock and the control vector, exactly as
    /// [`Figure::seek`] is: the same positions always produce the same figure,
    /// regardless of the order the user reached them in.
    ///
    /// # Errors
    ///
    /// [`PortableError::Malformed`](crate::portable::PortableError::Malformed)
    /// if `index` is out of range, or as [`Figure::seek`] when a track
    /// addresses an artist that does not exist.
    #[cfg(feature = "portable")]
    pub fn set_control(&mut self, index: usize, value: f64) -> Result<(), PortableError> {
        let control = self.controls.get(index).ok_or_else(|| {
            PortableError::Malformed(format!(
                "control {index} does not exist; the figure declares {}",
                self.controls.len()
            ))
        })?;
        self.control_values[index] = control.normalize(value);
        self.apply_state()
    }

    /// Serialize this figure to a **portable figure** (`.riz`): the semantic
    /// model — axes, artists, data, and rcparams — in a chunked binary
    /// container, from which [`Figure::from_portable`] reconstructs an
    /// identical figure.
    ///
    /// Unlike PNG/SVG/PDF export, which flattens the figure to geometry, this
    /// carries the *data*, so a consumer can re-run layout and rasterization
    /// from it — the prerequisite for pan, zoom, and resolution independence
    /// away from the process that built the figure. See
    /// `design/10-portable-figure.md`.
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// let mut fig = Figure::new(4.0, 3.0);
    /// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0]);
    ///
    /// let bytes = fig.to_portable()?;
    /// let restored = Figure::from_portable(&bytes)?;
    /// // The round trip is pixel-exact, not merely similar.
    /// assert_eq!(restored.encode_png()?, fig.encode_png()?);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`PortableError::Unsupported`] if the figure holds state with
    /// no wire form — today, only a
    /// [`FuncFormatter`](crate::axis::ticker::FuncFormatter) on an axis, whose
    /// tick labels come from a Rust closure. Export refuses rather than
    /// silently substitute a formatter that labels ticks differently.
    #[cfg(feature = "portable")]
    pub fn to_portable(&self) -> Result<Vec<u8>, PortableError> {
        self.to_portable_with(&crate::portable::PortableConfig::default())
    }

    /// Serialize this figure to a portable figure with explicit options.
    ///
    /// The default ([`Figure::to_portable`]) embeds a PNG poster, which is
    /// what every non-rendering path shows: scripting disabled, a schema no
    /// runtime on hand supports, an archive preview, or the card displayed
    /// while a renderer downloads. Turn it off only when the consumer is known
    /// to always render live.
    ///
    /// # Errors
    ///
    /// As [`Figure::to_portable`], plus a
    /// [`PortableError::Unsupported`](crate::portable::PortableError::Unsupported)
    /// if the poster fails to encode.
    #[cfg(feature = "portable")]
    pub fn to_portable_with(
        &self,
        cfg: &crate::portable::PortableConfig,
    ) -> Result<Vec<u8>, PortableError> {
        let mut bank = crate::portable::data::BankWriter::new();
        let axes = self
            .axes
            .iter()
            .map(|ax| ax.to_portable(&mut bank))
            .collect::<Result<Vec<_>, _>>()?;

        // The poster is a byproduct of a figure the exporter is already
        // holding: it renders through the ordinary pipeline, so it is exactly
        // what a live renderer would draw at authoring size.
        let poster = if cfg.poster {
            Some(self.encode_png().map_err(|e| {
                crate::portable::PortableError::Unsupported(format!(
                    "figure poster failed to encode: {e}"
                ))
            })?)
        } else {
            None
        };

        let (w, h) = self.size_px();
        // The figure's own title if it has one, else the first axes' title —
        // for a single-panel figure that is the name a reader would give it.
        let title = self
            .suptitle
            .clone()
            .or_else(|| self.axes.first().and_then(Axes::title_text));
        let meta = crate::portable::Meta {
            width_px: w.max(0.0).round() as u32,
            height_px: h.max(0.0).round() as u32,
            alt: cfg.alt.clone(),
            title,
            poster: poster.as_ref().map(|p| crate::portable::PosterRef {
                chunk: "PSTR".to_string(),
                mime: "image/png".to_string(),
                bytes: p.len(),
            }),
            animated: self.is_animated(),
            duration: self.timeline.as_ref().map_or(0.0, |t| t.duration),
        };

        let spec = crate::portable::spec::PortableSpec {
            schema: crate::portable::SCHEMA_VERSION,
            generator: crate::portable::GeneratorRef {
                rizzma: env!("CARGO_PKG_VERSION").to_string(),
            },
            renderer: crate::portable::RendererRef {
                version: env!("CARGO_PKG_VERSION").to_string(),
                sha256: None,
            },
            meta: Some(meta),
            timeline: self.timeline.clone(),
            controls: self.controls.clone(),
            figure: crate::portable::spec::FigureSpec {
                width_in: self.width_in,
                height_in: self.height_in,
                dpi: self.dpi,
                facecolor: self.facecolor,
                rc: self.rc.clone(),
                suptitle: self.suptitle.clone(),
                suptitle_color: self.suptitle_color,
                axes,
                colorbars: self.colorbars.clone(),
            },
            accessors: bank.accessors.clone(),
        };
        let json = serde_json::to_vec(&spec)?;

        // JSON and the poster precede the bulk payload so a consumer can paint
        // a card from a partial read.
        let mut chunks: Vec<([u8; 4], &[u8])> = vec![(crate::portable::container::TAG_JSON, &json)];
        if let Some(poster) = &poster {
            chunks.push((crate::portable::container::TAG_PSTR, poster));
        }
        chunks.push((crate::portable::container::TAG_BIN, &bank.bytes));
        Ok(crate::portable::container::write(&chunks))
    }

    /// Write this figure to `path` as a portable figure (`.riz`).
    ///
    /// # Errors
    ///
    /// As [`Figure::to_portable`], plus [`PortableError::Io`] if writing the
    /// file fails.
    #[cfg(feature = "portable")]
    pub fn save_portable<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), PortableError> {
        std::fs::write(path, self.to_portable()?)?;
        Ok(())
    }

    /// Write this figure as a poster-tier `.riz.html`: one file a browser
    /// opens (showing the embedded poster and alt text, offline) and a host
    /// ingests — the canonical artifact is carried inside, recoverable
    /// byte-for-byte with [`portable::unwrap_html`](crate::portable::unwrap_html).
    ///
    /// # Errors
    ///
    /// As [`Figure::to_portable`], plus I/O errors writing the file.
    #[cfg(feature = "portable")]
    pub fn save_html<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), PortableError> {
        let bytes = self.to_portable()?;
        let html = crate::portable::wrap_html(&bytes, &crate::portable::Limits::default())?;
        std::fs::write(path, html)?;
        Ok(())
    }

    /// Write this figure as a live-tier `.riz.html` with an embedded,
    /// caller-supplied runtime: double-click it and the figure pans, zooms,
    /// and animates, entirely offline. The poster still renders first, so an
    /// environment where wasm cannot start degrades to the poster.
    ///
    /// The caller chooses the runtime, exactly as a mounting host does — pass
    /// the three published assets whose digests `runtime.json` pins.
    ///
    /// # Errors
    ///
    /// As [`Figure::save_html`].
    #[cfg(feature = "portable")]
    pub fn save_html_live<P: AsRef<std::path::Path>>(
        &self,
        path: P,
        rt: &crate::portable::HtmlRuntime<'_>,
    ) -> Result<(), PortableError> {
        let bytes = self.to_portable()?;
        let html =
            crate::portable::wrap_html_live(&bytes, &crate::portable::Limits::default(), rt)?;
        std::fs::write(path, html)?;
        Ok(())
    }

    /// Reconstruct a figure from portable-figure bytes produced by
    /// [`Figure::to_portable`].
    ///
    /// Import is strict: unknown fields, unknown enum variants, a schema
    /// version this build does not support, malformed chunks, out-of-bounds
    /// accessors, and inconsistent array lengths are all errors. A figure that
    /// silently dropped an artist would be a scientifically wrong figure, so
    /// nothing is skipped or best-guessed.
    ///
    /// # Errors
    ///
    /// [`PortableError::Malformed`], [`PortableError::Schema`], or
    /// [`PortableError::Json`] depending on how the artifact is invalid.
    #[cfg(feature = "portable")]
    pub fn from_portable(bytes: &[u8]) -> Result<Figure, PortableError> {
        Figure::from_portable_limited(bytes, &crate::portable::Limits::default())
    }

    /// Reconstruct a figure, enforcing caller-supplied budgets.
    ///
    /// Use this for artifacts from an untrusted source: a trusted renderer
    /// still parses attacker-influenced data, and memory safety removes a
    /// corruption class rather than parser denial-of-service.
    ///
    /// # Errors
    ///
    /// As [`Figure::from_portable`], plus
    /// [`PortableError::Budget`](crate::portable::PortableError::Budget) when
    /// `limits` are exceeded.
    #[cfg(feature = "portable")]
    pub fn from_portable_limited(
        bytes: &[u8],
        limits: &crate::portable::Limits,
    ) -> Result<Figure, PortableError> {
        use crate::portable::container;

        let dir = container::directory(bytes, limits)?;
        let json = container::chunk(bytes, &dir, container::TAG_JSON)
            .expect("directory guarantees a JSON chunk");
        if json.len() > limits.max_json_bytes {
            return Err(PortableError::Budget(format!(
                "JSON chunk is {} bytes, over the {} byte limit",
                json.len(),
                limits.max_json_bytes
            )));
        }
        for tag in [container::TAG_FONT, container::TAG_WASM] {
            if container::chunk(bytes, &dir, tag).is_some() {
                return Err(PortableError::Malformed(format!(
                    "chunk {:?} (self-contained profile) is not supported by this build",
                    container::tag_name(tag)
                )));
            }
        }
        let bin = container::chunk(bytes, &dir, container::TAG_BIN).unwrap_or(&[]);

        let spec: crate::portable::spec::PortableSpec = serde_json::from_slice(json)?;
        if spec.schema < crate::portable::SCHEMA_MIN
            || spec.schema > crate::portable::SCHEMA_VERSION
        {
            return Err(PortableError::Schema {
                found: spec.schema,
                min: crate::portable::SCHEMA_MIN,
                max: crate::portable::SCHEMA_VERSION,
            });
        }
        let reader = crate::portable::data::BankReader::new(bin, &spec.accessors);
        let fig = spec.figure;
        Ok(Figure {
            width_in: fig.width_in,
            height_in: fig.height_in,
            dpi: fig.dpi,
            facecolor: fig.facecolor,
            rc: fig.rc,
            // Text renders from the embedded face on every target; carrying a
            // subsetted font in the artifact is phase P3 of design/10.
            font: FontSource::dejavu_sans(),
            axes: fig
                .axes
                .iter()
                .map(|ax| Axes::from_portable(ax, &reader))
                .collect::<Result<_, _>>()?,
            suptitle: fig.suptitle,
            suptitle_color: fig.suptitle_color,
            colorbars: fig.colorbars,
            timeline: spec.timeline,
            control_values: spec.controls.iter().map(|c| c.default).collect(),
            controls: spec.controls,
            time: 0.0,
        })
    }

    /// Open this figure in the system browser as an interactive viewer
    /// (matplotlib's `plt.show()`), blocking until the window is closed. Pan by
    /// dragging, zoom with the wheel, double-click or **⌂ Home** to reset, and
    /// export to PNG/SVG/PDF from the toolbar. See [`crate::show`].
    #[cfg(all(feature = "show", not(target_arch = "wasm32")))]
    pub fn show(self) {
        crate::show::show(self);
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
    fn subplot_envelopes_tile_without_gaps() {
        // Tight layout derives every frame by insetting its envelope by the
        // decorations it measured, so the envelopes themselves must tile the
        // figure edge to edge. If they carried matplotlib's default
        // `wspace`/`hspace` as well, that gap would reserve room for the same
        // decorations a second time and leave an empty band between panels.
        let mut fig = Figure::new(12.0, 7.0);
        for i in 1..=4 {
            fig.add_subplot(2, 2, i);
        }
        let env: Vec<Bbox> = fig
            .axes
            .iter()
            .map(|ax| ax.layout_envelope.expect("subplot axes carry an envelope"))
            .collect();

        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        // Envelopes are figure fractions, y-up: index 0/1 are the top row.
        assert!(close(env[0].xmax(), env[1].xmin()), "column gap in row 0");
        assert!(close(env[2].xmax(), env[3].xmin()), "column gap in row 1");
        assert!(close(env[0].ymin(), env[2].ymax()), "row gap in column 0");
        assert!(close(env[1].ymin(), env[3].ymax()), "row gap in column 1");
        // …and they cover the whole figure.
        assert!(close(env[0].xmin(), 0.0) && close(env[1].xmax(), 1.0));
        assert!(close(env[2].ymin(), 0.0) && close(env[0].ymax(), 1.0));
    }

    #[test]
    fn stacked_subplots_are_separated_only_by_their_decorations() {
        // The visible band between two stacked panels should hold the upper
        // panel's tick labels and the lower panel's title, and nothing more.
        // Regression guard: this gap was once ~2.2x larger because an empty
        // `hspace` band sat between the two decoration insets.
        let mut fig = Figure::new(12.0, 7.0);
        for i in 1..=4 {
            let ax = fig.add_subplot(2, 2, i);
            ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
            ax.set_title("panel");
            if i > 2 {
                ax.set_xlabel("time");
            }
        }
        let (w, h) = fig.size_px();
        let insets: Vec<(f64, f64, f64, f64)> = (0..4)
            .map(|i| fig.axes[i].layout_insets(&fig.font, 1.0, None))
            .collect();
        let top_env = fig.axes[0].layout_envelope.expect("envelope");
        let bottom_env = fig.axes[2].layout_envelope.expect("envelope");

        // y-up fractions -> top-down pixels.
        let top_frame_bottom = (1.0 - top_env.ymin()) * h - insets[0].2;
        let bottom_frame_top = (1.0 - bottom_env.ymax()) * h + insets[2].3;
        let gap = bottom_frame_top - top_frame_bottom;

        assert!(
            gap > 0.0,
            "stacked panels must not overlap (gap was {gap:.1}px)"
        );
        let budget = insets[0].2 + insets[2].3;
        assert!(
            gap <= budget + 1.0,
            "gap {gap:.1}px exceeds the {budget:.1}px its decorations need — \
             an empty band crept back into the envelope grid"
        );
        let _ = w;
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
    fn stacked_subplots_get_equal_frame_widths() {
        // Different y magnitudes produce different y-tick-label widths; the
        // column union must still give both frames identical x extents.
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(2, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
        fig.add_subplot(2, 1, 2)
            .plot(&[0.0, 1.0], &[10000.0, 30000.0]);
        let (w, h) = fig.size_px();
        let a = fig.layout_rect_for(0, w, h).unwrap();
        let b = fig.layout_rect_for(1, w, h).unwrap();
        assert!((a.xmin() - b.xmin()).abs() < 1e-9, "left edges align");
        assert!((a.xmax() - b.xmax()).abs() < 1e-9, "right edges align");
        assert!(a.ymin() > b.ymax(), "still stacked, not overlapping");
    }

    #[test]
    fn sharex_hides_the_upper_x_tick_labels_and_reclaims_the_band() {
        let mut fig = Figure::new(4.0, 3.0);
        fig.add_subplot(2, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
        fig.add_subplot(2, 1, 2).plot(&[0.0, 1.0], &[0.0, 1.0]);
        let (w, h) = fig.size_px();
        let before = fig.layout_rect_for(0, w, h).unwrap();

        fig.sharex(1, 0);
        let after = fig.layout_rect_for(0, w, h).unwrap();
        // The upper subplot's x tick-label band is released to its frame.
        assert!(
            after.ymin() < before.ymin() - 5.0,
            "upper frame must grow downward: {} -> {}",
            before.ymin(),
            after.ymin()
        );
        // The lower subplot keeps its labels (its frame bottom is unchanged).
        let lower = fig.layout_rect_for(1, w, h).unwrap();
        assert!(lower.ymin() > h * 0.05, "lower keeps its tick-label room");
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
    fn suptitle_renders_in_the_top_canvas_band() {
        let mut fig = Figure::new(3.0, 2.0);
        fig.add_subplot(1, 1, 1);
        fig.suptitle("Overview");
        let rendered = fig.render();

        let has_ink =
            (5..35).any(|y| (80..220).any(|x| pixel(&rendered, x, y) != [255, 255, 255, 255]));
        assert!(has_ink, "expected supertitle ink near the top center");
    }

    #[test]
    fn suptitle_reserves_space_above_tight_layout_axes() {
        let mut fig = Figure::new(3.0, 2.0);
        fig.add_subplot(1, 1, 1);
        let without = fig.layout_rect_for(0, 300.0, 200.0).unwrap();
        fig.suptitle("Overview");
        let with = fig.layout_rect_for(0, 300.0, 200.0).unwrap();

        assert!(with.ymax() < without.ymax());
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
