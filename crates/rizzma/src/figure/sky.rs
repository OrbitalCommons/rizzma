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
use crate::core::{Affine2D, Path, PathCode};
use crate::render::{GraphicsContext, Renderer};
use crate::skia::SkiaRenderer;
use crate::text::FontSource;

/// Which all-sky projection a [`SkyAxes`] uses, or an [`Axes`](super::Axes)
/// draws through ([`Axes::set_sky_projection`](super::Axes::set_sky_projection)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyProjection {
    /// The Aitoff projection (matplotlib's `projection="aitoff"`).
    ///
    /// ![aitoff](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky_aitoff.png)
    Aitoff,
    /// The Mollweide equal-area projection (`projection="mollweide"`).
    Mollweide,
    /// The zenithal equidistant ("fisheye") view of the sky overhead: data
    /// is azimuth (from north through east) and altitude. The zenith sits at
    /// the center, the horizon on the unit circle and the radius runs
    /// linearly in altitude; north is up and east to the left, as seen
    /// looking up at the sky.
    ///
    /// ![zenithal](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky_zenithal.png)
    Zenithal,
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
/// `y ∈ [-√2, √2]`. Both are 2:1. Zenithal takes `(azimuth, altitude)`
/// and puts the sky above the horizon in the unit disk, east at `-x`.
pub(crate) fn project(projection: SkyProjection, lon: f64, lat: f64) -> (f64, f64) {
    match projection {
        SkyProjection::Zenithal => {
            let r = (FRAC_PI_2 - lat) / FRAC_PI_2;
            (-r * lon.sin(), r * lon.cos())
        }
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
pub(crate) fn native_extents(projection: SkyProjection) -> (f64, f64) {
    match projection {
        SkyProjection::Aitoff => (PI, FRAC_PI_2),
        SkyProjection::Mollweide => (2.0 * SQRT_2, SQRT_2),
        SkyProjection::Zenithal => (1.0, 1.0),
    }
}

/// Longest step, in radians of longitude or latitude, a projected straight
/// segment takes before it is subdivided.
const SEGMENT_STEP: f64 = PI / 180.0;

/// Most pieces one segment subdivides into — matplotlib's geo-axes
/// interpolation resolution, which every segment gets there.
const SEGMENT_MAX_STEPS: usize = 75;

/// Project `path` (`(lon, lat)` radians) to the native plane, subdividing
/// each straight segment — closing edges included — into steps of at most
/// one degree, so a segment follows the curved line it stands for, as
/// matplotlib's geo axes interpolate. Curve control points project as they
/// are; latitudes clamp to the poles.
pub(crate) fn project_path(projection: SkyProjection, path: &Path) -> Path {
    let vertices = path.vertices();
    let codes = path.codes();
    let mut out: Vec<([f64; 2], PathCode)> = Vec::with_capacity(vertices.len());
    let (mut start, mut prev) = (None, None);
    for (i, &v) in vertices.iter().enumerate() {
        let code = match codes {
            Some(c) => c[i],
            None if i == 0 => PathCode::MoveTo,
            None => PathCode::LineTo,
        };
        match code {
            PathCode::MoveTo => start = Some(v),
            PathCode::LineTo => {
                if let Some(a) = prev {
                    out.extend(segment_interior(a, v).map(|p| (p, PathCode::LineTo)));
                }
            }
            PathCode::ClosePoly => {
                if let (Some(a), Some(s)) = (prev, start) {
                    out.extend(segment_interior(a, s).map(|p| (p, PathCode::LineTo)));
                    out.push((s, PathCode::LineTo));
                }
            }
            PathCode::CurveTo3 | PathCode::CurveTo4 => {}
        }
        out.push((v, code));
        prev = if code == PathCode::ClosePoly {
            start
        } else {
            Some(v)
        };
    }
    let projected = out
        .iter()
        .map(|&([lon, lat], _)| {
            let (x, y) = project(projection, lon, lat.clamp(-FRAC_PI_2, FRAC_PI_2));
            [x, y]
        })
        .collect();
    // A code-less path is all line-tos, and subdividing keeps it so.
    let out_codes = codes.map(|_| out.iter().map(|&(_, code)| code).collect());
    Path::new(projected, out_codes)
}

/// The interior points of the straight segment `a`→`b` that split it into
/// steps of at most [`SEGMENT_STEP`] in either coordinate (none for a
/// short or non-finite segment).
fn segment_interior(a: [f64; 2], b: [f64; 2]) -> impl Iterator<Item = [f64; 2]> {
    let span = (b[0] - a[0]).abs().max((b[1] - a[1]).abs());
    let steps = if span.is_finite() {
        ((span / SEGMENT_STEP).ceil() as usize).clamp(1, SEGMENT_MAX_STEPS)
    } else {
        1
    };
    (1..steps).map(move |k| {
        let t = k as f64 / steps as f64;
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
    })
}

/// Invert [`project`]: the `(lon, lat)` (radians) at native point `(x, y)`.
///
/// Points outside the all-sky projections' outlines come back clamped onto
/// them; outside the zenithal horizon they come back below it. Mollweide and
/// zenithal invert in closed form; Aitoff, which has none, by Newton
/// iteration on the forward map from the equirectangular guess.
pub(crate) fn unproject(projection: SkyProjection, x: f64, y: f64) -> (f64, f64) {
    match projection {
        SkyProjection::Zenithal => {
            // atan2 lands in (-π, π]; one turn lifts it into [0, 2π).
            // (`rem_euclid` would cost the wasm bundle `fmod`.)
            let az = (-x).atan2(y);
            let az = if az < 0.0 { az + 2.0 * PI } else { az };
            (az, FRAC_PI_2 * (1.0 - x.hypot(y)))
        }
        SkyProjection::Mollweide => {
            let theta = (y / SQRT_2).clamp(-1.0, 1.0).asin();
            let lat = ((2.0 * theta + (2.0 * theta).sin()) / PI)
                .clamp(-1.0, 1.0)
                .asin();
            let cos = theta.cos();
            let lon = if cos < 1e-12 {
                0.0
            } else {
                PI * x / (2.0 * SQRT_2 * cos)
            };
            (lon.clamp(-PI, PI), lat)
        }
        SkyProjection::Aitoff => {
            let (mut lon, mut lat) = (x.clamp(-PI, PI), y.clamp(-FRAC_PI_2, FRAC_PI_2));
            for _ in 0..30 {
                let (fx, fy) = project(projection, lon, lat);
                let (ex, ey) = (fx - x, fy - y);
                if ex.hypot(ey) < 1e-12 {
                    break;
                }
                // Forward-difference Jacobian: robust at the center, where
                // the analytic one needs the sinc limit.
                let h = 1e-7;
                let (ax, ay) = project(projection, lon + h, lat);
                let (bx, by) = project(projection, lon, lat + h);
                let (j11, j21) = ((ax - fx) / h, (ay - fy) / h);
                let (j12, j22) = ((bx - fx) / h, (by - fy) / h);
                let det = j11 * j22 - j12 * j21;
                if det.abs() < 1e-15 {
                    break;
                }
                lon = (lon - (j22 * ex - j12 * ey) / det).clamp(-PI, PI);
                lat = (lat - (j11 * ey - j21 * ex) / det).clamp(-FRAC_PI_2, FRAC_PI_2);
            }
            (lon, lat)
        }
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
    /// The lowest latitude the graticule covers: the south pole, or the
    /// horizon for the zenithal view.
    fn lowest_lat_deg(&self) -> i32 {
        if self.projection == SkyProjection::Zenithal {
            0
        } else {
            -90
        }
    }

    fn graticule_line(
        &self,
        fixed: f64,
        along_lat: bool,
        width: f64,
        height: f64,
    ) -> Vec<[f64; 2]> {
        let lowest = f64::from(self.lowest_lat_deg()).to_radians();
        (0..=GRATICULE_SAMPLES)
            .map(|i| {
                let t = i as f64 / GRATICULE_SAMPLES as f64;
                let (lon, lat) = if along_lat {
                    (fixed, lowest + t * (FRAC_PI_2 - lowest))
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
        let mut lat_deg = self.lowest_lat_deg() + PARALLEL_STEP_DEG;
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

        // 2. The sky boundary: the ±π meridians joined into a closed outline,
        // or the zenithal view's horizon.
        let boundary = if self.projection == SkyProjection::Zenithal {
            self.graticule_line(0.0, false, width, height)
        } else {
            let mut boundary = self.graticule_line(PI - 1e-9, true, width, height);
            let west = self.graticule_line(-PI + 1e-9, true, width, height);
            boundary.extend(west.into_iter().rev());
            if let Some(&first) = boundary.first() {
                boundary.push(first);
            }
            boundary
        };
        Self::stroke_polyline(renderer, &boundary, SPINE_COLOR, 1.0);

        // 3. Labels: latitudes at the left boundary, longitudes along the
        // equator (matplotlib's convention for these projections).
        let mut lat_deg = self.lowest_lat_deg() + PARALLEL_STEP_DEG;
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
    fn unproject_inverts_project() {
        for projection in [SkyProjection::Aitoff, SkyProjection::Mollweide] {
            for lon_deg in (-170..=170).step_by(20) {
                for lat_deg in (-80..=80).step_by(20) {
                    let (lon, lat) = (
                        f64::from(lon_deg).to_radians(),
                        f64::from(lat_deg).to_radians(),
                    );
                    let (x, y) = project(projection, lon, lat);
                    let (lon2, lat2) = unproject(projection, x, y);
                    approx(lon2, lon, 1e-9);
                    approx(lat2, lat, 1e-9);
                }
            }
        }
    }

    #[test]
    fn zenithal_unproject_inverts_project() {
        for az_deg in (0..360).step_by(15) {
            for alt_deg in (-20..90).step_by(10) {
                let (az, alt) = (
                    f64::from(az_deg).to_radians(),
                    f64::from(alt_deg).to_radians(),
                );
                let (x, y) = project(SkyProjection::Zenithal, az, alt);
                let (az2, alt2) = unproject(SkyProjection::Zenithal, x, y);
                approx(az2, az, 1e-12);
                approx(alt2, alt, 1e-12);
            }
        }
        // Below the horizon lies outside the unit circle.
        let (x, y) = project(SkyProjection::Zenithal, 0.3, -0.2);
        assert!(x.hypot(y) > 1.0);
    }

    #[test]
    fn standalone_zenithal_draws_its_horizon() {
        let mut ax = SkyAxes::new(SkyProjection::Zenithal);
        ax.scatter(&[0.0], &[FRAC_PI_2]);
        let renderer = ax.render_png(400, 400, 100.0);
        let pm = renderer.pixmap();
        let ink = |x: u32, y: u32| pm.pixel(x, y).is_some_and(|p| p.red() < 200);
        // The zenith marker sits at the center.
        assert!(ink(200, 200));
        // The horizon is a circle: inked straight above the center, and
        // nothing in the corner beyond it.
        assert!((0..200).any(|y| ink(200, y)));
        assert!(!ink(30, 30));
    }

    #[test]
    fn project_path_subdivides_segments_and_closing_edges() {
        // A 20°-wide box: each edge splits into 1° steps, including the
        // closing edge back to the start.
        let d = PI / 180.0;
        let path = Path::new(
            vec![
                [0.0, 0.0],
                [20.0 * d, 0.0],
                [20.0 * d, 20.0 * d],
                [0.0, 0.0],
            ],
            Some(vec![
                PathCode::MoveTo,
                PathCode::LineTo,
                PathCode::LineTo,
                PathCode::ClosePoly,
            ]),
        );
        let projected = project_path(SkyProjection::Mollweide, &path);
        let codes = projected.codes().expect("codes are kept");
        // 1 move + 20 + 20 line-tos, then the 20-step diagonal close (its
        // interior, the start again, and the close itself).
        assert_eq!(projected.vertices().len(), 1 + 20 + 20 + 19 + 1 + 1);
        assert_eq!(codes.last(), Some(&PathCode::ClosePoly));
        // Every vertex lies on the projection of its subdivided point: the
        // tenth step along the bottom edge is (10°, 0).
        let (x, y) = project(SkyProjection::Mollweide, 10.0 * d, 0.0);
        approx(projected.vertices()[10][0], x, 1e-12);
        approx(projected.vertices()[10][1], y, 1e-12);
        // Code-less polylines stay code-less.
        let line = Path::from_polyline(&[[-PI, 0.5], [PI, 0.5]]);
        let projected = project_path(SkyProjection::Aitoff, &line);
        assert!(projected.codes().is_none());
        assert_eq!(projected.vertices().len(), 1 + SEGMENT_MAX_STEPS);
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
