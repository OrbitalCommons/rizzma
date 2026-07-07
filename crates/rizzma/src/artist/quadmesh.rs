//! The [`QuadMesh`] artist: a colormapped quadrilateral mesh over a regular grid
//! (`pcolormesh`).
//!
//! Mirrors matplotlib's `QuadMesh`. A regular `nrows x ncols` grid of cells is
//! described by `(nrows + 1) * (ncols + 1)` corner coordinates (row-major) and
//! one face color per cell. Each cell is drawn as a filled, closed quadrilateral
//! path through its four corner grid points, optionally stroked with a shared
//! edge color.

use crate::core::{Affine2D, Bbox, Path, color::Rgba};
use crate::render::{GraphicsContext, Renderer};

use crate::artist::Artist;

/// A colormapped quadrilateral mesh over a regular grid.
///
/// Construct with [`QuadMesh::new`] from a row-major grid of corner
/// `coordinates` (length `(nrows + 1) * (ncols + 1)`) and per-cell `facecolors`
/// (length `nrows * ncols`). Cell `(r, c)` is the quad through grid corners
/// `(r, c)`, `(r, c + 1)`, `(r + 1, c + 1)`, `(r + 1, c)`. Defaults: no edge,
/// zorder `0.0` (so the mesh draws beneath lines/patches), and visible.
#[derive(Debug, Clone, PartialEq)]
pub struct QuadMesh {
    /// Number of cell rows.
    nrows: usize,
    /// Number of cell columns.
    ncols: usize,
    /// Grid corner coordinates in data space, row-major, length
    /// `(nrows + 1) * (ncols + 1)`.
    coordinates: Vec<[f64; 2]>,
    /// Per-cell fill colors, row-major, length `nrows * ncols`.
    facecolors: Vec<Rgba>,
    /// Per-*corner* colors for gouraud (smooth) shading, row-major, length
    /// `(nrows + 1) * (ncols + 1)`. When set, cells are drawn as
    /// color-interpolated triangles and `facecolors`/`edgecolor` are ignored.
    vertex_colors: Option<Vec<Rgba>>,
    /// Optional shared edge (stroke) color; `None` means no edge is drawn.
    edgecolor: Option<Rgba>,
    /// Edge width in points (used only when `edgecolor` is set).
    linewidth: f64,
    /// Stacking order; higher draws on top. Defaults to `0.0`.
    zorder: f64,
    /// Whether the mesh is drawn.
    visible: bool,
}

impl QuadMesh {
    /// Construct a [`QuadMesh`] from a regular grid.
    ///
    /// `coordinates` is the row-major list of grid corners, length
    /// `(nrows + 1) * (ncols + 1)`, and `facecolors` is one fill color per cell,
    /// row-major, length `nrows * ncols`.
    ///
    /// # Panics
    ///
    /// Panics if `coordinates.len()` is not `(nrows + 1) * (ncols + 1)` or
    /// `facecolors.len()` is not `nrows * ncols`.
    #[must_use]
    pub fn new(
        nrows: usize,
        ncols: usize,
        coordinates: Vec<[f64; 2]>,
        facecolors: Vec<Rgba>,
    ) -> Self {
        let expected_coords = (nrows + 1) * (ncols + 1);
        assert_eq!(
            coordinates.len(),
            expected_coords,
            "QuadMesh: coordinates length {} must equal (nrows + 1) * (ncols + 1) = {}",
            coordinates.len(),
            expected_coords
        );
        assert_eq!(
            facecolors.len(),
            nrows * ncols,
            "QuadMesh: facecolors length {} must equal nrows * ncols = {}",
            facecolors.len(),
            nrows * ncols
        );
        Self {
            nrows,
            ncols,
            coordinates,
            facecolors,
            vertex_colors: None,
            edgecolor: None,
            linewidth: 1.0,
            zorder: 0.0,
            visible: true,
        }
    }

    /// Switch to gouraud (smooth) shading with one color per grid *corner*,
    /// row-major, length `(nrows + 1) * (ncols + 1)`, returning `self`.
    ///
    /// Colors are barycentrically interpolated across each cell (matplotlib's
    /// `shading="gouraud"`), approximated by recursive triangle subdivision
    /// until the corner colors of a patch agree to ~1 LSB. Gouraud cells draw
    /// no edges.
    ///
    /// # Panics
    ///
    /// Panics if `vertex_colors.len()` is not `(nrows + 1) * (ncols + 1)`.
    #[must_use]
    pub fn with_vertex_colors(mut self, vertex_colors: Vec<Rgba>) -> Self {
        let expected = (self.nrows + 1) * (self.ncols + 1);
        assert_eq!(
            vertex_colors.len(),
            expected,
            "QuadMesh: vertex_colors length {} must equal (nrows + 1) * (ncols + 1) = {}",
            vertex_colors.len(),
            expected
        );
        self.vertex_colors = Some(vertex_colors);
        self
    }

    /// Set the shared edge color (or `None` for no edge), returning `self`.
    #[must_use]
    pub fn with_edgecolor(mut self, edgecolor: Option<Rgba>) -> Self {
        self.edgecolor = edgecolor;
        self
    }

    /// Set the edge width in points, returning `self`.
    #[must_use]
    pub fn with_linewidth(mut self, linewidth: f64) -> Self {
        self.linewidth = linewidth;
        self
    }

    /// Set the stacking order, returning `self`.
    #[must_use]
    pub fn with_zorder(mut self, zorder: f64) -> Self {
        self.zorder = zorder;
        self
    }

    /// Set whether the mesh is drawn, returning `self`.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// The grid corner at row `r`, column `c` (`0 <= r <= nrows`,
    /// `0 <= c <= ncols`).
    fn corner(&self, r: usize, c: usize) -> [f64; 2] {
        self.coordinates[r * (self.ncols + 1) + c]
    }

    /// The fill color of cell `(r, c)`.
    fn facecolor(&self, r: usize, c: usize) -> Rgba {
        self.facecolors[r * self.ncols + c]
    }

    /// The gouraud corner color at grid point `(r, c)`.
    fn vertex_color(&self, colors: &[Rgba], r: usize, c: usize) -> Rgba {
        colors[r * (self.ncols + 1) + c]
    }
}

/// Maximum recursive subdivision depth for a gouraud triangle (each level
/// quarters the triangle: depth 3 caps a cell at `2 * 4^3 = 128` patches).
const GOURAUD_MAX_DEPTH: usize = 3;

/// Corner colors closer than this per channel are treated as flat (~2 LSB of
/// 8-bit output).
const GOURAUD_FLAT_EPS: f64 = 2.0 / 255.0;

/// Average of three colors, channel-wise.
fn mix3(a: Rgba, b: Rgba, c: Rgba) -> Rgba {
    Rgba::new(
        (a.r + b.r + c.r) / 3.0,
        (a.g + b.g + c.g) / 3.0,
        (a.b + b.b + c.b) / 3.0,
        (a.a + b.a + c.a) / 3.0,
    )
}

/// Midpoint of two colors, channel-wise.
fn mix2(a: Rgba, b: Rgba) -> Rgba {
    Rgba::new(
        (a.r + b.r) / 2.0,
        (a.g + b.g) / 2.0,
        (a.b + b.b) / 2.0,
        (a.a + b.a) / 2.0,
    )
}

/// Largest per-channel difference among three colors.
fn color_spread(a: Rgba, b: Rgba, c: Rgba) -> f64 {
    let ch = |f: fn(&Rgba) -> f64| {
        let (lo, hi) = [f(&a), f(&b), f(&c)]
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                (lo.min(v), hi.max(v))
            });
        hi - lo
    };
    ch(|c| c.r)
        .max(ch(|c| c.g))
        .max(ch(|c| c.b))
        .max(ch(|c| c.a))
}

/// Fill one gouraud triangle by recursive subdivision: when the corner colors
/// agree within [`GOURAUD_FLAT_EPS`] (or the depth budget is spent) the
/// triangle is flat-filled with the average color; otherwise it splits into
/// four at the edge midpoints, interpolating colors alongside positions.
fn draw_gouraud_triangle(
    renderer: &mut dyn Renderer,
    transform: &Affine2D,
    pts: [[f64; 2]; 3],
    cols: [Rgba; 3],
    depth: usize,
) {
    if depth == 0 || color_spread(cols[0], cols[1], cols[2]) <= GOURAUD_FLAT_EPS {
        let path = Path::from_polyline(&[pts[0], pts[1], pts[2], pts[0]]);
        let fill = mix3(cols[0], cols[1], cols[2]);
        // Stroke with the fill color so adjacent triangles' antialiased edges
        // don't leave background-colored seams (same trick as tripcolor).
        let gc = GraphicsContext::new()
            .with_stroke(fill)
            .with_line_width(0.5);
        renderer.draw_path(&gc, &path, transform, Some(fill));
        return;
    }
    let mid = |i: usize, j: usize| [(pts[i][0] + pts[j][0]) / 2.0, (pts[i][1] + pts[j][1]) / 2.0];
    let (m01, m12, m20) = (mid(0, 1), mid(1, 2), mid(2, 0));
    let (c01, c12, c20) = (
        mix2(cols[0], cols[1]),
        mix2(cols[1], cols[2]),
        mix2(cols[2], cols[0]),
    );
    let d = depth - 1;
    draw_gouraud_triangle(
        renderer,
        transform,
        [pts[0], m01, m20],
        [cols[0], c01, c20],
        d,
    );
    draw_gouraud_triangle(
        renderer,
        transform,
        [m01, pts[1], m12],
        [c01, cols[1], c12],
        d,
    );
    draw_gouraud_triangle(
        renderer,
        transform,
        [m20, m12, pts[2]],
        [c20, c12, cols[2]],
        d,
    );
    draw_gouraud_triangle(renderer, transform, [m01, m12, m20], [c01, c12, c20], d);
}

impl Artist for QuadMesh {
    /// Draw each cell as a filled, closed quadrilateral.
    ///
    /// For cell `(r, c)` a four-corner closed [`Path`] is built from grid points
    /// `(r, c)`, `(r, c + 1)`, `(r + 1, c + 1)`, `(r + 1, c)` and filled with the
    /// cell's face color, stroked with `edgecolor` when set.
    ///
    // TODO: this fans out to one `draw_path` per cell, which is correctness-first
    // but O(cells) calls. A future perf path is a single batched
    // `draw_path_collection` once the renderer grows one.
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        if !self.visible {
            return;
        }
        if let Some(colors) = &self.vertex_colors {
            // Gouraud: each cell splits into two triangles whose corner colors
            // interpolate smoothly; no edges are stroked.
            for r in 0..self.nrows {
                for c in 0..self.ncols {
                    let (p00, p01) = (self.corner(r, c), self.corner(r, c + 1));
                    let (p11, p10) = (self.corner(r + 1, c + 1), self.corner(r + 1, c));
                    let (c00, c01) = (
                        self.vertex_color(colors, r, c),
                        self.vertex_color(colors, r, c + 1),
                    );
                    let (c11, c10) = (
                        self.vertex_color(colors, r + 1, c + 1),
                        self.vertex_color(colors, r + 1, c),
                    );
                    draw_gouraud_triangle(
                        renderer,
                        transform,
                        [p00, p01, p11],
                        [c00, c01, c11],
                        GOURAUD_MAX_DEPTH,
                    );
                    draw_gouraud_triangle(
                        renderer,
                        transform,
                        [p00, p11, p10],
                        [c00, c11, c10],
                        GOURAUD_MAX_DEPTH,
                    );
                }
            }
            return;
        }
        let gc = GraphicsContext {
            line_width: self.linewidth,
            stroke: self.edgecolor,
            ..GraphicsContext::new()
        };
        for r in 0..self.nrows {
            for c in 0..self.ncols {
                let quad = Path::from_polyline(&[
                    self.corner(r, c),
                    self.corner(r, c + 1),
                    self.corner(r + 1, c + 1),
                    self.corner(r + 1, c),
                    self.corner(r, c),
                ]);
                renderer.draw_path(&gc, &quad, transform, Some(self.facecolor(r, c)));
            }
        }
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        let mut any = false;
        for &[x, y] in &self.coordinates {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x);
            ymax = ymax.max(y);
            any = true;
        }
        if any {
            Some(Bbox::from_extents(xmin, ymin, xmax, ymax))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that records, per `draw_path` call, the path's vertex
    /// count and the fill (face) and stroke colors.
    #[derive(Default)]
    struct MockRenderer {
        calls: Vec<Call>,
    }

    /// One recorded `draw_path` invocation.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Call {
        vertices: usize,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
    }

    impl Renderer for MockRenderer {
        fn draw_path(
            &mut self,
            gc: &GraphicsContext,
            path: &Path,
            _transform: &Affine2D,
            fill: Option<Rgba>,
        ) {
            self.calls.push(Call {
                vertices: path.vertices().len(),
                fill,
                stroke: gc.stroke,
            });
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    /// A unit `nrows x ncols` grid (x = 0..=ncols, y = 0..=nrows), row-major.
    fn unit_grid(nrows: usize, ncols: usize) -> Vec<[f64; 2]> {
        let mut coords = Vec::with_capacity((nrows + 1) * (ncols + 1));
        for r in 0..=nrows {
            for c in 0..=ncols {
                coords.push([c as f64, r as f64]);
            }
        }
        coords
    }

    #[test]
    fn draws_one_quad_per_cell() {
        let (nrows, ncols) = (3, 4);
        let mesh = QuadMesh::new(
            nrows,
            ncols,
            unit_grid(nrows, ncols),
            vec![Rgba::RED; nrows * ncols],
        );
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls.len(), nrows * ncols);
        // Each quad is a closed 5-vertex polyline, filled red.
        for call in &r.calls {
            assert_eq!(call.vertices, 5);
            assert_eq!(call.fill, Some(Rgba::RED));
        }
    }

    #[test]
    fn per_cell_facecolors_apply_in_row_major_order() {
        let mesh = QuadMesh::new(1, 2, unit_grid(1, 2), vec![Rgba::RED, Rgba::GREEN]);
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        let fills: Vec<_> = r.calls.iter().map(|c| c.fill).collect();
        assert_eq!(fills, vec![Some(Rgba::RED), Some(Rgba::GREEN)]);
    }

    #[test]
    fn no_edgecolor_means_no_stroke() {
        let mesh = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED]);
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls[0].stroke, None);
    }

    #[test]
    fn edgecolor_drives_the_stroke() {
        let mesh =
            QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED]).with_edgecolor(Some(Rgba::BLACK));
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls[0].stroke, Some(Rgba::BLACK));
    }

    #[test]
    fn data_extents_match_the_grid() {
        let (nrows, ncols) = (2, 3);
        let mesh = QuadMesh::new(
            nrows,
            ncols,
            unit_grid(nrows, ncols),
            vec![Rgba::RED; nrows * ncols],
        );
        let e = mesh.data_extents().expect("non-empty grid");
        assert_eq!((e.xmin(), e.xmax()), (0.0, ncols as f64));
        assert_eq!((e.ymin(), e.ymax()), (0.0, nrows as f64));
    }

    #[test]
    fn invisible_mesh_draws_nothing() {
        let mesh = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED]).with_visible(false);
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert!(r.calls.is_empty());
    }

    #[test]
    fn default_zorder_is_zero() {
        let mesh = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED]);
        assert_eq!(Artist::zorder(&mesh), 0.0);
    }

    #[test]
    #[should_panic(expected = "coordinates length")]
    fn wrong_coordinate_count_panics() {
        let _ = QuadMesh::new(1, 1, vec![[0.0, 0.0]], vec![Rgba::RED]);
    }

    #[test]
    #[should_panic(expected = "facecolors length")]
    fn wrong_facecolor_count_panics() {
        let _ = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED, Rgba::GREEN]);
    }

    #[test]
    fn gouraud_uniform_corners_emit_two_flat_triangles() {
        // All four corners the same color: no subdivision, one flat fill per
        // triangle, both exactly the corner color.
        let mesh = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::TRANSPARENT])
            .with_vertex_colors(vec![Rgba::RED; 4]);
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls.len(), 2, "one quad = two unsubdivided triangles");
        for call in &r.calls {
            assert_eq!(call.fill, Some(Rgba::RED));
        }
    }

    #[test]
    fn gouraud_gradient_subdivides_and_stays_within_corner_bounds() {
        // Black -> red gradient across the cell: subdivision kicks in and
        // every emitted patch color is a convex mix of the corners.
        let corners = vec![Rgba::BLACK, Rgba::RED, Rgba::BLACK, Rgba::RED];
        let mesh = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::TRANSPARENT])
            .with_vertex_colors(corners);
        let mut r = MockRenderer::default();
        mesh.draw(&mut r, &Affine2D::identity());
        assert!(
            r.calls.len() > 2,
            "a strong gradient must subdivide, got {} patches",
            r.calls.len()
        );
        let mut distinct = std::collections::BTreeSet::new();
        for call in &r.calls {
            let fill = call.fill.expect("gouraud patches are filled");
            assert!((0.0..=1.0).contains(&fill.r), "r out of bounds");
            assert!(
                fill.g.abs() < 1e-9 && fill.b.abs() < 1e-9,
                "black->red mix has no green/blue"
            );
            distinct.insert((fill.r * 1000.0) as i64);
        }
        assert!(
            distinct.len() > 4,
            "smooth shading needs several distinct reds, got {}",
            distinct.len()
        );
    }

    #[test]
    #[should_panic(expected = "vertex_colors length")]
    fn wrong_vertex_color_count_panics() {
        let _ = QuadMesh::new(1, 1, unit_grid(1, 1), vec![Rgba::RED])
            .with_vertex_colors(vec![Rgba::RED; 3]);
    }
}
