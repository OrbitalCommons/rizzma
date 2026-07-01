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
            edgecolor: None,
            linewidth: 1.0,
            zorder: 0.0,
            visible: true,
        }
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
}
