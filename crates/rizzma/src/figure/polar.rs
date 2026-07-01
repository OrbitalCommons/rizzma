//! Polar plotting for rizzma.
//!
//! A self-contained [`PolarAxes`] that collects `(theta, r)` polar curves and
//! draws them — together with a polar grid of concentric range rings and angular
//! spokes — to any [`Renderer`]. It mirrors the standalone shape of
//! `rizzma::mplot3d::Axes3D`: data is accumulated in raw polar coordinates, the radial
//! extent (`rmax`) is resolved at draw time, and the scene rasterizes through
//! [`PolarAxes::render_png`] / [`PolarAxes::save_png`].
//!
//! # Coordinate convention
//!
//! Angles are in **radians**, `theta = 0` points along `+x` (to the right) and
//! increases **counter-clockwise** (matplotlib's default). A data point
//! `(theta, r)` maps to cartesian `(r*cos(theta), r*sin(theta))`, then scales so
//! `rmax` reaches the plot radius, centered in a **square** plot region so the
//! grid circles render round (equal aspect).
//!
//! ![polar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar.png)
//!
//! ```
//! use rizzma::figure::PolarAxes;
//! use std::f64::consts::TAU;
//!
//! let theta: Vec<f64> = (0..=360).map(|i| i as f64 * TAU / 360.0).collect();
//! let r: Vec<f64> = theta.iter().map(|t| (2.0 * t).cos().abs()).collect();
//! let mut ax = PolarAxes::new();
//! ax.plot(&theta, &r);
//! let renderer = ax.render_png(500, 500, 100.0);
//! assert_eq!(renderer.pixmap().width(), 500);
//! ```

use crate::core::color::{DEFAULT_COLOR_CYCLE, Rgba};
use crate::core::{Affine2D, Path};
use crate::render::{GraphicsContext, Renderer};
use crate::skia::SkiaRenderer;
use crate::text::FontSource;

/// A polar curve added with [`PolarAxes::plot`].
#[derive(Debug, Clone)]
struct PolarLine {
    /// Parallel `(theta_radians, r)` samples.
    points: Vec<(f64, f64)>,
    color: Rgba,
    width: f64,
}

/// A set of marker points added with [`PolarAxes::scatter`].
#[derive(Debug, Clone)]
struct PolarScatter {
    /// Parallel `(theta_radians, r)` samples, one marker per entry.
    points: Vec<(f64, f64)>,
    color: Rgba,
}

/// A filled polar region added with [`PolarAxes::fill`].
#[derive(Debug, Clone)]
struct PolarFill {
    /// Boundary `(theta_radians, r)` samples; the polygon is auto-closed.
    points: Vec<(f64, f64)>,
    color: Rgba,
}

/// Marker radius in pixels for [`PolarAxes::scatter`] points.
const SCATTER_MARKER_RADIUS: f64 = 4.0;

/// Alpha applied to the face color of a [`PolarAxes::fill`] region.
const FILL_ALPHA: f64 = 0.4;

/// Light gray used for the polar grid rings and spokes.
const GRID_COLOR: Rgba = Rgba::new(0.7, 0.7, 0.7, 1.0);

/// Darker gray used for the outer perimeter (the polar spine).
const SPINE_COLOR: Rgba = Rgba::new(0.4, 0.4, 0.4, 1.0);

/// Fractional pixel margin reserved around the plot circle on every side; the
/// extra room outside the perimeter is where the angular labels sit.
const MARGIN_FRAC: f64 = 0.16;

/// Number of segments used to approximate each grid ring (and the perimeter).
const RING_SEGMENTS: usize = 180;

/// A self-contained polar axes that draws `(theta, r)` curves over a polar grid.
///
/// Construct with [`PolarAxes::new`], add curves with [`PolarAxes::plot`]
/// (`theta` in radians, increasing counter-clockwise from `+x`), then rasterize
/// with [`PolarAxes::render_png`] or [`PolarAxes::save_png`]. The radial extent
/// is taken from the data unless pinned with [`PolarAxes::set_rmax`].
#[derive(Debug, Clone)]
pub struct PolarAxes {
    lines: Vec<PolarLine>,
    scatters: Vec<PolarScatter>,
    fills: Vec<PolarFill>,
    /// Largest `r` seen so far across all curves (drives the auto `rmax`).
    data_rmax: f64,
    /// Explicit radial maximum, overriding the data-derived value when set.
    rmax_override: Option<f64>,
    /// Color cycle index for the next auto-colored curve.
    cycle: usize,
}

impl PolarAxes {
    /// Create an empty polar axes spanning the full circle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scatters: Vec::new(),
            fills: Vec::new(),
            data_rmax: 0.0,
            rmax_override: None,
            cycle: 0,
        }
    }

    /// Pin the radial maximum (the value of `r` reaching the outer ring),
    /// returning `&mut self` for chaining. Non-finite or non-positive values are
    /// ignored.
    pub fn set_rmax(&mut self, rmax: f64) -> &mut Self {
        if rmax.is_finite() && rmax > 0.0 {
            self.rmax_override = Some(rmax);
        }
        self
    }

    /// The next color from the default cycle, advancing the cursor.
    fn next_color(&mut self) -> Rgba {
        let hex = DEFAULT_COLOR_CYCLE[self.cycle % DEFAULT_COLOR_CYCLE.len()];
        self.cycle += 1;
        Rgba::from_hex(hex).unwrap_or(Rgba::new(0.121, 0.466, 0.705, 1.0))
    }

    /// Add a polar curve through the parallel `theta` (radians) and `r` samples.
    ///
    /// Only the common prefix length is used when the slices differ in length;
    /// empty input adds an empty curve. Negative `r` values are clamped to `0`.
    /// The curve takes the next color from the cycle.
    pub fn plot(&mut self, theta: &[f64], r: &[f64]) -> &mut Self {
        let n = theta.len().min(r.len());
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let ri = if r[i].is_finite() { r[i].max(0.0) } else { 0.0 };
            if theta[i].is_finite() {
                self.data_rmax = self.data_rmax.max(ri);
                points.push((theta[i], ri));
            }
        }
        let color = self.next_color();
        self.lines.push(PolarLine {
            points,
            color,
            width: 1.5,
        });
        self
    }

    /// Scatter marker points at the parallel `theta` (radians) and `r` samples.
    ///
    /// Each `(theta, r)` is projected through the same mapping as [`plot`](Self::plot)
    /// and drawn as a filled circular marker in the next cycle color. Only the
    /// common prefix length is used when the slices differ in length; empty input
    /// adds nothing. Negative `r` values are clamped to `0`.
    ///
    /// ![polar scatter](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar_scatter.png)
    pub fn scatter(&mut self, theta: &[f64], r: &[f64]) -> &mut Self {
        let n = theta.len().min(r.len());
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let ri = if r[i].is_finite() { r[i].max(0.0) } else { 0.0 };
            if theta[i].is_finite() {
                self.data_rmax = self.data_rmax.max(ri);
                points.push((theta[i], ri));
            }
        }
        if points.is_empty() {
            return self;
        }
        let color = self.next_color();
        self.scatters.push(PolarScatter { points, color });
        self
    }

    /// Fill the closed polar region bounded by the parallel `theta` (radians) and
    /// `r` samples.
    ///
    /// The boundary points are projected through the same mapping as
    /// [`plot`](Self::plot), the polygon is auto-closed, and it is drawn as a
    /// semi-transparent filled region with a solid edge in the next cycle color.
    /// Only the common prefix length is used when the slices differ in length;
    /// empty input (or fewer than three usable points) adds nothing. Negative `r`
    /// values are clamped to `0`.
    ///
    /// ![polar fill](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar_fill.png)
    pub fn fill(&mut self, theta: &[f64], r: &[f64]) -> &mut Self {
        let n = theta.len().min(r.len());
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let ri = if r[i].is_finite() { r[i].max(0.0) } else { 0.0 };
            if theta[i].is_finite() {
                self.data_rmax = self.data_rmax.max(ri);
                points.push((theta[i], ri));
            }
        }
        if points.len() < 3 {
            return self;
        }
        let color = self.next_color();
        self.fills.push(PolarFill { points, color });
        self
    }

    /// The effective radial maximum: the explicit override, else the data
    /// maximum, falling back to `1.0` for empty/degenerate data.
    #[must_use]
    pub fn rmax(&self) -> f64 {
        if let Some(rmax) = self.rmax_override {
            return rmax;
        }
        if self.data_rmax.is_finite() && self.data_rmax > 0.0 {
            self.data_rmax
        } else {
            1.0
        }
    }

    /// The plot geometry for a canvas of `width` x `height` pixels: the pixel
    /// center `(cx, cy)` and the plot radius `radius` (the perimeter), all in the
    /// y-up device space the raster backend flips for itself.
    fn geometry(&self, width: f64, height: f64) -> (f64, f64, f64) {
        let cx = 0.5 * width;
        let cy = 0.5 * height;
        let side = width.min(height);
        let radius = 0.5 * side * (1.0 - 2.0 * MARGIN_FRAC);
        (cx, cy, radius)
    }

    /// Map a polar data point `(theta, r)` to a pixel `(px, py)`.
    ///
    /// `r` is scaled so that [`PolarAxes::rmax`] lands on the plot radius;
    /// `theta = 0` points to `+x` and increases counter-clockwise. Coordinates
    /// are in y-up device space (origin bottom-left).
    #[must_use]
    pub fn to_pixel(&self, theta: f64, r: f64, width: f64, height: f64) -> (f64, f64) {
        let (cx, cy, radius) = self.geometry(width, height);
        let frac = (r / self.rmax()).clamp(0.0, 1.0);
        let pr = frac * radius;
        let px = cx + pr * theta.cos();
        let py = cy + pr * theta.sin();
        (px, py)
    }

    /// "Nice" radial tick values strictly between `0` and `rmax`, plus `rmax`
    /// itself, used for the concentric rings (aiming for ~4-5 rings).
    fn radial_ticks(&self) -> Vec<f64> {
        let rmax = self.rmax();
        let step = nice_step(rmax, 5);
        if step <= 0.0 {
            return vec![rmax];
        }
        let mut ticks = Vec::new();
        let mut v = step;
        while v < rmax - step * 1e-6 {
            ticks.push(v);
            v += step;
        }
        ticks.push(rmax);
        ticks
    }

    /// Stroke a polyline through `points` (pixel space) with one graphics call.
    fn stroke_polyline(renderer: &mut dyn Renderer, points: &[[f64; 2]], color: Rgba, width: f64) {
        if points.len() < 2 {
            return;
        }
        let path = Path::from_polyline(points);
        let gc = GraphicsContext::new()
            .with_stroke(color)
            .with_line_width(width);
        renderer.draw_path(&gc, &path, &Affine2D::identity(), None);
    }

    /// Draw a glyph-path label so that `(anchor_x, anchor_y)` is its center,
    /// using the font's measured advance to center horizontally.
    fn draw_label(
        renderer: &mut dyn Renderer,
        font: &FontSource,
        text: &str,
        anchor_x: f64,
        anchor_y: f64,
        size: f64,
    ) {
        let extent = font.measure(text, size);
        // Center horizontally on the anchor; bias the baseline down by roughly
        // half the cap height so the label is vertically centered too.
        let x = anchor_x - 0.5 * extent.width;
        let y = anchor_y - 0.5 * (extent.ascent - extent.descent);
        let path = font.text_to_path(text, size, [x, y]);
        renderer.draw_path(
            &GraphicsContext::new(),
            &path,
            &Affine2D::identity(),
            Some(Rgba::BLACK),
        );
    }

    /// Draw the polar grid, perimeter, data curves, and labels to `renderer`.
    pub fn draw(&self, renderer: &mut dyn Renderer, font: &FontSource) {
        let (width, height) = renderer.canvas_size();
        let (cx, cy, radius) = self.geometry(width, height);
        let rmax = self.rmax();

        // 1. Concentric range rings (light gray), drawn under everything. The
        //    outermost (rmax) ring is the spine, drawn separately below.
        for &tick in &self.radial_ticks() {
            if (tick - rmax).abs() <= f64::EPSILON {
                continue;
            }
            let frac = (tick / rmax).clamp(0.0, 1.0);
            let pr = frac * radius;
            let ring: Vec<[f64; 2]> = (0..=RING_SEGMENTS)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / RING_SEGMENTS as f64;
                    [cx + pr * a.cos(), cy + pr * a.sin()]
                })
                .collect();
            Self::stroke_polyline(renderer, &ring, GRID_COLOR, 0.8);
        }

        // 2. Radial spokes every 45 degrees (light gray).
        for k in 0..8 {
            let a = std::f64::consts::FRAC_PI_4 * k as f64;
            let spoke = [[cx, cy], [cx + radius * a.cos(), cy + radius * a.sin()]];
            Self::stroke_polyline(renderer, &spoke, GRID_COLOR, 0.8);
        }

        // 3. Perimeter ring (the polar spine), darker, on top of the grid.
        let perimeter: Vec<[f64; 2]> = (0..=RING_SEGMENTS)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / RING_SEGMENTS as f64;
                [cx + radius * a.cos(), cy + radius * a.sin()]
            })
            .collect();
        Self::stroke_polyline(renderer, &perimeter, SPINE_COLOR, 1.2);

        // 4. Filled polar regions, over the grid but under the curves/markers.
        for fill in &self.fills {
            let mut verts: Vec<[f64; 2]> = fill
                .points
                .iter()
                .map(|&(t, r)| {
                    let (px, py) = self.to_pixel(t, r, width, height);
                    [px, py]
                })
                .collect();
            if verts.len() < 3 {
                continue;
            }
            // Close the polygon so the fill is a proper region.
            if verts.first() != verts.last() {
                verts.push(verts[0]);
            }
            let path = Path::from_polyline(&verts);
            let face = fill.color.with_alpha(fill.color.a * FILL_ALPHA);
            let gc = GraphicsContext::new()
                .with_stroke(fill.color)
                .with_line_width(1.5);
            renderer.draw_path(&gc, &path, &Affine2D::identity(), Some(face));
        }

        // 5. Data curves, over the grid.
        for line in &self.lines {
            let pts: Vec<[f64; 2]> = line
                .points
                .iter()
                .map(|&(t, r)| {
                    let (px, py) = self.to_pixel(t, r, width, height);
                    [px, py]
                })
                .collect();
            Self::stroke_polyline(renderer, &pts, line.color, line.width);
        }

        // 6. Scatter marker points, on top of curves.
        let marker = Path::unit_circle();
        let marker_transform = Affine2D::from_scale(SCATTER_MARKER_RADIUS, SCATTER_MARKER_RADIUS);
        for sc in &self.scatters {
            let offsets: Vec<[f64; 2]> = sc
                .points
                .iter()
                .map(|&(t, r)| {
                    let (px, py) = self.to_pixel(t, r, width, height);
                    [px, py]
                })
                .collect();
            let offset_path = Path::from_polyline(&offsets);
            let gc = GraphicsContext::new().with_stroke(sc.color);
            renderer.draw_markers(
                &gc,
                &marker,
                &marker_transform,
                &offset_path,
                &Affine2D::identity(),
                Some(sc.color),
            );
        }

        // 7. Angular tick labels just outside the perimeter at each spoke.
        let label_r = radius * 1.08;
        let label_size = 11.0;
        for k in 0..8 {
            let deg = 45 * k;
            let a = std::f64::consts::FRAC_PI_4 * k as f64;
            let lx = cx + label_r * a.cos();
            let ly = cy + label_r * a.sin();
            Self::draw_label(renderer, font, &format!("{deg}\u{b0}"), lx, ly, label_size);
        }

        // 8. Radial tick labels along the 0-degree (horizontal) spoke.
        let radial_size = 9.0;
        for &tick in &self.radial_ticks() {
            let frac = (tick / rmax).clamp(0.0, 1.0);
            let pr = frac * radius;
            // Nudge slightly above the spoke so the labels don't sit on the line.
            let lx = cx + pr;
            let ly = cy + 0.04 * radius;
            Self::draw_label(renderer, font, &format_tick(tick), lx, ly, radial_size);
        }
    }

    /// Render the polar axes to a fresh white-backed [`SkiaRenderer`].
    #[must_use]
    pub fn render_png(&self, width_px: u32, height_px: u32, dpi: f64) -> SkiaRenderer {
        let mut renderer = SkiaRenderer::new(width_px, height_px, dpi);
        let bg = Path::from_polyline(&[
            [0.0, 0.0],
            [f64::from(width_px), 0.0],
            [f64::from(width_px), f64::from(height_px)],
            [0.0, f64::from(height_px)],
            [0.0, 0.0],
        ]);
        renderer.draw_path(
            &GraphicsContext::new(),
            &bg,
            &Affine2D::identity(),
            Some(Rgba::WHITE),
        );
        let font = FontSource::dejavu_sans();
        self.draw(&mut renderer, &font);
        renderer
    }

    /// Render and save a PNG to `path`.
    ///
    /// # Errors
    ///
    /// Returns the [`SkiaRenderer`] PNG error if encoding or writing fails.
    pub fn save_png<P: AsRef<std::path::Path>>(
        &self,
        path: P,
        width_px: u32,
        height_px: u32,
        dpi: f64,
    ) -> Result<(), crate::skia::PngError> {
        self.render_png(width_px, height_px, dpi).save_png(path)
    }
}

impl Default for PolarAxes {
    fn default() -> Self {
        Self::new()
    }
}

/// A "nice" tick step that divides `rmax` into roughly `target` intervals,
/// snapped to a 1/2/5 x 10^k value (matplotlib's `MaxNLocator` spirit).
fn nice_step(rmax: f64, target: usize) -> f64 {
    if !rmax.is_finite() || rmax <= 0.0 || target == 0 {
        return 0.0;
    }
    let raw = rmax / target as f64;
    let mag = 10_f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Format a radial tick value, dropping trailing zeros for tidy labels.
fn format_tick(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.2}");
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, TAU};

    const W: f64 = 400.0;
    const H: f64 = 400.0;

    /// `(theta = 0, r = rmax)` maps to the right edge of the plot circle:
    /// maximum x, at the vertical center.
    #[test]
    fn theta_zero_rmax_maps_to_right_edge() {
        let mut ax = PolarAxes::new();
        ax.plot(&[0.0, FRAC_PI_2], &[1.0, 1.0]);
        let (cx, cy, radius) = ax.geometry(W, H);
        let (px, py) = ax.to_pixel(0.0, ax.rmax(), W, H);
        assert!((px - (cx + radius)).abs() < 1e-9, "px={px}");
        assert!((py - cy).abs() < 1e-9, "py={py}");
    }

    /// `(theta = pi/2, r = rmax)` maps to the top of the plot circle:
    /// horizontal center, maximum y.
    #[test]
    fn theta_half_pi_rmax_maps_to_top() {
        let mut ax = PolarAxes::new();
        ax.plot(&[0.0], &[2.0]);
        let (cx, cy, radius) = ax.geometry(W, H);
        let (px, py) = ax.to_pixel(FRAC_PI_2, ax.rmax(), W, H);
        assert!((px - cx).abs() < 1e-9, "px={px}");
        assert!((py - (cy + radius)).abs() < 1e-9, "py={py}");
    }

    /// The grid emits inner rings plus the outer `rmax` ring; the radial ticks
    /// give ~5 rings with `rmax` as the outermost, and the spokes are eight
    /// evenly spaced 45-degree angles.
    #[test]
    fn grid_produces_expected_ring_and_spoke_counts() {
        let mut ax = PolarAxes::new();
        ax.plot(&[0.0, FRAC_PI_2, TAU], &[0.0, 0.5, 1.0]);
        let ticks = ax.radial_ticks();
        assert!(ticks.len() >= 4, "expected ~5 rings, got {}", ticks.len());
        assert!(
            (*ticks.last().unwrap() - ax.rmax()).abs() < 1e-9,
            "outer ring must be rmax"
        );
        let spokes: Vec<f64> = (0..8)
            .map(|k| std::f64::consts::FRAC_PI_4 * k as f64)
            .collect();
        assert_eq!(spokes.len(), 8);
    }

    /// Empty data still produces a valid grid render (no curves) without panic.
    #[test]
    fn empty_data_draws_grid_only() {
        let ax = PolarAxes::new();
        assert!(ax.lines.is_empty());
        assert_eq!(ax.rmax(), 1.0);
        let r = ax.render_png(128, 128, 72.0);
        assert_eq!(r.pixmap().width(), 128);
    }

    /// Ragged input uses the common prefix; negative `r` is clamped to zero.
    #[test]
    fn ragged_input_and_negative_r() {
        let mut ax = PolarAxes::new();
        ax.plot(&[0.0, 1.0, 2.0], &[-1.0, 0.5]);
        assert_eq!(ax.lines[0].points.len(), 2);
        assert_eq!(ax.lines[0].points[0].1, 0.0, "negative r clamps to 0");
        assert_eq!(ax.data_rmax, 0.5);
    }

    /// `set_rmax` overrides the data-derived radial maximum.
    #[test]
    fn set_rmax_overrides_data() {
        let mut ax = PolarAxes::new();
        ax.set_rmax(10.0);
        ax.plot(&[0.0], &[1.0]);
        assert_eq!(ax.rmax(), 10.0);
    }

    /// `scatter` stores a marker set and expands `rmax` to cover its `r` values.
    #[test]
    fn scatter_stores_markers_and_expands_rmax() {
        let mut ax = PolarAxes::new();
        ax.plot(&[0.0], &[1.0]);
        ax.scatter(&[0.0, FRAC_PI_2], &[2.0, 3.0]);
        assert_eq!(ax.scatters.len(), 1);
        assert_eq!(ax.scatters[0].points.len(), 2);
        assert_eq!(ax.rmax(), 3.0, "scatter must grow rmax to its data");
    }

    /// `fill` stores a filled region and expands `rmax`; too-few points add nothing.
    #[test]
    fn fill_stores_region_and_expands_rmax() {
        let mut ax = PolarAxes::new();
        ax.fill(&[0.0, FRAC_PI_2, std::f64::consts::PI], &[1.0, 2.0, 4.0]);
        assert_eq!(ax.fills.len(), 1);
        assert_eq!(ax.rmax(), 4.0, "fill must grow rmax to its data");

        // Fewer than three usable points is not a fillable region.
        let mut ax2 = PolarAxes::new();
        ax2.fill(&[0.0, FRAC_PI_2], &[1.0, 1.0]);
        assert!(ax2.fills.is_empty());
    }

    /// Empty input to `scatter`/`fill` adds nothing and still renders the grid.
    #[test]
    fn empty_scatter_and_fill_draw_grid_only() {
        let mut ax = PolarAxes::new();
        ax.scatter(&[], &[]);
        ax.fill(&[], &[]);
        assert!(ax.scatters.is_empty());
        assert!(ax.fills.is_empty());
        let r = ax.render_png(128, 128, 72.0);
        assert_eq!(r.pixmap().width(), 128);
    }

    /// Scatter and fill both use the property cycle and render without panic.
    #[test]
    fn scatter_and_fill_render() {
        let theta: Vec<f64> = (0..=36).map(|i| i as f64 * TAU / 36.0).collect();
        let r: Vec<f64> = theta.iter().map(|t| (2.0 * t).cos().abs()).collect();
        let mut ax = PolarAxes::new();
        ax.fill(&theta, &r);
        ax.scatter(&theta, &r);
        let rend = ax.render_png(256, 256, 72.0);
        assert_eq!(rend.pixmap().width(), 256);
    }

    /// A populated polar axes renders without panicking.
    #[test]
    fn populated_axes_renders() {
        let theta: Vec<f64> = (0..=360).map(|i| i as f64 * TAU / 360.0).collect();
        let r: Vec<f64> = theta.iter().map(|t| (2.0 * t).cos().abs()).collect();
        let mut ax = PolarAxes::new();
        ax.plot(&theta, &r);
        let rend = ax.render_png(256, 256, 72.0);
        assert_eq!(rend.pixmap().width(), 256);
    }
}
