//! Matrix and 2D-field display helpers: [`Axes::matshow`], [`Axes::spy`], and
//! [`Axes::hist2d`].
//!
//! These are thin reuses of the existing rasterization machinery. `matshow` and
//! `spy` delegate to [`Axes::imshow`](super::Axes::imshow) (a colormapped
//! [`AxesImage`]); `hist2d` bins points into a grid and renders the counts via
//! [`Axes::pcolormesh`](super::Axes::pcolormesh) (a [`QuadMesh`]). All follow
//! matplotlib's conventions: matrices use `origin="upper"` (row `0` at the top)
//! and the 2D histogram lays its mesh over the true data extents.

use rizzma_artist::{AxesImage, QuadMesh};
use rizzma_core::color::{LinearNorm, Normalize, colormap};

use crate::Axes;

impl Axes {
    /// Display a matrix `data` (`nrows x ncols`, row-major) as an image.
    ///
    /// This is [`imshow`](Axes::imshow) with matrix conventions: the data is
    /// interpreted row-major with `origin="upper"` (row `0` at the top), the
    /// extent is `(0, ncols, 0, nrows)`, and the default `viridis` colormap
    /// colorizes the values. Tune the returned handle exactly as for `imshow`
    /// (e.g. `.cmap(..)`, `.vmin(..)`, `.vmax(..)`).
    ///
    /// # Panics
    ///
    /// Panics if `data.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A 2x3 matrix; the image extent spans x in [0, 3], y in [0, 2].
    /// ax.matshow(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 2, 3);
    /// let limits = ax.data_limits().expect("matshow provides data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 3.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    /// ```
    // TODO: aspect='equal' once `Axes` supports an aspect ratio (matplotlib's
    // `matshow` forces square cells).
    pub fn matshow(&mut self, data: &[f64], nrows: usize, ncols: usize) -> &mut AxesImage {
        self.imshow(data, nrows, ncols)
    }

    /// Plot the sparsity pattern of a matrix `data` (`nrows x ncols`, row-major).
    ///
    /// Each entry is mapped to a binary field: `1.0` where `data[i].abs() > 0`
    /// (a structural nonzero), else `0.0`. The field is drawn with
    /// [`imshow`](Axes::imshow) using the reversed-gray colormap (`"gray_r"`)
    /// pinned to `vmin = 0.0`, `vmax = 1.0`, so nonzeros render dark on a light
    /// background — matching matplotlib's `spy` convention. Row `0` is at the
    /// top (`origin="upper"`).
    ///
    /// # Panics
    ///
    /// Panics if `data.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // Identity-like 2x2: nonzeros on the diagonal become filled cells.
    /// ax.spy(&[1.0, 0.0, 0.0, 2.0], 2, 2);
    /// let limits = ax.data_limits().expect("spy provides data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 2.0));
    /// ```
    pub fn spy(&mut self, data: &[f64], nrows: usize, ncols: usize) -> &mut AxesImage {
        let pattern: Vec<f64> = data
            .iter()
            .map(|&v| if v.abs() > 0.0 { 1.0 } else { 0.0 })
            .collect();
        let image = self.imshow(&pattern, nrows, ncols);
        image.cmap("gray_r").vmin(0.0).vmax(1.0)
    }

    /// Draw a 2D histogram of the points `(x, y)` as a colormapped mesh.
    ///
    /// The points are binned into a `bins x bins` grid spanning
    /// `[xmin, xmax] x [ymin, ymax]` (the data extents). Each cell counts the
    /// points that fall in it (row-major, row `0` at `ymin`); points on the
    /// upper edge are folded into the last bin. The counts are rendered with
    /// [`pcolormesh`](Axes::pcolormesh) (the `viridis` colormap), and the mesh
    /// grid is placed over the true data extents so the axes show the real `x`
    /// and `y` ranges. Returns the mesh.
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != y.len()`, or if `bins` is `0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma_core::Bbox;
    /// use rizzma_figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let x = [0.0, 1.0, 0.0, 1.0];
    /// let y = [0.0, 0.0, 1.0, 1.0];
    /// ax.hist2d(&x, &y, 2);
    /// // The mesh spans the data extents, x in [0, 1], y in [0, 1].
    /// let limits = ax.data_limits().expect("hist2d provides data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 1.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 1.0));
    /// ```
    pub fn hist2d(&mut self, x: &[f64], y: &[f64], bins: usize) -> &mut QuadMesh {
        assert_eq!(
            x.len(),
            y.len(),
            "hist2d: x length {} must equal y length {}",
            x.len(),
            y.len()
        );
        assert!(bins > 0, "hist2d: bins must be non-zero");

        let (xmin, xmax) = data_extent(x);
        let (ymin, ymax) = data_extent(y);
        let xspan = xmax - xmin;
        let yspan = ymax - ymin;

        // Bin each point into the `bins x bins` grid; row-major, row 0 at ymin.
        let mut counts = vec![0.0_f64; bins * bins];
        let bin_index = |v: f64, lo: f64, span: f64| -> usize {
            if span <= 0.0 {
                return 0;
            }
            let frac = (v - lo) / span;
            let idx = (frac * bins as f64) as usize;
            idx.min(bins - 1)
        };
        for (&xi, &yi) in x.iter().zip(y.iter()) {
            if !xi.is_finite() || !yi.is_finite() {
                continue;
            }
            let col = bin_index(xi, xmin, xspan);
            let row = bin_index(yi, ymin, yspan);
            counts[row * bins + col] += 1.0;
        }

        // Grid corners over the true data extents, row-major:
        // (bins + 1) * (bins + 1) points.
        let mut coordinates = Vec::with_capacity((bins + 1) * (bins + 1));
        for r in 0..=bins {
            let yy = ymin + yspan * r as f64 / bins as f64;
            for c in 0..=bins {
                let xx = xmin + xspan * c as f64 / bins as f64;
                coordinates.push([xx, yy]);
            }
        }

        // Colormap the counts through LinearNorm + viridis.
        let cmax = counts.iter().cloned().fold(0.0_f64, f64::max);
        let norm = LinearNorm::new(0.0, cmax);
        let cmap = colormap("viridis").expect("viridis is built in");
        let facecolors = counts
            .iter()
            .map(|&v| cmap.sample(norm.normalize(v)))
            .collect();

        let mesh = QuadMesh::new(bins, bins, coordinates, facecolors);
        self.meshes.push(mesh);
        self.meshes.last_mut().expect("just pushed a mesh")
    }
}

/// The finite min and max of `data`, or `(0.0, 1.0)` when there is no finite
/// value (empty or all-NaN input).
fn data_extent(data: &[f64]) -> (f64, f64) {
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

#[cfg(test)]
mod tests {
    use crate::Axes;
    use rizzma_core::Bbox;
    use rizzma_core::color::{Colormap, Rgba, viridis};

    /// A [`Renderer`](rizzma_render::Renderer) that records the fill color of
    /// each `draw_path` call.
    #[derive(Default)]
    struct ColorRecorder {
        fills: Vec<Option<Rgba>>,
    }

    impl rizzma_render::Renderer for ColorRecorder {
        fn draw_path(
            &mut self,
            _gc: &rizzma_render::GraphicsContext,
            _path: &rizzma_core::Path,
            _transform: &rizzma_core::Affine2D,
            fill: Option<Rgba>,
        ) {
            self.fills.push(fill);
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    #[test]
    fn matshow_sets_data_limits_to_matrix_extent() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // A 2-row, 3-column matrix; default extent is (0, 3, 0, 2).
        ax.matshow(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 2, 3);
        let limits = ax.data_limits().expect("matshow provides data limits");
        assert_eq!((limits.xmin(), limits.xmax()), (0.0, 3.0));
        assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    }

    #[test]
    fn spy_builds_binary_field_with_right_nonzeros() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Diagonal nonzeros (one negative) plus zeros off-diagonal.
        ax.spy(&[3.0, 0.0, 0.0, -2.0], 2, 2);
        let image = &ax.images[0];
        // spy pins the reversed-gray colormap to a 0..1 scale.
        assert_eq!(image.clim(), (0.0, 1.0));
        // With "gray_r" (0 -> white, 1 -> black), nonzero cells (1.0) colorize
        // to black and zero cells (0.0) to white.
        let black = Rgba::BLACK.to_u8_array();
        let white = Rgba::WHITE.to_u8_array();
        assert_eq!(image.colorize_cell(0, 0), black); // nonzero
        assert_eq!(image.colorize_cell(0, 1), white); // zero
        assert_eq!(image.colorize_cell(1, 0), white); // zero
        assert_eq!(image.colorize_cell(1, 1), black); // nonzero
    }

    #[test]
    fn hist2d_counts_known_points_per_cell() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // A 2x2 grid over x,y in [0, 1]. Two points in the lower-left cell, one
        // in each of the other three cells.
        let x = [0.0, 1.0, 0.0, 1.0, 0.0];
        let y = [0.0, 0.0, 1.0, 1.0, 0.0];
        let mesh = ax.hist2d(&x, &y, 2);

        // Render and capture per-cell fill colors (row-major). Max count is 2,
        // so a count of 2 maps to viridis(1.0) and a count of 1 to viridis(0.5).
        let mut rec = ColorRecorder::default();
        rizzma_artist::Artist::draw(mesh, &mut rec, &rizzma_core::Affine2D::identity());
        let cm = viridis();
        let two = cm.sample(1.0);
        let one = cm.sample(0.5);
        let fills: Vec<Rgba> = rec
            .fills
            .iter()
            .map(|f| f.expect("cell has a fill"))
            .collect();
        // Cells row-major: (0,0)=2, (0,1)=1, (1,0)=1, (1,1)=1.
        assert_eq!(fills, vec![two, one, one, one]);
    }

    #[test]
    fn hist2d_mesh_spans_data_extents() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [-2.0, 4.0, 1.0];
        let y = [10.0, 20.0, 15.0];
        ax.hist2d(&x, &y, 3);
        let limits = ax.data_limits().expect("hist2d provides data limits");
        assert_eq!((limits.xmin(), limits.xmax()), (-2.0, 4.0));
        assert_eq!((limits.ymin(), limits.ymax()), (10.0, 20.0));
    }
}
