//! [`triplot`](Axes::triplot) and [`tripcolor`](Axes::tripcolor): plots over an
//! unstructured triangulation.
//!
//! The caller supplies the vertex coordinates (`x`, `y`) and a connectivity
//! list `triangles`, where each `[a, b, c]` names the three vertex indices of
//! one triangle. [`triplot`](Axes::triplot) draws the mesh as a wireframe of
//! [`Line2D`] triangle outlines; [`tripcolor`](Axes::tripcolor) fills each
//! triangle with a flat [`Patch`] color obtained by averaging the three
//! vertices' scalar values and mapping that through a [`LinearNorm`] and the
//! `viridis` colormap. Both fold into autoscaling via
//! [`data_limits`](Axes::data_limits).
//!
//! Triangles whose indices fall outside `x`/`y` are skipped (never panic), and
//! mismatched `x`/`y` lengths or empty input draw nothing.

use crate::artist::{Line2D, Patch};
use crate::core::color::{LinearNorm, Normalize, colormap};

use crate::figure::Axes;

/// Yield each triangle whose three indices are all in range `0..n`, skipping
/// any that reference a vertex beyond the supplied coordinates.
fn valid_triangles(triangles: &[[usize; 3]], n: usize) -> impl Iterator<Item = &[usize; 3]> {
    triangles.iter().filter(move |t| t.iter().all(|&i| i < n))
}

impl Axes {
    /// Draw the triangulation `(x, y, triangles)` as a wireframe.
    ///
    /// Each entry of `triangles` is `[a, b, c]`, the three vertex indices of one
    /// triangle into `x`/`y`. For every triangle a closed [`Line2D`] outline
    /// `v0 -> v1 -> v2 -> v0` is drawn; all outlines share a single color (the
    /// next property-cycle color). Edges shared by two triangles are drawn twice,
    /// which matches a simple triplot.
    ///
    /// Triangles with an out-of-range index are skipped (never panic). If
    /// `x.len() != y.len()` or either is empty, nothing is drawn. The data limits
    /// expand to cover every referenced vertex. Returns `&mut Self`.
    ///
    /// ![triplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_triplot.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A unit square split into two triangles.
    /// let x = [0.0, 1.0, 1.0, 0.0];
    /// let y = [0.0, 0.0, 1.0, 1.0];
    /// ax.triplot(&x, &y, &[[0, 1, 2], [0, 2, 3]]);
    /// let limits = ax.data_limits().expect("triplot contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 1.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 1.0));
    /// ```
    pub fn triplot(&mut self, x: &[f64], y: &[f64], triangles: &[[usize; 3]]) -> &mut Self {
        if x.is_empty() || x.len() != y.len() {
            return self;
        }
        let n = x.len();

        let mut any = false;
        let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
        let color = self.next_cycle_color();

        for t in valid_triangles(triangles, n) {
            let [a, b, c] = *t;
            let xs = vec![x[a], x[b], x[c], x[a]];
            let ys = vec![y[a], y[b], y[c], y[a]];
            for &i in &[a, b, c] {
                xmin = xmin.min(x[i]);
                xmax = xmax.max(x[i]);
                ymin = ymin.min(y[i]);
                ymax = ymax.max(y[i]);
                any = true;
            }
            self.add_line(Line2D::new(xs, ys).with_color(color));
        }

        if any {
            self.include_data_bbox(xmin, ymin, xmax, ymax);
        }
        self
    }

    /// Fill each triangle of `(x, y, triangles)` with a flat color derived from
    /// the per-vertex scalar field `values`.
    ///
    /// `values` is per-vertex (`values.len()` must equal `x.len()`). For each
    /// triangle the three vertices' values are averaged, normalized through a
    /// [`LinearNorm`] over `[min(values), max(values)]`, and mapped through the
    /// `viridis` colormap to a single face color. Each triangle is drawn as a
    /// filled [`Patch`] whose edge matches its face, so adjacent triangles tile
    /// without visible seams.
    ///
    /// Triangles with an out-of-range index are skipped (never panic). If
    /// `values.len() != x.len()`, `x.len() != y.len()`, or input is empty,
    /// nothing is drawn. The data limits expand to cover every referenced
    /// vertex. Returns `&mut Self`.
    ///
    /// ![tripcolor](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_tripcolor.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let x = [0.0, 1.0, 1.0, 0.0];
    /// let y = [0.0, 0.0, 1.0, 1.0];
    /// // Color by a smooth field value = x + y.
    /// let values = [0.0, 1.0, 2.0, 1.0];
    /// ax.tripcolor(&x, &y, &[[0, 1, 2], [0, 2, 3]], &values);
    /// assert!(ax.data_limits().is_some());
    /// ```
    pub fn tripcolor(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
    ) -> &mut Self {
        if x.is_empty() || x.len() != y.len() || values.len() != x.len() {
            return self;
        }
        let n = x.len();

        // Normalize over the full finite value range; guard a degenerate range.
        let mut vmin = f64::INFINITY;
        let mut vmax = f64::NEG_INFINITY;
        for &v in values {
            if v.is_finite() {
                vmin = vmin.min(v);
                vmax = vmax.max(v);
            }
        }
        if vmin > vmax {
            vmin = 0.0;
            vmax = 1.0;
        }
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = colormap("viridis").expect("viridis is built in");

        let mut any = false;
        let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);

        for t in valid_triangles(triangles, n) {
            let [a, b, c] = *t;
            let avg = (values[a] + values[b] + values[c]) / 3.0;
            let color = cmap.sample(norm.normalize(avg));

            for &i in &[a, b, c] {
                xmin = xmin.min(x[i]);
                xmax = xmax.max(x[i]);
                ymin = ymin.min(y[i]);
                ymax = ymax.max(y[i]);
                any = true;
            }

            let tri = Patch::polygon(&[[x[a], y[a]], [x[b], y[b]], [x[c], y[c]]])
                .facecolor(Some(color))
                .edgecolor(Some(color));
            self.add_patch(tri);
        }

        if any {
            self.include_data_bbox(xmin, ymin, xmax, ymax);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::artist::Artist;
    use crate::core::color::{Colormap, Rgba, viridis};
    use crate::core::{Affine2D, Bbox, Path};

    use crate::figure::Axes;

    /// A [`Renderer`] that records the fill color of each `draw_path` call.
    #[derive(Default)]
    struct ColorRecorder {
        fills: Vec<Option<Rgba>>,
    }

    impl crate::render::Renderer for ColorRecorder {
        fn draw_path(
            &mut self,
            _gc: &crate::render::GraphicsContext,
            _path: &Path,
            _transform: &Affine2D,
            fill: Option<Rgba>,
        ) {
            self.fills.push(fill);
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    /// The single fill color a patch emits when drawn.
    fn patch_fill(patch: &crate::artist::Patch) -> Option<Rgba> {
        let mut rec = ColorRecorder::default();
        patch.draw(&mut rec, &Affine2D::identity());
        rec.fills.first().copied().flatten()
    }

    /// A unit square split into two triangles, shared diagonal 0->2.
    fn square() -> ([f64; 4], [f64; 4], [[usize; 3]; 2]) {
        let x = [0.0, 1.0, 1.0, 0.0];
        let y = [0.0, 0.0, 1.0, 1.0];
        (x, y, [[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn triplot_emits_one_line_per_triangle() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (x, y, tris) = square();
        ax.triplot(&x, &y, &tris);
        assert_eq!(ax.lines.len(), 2);
        // Each outline is a closed 4-point polyline.
        assert_eq!(ax.lines[0].points().len(), 4);
    }

    #[test]
    fn triplot_data_limits_cover_vertices() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (x, y, tris) = square();
        ax.triplot(&x, &y, &tris);
        let limits = ax.data_limits().expect("triplot contributes data limits");
        assert_eq!((limits.xmin(), limits.xmax()), (0.0, 1.0));
        assert_eq!((limits.ymin(), limits.ymax()), (0.0, 1.0));
    }

    #[test]
    fn tripcolor_emits_one_patch_per_triangle() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (x, y, tris) = square();
        let values = [0.0, 1.0, 2.0, 1.0];
        ax.tripcolor(&x, &y, &tris, &values);
        assert_eq!(ax.patches.len(), 2);
    }

    #[test]
    fn tripcolor_colors_span_the_value_range() {
        // Two well-separated triangles: one entirely at the min value, one at
        // the max, so their averaged colors are the viridis endpoints.
        let x = [0.0, 1.0, 0.0, 2.0, 3.0, 2.0];
        let y = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let values = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let tris = [[0, 1, 2], [3, 4, 5]];
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.tripcolor(&x, &y, &tris, &values);
        let cm = viridis();
        let lo = cm.sample(0.0);
        let hi = cm.sample(1.0);
        assert_eq!(patch_fill(&ax.patches[0]), Some(lo));
        assert_eq!(patch_fill(&ax.patches[1]), Some(hi));
        assert_ne!(patch_fill(&ax.patches[0]), patch_fill(&ax.patches[1]));
    }

    #[test]
    fn out_of_range_triangle_is_skipped() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (x, y, _) = square();
        // Second triangle references vertex 9, which does not exist.
        let tris = [[0, 1, 2], [0, 2, 9]];
        ax.triplot(&x, &y, &tris);
        assert_eq!(ax.lines.len(), 1);
    }

    #[test]
    fn tripcolor_mismatched_values_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let (x, y, tris) = square();
        // Only three values for four vertices.
        ax.tripcolor(&x, &y, &tris, &[0.0, 1.0, 2.0]);
        assert!(ax.patches.is_empty());
        assert!(ax.data_limits().is_none());
    }

    #[test]
    fn empty_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.triplot(&[], &[], &[]);
        ax.tripcolor(&[], &[], &[], &[]);
        assert!(ax.lines.is_empty());
        assert!(ax.patches.is_empty());
        assert!(ax.data_limits().is_none());
    }
}
