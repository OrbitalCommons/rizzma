//! 3D plotting for rizzma.
//!
//! A first-cut mplot3d-equivalent: an orthographic [`Axes3D`] that collects raw
//! 3D `(x, y, z)` data, normalizes it into a unit cube, projects every point to
//! 2D through a `(elev, azim)` view transform, and draws the result to any
//! [`Renderer`]. A light-gray wireframe of the data cube supplies the depth cue.
//!
//! What is implemented here: [`Axes3D::plot3d`] (3D polylines),
//! [`Axes3D::scatter3d`] (projected point markers), [`Axes3D::plot_surface`]
//! (flat-shaded colormapped surfaces), [`Axes3D::plot_wireframe`] (grid-line
//! wireframe surfaces), [`Axes3D::bar3d`] (cuboid bar charts colored by height),
//! and the wireframe bounding box, all depth-sorted with a basic painter's
//! algorithm. Deferred for later cuts: perspective projection,
//! 3D tick labels, surface lighting/Gouraud shading, and wiring `Axes3D` into
//! `Figure`.
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

use rizzma_core::color::viridis;
use rizzma_core::{Affine2D, Colormap, LinearNorm, Normalize, Path, color::Rgba};
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

/// A single flat-shaded surface quad: four 3D corners plus its mean-height color.
#[derive(Debug, Clone)]
struct SurfaceQuad {
    corners: [[f64; 3]; 4],
    color: Rgba,
}

/// A flat-shaded surface drawn with [`Axes3D::plot_surface`].
///
/// Each grid cell becomes one [`SurfaceQuad`], colored by its mean height through
/// a [`viridis`] colormap; quads are depth-sorted with the rest of the scene.
#[derive(Debug, Clone)]
struct Surface3D {
    quads: Vec<SurfaceQuad>,
    edge: Rgba,
}

/// A single wireframe grid edge: its two 3D endpoints.
#[derive(Debug, Clone)]
struct WireEdge {
    a: [f64; 3],
    b: [f64; 3],
}

/// A wireframe surface drawn with [`Axes3D::plot_wireframe`].
///
/// The surface's row and column grid lines are stored as individual 2-point
/// edges so each one joins the scene's painter's-algorithm depth sort by its
/// own midpoint depth (nearer edges draw over farther ones for a correct
/// wireframe look). Every edge shares one uniform `color`.
#[derive(Debug, Clone)]
struct Wireframe3D {
    edges: Vec<WireEdge>,
    color: Rgba,
}

/// A single cuboid bar drawn with [`Axes3D::bar3d`].
///
/// The bar rises from `z = 0` to `height` over a `dx`-by-`dy` footprint whose
/// near corner is `(x, y)`. Its faces are flat-shaded from `color` (top face) or
/// a darkened `color` (side faces); each face joins the scene's depth sort as a
/// [`SurfaceQuad`].
#[derive(Debug, Clone)]
struct Bar3D {
    faces: Vec<SurfaceQuad>,
    edge: Rgba,
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
    surfaces: Vec<Surface3D>,
    wireframes: Vec<Wireframe3D>,
    bars: Vec<Bar3D>,
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

/// Thin dark gray edge stroked around each surface quad so the mesh reads.
const SURFACE_EDGE_COLOR: Rgba = Rgba::new(0.15, 0.15, 0.15, 0.55);

/// Thin dark edge stroked around each bar face so the cuboid form reads.
const BAR_EDGE_COLOR: Rgba = Rgba::new(0.1, 0.1, 0.1, 0.8);

/// Multiplier applied to a bar's base color on its side faces so the vertical
/// faces read as darker than the top face and the 3D form is legible.
const BAR_SIDE_SHADE: f64 = 0.72;

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
            surfaces: Vec::new(),
            wireframes: Vec::new(),
            bars: Vec::new(),
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

    /// Add a flat-shaded surface over a regular `nx`-by-`ny` grid.
    ///
    /// `x` has length `nx`, `y` has length `ny`, and `z` has length `nx * ny` in
    /// row-major order: `z[j * nx + i]` is the height at `(x[i], y[j])`. Each grid
    /// cell `(i, j)` becomes a filled quadrilateral from its four corner heights,
    /// colored by the cell's mean height through a [`viridis`] colormap (flat
    /// shading) with a thin darker edge so the mesh reads. The quads join the
    /// rest of the scene in the painter's-algorithm depth sort.
    ///
    /// Degenerate input — a `z` length mismatch, a grid narrower than `2` in
    /// either axis, or empty slices — adds nothing and never panics.
    pub fn plot_surface(&mut self, x: &[f64], y: &[f64], z: &[f64]) -> &mut Self {
        let nx = x.len();
        let ny = y.len();
        if nx < 2 || ny < 2 || z.len() != nx * ny {
            return self;
        }

        // Height extent drives both the bounds expansion and the color norm.
        let mut zmin = f64::INFINITY;
        let mut zmax = f64::NEG_INFINITY;
        for &v in z {
            if v.is_finite() {
                zmin = zmin.min(v);
                zmax = zmax.max(v);
            }
        }

        // Expand data bounds to cover every surface vertex.
        for &xi in x {
            self.xr.expand(xi);
        }
        for &yj in y {
            self.yr.expand(yj);
        }
        for &zv in z {
            self.zr.expand(zv);
        }

        let norm = LinearNorm::new(zmin, zmax);
        let cmap = viridis();
        let at = |i: usize, j: usize| z[j * nx + i];

        let mut quads = Vec::with_capacity((nx - 1) * (ny - 1));
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let z00 = at(i, j);
                let z10 = at(i + 1, j);
                let z11 = at(i + 1, j + 1);
                let z01 = at(i, j + 1);
                let corners = [
                    [x[i], y[j], z00],
                    [x[i + 1], y[j], z10],
                    [x[i + 1], y[j + 1], z11],
                    [x[i], y[j + 1], z01],
                ];
                let mean = 0.25 * (z00 + z10 + z11 + z01);
                let color = cmap.sample(norm.normalize(mean));
                quads.push(SurfaceQuad { corners, color });
            }
        }

        self.surfaces.push(Surface3D {
            quads,
            edge: SURFACE_EDGE_COLOR,
        });
        self
    }

    /// Add a wireframe surface over a regular `nx`-by-`ny` grid.
    ///
    /// `x` has length `nx`, `y` has length `ny`, and `z` has length `nx * ny` in
    /// row-major order: `z[j * nx + i]` is the height at `(x[i], y[j])` — the same
    /// convention as [`Axes3D::plot_surface`]. The surface is drawn as its grid
    /// lines only (no fills): for each row `j` a polyline through
    /// `(x[i], y[j], z[j*nx+i])` for `i = 0..nx`, and for each column `i` a
    /// polyline through `j = 0..ny`. Every grid edge is emitted as its own 2-point
    /// segment so it joins the painter's-algorithm depth sort by its midpoint
    /// depth (nearer edges draw over farther ones). All edges share one uniform
    /// color taken from the color cycle.
    ///
    /// Degenerate input — a `z` length mismatch, a grid narrower than `2` in
    /// either axis, or empty slices — adds nothing and never panics.
    pub fn plot_wireframe(&mut self, x: &[f64], y: &[f64], z: &[f64]) -> &mut Self {
        let nx = x.len();
        let ny = y.len();
        if nx < 2 || ny < 2 || z.len() != nx * ny {
            return self;
        }

        // Expand data bounds to cover every surface vertex.
        for &xi in x {
            self.xr.expand(xi);
        }
        for &yj in y {
            self.yr.expand(yj);
        }
        for &zv in z {
            self.zr.expand(zv);
        }

        let at = |i: usize, j: usize| z[j * nx + i];
        let vertex = |i: usize, j: usize| [x[i], y[j], at(i, j)];

        let mut edges = Vec::with_capacity(nx * (ny - 1) + ny * (nx - 1));
        // Row lines: connect adjacent columns within each row.
        for j in 0..ny {
            for i in 0..nx - 1 {
                edges.push(WireEdge {
                    a: vertex(i, j),
                    b: vertex(i + 1, j),
                });
            }
        }
        // Column lines: connect adjacent rows within each column.
        for i in 0..nx {
            for j in 0..ny - 1 {
                edges.push(WireEdge {
                    a: vertex(i, j),
                    b: vertex(i, j + 1),
                });
            }
        }

        let color = self.next_color();
        self.wireframes.push(Wireframe3D { edges, color });
        self
    }

    /// Add a set of 3D bars (a cuboid bar chart).
    ///
    /// Each `(x[i], y[i])` is the near base corner of a bar that rises from
    /// `z = 0` to `z[i]` over a `dx`-by-`dy` footprint. Every bar is built as a
    /// rectangular box (cuboid) whose faces are emitted as filled quads that join
    /// the rest of the scene in the painter's-algorithm depth sort, so nearer
    /// faces correctly occlude farther ones. Each bar is colored by its height
    /// `z[i]` through a [`viridis`] colormap (top face at the base color, side
    /// faces a fixed darker shade) with thin dark edges so the 3D form reads.
    ///
    /// Only the common prefix length is used when the slices differ in length;
    /// empty input adds nothing and never panics.
    pub fn bar3d(&mut self, x: &[f64], y: &[f64], z: &[f64], dx: f64, dy: f64) -> &mut Self {
        let n = x.len().min(y.len()).min(z.len());
        if n == 0 {
            return self;
        }

        // Height extent drives both the bounds expansion and the color norm.
        let mut zmin = f64::INFINITY;
        let mut zmax = f64::NEG_INFINITY;
        for &v in z.iter().take(n) {
            if v.is_finite() {
                zmin = zmin.min(v);
                zmax = zmax.max(v);
            }
        }

        // Expand bounds over every bar's footprint and its full height (from 0).
        for i in 0..n {
            self.xr.expand(x[i]);
            self.xr.expand(x[i] + dx);
            self.yr.expand(y[i]);
            self.yr.expand(y[i] + dy);
            self.zr.expand(0.0);
            self.zr.expand(z[i]);
        }

        let norm = LinearNorm::new(zmin, zmax);
        let cmap = viridis();

        for i in 0..n {
            let base = cmap.sample(norm.normalize(z[i]));
            let faces = bar_faces([x[i], y[i]], [dx, dy], z[i], base);
            self.bars.push(Bar3D {
                faces,
                edge: BAR_EDGE_COLOR,
            });
        }
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

        // Wireframe box: 12 edges of the data cube, projected. Each edge is
        // depth-sorted with the rest of the scene by its midpoint depth, so the
        // near edges of the cage draw in front of the data and the far edges
        // behind it (a flat "always behind" bias makes the cube look detached).
        for (a, b) in cube_edges(&self.xr, &self.yr, &self.zr) {
            let (ax, ay, ad) = self.project(a[0], a[1], a[2], width, height);
            let (bx, by, bd) = self.project(b[0], b[1], b[2], width, height);
            items.push(Drawable {
                depth: 0.5 * (ad + bd),
                kind: DrawKind::Line {
                    points: vec![[ax, ay], [bx, by]],
                    color: BOX_COLOR,
                    width: 1.0,
                },
            });
        }

        for surface in &self.surfaces {
            for quad in &surface.quads {
                let mut projected = [[0.0; 2]; 4];
                let mut depth = 0.0;
                for (k, c) in quad.corners.iter().enumerate() {
                    let (px, py, d) = self.project(c[0], c[1], c[2], width, height);
                    projected[k] = [px, py];
                    depth += d;
                }
                depth *= 0.25;
                items.push(Drawable {
                    depth,
                    kind: DrawKind::Quad {
                        points: projected,
                        fill: quad.color,
                        edge: surface.edge,
                    },
                });
            }
        }

        for wire in &self.wireframes {
            for edge in &wire.edges {
                let (ax, ay, ad) = self.project(edge.a[0], edge.a[1], edge.a[2], width, height);
                let (bx, by, bd) = self.project(edge.b[0], edge.b[1], edge.b[2], width, height);
                items.push(Drawable {
                    depth: 0.5 * (ad + bd),
                    kind: DrawKind::Line {
                        points: vec![[ax, ay], [bx, by]],
                        color: wire.color,
                        width: 1.0,
                    },
                });
            }
        }

        for bar in &self.bars {
            for face in &bar.faces {
                let mut projected = [[0.0; 2]; 4];
                let mut depth = 0.0;
                for (k, c) in face.corners.iter().enumerate() {
                    let (px, py, d) = self.project(c[0], c[1], c[2], width, height);
                    projected[k] = [px, py];
                    depth += d;
                }
                depth *= 0.25;
                items.push(Drawable {
                    depth,
                    kind: DrawKind::Quad {
                        points: projected,
                        fill: face.color,
                        edge: bar.edge,
                    },
                });
            }
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
                DrawKind::Quad { points, fill, edge } => {
                    let path = Path::from_polyline(&[
                        points[0], points[1], points[2], points[3], points[0],
                    ]);
                    let gc = GraphicsContext::new()
                        .with_stroke(edge)
                        .with_line_width(0.5);
                    renderer.draw_path(&gc, &path, &Affine2D::identity(), Some(fill));
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
    Quad {
        points: [[f64; 2]; 4],
        fill: Rgba,
        edge: Rgba,
    },
    Marker {
        center: [f64; 2],
        color: Rgba,
        size: f64,
    },
}

/// Darken `c` toward black by `factor` (per RGB channel, alpha preserved).
fn shade(c: Rgba, factor: f64) -> Rgba {
    Rgba::new(c.r * factor, c.g * factor, c.b * factor, c.a)
}

/// Build the six faces of a bar's cuboid as flat-shaded [`SurfaceQuad`]s.
///
/// The box spans `(base.x, base.y, 0)` to `(base.x + d.x, base.y + d.y, height)`.
/// The top face takes `color`; the four side faces and the bottom take a fixed
/// darker shade so the vertical form reads. Each quad's corners are wound so the
/// polygon is a simple rectangle in 3D; depth sorting resolves occlusion.
fn bar_faces(base: [f64; 2], d: [f64; 2], height: f64, color: Rgba) -> Vec<SurfaceQuad> {
    let x0 = base[0];
    let y0 = base[1];
    let x1 = base[0] + d[0];
    let y1 = base[1] + d[1];
    let z0 = 0.0;
    let z1 = height;

    let side = shade(color, BAR_SIDE_SHADE);

    vec![
        // Bottom (z = z0), shaded like a side face.
        SurfaceQuad {
            corners: [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
            color: side,
        },
        // Top (z = z1), base color.
        SurfaceQuad {
            corners: [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            color,
        },
        // Front (y = y0).
        SurfaceQuad {
            corners: [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            color: side,
        },
        // Back (y = y1).
        SurfaceQuad {
            corners: [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
            color: side,
        },
        // Left (x = x0).
        SurfaceQuad {
            corners: [[x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1]],
            color: side,
        },
        // Right (x = x1).
        SurfaceQuad {
            corners: [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
            color: side,
        },
    ]
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

    /// A small ramp grid for surface tests: z increases monotonically.
    fn ramp_surface(nx: usize, ny: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let x: Vec<f64> = (0..nx).map(|i| i as f64).collect();
        let y: Vec<f64> = (0..ny).map(|j| j as f64).collect();
        let z: Vec<f64> = (0..nx * ny).map(|k| k as f64).collect();
        (x, y, z)
    }

    /// Count the quad primitives `collect_drawables` emits.
    fn quad_count(ax: &Axes3D) -> usize {
        ax.collect_drawables(W, H)
            .iter()
            .filter(|d| matches!(d.kind, DrawKind::Quad { .. }))
            .count()
    }

    /// `plot_surface` over an `nx*ny` grid emits exactly `(nx-1)*(ny-1)` quads.
    #[test]
    fn plot_surface_emits_one_quad_per_cell() {
        let (nx, ny) = (5_usize, 3_usize);
        let (x, y, z) = ramp_surface(nx, ny);
        let mut ax = Axes3D::new();
        ax.plot_surface(&x, &y, &z);
        assert_eq!(ax.surfaces[0].quads.len(), (nx - 1) * (ny - 1));
        assert_eq!(quad_count(&ax), (nx - 1) * (ny - 1));
    }

    /// The lowest- and highest-mean-height quads get distinct viridis colors.
    #[test]
    fn surface_min_and_max_quads_differ_in_color() {
        let (x, y, z) = ramp_surface(4, 4);
        let mut ax = Axes3D::new();
        ax.plot_surface(&x, &y, &z);
        let quads = &ax.surfaces[0].quads;
        // The ramp's first quad covers the smallest heights, the last the largest.
        let first = quads.first().unwrap().color;
        let last = quads.last().unwrap().color;
        assert!(
            (first.r - last.r).abs() + (first.g - last.g).abs() + (first.b - last.b).abs() > 1e-3,
            "min and max quads should map to different colors: {first:?} vs {last:?}"
        );
    }

    /// Mismatched `z` length, too-small grids, and empty slices add nothing.
    #[test]
    fn surface_rejects_degenerate_input() {
        let mut ax = Axes3D::new();
        // z length mismatch (needs 6, given 5).
        ax.plot_surface(&[0.0, 1.0, 2.0], &[0.0, 1.0], &[0.0, 1.0, 2.0, 3.0, 4.0]);
        // nx < 2.
        ax.plot_surface(&[0.0], &[0.0, 1.0], &[0.0, 1.0]);
        // ny < 2.
        ax.plot_surface(&[0.0, 1.0], &[0.0], &[0.0, 1.0]);
        // empty.
        ax.plot_surface(&[], &[], &[]);
        assert!(ax.surfaces.is_empty());
        assert_eq!(quad_count(&ax), 0);
    }

    /// `plot_surface` widens the data bounds to the surface extent.
    #[test]
    fn surface_expands_bounds() {
        let x = vec![-2.0, 0.0, 3.0];
        let y = vec![1.0, 4.0];
        let z = vec![-5.0, 0.0, 2.0, 7.0, 1.0, -1.0];
        let mut ax = Axes3D::new();
        ax.plot_surface(&x, &y, &z);
        assert_eq!((ax.xr.min, ax.xr.max), (-2.0, 3.0));
        assert_eq!((ax.yr.min, ax.yr.max), (1.0, 4.0));
        assert_eq!((ax.zr.min, ax.zr.max), (-5.0, 7.0));
    }

    /// A surface renders without panicking.
    #[test]
    fn surface_renders() {
        let (x, y, z) = ramp_surface(6, 6);
        let mut ax = Axes3D::new();
        ax.plot_surface(&x, &y, &z);
        let r = ax.render_png(128, 128, 72.0);
        assert_eq!(r.pixmap().width(), 128);
    }

    /// `bar3d` with `n` bars emits `6 * n` quad faces (six per cuboid).
    #[test]
    fn bar3d_emits_six_faces_per_bar() {
        let mut ax = Axes3D::new();
        ax.bar3d(
            &[0.0, 2.0, 4.0],
            &[0.0, 0.0, 0.0],
            &[1.0, 2.0, 3.0],
            1.0,
            1.0,
        );
        assert_eq!(ax.bars.len(), 3);
        assert_eq!(quad_count(&ax), 6 * 3);
    }

    /// A taller bar's top-face vertices project higher on screen. `project` uses
    /// the matplotlib y-up convention (origin bottom-left, `py` growing upward),
    /// so a greater height maps to a *larger* `py`.
    #[test]
    fn taller_bar_top_projects_higher() {
        let mut ax = Axes3D::new();
        // Two bars sharing a footprint origin but different heights.
        ax.bar3d(&[0.0, 0.0], &[0.0, 0.0], &[1.0, 5.0], 1.0, 1.0);
        // Top face near-corner of the short vs. tall bar.
        let (_, py_short, _) = ax.project(0.0, 0.0, 1.0, W, H);
        let (_, py_tall, _) = ax.project(0.0, 0.0, 5.0, W, H);
        assert!(
            py_tall > py_short,
            "taller bar top should project higher (larger py in y-up): {py_tall} vs {py_short}"
        );
    }

    /// Empty and ragged-to-empty input adds nothing and never panics.
    #[test]
    fn bar3d_rejects_empty_input() {
        let mut ax = Axes3D::new();
        ax.bar3d(&[], &[], &[], 1.0, 1.0);
        // Ragged where one slice is empty -> common prefix length 0.
        ax.bar3d(&[1.0, 2.0], &[], &[3.0], 1.0, 1.0);
        assert!(ax.bars.is_empty());
        assert_eq!(quad_count(&ax), 0);
        let _ = ax.render_png(32, 32, 72.0);
    }

    /// `bar3d` widens the data bounds over each bar's footprint and full height,
    /// always including `z = 0` as the floor.
    #[test]
    fn bar3d_expands_bounds_over_footprint_and_height() {
        let mut ax = Axes3D::new();
        ax.bar3d(&[2.0], &[3.0], &[5.0], 0.5, 0.75);
        assert_eq!((ax.xr.min, ax.xr.max), (2.0, 2.5));
        assert_eq!((ax.yr.min, ax.yr.max), (3.0, 3.75));
        assert_eq!((ax.zr.min, ax.zr.max), (0.0, 5.0));
    }

    /// Bars of different height receive different viridis colors.
    #[test]
    fn bar3d_colors_by_height() {
        let mut ax = Axes3D::new();
        ax.bar3d(&[0.0, 2.0], &[0.0, 0.0], &[1.0, 9.0], 1.0, 1.0);
        // Top face is the second quad emitted per bar (index 1 in bar_faces).
        let short_top = ax.bars[0].faces[1].color;
        let tall_top = ax.bars[1].faces[1].color;
        assert!(
            (short_top.r - tall_top.r).abs()
                + (short_top.g - tall_top.g).abs()
                + (short_top.b - tall_top.b).abs()
                > 1e-3,
            "bars of different height should differ in color: {short_top:?} vs {tall_top:?}"
        );
    }

    /// Side faces of a bar are darker than its top face.
    #[test]
    fn bar3d_side_faces_darker_than_top() {
        let mut ax = Axes3D::new();
        ax.bar3d(&[0.0], &[0.0], &[3.0], 1.0, 1.0);
        let top = ax.bars[0].faces[1].color;
        let front = ax.bars[0].faces[2].color;
        assert!(front.r < top.r && front.g < top.g && front.b < top.b);
    }

    /// Count the line primitives `collect_drawables` emits.
    fn line_count(ax: &Axes3D) -> usize {
        ax.collect_drawables(W, H)
            .iter()
            .filter(|d| matches!(d.kind, DrawKind::Line { .. }))
            .count()
    }

    /// `plot_wireframe` over an `nx*ny` grid emits `nx*(ny-1) + ny*(nx-1)` edges.
    #[test]
    fn wireframe_emits_expected_edge_count() {
        let (nx, ny) = (5_usize, 3_usize);
        let (x, y, z) = ramp_surface(nx, ny);
        let mut ax = Axes3D::new();
        ax.plot_wireframe(&x, &y, &z);
        let expected = nx * (ny - 1) + ny * (nx - 1);
        assert_eq!(ax.wireframes[0].edges.len(), expected);
        // Line drawables = wireframe edges + the 12 cube-box edges.
        assert_eq!(line_count(&ax), expected + 12);
    }

    /// Every wireframe edge shares one uniform color.
    #[test]
    fn wireframe_uses_uniform_color() {
        let (x, y, z) = ramp_surface(4, 4);
        let mut ax = Axes3D::new();
        ax.plot_wireframe(&x, &y, &z);
        let color = ax.wireframes[0].color;
        assert!(ax.wireframes[0].edges.len() > 1);
        for d in ax.collect_drawables(W, H) {
            if let DrawKind::Line { color: c, .. } = d.kind {
                // Skip the gray box edges; only assert on wireframe-colored lines.
                if c == color {
                    assert_eq!(c, color);
                }
            }
        }
    }

    /// Mismatched `z` length, too-small grids, and empty slices add nothing.
    #[test]
    fn wireframe_rejects_degenerate_input() {
        let mut ax = Axes3D::new();
        // z length mismatch (needs 6, given 5).
        ax.plot_wireframe(&[0.0, 1.0, 2.0], &[0.0, 1.0], &[0.0, 1.0, 2.0, 3.0, 4.0]);
        // nx < 2.
        ax.plot_wireframe(&[0.0], &[0.0, 1.0], &[0.0, 1.0]);
        // ny < 2.
        ax.plot_wireframe(&[0.0, 1.0], &[0.0], &[0.0, 1.0]);
        // empty.
        ax.plot_wireframe(&[], &[], &[]);
        assert!(ax.wireframes.is_empty());
        // Only the 12 cube-box edges remain (no data yet -> default box).
        assert_eq!(line_count(&ax), 12);
    }

    /// `plot_wireframe` widens the data bounds to the surface extent.
    #[test]
    fn wireframe_expands_bounds() {
        let x = vec![-2.0, 0.0, 3.0];
        let y = vec![1.0, 4.0];
        let z = vec![-5.0, 0.0, 2.0, 7.0, 1.0, -1.0];
        let mut ax = Axes3D::new();
        ax.plot_wireframe(&x, &y, &z);
        assert_eq!((ax.xr.min, ax.xr.max), (-2.0, 3.0));
        assert_eq!((ax.yr.min, ax.yr.max), (1.0, 4.0));
        assert_eq!((ax.zr.min, ax.zr.max), (-5.0, 7.0));
    }

    /// A wireframe renders without panicking.
    #[test]
    fn wireframe_renders() {
        let (x, y, z) = ramp_surface(6, 6);
        let mut ax = Axes3D::new();
        ax.plot_wireframe(&x, &y, &z);
        let r = ax.render_png(128, 128, 72.0);
        assert_eq!(r.pixmap().width(), 128);
    }

    /// A bar chart renders without panicking.
    #[test]
    fn bar3d_renders() {
        let mut ax = Axes3D::new();
        ax.bar3d(
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0, 2.0],
            &[1.0, 3.0, 2.0],
            0.8,
            0.8,
        );
        let r = ax.render_png(128, 128, 72.0);
        assert_eq!(r.pixmap().width(), 128);
    }
}
