//! 3D plotting for rizzma.
//!
//! A first-cut mplot3d-equivalent: an orthographic [`Axes3D`] that collects raw
//! 3D `(x, y, z)` data, normalizes it into a unit cube, projects every point to
//! 2D through a `(elev, azim)` view transform, and draws the result to any
//! [`Renderer`]. A light-gray wireframe of the data cube supplies the depth cue.
//!
//! What is implemented here: [`Axes3D::plot3d`] (3D polylines),
//! [`Axes3D::scatter3d`] (projected point markers), and the wireframe bounding
//! box, all depth-sorted with a basic painter's algorithm. Deferred for later
//! cuts: perspective projection, 3D tick labels, surfaces/bar3d, and wiring
//! `Axes3D` into `Figure`.
//!
//! ```
//! use rizzma_3d::Axes3D;
//!
//! let t: Vec<f64> = (0..200).map(|i| i as f64 * 0.1).collect();
//! let x: Vec<f64> = t.iter().map(|t| t.cos()).collect();
//! let y: Vec<f64> = t.iter().map(|t| t.sin()).collect();
//! let mut ax = Axes3D::new();
//! ax.plot3d(&x, &y, &t);
//! let renderer = ax.render_png(600, 600, 100.0);
//! assert_eq!(renderer.pixmap().width(), 600);
//! ```
//!
//! Build-order home: Phase 11 (its own epic) of `design/04-implementation-plan.md`.

use rizzma_core::{Affine2D, Path, color::Rgba};
use rizzma_render::{GraphicsContext, Renderer};
use rizzma_skia::SkiaRenderer;

/// An inclusive min/max span along one data axis.
///
/// Initialized empty (`min = +inf`, `max = -inf`) so the first [`Range::expand`]
/// adopts the value exactly; [`Range::span`] guards the degenerate zero-width
/// case so normalization never divides by zero.
#[derive(Debug, Clone, Copy)]
struct Range {
    min: f64,
    max: f64,
}

impl Range {
    /// An empty range that any finite value widens.
    fn empty() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    /// Widen the range to include `v` (ignoring non-finite values).
    fn expand(&mut self, v: f64) {
        if v.is_finite() {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
    }

    /// The midpoint, or `0.0` when the range is still empty.
    fn mid(&self) -> f64 {
        if self.min <= self.max {
            0.5 * (self.min + self.max)
        } else {
            0.0
        }
    }

    /// The width, guarded to `1.0` for empty or zero-width ranges so that
    /// normalization (`(v - mid) / span`) stays finite for degenerate data.
    fn span(&self) -> f64 {
        let s = self.max - self.min;
        if s.is_finite() && s > 0.0 { s } else { 1.0 }
    }
}

/// A 3D polyline drawn with [`Axes3D::plot3d`].
#[derive(Debug, Clone)]
struct Line3D {
    points: Vec<[f64; 3]>,
    color: Rgba,
    width: f64,
}

/// A cloud of 3D points drawn with [`Axes3D::scatter3d`].
#[derive(Debug, Clone)]
struct Scatter3D {
    points: Vec<[f64; 3]>,
    color: Rgba,
    size: f64,
}

/// An orthographic 3D axes that projects collected `(x, y, z)` data to 2D.
///
/// Data is accumulated in raw coordinates; the data bounds and the `(elev,
/// azim)` view are resolved at draw time. Construct with [`Axes3D::new`], orient
/// with [`Axes3D::with_view`], add geometry with [`Axes3D::plot3d`] /
/// [`Axes3D::scatter3d`], then rasterize with [`Axes3D::render_png`] or
/// [`Axes3D::save_png`].
#[derive(Debug, Clone)]
pub struct Axes3D {
    elev_deg: f64,
    azim_deg: f64,
    xr: Range,
    yr: Range,
    zr: Range,
    lines: Vec<Line3D>,
    scatters: Vec<Scatter3D>,
    /// Color cycle index for the next auto-colored artist.
    cycle: usize,
}

/// The default matplotlib-ish color cycle (tableau palette).
const COLOR_CYCLE: &[Rgba] = &[
    Rgba::new(0.121, 0.466, 0.705, 1.0), // tab:blue
    Rgba::new(1.0, 0.498, 0.054, 1.0),   // tab:orange
    Rgba::new(0.172, 0.627, 0.172, 1.0), // tab:green
    Rgba::new(0.839, 0.152, 0.156, 1.0), // tab:red
    Rgba::new(0.580, 0.403, 0.741, 1.0), // tab:purple
];

/// Light gray used for the wireframe bounding box.
const BOX_COLOR: Rgba = Rgba::new(0.7, 0.7, 0.7, 1.0);

/// Fractional pixel margin reserved around the projected cube on every side.
const MARGIN_FRAC: f64 = 0.12;

impl Axes3D {
    /// Create an empty axes with the default view (`elev = 30°`, `azim = -60°`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            elev_deg: 30.0,
            azim_deg: -60.0,
            xr: Range::empty(),
            yr: Range::empty(),
            zr: Range::empty(),
            lines: Vec::new(),
            scatters: Vec::new(),
            cycle: 0,
        }
    }

    /// Set the elevation and azimuth (degrees), returning `self` for chaining.
    #[must_use]
    pub fn with_view(mut self, elev: f64, azim: f64) -> Self {
        self.elev_deg = elev;
        self.azim_deg = azim;
        self
    }

    /// The current `(elev, azim)` view angles in degrees.
    #[must_use]
    pub fn view(&self) -> (f64, f64) {
        (self.elev_deg, self.azim_deg)
    }

    /// The next color from the cycle, advancing the cursor.
    fn next_color(&mut self) -> Rgba {
        let c = COLOR_CYCLE[self.cycle % COLOR_CYCLE.len()];
        self.cycle += 1;
        c
    }

    /// Widen the data bounds to include every `(x, y, z)` triple.
    fn expand_bounds(&mut self, x: &[f64], y: &[f64], z: &[f64]) {
        let n = x.len().min(y.len()).min(z.len());
        for i in 0..n {
            self.xr.expand(x[i]);
            self.yr.expand(y[i]);
            self.zr.expand(z[i]);
        }
    }

    /// Add a 3D polyline through the parallel `x`/`y`/`z` samples.
    ///
    /// Only the common prefix length is used when the slices differ in length.
    /// The line takes the next color from the cycle.
    pub fn plot3d(&mut self, x: &[f64], y: &[f64], z: &[f64]) -> &mut Self {
        self.expand_bounds(x, y, z);
        let n = x.len().min(y.len()).min(z.len());
        let points = (0..n).map(|i| [x[i], y[i], z[i]]).collect();
        let color = self.next_color();
        self.lines.push(Line3D {
            points,
            color,
            width: 1.5,
        });
        self
    }

    /// Add a cloud of 3D scatter markers at the `x`/`y`/`z` samples.
    ///
    /// Only the common prefix length is used when the slices differ in length.
    /// The markers take the next color from the cycle.
    pub fn scatter3d(&mut self, x: &[f64], y: &[f64], z: &[f64]) -> &mut Self {
        self.expand_bounds(x, y, z);
        let n = x.len().min(y.len()).min(z.len());
        let points = (0..n).map(|i| [x[i], y[i], z[i]]).collect();
        let color = self.next_color();
        self.scatters.push(Scatter3D {
            points,
            color,
            size: 5.0,
        });
        self
    }

    /// Normalize a raw `(x, y, z)` point into the unit cube centered on the
    /// origin (each coordinate roughly in `[-0.5, 0.5]`).
    fn normalize(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        let nx = (x - self.xr.mid()) / self.xr.span();
        let ny = (y - self.yr.mid()) / self.yr.span();
        let nz = (z - self.zr.mid()) / self.zr.span();
        (nx, ny, nz)
    }

    /// Project a raw 3D data point to `(px, py, depth)`.
    ///
    /// `px`/`py` are pixel coordinates inside `(width, height)` with the
    /// matplotlib convention (origin bottom-left, y growing upward). `depth`
    /// increases toward the viewer and is used for painter's-algorithm ordering.
    ///
    /// The point is first normalized into the centered unit cube, then rotated by
    /// the azimuth and tilted by the elevation:
    ///
    /// ```text
    /// x2 =  nx*cos(a) - ny*sin(a)
    /// y2 =  nx*sin(a) + ny*cos(a)
    /// screen_x =  x2
    /// screen_y = -y2*sin(e) + nz*cos(e)
    /// depth    =  y2*cos(e) + nz*sin(e)
    /// ```
    ///
    /// `(screen_x, screen_y)` (roughly in `[-1, 1]`) is mapped into the pixel
    /// rectangle keeping a square aspect so the cube is not skewed.
    #[must_use]
    pub fn project(&self, x: f64, y: f64, z: f64, width: f64, height: f64) -> (f64, f64, f64) {
        let (nx, ny, nz) = self.normalize(x, y, z);
        let a = self.azim_deg.to_radians();
        let e = self.elev_deg.to_radians();

        let x2 = nx * a.cos() - ny * a.sin();
        let y2 = nx * a.sin() + ny * a.cos();

        let screen_x = x2;
        let screen_y = -y2 * e.sin() + nz * e.cos();
        let depth = y2 * e.cos() + nz * e.sin();

        // The normalized cube projects into roughly [-sqrt(3)/2, sqrt(3)/2]; map
        // that span into the smaller pixel dimension, keeping aspect square.
        let side = width.min(height);
        let usable = side * (1.0 - 2.0 * MARGIN_FRAC);
        // Half-extent of the projected cube: a cube of side 1 has half-diagonal
        // sqrt(3)/2 ~= 0.866; use that as the worst-case bound.
        let half = 0.866_025_4_f64;
        let scale = 0.5 * usable / half;

        let px = 0.5 * width + screen_x * scale;
        let py = 0.5 * height + screen_y * scale;
        (px, py, depth)
    }

    /// Collect every primitive in pixel space, depth-sorted back-to-front.
    fn collect_drawables(&self, width: f64, height: f64) -> Vec<Drawable> {
        let mut items: Vec<Drawable> = Vec::new();

        // Wireframe box: 12 edges of the data cube, projected. The box is biased
        // behind the data so the spiral always reads in front of its cage.
        for (a, b) in cube_edges(&self.xr, &self.yr, &self.zr) {
            let (ax, ay, ad) = self.project(a[0], a[1], a[2], width, height);
            let (bx, by, bd) = self.project(b[0], b[1], b[2], width, height);
            items.push(Drawable {
                depth: 0.5 * (ad + bd) - 1e6,
                kind: DrawKind::Line {
                    points: vec![[ax, ay], [bx, by]],
                    color: BOX_COLOR,
                    width: 1.0,
                },
            });
        }

        for line in &self.lines {
            let projected: Vec<[f64; 2]> = line
                .points
                .iter()
                .map(|p| {
                    let (px, py, _) = self.project(p[0], p[1], p[2], width, height);
                    [px, py]
                })
                .collect();
            let depth = if line.points.is_empty() {
                0.0
            } else {
                line.points
                    .iter()
                    .map(|p| self.project(p[0], p[1], p[2], width, height).2)
                    .sum::<f64>()
                    / line.points.len() as f64
            };
            items.push(Drawable {
                depth,
                kind: DrawKind::Line {
                    points: projected,
                    color: line.color,
                    width: line.width,
                },
            });
        }

        for sc in &self.scatters {
            for p in &sc.points {
                let (px, py, depth) = self.project(p[0], p[1], p[2], width, height);
                items.push(Drawable {
                    depth,
                    kind: DrawKind::Marker {
                        center: [px, py],
                        color: sc.color,
                        size: sc.size,
                    },
                });
            }
        }

        // Painter's algorithm: smaller depth (farther from viewer) drawn first.
        items.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        items
    }

    /// Draw the wireframe box and all data to `renderer`, back-to-front.
    pub fn draw(&self, renderer: &mut dyn Renderer) {
        let (width, height) = renderer.canvas_size();
        let dpi = renderer.points_to_pixels(72.0) / 72.0;
        for item in self.collect_drawables(width, height) {
            match item.kind {
                DrawKind::Line {
                    points,
                    color,
                    width: lw,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    let path = Path::from_polyline(&points);
                    let gc = GraphicsContext::new()
                        .with_stroke(color)
                        .with_line_width(lw);
                    renderer.draw_path(&gc, &path, &Affine2D::identity(), None);
                }
                DrawKind::Marker {
                    center,
                    color,
                    size,
                } => {
                    let r = size * dpi;
                    let marker = Path::unit_circle()
                        .transformed(&Affine2D::from_scale(r, r))
                        .transformed(&Affine2D::from_translation(center[0], center[1]));
                    let gc = GraphicsContext::new()
                        .with_stroke(color)
                        .with_line_width(1.0);
                    renderer.draw_path(&gc, &marker, &Affine2D::identity(), Some(color));
                }
            }
        }
    }

    /// Render the axes to a fresh white-backed [`SkiaRenderer`].
    #[must_use]
    pub fn render_png(&self, width_px: u32, height_px: u32, dpi: f64) -> SkiaRenderer {
        let mut renderer = SkiaRenderer::new(width_px, height_px, dpi);
        // White background filling the whole canvas.
        let bg = Path::from_polyline(&[
            [0.0, 0.0],
            [f64::from(width_px), 0.0],
            [f64::from(width_px), f64::from(height_px)],
            [0.0, f64::from(height_px)],
            [0.0, 0.0],
        ]);
        let gc = GraphicsContext::new();
        renderer.draw_path(&gc, &bg, &Affine2D::identity(), Some(Rgba::WHITE));
        self.draw(&mut renderer);
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
    ) -> Result<(), rizzma_skia::PngError> {
        self.render_png(width_px, height_px, dpi).save_png(path)
    }
}

impl Default for Axes3D {
    fn default() -> Self {
        Self::new()
    }
}

/// A projected, depth-tagged primitive awaiting rasterization.
struct Drawable {
    depth: f64,
    kind: DrawKind,
}

/// The geometry variant of a [`Drawable`], already in pixel space.
enum DrawKind {
    Line {
        points: Vec<[f64; 2]>,
        color: Rgba,
        width: f64,
    },
    Marker {
        center: [f64; 2],
        color: Rgba,
        size: f64,
    },
}

/// The 12 edges of the data cube as raw `(from, to)` corner pairs.
///
/// Corners are taken at the data min/max along each axis so the projected box
/// hugs the real data extents.
fn cube_edges(xr: &Range, yr: &Range, zr: &Range) -> Vec<([f64; 3], [f64; 3])> {
    let xs = bounds(xr);
    let ys = bounds(yr);
    let zs = bounds(zr);
    let corner = |i: usize, j: usize, k: usize| [xs[i], ys[j], zs[k]];

    let mut edges = Vec::with_capacity(12);
    // Edges along x.
    for j in 0..2 {
        for k in 0..2 {
            edges.push((corner(0, j, k), corner(1, j, k)));
        }
    }
    // Edges along y.
    for i in 0..2 {
        for k in 0..2 {
            edges.push((corner(i, 0, k), corner(i, 1, k)));
        }
    }
    // Edges along z.
    for i in 0..2 {
        for j in 0..2 {
            edges.push((corner(i, j, 0), corner(i, j, 1)));
        }
    }
    edges
}

/// The `[min, max]` bounds of a range, falling back to `[-0.5, 0.5]` around the
/// midpoint for empty or zero-width ranges so the box stays visible.
fn bounds(r: &Range) -> [f64; 2] {
    if r.min < r.max {
        [r.min, r.max]
    } else {
        let m = r.mid();
        [m - 0.5, m + 0.5]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 400.0;
    const H: f64 = 400.0;

    /// The eight cube corners project to eight distinct, finite 2D points with
    /// finite depths.
    #[test]
    fn cube_corners_project_to_distinct_points() {
        let mut ax = Axes3D::new();
        ax.plot3d(&[-1.0, 1.0], &[-1.0, 1.0], &[-1.0, 1.0]);

        let mut pts = Vec::new();
        for &x in &[-1.0, 1.0] {
            for &y in &[-1.0, 1.0] {
                for &z in &[-1.0, 1.0] {
                    let (px, py, depth) = ax.project(x, y, z, W, H);
                    assert!(px.is_finite() && py.is_finite() && depth.is_finite());
                    pts.push((px, py));
                }
            }
        }
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let dx = pts[i].0 - pts[j].0;
                let dy = pts[i].1 - pts[j].1;
                assert!(
                    dx.hypot(dy) > 1e-6,
                    "corners {i} and {j} projected to the same point"
                );
            }
        }
    }

    /// Under the default view a point nearer the viewer (larger z) has a greater
    /// projected depth than one farther away.
    #[test]
    fn larger_z_yields_greater_depth() {
        let mut ax = Axes3D::new();
        ax.plot3d(&[0.0, 0.0], &[0.0, 0.0], &[-1.0, 1.0]);
        let (_, _, d_low) = ax.project(0.0, 0.0, -1.0, W, H);
        let (_, _, d_high) = ax.project(0.0, 0.0, 1.0, W, H);
        assert!(
            d_high > d_low,
            "expected larger z to be nearer the viewer: {d_high} vs {d_low}"
        );
    }

    /// `plot3d` and `scatter3d` both widen the data bounds.
    #[test]
    fn plot_and_scatter_expand_bounds() {
        let mut ax = Axes3D::new();
        ax.plot3d(&[0.0, 2.0], &[0.0, 4.0], &[0.0, 6.0]);
        assert_eq!((ax.xr.min, ax.xr.max), (0.0, 2.0));
        assert_eq!((ax.yr.min, ax.yr.max), (0.0, 4.0));
        assert_eq!((ax.zr.min, ax.zr.max), (0.0, 6.0));

        ax.scatter3d(&[-3.0, 5.0], &[-1.0, 9.0], &[-2.0, 8.0]);
        assert_eq!((ax.xr.min, ax.xr.max), (-3.0, 5.0));
        assert_eq!((ax.yr.min, ax.yr.max), (-1.0, 9.0));
        assert_eq!((ax.zr.min, ax.zr.max), (-2.0, 8.0));
    }

    /// Degenerate data (single point / zero span) must not panic and must yield
    /// finite projected coordinates thanks to guarded normalization.
    #[test]
    fn degenerate_data_does_not_panic() {
        let mut ax = Axes3D::new();
        ax.scatter3d(&[1.0], &[1.0], &[1.0]);
        let (px, py, depth) = ax.project(1.0, 1.0, 1.0, W, H);
        assert!(px.is_finite() && py.is_finite() && depth.is_finite());
        let _ = ax.render_png(64, 64, 72.0);
    }

    /// An empty axes renders without panicking.
    #[test]
    fn empty_axes_renders() {
        let ax = Axes3D::new();
        let r = ax.render_png(32, 32, 72.0);
        assert_eq!(r.pixmap().width(), 32);
    }

    /// The default view leaves the configured angles intact and `with_view`
    /// overrides them.
    #[test]
    fn view_angles() {
        assert_eq!(Axes3D::new().view(), (30.0, -60.0));
        assert_eq!(Axes3D::new().with_view(45.0, 10.0).view(), (45.0, 10.0));
    }
}
