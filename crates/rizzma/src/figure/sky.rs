//! All-sky map projections for rizzma: aitoff and mollweide.
//!
//! A self-contained [`SkyAxes`] mirroring the standalone shape of
//! [`PolarAxes`](crate::figure::PolarAxes): data is accumulated as
//! `(longitude, latitude)` pairs in **radians**, projected at draw time
//! through an equal-area-style all-sky projection, and drawn over a graticule
//! of meridians and parallels — matplotlib's
//! `add_subplot(projection="aitoff")` / `"mollweide"` figures.
//!
//! # Coordinate convention
//!
//! Longitude `λ ∈ [-π, π]` (wrapped on input) increases to the right;
//! latitude `φ ∈ [-π/2, π/2]` (clamped on input) increases upward. The whole
//! sky maps into a `2:1` ellipse-ish outline, centered in the canvas with
//! equal aspect.
//!
//! ![sky](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky.png)
//!
//! ```
//! use rizzma::figure::{SkyAxes, SkyProjection};
//!
//! let mut ax = SkyAxes::new(SkyProjection::Mollweide);
//! ax.scatter(&[0.0, 1.0, -2.0], &[0.3, -0.5, 1.0]);
//! let renderer = ax.render_png(600, 300, 100.0);
//! assert_eq!(renderer.pixmap().width(), 600);
//! ```

use std::f64::consts::{FRAC_PI_2, PI, SQRT_2};

use crate::core::color::{DEFAULT_COLOR_CYCLE, Rgba};
use crate::core::{Affine2D, Path};
use crate::render::{GraphicsContext, Renderer};
use crate::skia::SkiaRenderer;
use crate::text::FontSource;

/// Which all-sky projection a [`SkyAxes`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyProjection {
    /// The Aitoff projection (matplotlib's `projection="aitoff"`).
    ///
    /// ![aitoff](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky_aitoff.png)
    Aitoff,
    /// The Mollweide equal-area projection (`projection="mollweide"`).
    Mollweide,
}

/// A sky curve added with [`SkyAxes::plot`].
#[derive(Debug, Clone)]
struct SkyLine {
    /// `(lon, lat)` samples in radians.
    points: Vec<(f64, f64)>,
    color: Rgba,
    width: f64,
}

/// Marker points added with [`SkyAxes::scatter`].
#[derive(Debug, Clone)]
struct SkyScatter {
    /// `(lon, lat)` samples in radians, one marker per entry.
    points: Vec<(f64, f64)>,
    color: Rgba,
}

/// Marker radius in pixels for [`SkyAxes::scatter`] points.
const SCATTER_MARKER_RADIUS: f64 = 3.0;

/// Light gray used for the graticule meridians and parallels.
const GRID_COLOR: Rgba = Rgba::new(0.7, 0.7, 0.7, 1.0);

/// Darker gray used for the outer sky boundary.
const SPINE_COLOR: Rgba = Rgba::new(0.4, 0.4, 0.4, 1.0);

/// Fractional pixel margin reserved around the projection on every side (the
/// latitude labels sit in it).
const MARGIN_FRAC: f64 = 0.08;

/// Samples per graticule line and per boundary half.
const GRATICULE_SAMPLES: usize = 90;

/// Graticule spacing: meridians every 60° of longitude.
const MERIDIAN_STEP_DEG: i32 = 60;
/// Graticule spacing: parallels every 30° of latitude.
const PARALLEL_STEP_DEG: i32 = 30;

/// Font size of the graticule labels, in pixels.
const LABEL_SIZE: f64 = 9.0;

/// Font size of the title, in pixels (matches [`Axes`](super::Axes) titles).
const TITLE_SIZE: f64 = 12.0;

/// Gap between the canvas top and the title, in pixels.
const TITLE_PAD: f64 = 6.0;

/// Newton-iteration tolerance for the mollweide auxiliary angle.
const MOLLWEIDE_TOL: f64 = 1e-12;

/// A self-contained all-sky axes drawing `(lon, lat)` data over a graticule.
///
/// Construct with [`SkyAxes::new`], add data with [`SkyAxes::plot`] /
/// [`SkyAxes::scatter`] (radians; longitude wrapped to `[-π, π]`, latitude
/// clamped to `[-π/2, π/2]`), then rasterize with [`SkyAxes::render_png`] or
/// [`SkyAxes::save_png`].
#[derive(Debug, Clone)]
pub struct SkyAxes {
    projection: SkyProjection,
    lines: Vec<SkyLine>,
    scatters: Vec<SkyScatter>,
    /// Optional title drawn centered above the map.
    title: Option<String>,
    /// Color cycle index for the next auto-colored artist.
    cycle: usize,
}

/// Project `(lon, lat)` (radians, in range) to the projection's native
/// `(x, y)` plane.
///
/// Aitoff: `x ∈ [-π, π]`, `y ∈ [-π/2, π/2]`. Mollweide: `x ∈ [-2√2, 2√2]`,
/// `y ∈ [-√2, √2]`. Both are 2:1.
fn project(projection: SkyProjection, lon: f64, lat: f64) -> (f64, f64) {
    match projection {
        SkyProjection::Aitoff => {
            // α is the angular distance from the center along the great
            // circle; sinc(α) → 1 as α → 0 keeps the center regular.
            let alpha = (lat.cos() * (lon / 2.0).cos()).acos();
            let sinc = if alpha.abs() < 1e-12 {
                1.0
            } else {
                alpha.sin() / alpha
            };
            (2.0 * lat.cos() * (lon / 2.0).sin() / sinc, lat.sin() / sinc)
        }
        SkyProjection::Mollweide => {
            // Solve 2θ + sin(2θ) = π sin(φ) by Newton iteration.
            let rhs = PI * lat.sin();
            let mut theta = lat;
            for _ in 0..50 {
                let f = 2.0 * theta + (2.0 * theta).sin() - rhs;
                let fp = 2.0 + 2.0 * (2.0 * theta).cos();
                if fp.abs() < 1e-15 {
                    // At the poles the derivative vanishes exactly at the
                    // solution θ = ±π/2.
                    theta = FRAC_PI_2.copysign(lat);
                    break;
                }
                let next = theta - f / fp;
                if (next - theta).abs() < MOLLWEIDE_TOL {
                    theta = next;
                    break;
                }
                theta = next;
            }
            (2.0 * SQRT_2 / PI * lon * theta.cos(), SQRT_2 * theta.sin())
        }
    }
}

/// The native-plane half-extents `(xmax, ymax)` of `projection`.
fn native_extents(projection: SkyProjection) -> (f64, f64) {
    match projection {
        SkyProjection::Aitoff => (PI, FRAC_PI_2),
        SkyProjection::Mollweide => (2.0 * SQRT_2, SQRT_2),
    }
}

/// Wrap a longitude into `[-π, π]` and clamp a latitude into `[-π/2, π/2]`,
/// or `None` when either is non-finite.
fn sanitize(lon: f64, lat: f64) -> Option<(f64, f64)> {
    if !lon.is_finite() || !lat.is_finite() {
        return None;
    }
    let mut lon = (lon + PI).rem_euclid(2.0 * PI) - PI;
    if lon == -PI {
        lon = PI; // prefer the +π edge so rem_euclid's half-open range closes
    }
    Some((lon, lat.clamp(-FRAC_PI_2, FRAC_PI_2)))
}

impl SkyAxes {
    /// Create an empty sky axes with the given projection.
    #[must_use]
    pub fn new(projection: SkyProjection) -> Self {
        Self {
            projection,
            lines: Vec::new(),
            scatters: Vec::new(),
            title: None,
            cycle: 0,
        }
    }

    /// This axes' projection.
    #[must_use]
    pub fn projection(&self) -> SkyProjection {
        self.projection
    }

    /// Set a title drawn centered above the map.
    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.title = Some(title.into());
        self
    }

    /// The next color from the default cycle, advancing the cursor.
    fn next_color(&mut self) -> Rgba {
        let hex = DEFAULT_COLOR_CYCLE[self.cycle % DEFAULT_COLOR_CYCLE.len()];
        self.cycle += 1;
        Rgba::from_hex(hex).unwrap_or(Rgba::new(0.121, 0.466, 0.705, 1.0))
    }

    /// Collect the sanitized common-prefix `(lon, lat)` pairs of two slices.
    fn gather(lon: &[f64], lat: &[f64]) -> Vec<(f64, f64)> {
        lon.iter()
            .zip(lat)
            .filter_map(|(&lo, &la)| sanitize(lo, la))
            .collect()
    }

    /// Add a curve through the parallel `lon`/`lat` samples (radians).
    ///
    /// Only the common prefix length is used when the slices differ; non-finite
    /// pairs are skipped. The curve takes the next color from the cycle.
    /// Segments whose endpoints fall on opposite sides of the ±π seam draw
    /// straight across the map (no seam splitting yet).
    pub fn plot(&mut self, lon: &[f64], lat: &[f64]) -> &mut Self {
        let points = Self::gather(lon, lat);
        let color = self.next_color();
        self.lines.push(SkyLine {
            points,
            color,
            width: 1.5,
        });
        self
    }

    /// Scatter marker points at the parallel `lon`/`lat` samples (radians).
    pub fn scatter(&mut self, lon: &[f64], lat: &[f64]) -> &mut Self {
        let points = Self::gather(lon, lat);
        if points.is_empty() {
            return self;
        }
        let color = self.next_color();
        self.scatters.push(SkyScatter { points, color });
        self
    }

    /// The plot geometry for a `width` x `height` canvas: pixel center and the
    /// native→pixel scale factor, fitting the projection's 2:1 extents with
    /// equal aspect and [`MARGIN_FRAC`] margins.
    fn geometry(&self, width: f64, height: f64) -> (f64, f64, f64) {
        let (xmax, ymax) = native_extents(self.projection);
        let usable_w = width * (1.0 - 2.0 * MARGIN_FRAC);
        let usable_h = height * (1.0 - 2.0 * MARGIN_FRAC);
        let scale = (usable_w / (2.0 * xmax)).min(usable_h / (2.0 * ymax));
        (0.5 * width, 0.5 * height, scale)
    }

    /// Map a `(lon, lat)` data point (radians) to a pixel `(px, py)` in y-up
    /// device space, after wrapping/clamping the inputs.
    #[must_use]
    pub fn to_pixel(&self, lon: f64, lat: f64, width: f64, height: f64) -> (f64, f64) {
        let (lon, lat) = sanitize(lon, lat).unwrap_or((0.0, 0.0));
        let (cx, cy, scale) = self.geometry(width, height);
        let (x, y) = project(self.projection, lon, lat);
        (cx + x * scale, cy + y * scale)
    }

    /// Stroke a polyline through `points` (pixel space).
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

    /// Draw a text label centered on `(anchor_x, anchor_y)` (pixel space).
    fn draw_label(
        renderer: &mut dyn Renderer,
        font: &FontSource,
        text: &str,
        anchor_x: f64,
        anchor_y: f64,
        size: f64,
    ) {
        let extent = font.measure(text, size);
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

    /// Sample a graticule line: fixed-`lon` meridian (`along_lat = true`) or
    /// fixed-`lat` parallel, projected to pixels.
    fn graticule_line(
        &self,
        fixed: f64,
        along_lat: bool,
        width: f64,
        height: f64,
    ) -> Vec<[f64; 2]> {
        (0..=GRATICULE_SAMPLES)
            .map(|i| {
                let t = i as f64 / GRATICULE_SAMPLES as f64;
                let (lon, lat) = if along_lat {
                    (fixed, -FRAC_PI_2 + t * PI)
                } else {
                    (-PI + t * 2.0 * PI, fixed)
                };
                let (px, py) = self.to_pixel(lon, lat, width, height);
                [px, py]
            })
            .collect()
    }

    /// Draw the sky boundary, graticule, labels, and data to `renderer`.
    pub fn draw(&self, renderer: &mut dyn Renderer, font: &FontSource) {
        let (width, height) = renderer.canvas_size();
        // Labels and markers are px at the default 100 DPI, DPI-scaled.
        let s = renderer.decoration_scale();

        // 1. Graticule parallels (skipping the poles, which are points or
        // lines of zero visual weight) and meridians (skipping ±180°, which
        // trace the boundary drawn below).
        let mut lat_deg = -90 + PARALLEL_STEP_DEG;
        while lat_deg < 90 {
            let line = self.graticule_line(f64::from(lat_deg).to_radians(), false, width, height);
            Self::stroke_polyline(renderer, &line, GRID_COLOR, 0.8);
            lat_deg += PARALLEL_STEP_DEG;
        }
        let mut lon_deg = -180 + MERIDIAN_STEP_DEG;
        while lon_deg < 180 {
            let line = self.graticule_line(f64::from(lon_deg).to_radians(), true, width, height);
            Self::stroke_polyline(renderer, &line, GRID_COLOR, 0.8);
            lon_deg += MERIDIAN_STEP_DEG;
        }

        // 2. The sky boundary: the ±π meridians joined into a closed outline.
        let mut boundary = self.graticule_line(PI - 1e-9, true, width, height);
        let west = self.graticule_line(-PI + 1e-9, true, width, height);
        boundary.extend(west.into_iter().rev());
        if let Some(&first) = boundary.first() {
            boundary.push(first);
        }
        Self::stroke_polyline(renderer, &boundary, SPINE_COLOR, 1.0);

        // 3. Labels: latitudes at the left boundary, longitudes along the
        // equator (matplotlib's convention for these projections).
        let mut lat_deg = -90 + PARALLEL_STEP_DEG;
        while lat_deg < 90 {
            let lat = f64::from(lat_deg).to_radians();
            let (px, py) = self.to_pixel(-PI + 1e-9, lat, width, height);
            Self::draw_label(
                renderer,
                font,
                &format!("{lat_deg}°"),
                px - 18.0 * s,
                py,
                LABEL_SIZE * s,
            );
            lat_deg += PARALLEL_STEP_DEG;
        }
        let mut lon_deg = -180 + MERIDIAN_STEP_DEG;
        while lon_deg < 180 {
            if lon_deg != 0 {
                let (px, py) = self.to_pixel(f64::from(lon_deg).to_radians(), 0.0, width, height);
                Self::draw_label(
                    renderer,
                    font,
                    &format!("{lon_deg}°"),
                    px,
                    py + 8.0 * s,
                    LABEL_SIZE * s,
                );
            }
            lon_deg += MERIDIAN_STEP_DEG;
        }

        // 4. Data curves and markers.
        for line in &self.lines {
            let pts: Vec<[f64; 2]> = line
                .points
                .iter()
                .map(|&(lon, lat)| {
                    let (px, py) = self.to_pixel(lon, lat, width, height);
                    [px, py]
                })
                .collect();
            Self::stroke_polyline(renderer, &pts, line.color, line.width);
        }
        for scatter in &self.scatters {
            for &(lon, lat) in &scatter.points {
                let (px, py) = self.to_pixel(lon, lat, width, height);
                let r = SCATTER_MARKER_RADIUS * s;
                let marker =
                    Path::unit_circle().transformed(&Affine2D::from_scale(r, r).translate(px, py));
                renderer.draw_path(
                    &GraphicsContext::new(),
                    &marker,
                    &Affine2D::identity(),
                    Some(scatter.color),
                );
            }
        }

        // 5. Title, centered in the top margin band.
        if let Some(title) = &self.title {
            let size = TITLE_SIZE * s;
            let extent = font.measure(title, size);
            let anchor_y = height - TITLE_PAD * s - 0.5 * (extent.ascent + extent.descent);
            Self::draw_label(renderer, font, title, 0.5 * width, anchor_y, size);
        }
    }

    /// Render this sky axes to a fresh raster canvas of `width_px` x
    /// `height_px` at `dpi`.
    #[must_use]
    pub fn render_png(&self, width_px: u32, height_px: u32, dpi: f64) -> SkiaRenderer {
        let mut renderer = SkiaRenderer::new(width_px, height_px, dpi);
        // White canvas background.
        let (w, h) = renderer.canvas_size();
        let bg = Path::from_polyline(&[[0.0, 0.0], [w, 0.0], [w, h], [0.0, h], [0.0, 0.0]]);
        renderer.draw_path(
            &GraphicsContext::new(),
            &bg,
            &Affine2D::identity(),
            Some(Rgba::WHITE),
        );
        self.draw(&mut renderer, &FontSource::dejavu_sans());
        renderer
    }

    /// Render and save this sky axes as a PNG at `path`.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::skia::PngError`] if encoding or writing fails.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "expected {b}, got {a}");
    }

    #[test]
    fn aitoff_center_equator_and_poles() {
        // Center maps to the origin.
        let (x, y) = project(SkyProjection::Aitoff, 0.0, 0.0);
        approx(x, 0.0, 1e-12);
        approx(y, 0.0, 1e-12);
        // The equator is linear in longitude: x(λ, 0) = λ.
        for lon in [-PI + 1e-9, -1.0, 0.5, 2.0, PI] {
            let (x, y) = project(SkyProjection::Aitoff, lon, 0.0);
            approx(x, lon, 1e-6);
            approx(y, 0.0, 1e-12);
        }
        // The poles land at (0, ±π/2).
        let (x, y) = project(SkyProjection::Aitoff, 0.7, FRAC_PI_2);
        approx(x, 0.0, 1e-6);
        approx(y, FRAC_PI_2, 1e-6);
    }

    #[test]
    fn mollweide_matches_closed_forms() {
        // Equator: y = 0, x = 2√2 λ / π.
        for lon in [-PI, -1.0, 0.0, 2.5, PI] {
            let (x, y) = project(SkyProjection::Mollweide, lon, 0.0);
            approx(x, 2.0 * SQRT_2 * lon / PI, 1e-9);
            approx(y, 0.0, 1e-9);
        }
        // Poles: (0, ±√2).
        let (x, y) = project(SkyProjection::Mollweide, 1.0, FRAC_PI_2);
        approx(x, 0.0, 1e-9);
        approx(y, SQRT_2, 1e-9);
        // Newton solution satisfies the defining equation at a generic φ.
        let phi = 1.0;
        let (_, y) = project(SkyProjection::Mollweide, 0.0, phi);
        let theta = (y / SQRT_2).asin();
        approx(2.0 * theta + (2.0 * theta).sin(), PI * phi.sin(), 1e-9);
    }

    #[test]
    fn longitudes_wrap_and_latitudes_clamp() {
        let (lon, lat) = sanitize(3.0 * PI, 2.0).expect("finite");
        approx(lon, PI, 1e-9);
        approx(lat, FRAC_PI_2, 1e-12);
        assert!(sanitize(f64::NAN, 0.0).is_none());
        assert!(sanitize(0.0, f64::INFINITY).is_none());
    }

    #[test]
    fn to_pixel_is_centered_and_fits_the_margins() {
        for projection in [SkyProjection::Aitoff, SkyProjection::Mollweide] {
            let ax = SkyAxes::new(projection);
            let (w, h) = (600.0, 300.0);
            // The center of the sky is the center of the canvas.
            let (cx, cy) = ax.to_pixel(0.0, 0.0, w, h);
            approx(cx, 300.0, 1e-9);
            approx(cy, 150.0, 1e-9);
            // The extreme points stay inside the canvas.
            for (lon, lat) in [
                (PI, 0.0),
                (-PI + 1e-9, 0.0),
                (0.0, FRAC_PI_2),
                (0.0, -FRAC_PI_2),
            ] {
                let (px, py) = ax.to_pixel(lon, lat, w, h);
                assert!(
                    px >= 0.0 && px <= w,
                    "{projection:?}: px {px} out of canvas"
                );
                assert!(
                    py >= 0.0 && py <= h,
                    "{projection:?}: py {py} out of canvas"
                );
            }
            // Symmetry: east and west equator edges mirror about the center.
            let (e, _) = ax.to_pixel(PI, 0.0, w, h);
            let (we, _) = ax.to_pixel(-PI + 1e-9, 0.0, w, h);
            approx(e - 300.0, 300.0 - we, 1e-3);
        }
    }

    #[test]
    fn render_draws_graticule_and_markers() {
        let mut ax = SkyAxes::new(SkyProjection::Mollweide);
        ax.scatter(&[0.5], &[0.4]);
        let r = ax.render_png(400, 200, 100.0);

        // Some non-white ink exists (graticule + marker).
        let px = r.pixmap();
        let mut ink = 0;
        for p in px.pixels() {
            let d = p.demultiply();
            if (d.red(), d.green(), d.blue()) != (255, 255, 255) {
                ink += 1;
            }
        }
        assert!(ink > 100, "expected graticule/marker ink, got {ink} px");
    }

    #[test]
    fn empty_plot_and_scatter_are_safe() {
        let mut ax = SkyAxes::new(SkyProjection::Aitoff);
        ax.plot(&[], &[]);
        ax.scatter(&[f64::NAN], &[0.0]);
        let r = ax.render_png(200, 100, 100.0);
        assert_eq!(r.pixmap().width(), 200);
    }

    #[test]
    fn title_renders_ink_in_the_top_band() {
        let mut ax = SkyAxes::new(SkyProjection::Mollweide);
        ax.scatter(&[0.5], &[0.4]);
        ax.set_title("mollweide");
        let r = ax.render_png(400, 200, 100.0);

        // The map is inset by MARGIN_FRAC, so the top pixel rows are blank
        // without a title; with one, glyph ink appears there.
        let px = r.pixmap();
        let mut ink = 0;
        for y in 0..18u32 {
            for x in 0..px.width() {
                let d = px.pixel(x, y).unwrap().demultiply();
                if (d.red(), d.green(), d.blue()) != (255, 255, 255) {
                    ink += 1;
                }
            }
        }
        assert!(ink > 20, "expected title ink in the top band, got {ink} px");
    }
}
