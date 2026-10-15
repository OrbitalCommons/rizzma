//! The [`Axes::pcolormesh`] colormapped-mesh helper.
//!
//! Mirrors matplotlib's `Axes.pcolormesh` for the regular-grid case: it builds a
//! unit `nrows x ncols` grid, colormaps the row-major cell values through a
//! [`LinearNorm`] and the default colormap, stores the resulting [`QuadMesh`]
//! on the axes, and returns a mutable handle. Meshes draw beneath the other
//! artists (see [`Axes::draw`](super::Axes::draw)) and their data-space extent
//! participates in autoscaling.

use crate::artist::{PolyCollection, QuadMesh};
use crate::core::color::{ColorMapping, Colormap, LinearNorm, Normalize, default_colormap};

use crate::figure::Axes;

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
    /// default colormap to per-cell face colors. To retune the colors, build a
    /// fresh [`QuadMesh`] and overwrite the returned handle.
    ///
    /// The returned handle exposes the [`QuadMesh`] builder setters
    /// ([`with_edgecolor`](QuadMesh::with_edgecolor),
    /// [`with_zorder`](QuadMesh::with_zorder), …) for in-place restyling, e.g.
    /// `*ax.pcolormesh(&c, nr, nc) = mesh.with_edgecolor(Some(Rgba::BLACK))`.
    /// [`pcolormesh_xy`](Axes::pcolormesh_xy) takes explicit cell edges.
    ///
    /// # Panics
    ///
    /// Panics if `c.len()` is not exactly `nrows * ncols`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
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
        self.pcolormesh_mapped(c, nrows, ncols, ColorMapping::default())
    }

    /// [`pcolormesh`](Axes::pcolormesh) with an explicit color `mapping`: a
    /// colormap name, or a [`ColorMapping`] with `vmin`/`vmax`, a norm, and
    /// bad/over/under colors. NaN cells take the bad color (transparent by
    /// default).
    ///
    /// ![norms](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_norms.png)
    ///
    /// # Panics
    ///
    /// Panics if `c.len()` is not exactly `nrows * ncols`.
    ///
    /// ```
    /// use rizzma::Figure;
    /// use rizzma::core::color::{ColorMapping, LogNorm, Rgba};
    /// let c = [1.0, 10.0, 100.0, 1000.0, f64::NAN, 5.0];
    /// let mut fig = Figure::new(4.0, 3.0);
    /// fig.add_subplot(1, 1, 1).pcolormesh_mapped(
    ///     &c,
    ///     2,
    ///     3,
    ///     ColorMapping::new("magma")
    ///         .norm(LogNorm::new(1.0, 1000.0))
    ///         .bad(Rgba::BLACK),
    /// );
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn pcolormesh_mapped(
        &mut self,
        c: &[f64],
        nrows: usize,
        ncols: usize,
        mapping: impl Into<ColorMapping>,
    ) -> &mut QuadMesh {
        assert_eq!(
            c.len(),
            nrows * ncols,
            "pcolormesh: c length {} must equal nrows * ncols = {}",
            c.len(),
            nrows * ncols
        );

        let x: Vec<f64> = (0..=ncols).map(|col| col as f64).collect();
        let y: Vec<f64> = (0..=nrows).map(|r| r as f64).collect();
        self.pcolormesh_xy(&x, &y, c, mapping)
    }

    /// Draw row-major scalar `c` as a colormapped quad mesh over explicit
    /// cell edges (matplotlib's `pcolormesh(x, y, c)` with 1-D `x` and `y`).
    ///
    /// `x` holds the `ncols + 1` column edges and `y` the `nrows + 1` row
    /// edges, so cell `(r, col)` spans `[x[col], x[col + 1]] x [y[r], y[r + 1]]`
    /// and takes value `c[r * ncols + col]`; the edges need not be evenly
    /// spaced. On a [sky projection](Axes::set_sky_projection) they are
    /// longitude and latitude edges in radians. Colors map as in
    /// [`pcolormesh_mapped`](Axes::pcolormesh_mapped).
    ///
    /// # Panics
    ///
    /// Panics if `x` or `y` is empty, or `c.len()` is not
    /// `(y.len() - 1) * (x.len() - 1)`.
    ///
    /// ```
    /// use rizzma::Figure;
    ///
    /// // Two columns of uneven width over three rows.
    /// let (x, y) = ([0.0, 1.0, 4.0], [0.0, 1.0, 2.0, 3.0]);
    /// let c = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    /// let mut fig = Figure::new(4.0, 3.0);
    /// let ax = fig.add_subplot(1, 1, 1);
    /// ax.pcolormesh_xy(&x, &y, &c, "magma");
    /// let limits = ax.data_limits().expect("mesh provides data limits");
    /// assert_eq!((limits.xmax(), limits.ymax()), (4.0, 3.0));
    /// ```
    pub fn pcolormesh_xy(
        &mut self,
        x: &[f64],
        y: &[f64],
        c: &[f64],
        mapping: impl Into<ColorMapping>,
    ) -> &mut QuadMesh {
        assert!(
            !x.is_empty() && !y.is_empty(),
            "pcolormesh_xy: x and y need at least one edge each, got {} and {}",
            x.len(),
            y.len()
        );
        let (nrows, ncols) = (y.len() - 1, x.len() - 1);
        assert_eq!(
            c.len(),
            nrows * ncols,
            "pcolormesh_xy: c length {} must equal (y.len() - 1) * (x.len() - 1) = {}",
            c.len(),
            nrows * ncols
        );
        // Grid corners, row-major: (nrows + 1) * (ncols + 1) points.
        let coordinates = y
            .iter()
            .flat_map(|&yv| x.iter().map(move |&xv| [xv, yv]))
            .collect();
        let scale = mapping.into().resolve(c);
        let mesh =
            QuadMesh::new(nrows, ncols, coordinates, scale.colors(c)).with_color_scale(scale);
        self.meshes.push(mesh);
        self.meshes.last_mut().expect("just pushed a mesh")
    }

    /// Fill each of `polygons` with the color of its value in `c`, through
    /// `mapping` — matplotlib's `PolyCollection` with `set_array(c)` added
    /// to the axes.
    ///
    /// Each polygon is a list of data-space vertices, closed implicitly;
    /// they need not share a grid, so this fills sky pixelizations such as
    /// HEALPix, survey footprints and Voronoi cells. On a
    /// [sky projection](Axes::set_sky_projection) the vertices are
    /// longitude/latitude in radians and each edge follows the projection.
    /// Colors map as in [`pcolormesh_mapped`](Axes::pcolormesh_mapped); NaN
    /// values take the bad color. The collection draws in zorder with the
    /// lines, patches and scatters (zorder `1.0`, no edge), and
    /// [`Mappable::Polygons`](crate::figure::Mappable::Polygons) gives it a
    /// colorbar.
    ///
    /// ![sky polygons](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky_polygons.png)
    ///
    /// # Panics
    ///
    /// Panics if `c.len()` differs from `polygons.len()`, or a polygon is
    /// empty or has a non-finite vertex.
    ///
    /// ```
    /// use rizzma::Figure;
    /// use rizzma::figure::Mappable;
    ///
    /// // Two triangles and a square, colored 0, 5 and 10.
    /// let polygons = vec![
    ///     vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
    ///     vec![[1.0, 0.0], [2.0, 0.0], [1.5, 1.0]],
    ///     vec![[2.5, 0.0], [3.5, 0.0], [3.5, 1.0], [2.5, 1.0]],
    /// ];
    /// let mut fig = Figure::new(4.0, 2.0);
    /// let ax = fig.add_subplot(1, 1, 1);
    /// ax.poly_collection(&polygons, &[0.0, 5.0, 10.0], "plasma");
    /// ax.colorbar(Mappable::Polygons(0));
    /// assert_eq!(ax.data_limits().unwrap().xmax(), 3.5);
    /// ```
    pub fn poly_collection(
        &mut self,
        polygons: &[Vec<[f64; 2]>],
        c: &[f64],
        mapping: impl Into<ColorMapping>,
    ) -> &mut PolyCollection {
        assert_eq!(
            c.len(),
            polygons.len(),
            "poly_collection: {} values for {} polygons",
            c.len(),
            polygons.len()
        );
        let scale = mapping.into().resolve(c);
        let polys = PolyCollection::new(polygons, scale.colors(c)).with_color_scale(scale);
        self.poly_collections.push(polys);
        self.poly_collections
            .last_mut()
            .expect("just pushed a polygon collection")
    }

    /// Draw a smoothly shaded pseudocolor mesh (matplotlib's
    /// `pcolormesh(..., shading="gouraud")`).
    ///
    /// Unlike [`pcolormesh`](Axes::pcolormesh), `c` is **vertex-centered**: the
    /// row-major `nrows x ncols` values sit at the grid *corners* (integer
    /// coordinates `x = 0..ncols-1`, `y = 0..nrows-1`), yielding
    /// `(nrows - 1) x (ncols - 1)` cells whose colors interpolate smoothly
    /// between the corner values instead of flat per-cell blocks. Values map
    /// through a [`LinearNorm`] and the default colormap. Gouraud cells draw no
    /// edges.
    ///
    /// ![gouraud](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_gouraud.png)
    ///
    /// # Panics
    ///
    /// Panics if `c.len()` is not exactly `nrows * ncols` or the grid is
    /// smaller than `2 x 2`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // 2x2 corner values: one smoothly shaded cell.
    /// ax.pcolormesh_gouraud(&[0.0, 1.0, 2.0, 3.0], 2, 2);
    /// let limits = ax.data_limits().expect("mesh provides data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 1.0));
    /// ```
    pub fn pcolormesh_gouraud(&mut self, c: &[f64], nrows: usize, ncols: usize) -> &mut QuadMesh {
        assert_eq!(
            c.len(),
            nrows * ncols,
            "pcolormesh_gouraud: c length {} must equal nrows * ncols = {}",
            c.len(),
            nrows * ncols
        );
        assert!(
            nrows >= 2 && ncols >= 2,
            "pcolormesh_gouraud: needs at least a 2 x 2 grid of corner values"
        );

        // Corners at the value positions themselves: the cells sit between
        // them, (nrows - 1) x (ncols - 1).
        let (cell_rows, cell_cols) = (nrows - 1, ncols - 1);
        let mut coordinates = Vec::with_capacity(nrows * ncols);
        for r in 0..nrows {
            for col in 0..ncols {
                coordinates.push([col as f64, r as f64]);
            }
        }

        let (vmin, vmax) = data_min_max(c);
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = default_colormap();
        let vertex_colors: Vec<_> = c.iter().map(|&v| cmap.sample(norm.normalize(v))).collect();

        // Flat facecolors are required by the constructor but unused in
        // gouraud mode.
        let facecolors = vec![crate::core::color::Rgba::TRANSPARENT; cell_rows * cell_cols];
        let mesh = QuadMesh::new(cell_rows, cell_cols, coordinates, facecolors)
            .with_vertex_colors(vertex_colors);
        self.meshes.push(mesh);
        self.meshes.last_mut().expect("just pushed a mesh")
    }
}

#[cfg(test)]
mod tests {
    use crate::core::Bbox;
    use crate::core::color::{Colormap, default_colormap};
    use crate::figure::Axes;

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
    fn min_and_max_cells_get_default_cmap_endpoints() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // 2x2 with min = 0.0 (cell 0) and max = 3.0 (cell 3).
        ax.pcolormesh(&[0.0, 1.0, 2.0, 3.0], 2, 2);
        let cm = default_colormap();
        let lo = cm.sample(0.0);
        let hi = cm.sample(1.0);
        let mesh = &ax.meshes[0];
        let mut r = ColorRecorder::default();
        crate::artist::Artist::draw(mesh, &mut r, &crate::core::Affine2D::identity());
        // The first cell holds the minimum -> cmap(0); the last -> cmap(1).
        assert_eq!(r.fills.first().copied().flatten(), Some(lo));
        assert_eq!(r.fills.last().copied().flatten(), Some(hi));
    }

    /// A [`Renderer`] that records the fill color of each `draw_path` call.
    #[derive(Default)]
    struct ColorRecorder {
        fills: Vec<Option<crate::core::color::Rgba>>,
    }

    impl crate::render::Renderer for ColorRecorder {
        fn draw_path(
            &mut self,
            _gc: &crate::render::GraphicsContext,
            _path: &crate::core::Path,
            _transform: &crate::core::Affine2D,
            fill: Option<crate::core::color::Rgba>,
        ) {
            self.fills.push(fill);
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }
}
