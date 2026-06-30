//! The [`Axes::pcolormesh`] colormapped-mesh helper.
//!
//! Mirrors matplotlib's `Axes.pcolormesh` for the regular-grid case: it builds a
//! unit `nrows x ncols` grid, colormaps the row-major cell values through a
//! [`LinearNorm`] and the `viridis` colormap, stores the resulting [`QuadMesh`]
//! on the axes, and returns a mutable handle. Meshes draw beneath the other
//! artists (see [`Axes::draw`](super::Axes::draw)) and their data-space extent
//! participates in autoscaling.

use rizzma_artist::QuadMesh;
use rizzma_core::color::{LinearNorm, Normalize, colormap};

use crate::Axes;

/// The finite min and max of `data`, or `(0.0, 1.0)` when there is no finite
/// value (empty or all-NaN input).
fn data_min_max(data: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min <= max { (min, max) } else { (0.0, 1.0) }
}

impl Axes {
    /// Draw row-major scalar `c` (`nrows x ncols`) as a colormapped quad mesh.
    ///
    /// The mesh is laid out on a regular unit grid (`x = 0..=ncols`,
    /// `y = 0..=nrows`), so cell `(r, col)` spans data rectangle
    /// `[col, col + 1] x [r, r + 1]`. The cell values `c` (row-major,
    /// `c[r * ncols + col]`) are normalized through a [`LinearNorm`] with
    /// `vmin`/`vmax` set to the finite data min/max, then mapped through the
    /// `viridis` colormap to per-cell face colors. To retune the colors, build a
    /// fresh [`QuadMesh`] and overwrite the returned handle.
    ///
    /// The returned handle exposes the [`QuadMesh`] builder setters
    /// ([`with_edgecolor`](QuadMesh::with_edgecolor),
    /// [`with_zorder`](QuadMesh::with_zorder), …) for in-place restyling, e.g.
    /// `*ax.pcolormesh(&c, nr, nc) = mesh.with_edgecolor(Some(Rgba::BLACK))`.
    ///
    // TODO: a `pcolor`-style variant taking explicit x/y corner coordinates (an
    // irregular grid) is a follow-up.
    ///
    /// # Panics
    ///
    /// Panics if `c.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A 2x3 field; the unit grid spans x in [0, 3], y in [0, 2].
    /// let c = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    /// ax.pcolormesh(&c, 2, 3);
    /// let limits = ax.data_limits().expect("mesh provides data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 3.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    /// ```
    pub fn pcolormesh(&mut self, c: &[f64], nrows: usize, ncols: usize) -> &mut QuadMesh {
        assert_eq!(
            c.len(),
            nrows * ncols,
            "pcolormesh: c length {} must equal nrows * ncols = {}",
            c.len(),
            nrows * ncols
        );

        // Regular unit grid corners, row-major: (nrows + 1) * (ncols + 1) points.
        let mut coordinates = Vec::with_capacity((nrows + 1) * (ncols + 1));
        for r in 0..=nrows {
            for col in 0..=ncols {
                coordinates.push([col as f64, r as f64]);
            }
        }

        // Colormap the cell values through LinearNorm + viridis.
        let (vmin, vmax) = data_min_max(c);
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = colormap("viridis").expect("viridis is built in");
        let facecolors = c.iter().map(|&v| cmap.sample(norm.normalize(v))).collect();

        let mesh = QuadMesh::new(nrows, ncols, coordinates, facecolors);
        self.meshes.push(mesh);
        self.meshes.last_mut().expect("just pushed a mesh")
    }
}

#[cfg(test)]
mod tests {
    use crate::Axes;
    use rizzma_core::Bbox;
    use rizzma_core::color::{Colormap, viridis};

    #[test]
    fn pcolormesh_sets_data_limits_to_grid() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // A 2-row, 3-column field; the unit grid spans x in [0, 3], y in [0, 2].
        ax.pcolormesh(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 2, 3);
        let limits = ax.data_limits().expect("mesh provides data limits");
        assert_eq!((limits.xmin(), limits.xmax()), (0.0, 3.0));
        assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    }

    #[test]
    fn min_and_max_cells_get_viridis_endpoints() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // 2x2 with min = 0.0 (cell 0) and max = 3.0 (cell 3).
        ax.pcolormesh(&[0.0, 1.0, 2.0, 3.0], 2, 2);
        let cm = viridis();
        let lo = cm.sample(0.0);
        let hi = cm.sample(1.0);
        let mesh = &ax.meshes[0];
        let mut r = ColorRecorder::default();
        rizzma_artist::Artist::draw(mesh, &mut r, &rizzma_core::Affine2D::identity());
        // The first cell holds the minimum -> viridis(0); the last -> viridis(1).
        assert_eq!(r.fills.first().copied().flatten(), Some(lo));
        assert_eq!(r.fills.last().copied().flatten(), Some(hi));
    }

    /// A [`Renderer`] that records the fill color of each `draw_path` call.
    #[derive(Default)]
    struct ColorRecorder {
        fills: Vec<Option<rizzma_core::color::Rgba>>,
    }

    impl rizzma_render::Renderer for ColorRecorder {
        fn draw_path(
            &mut self,
            _gc: &rizzma_render::GraphicsContext,
            _path: &rizzma_core::Path,
            _transform: &rizzma_core::Affine2D,
            fill: Option<rizzma_core::color::Rgba>,
        ) {
            self.fills.push(fill);
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }
}
