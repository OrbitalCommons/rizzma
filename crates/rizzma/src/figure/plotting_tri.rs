//! [`triplot`](Axes::triplot) and [`tripcolor`](Axes::tripcolor): plots over an
//! unstructured triangulation.
//!
//! The caller supplies the vertex coordinates (`x`, `y`) and a connectivity
//! list `triangles`, where each `[a, b, c]` names the three vertex indices of
//! one triangle. [`triplot`](Axes::triplot) draws the mesh as a wireframe of
//! [`Line2D`] triangle outlines; [`tripcolor`](Axes::tripcolor) fills each
//! triangle with a flat [`Patch`] color obtained by averaging the three
//! vertices' scalar values and mapping that through a [`LinearNorm`] and the
//! default colormap. Both fold into autoscaling via
//! [`data_limits`](Axes::data_limits).
//!
//! Triangles whose indices fall outside `x`/`y` are skipped (never panic), and
//! mismatched `x`/`y` lengths or empty input draw nothing.

use crate::artist::{Line2D, Patch};
use crate::core::color::{Colormap, LinearNorm, Normalize, default_colormap};

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
    /// default colormap to a single face color. Each triangle is drawn as a
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
        let cmap = default_colormap();

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

    /// Draw isolines of the scalar field `values` over the triangulation
    /// `(x, y, triangles)` at the default seven levels.
    ///
    /// Equivalent to [`tricontour_levels`](Axes::tricontour_levels) with
    /// `n_levels = 7`.
    ///
    /// ![tricontour](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_tricontour.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // One triangle with a ramp field: isolines cross it.
    /// let x = [0.0, 1.0, 0.0];
    /// let y = [0.0, 0.0, 1.0];
    /// ax.tricontour(&x, &y, &[[0, 1, 2]], &[0.0, 1.0, 1.0]);
    /// assert!(ax.data_limits().is_some());
    /// ```
    pub fn tricontour(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
    ) -> &mut Self {
        self.tricontour_levels(x, y, triangles, values, TRI_DEFAULT_N_LEVELS)
    }

    /// Draw `n_levels` isolines of `values` over the triangulation.
    ///
    /// Levels are evenly spaced strictly between the finite value minimum and
    /// maximum (the same convention as [`contour_levels`](Axes::contour_levels)).
    /// Within each triangle the field is linearly interpolated, so a level
    /// crossing is a straight segment whose endpoints are found by inverse
    /// interpolation along the two crossed edges — the triangle analogue of
    /// marching squares, with no ambiguous cases. Each segment becomes a
    /// two-point [`Line2D`] colored by its level through the default colormap.
    ///
    /// Mismatched lengths, an empty mesh, a flat field, or `n_levels == 0`
    /// draw nothing (never panic).
    pub fn tricontour_levels(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
        n_levels: usize,
    ) -> &mut Self {
        let Some((vmin, vmax)) = self.tri_setup(x, y, triangles, values) else {
            return self;
        };
        if vmax <= vmin || n_levels == 0 {
            return self;
        }
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = default_colormap();
        let span = vmax - vmin;
        let n = x.len();

        for k in 0..n_levels {
            let level = vmin + (k + 1) as f64 / (n_levels + 1) as f64 * span;
            let color = cmap.sample(norm.normalize(level));
            for t in valid_triangles(triangles, n) {
                let [a, b, c] = *t;
                if let Some([p0, p1]) = triangle_level_segment(
                    [[x[a], y[a]], [x[b], y[b]], [x[c], y[c]]],
                    [values[a], values[b], values[c]],
                    level,
                ) {
                    self.add_line(
                        Line2D::new(vec![p0[0], p1[0]], vec![p0[1], p1[1]]).with_color(color),
                    );
                }
            }
        }
        self
    }

    /// Fill the bands between isolines of `values` over the triangulation, at
    /// the default seven bands.
    ///
    /// Equivalent to [`tricontourf_levels`](Axes::tricontourf_levels) with
    /// `n_bands = 7`.
    ///
    /// ![tricontour](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_tricontour.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// let x = [0.0, 1.0, 0.0];
    /// let y = [0.0, 0.0, 1.0];
    /// ax.tricontourf(&x, &y, &[[0, 1, 2]], &[0.0, 1.0, 1.0]);
    /// assert!(ax.data_limits().is_some());
    /// ```
    pub fn tricontourf(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
    ) -> &mut Self {
        self.tricontourf_levels(x, y, triangles, values, TRI_DEFAULT_N_LEVELS)
    }

    /// Fill `n_bands` value bands over the triangulation.
    ///
    /// The value range `[vmin, vmax]` is split into `n_bands` equal bands.
    /// Each triangle is clipped against each band's `[lo, hi]` interval in
    /// *value space* (Sutherland–Hodgman with the scalar field interpolated
    /// along edges), producing exact polygonal band pieces — smooth marching
    /// bands, unlike the per-cell flat banding of
    /// [`contourf`](Axes::contourf). Each piece is a [`Patch`] colored by the
    /// band's center value through the default colormap.
    ///
    /// Mismatched lengths, an empty mesh, a flat field, or `n_bands == 0`
    /// draw nothing (never panic).
    pub fn tricontourf_levels(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
        n_bands: usize,
    ) -> &mut Self {
        let Some((vmin, vmax)) = self.tri_setup(x, y, triangles, values) else {
            return self;
        };
        if vmax <= vmin || n_bands == 0 {
            return self;
        }
        let norm = LinearNorm::new(vmin, vmax);
        let cmap = default_colormap();
        let span = vmax - vmin;
        let n = x.len();

        for band in 0..n_bands {
            let lo = vmin + band as f64 / n_bands as f64 * span;
            let hi = vmin + (band + 1) as f64 / n_bands as f64 * span;
            let color = cmap.sample(norm.normalize((lo + hi) / 2.0));
            for t in valid_triangles(triangles, n) {
                let [a, b, c] = *t;
                let tri = [
                    ([x[a], y[a]], values[a]),
                    ([x[b], y[b]], values[b]),
                    ([x[c], y[c]], values[c]),
                ];
                // Clip to v >= lo, then v <= hi. The top band's upper bound is
                // widened a hair so vmax itself is included.
                let hi = if band == n_bands - 1 {
                    hi + span * 1e-12
                } else {
                    hi
                };
                let piece = clip_by_value(&tri, lo, true);
                let piece = clip_by_value(&piece, hi, false);
                if piece.len() >= 3 {
                    let poly: Vec<[f64; 2]> = piece.iter().map(|&(p, _)| p).collect();
                    self.add_patch(
                        Patch::polygon(&poly)
                            .facecolor(Some(color))
                            .edgecolor(Some(color)),
                    );
                }
            }
        }
        self
    }

    /// Shared validation + data-limits registration for the tricontour
    /// family. Returns the finite `(vmin, vmax)` of `values`, or `None` when
    /// the inputs cannot be drawn.
    fn tri_setup(
        &mut self,
        x: &[f64],
        y: &[f64],
        triangles: &[[usize; 3]],
        values: &[f64],
    ) -> Option<(f64, f64)> {
        if x.is_empty() || x.len() != y.len() || values.len() != x.len() {
            return None;
        }
        let n = x.len();
        let mut any = false;
        let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
        for t in valid_triangles(triangles, n) {
            for &i in t.iter() {
                xmin = xmin.min(x[i]);
                xmax = xmax.max(x[i]);
                ymin = ymin.min(y[i]);
                ymax = ymax.max(y[i]);
                any = true;
            }
        }
        if !any {
            return None;
        }
        self.include_data_bbox(xmin, ymin, xmax, ymax);

        let mut vmin = f64::INFINITY;
        let mut vmax = f64::NEG_INFINITY;
        for &v in values {
            if v.is_finite() {
                vmin = vmin.min(v);
                vmax = vmax.max(v);
            }
        }
        (vmin <= vmax).then_some((vmin, vmax))
    }
}

/// Default number of levels/bands for the tricontour family (matches the
/// rectangular-grid `contour`).
const TRI_DEFAULT_N_LEVELS: usize = 7;

/// The straight level-set segment of a linearly interpolated field across one
/// triangle, or `None` when `level` does not cross it.
///
/// Vertices are classified `v >= level`; when the sides differ, the two
/// crossed edges each contribute one inverse-interpolated point.
fn triangle_level_segment(pts: [[f64; 2]; 3], vals: [f64; 3], level: f64) -> Option<[[f64; 2]; 2]> {
    let above = [vals[0] >= level, vals[1] >= level, vals[2] >= level];
    if above.iter().all(|&a| a) || above.iter().all(|&a| !a) {
        return None;
    }
    let mut crossings = Vec::with_capacity(2);
    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
        if above[i] != above[j] {
            let t = (level - vals[i]) / (vals[j] - vals[i]);
            crossings.push([
                pts[i][0] + t * (pts[j][0] - pts[i][0]),
                pts[i][1] + t * (pts[j][1] - pts[i][1]),
            ]);
        }
    }
    (crossings.len() == 2).then(|| [crossings[0], crossings[1]])
}

/// Clip a value-annotated polygon against the half-space `v >= bound` (when
/// `keep_above`) or `v <= bound` (otherwise), interpolating both position and
/// value at the crossings (Sutherland–Hodgman with a scalar field).
fn clip_by_value(poly: &[([f64; 2], f64)], bound: f64, keep_above: bool) -> Vec<([f64; 2], f64)> {
    let inside = |v: f64| if keep_above { v >= bound } else { v <= bound };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (p, vp) = poly[k];
        let (q, vq) = poly[(k + 1) % poly.len()];
        let (pin, qin) = (inside(vp), inside(vq));
        if pin {
            out.push((p, vp));
        }
        if pin != qin {
            let t = (bound - vp) / (vq - vp);
            out.push(([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])], bound));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::artist::Artist;
    use crate::core::color::{Colormap, Rgba, default_colormap};
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
        // the max, so their averaged colors are the colormap endpoints.
        let x = [0.0, 1.0, 0.0, 2.0, 3.0, 2.0];
        let y = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let values = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let tris = [[0, 1, 2], [3, 4, 5]];
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.tripcolor(&x, &y, &tris, &values);
        let cm = default_colormap();
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

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn triangle_level_segment_crosses_a_ramp_at_the_right_x() {
        // Right triangle with v = x: the level set v = 0.5 is the vertical
        // line x = 0.5 clipped to the triangle.
        let pts = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let vals = [0.0, 1.0, 0.0];
        let seg = super::triangle_level_segment(pts, vals, 0.5).expect("level crosses");
        approx(seg[0][0], 0.5);
        approx(seg[1][0], 0.5);
        // No crossing outside the value range.
        assert!(super::triangle_level_segment(pts, vals, 1.5).is_none());
        assert!(super::triangle_level_segment(pts, vals, -0.5).is_none());
    }

    #[test]
    fn clip_by_value_splits_a_ramp_triangle_exactly() {
        // Same ramp triangle; keep v >= 0.5 leaves the right-hand sliver whose
        // area is 1/4 of the whole (similar triangle at half scale).
        let tri = [([0.0, 0.0], 0.0), ([1.0, 0.0], 1.0), ([0.0, 1.0], 0.0)];
        let piece = super::clip_by_value(&tri, 0.5, true);
        assert!(piece.len() >= 3);
        let area = |poly: &[([f64; 2], f64)]| {
            let mut a = 0.0;
            for k in 0..poly.len() {
                let (p, _) = poly[k];
                let (q, _) = poly[(k + 1) % poly.len()];
                a += p[0] * q[1] - q[0] * p[1];
            }
            a.abs() / 2.0
        };
        approx(area(&piece), 0.125);
        // Keeping both halves partitions the triangle's area (0.5).
        let below = super::clip_by_value(&tri, 0.5, false);
        approx(area(&piece) + area(&below), 0.5);
    }

    #[test]
    fn tricontour_draws_level_lines_on_a_ramp_mesh() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // Unit square split into two triangles, v = x.
        let x = [0.0, 1.0, 1.0, 0.0];
        let y = [0.0, 0.0, 1.0, 1.0];
        let v = [0.0, 1.0, 1.0, 0.0];
        ax.tricontour_levels(&x, &y, &[[0, 1, 2], [0, 2, 3]], &v, 3);
        assert!(!ax.lines.is_empty());
        // Every contour segment is vertical (v = x field) at one of the three
        // expected levels x = 0.25, 0.5, 0.75.
        for line in &ax.lines {
            let pts = line.points();
            approx(pts[0][0], pts[1][0]);
            let lx = pts[0][0];
            assert!(
                [0.25, 0.5, 0.75].iter().any(|&e| (lx - e).abs() < 1e-9),
                "unexpected contour x {lx}"
            );
        }
    }

    #[test]
    fn tricontourf_band_pieces_tile_the_mesh_area() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 1.0, 0.0];
        let y = [0.0, 0.0, 1.0, 1.0];
        let v = [0.0, 1.0, 1.0, 0.0];
        ax.tricontourf_levels(&x, &y, &[[0, 1, 2], [0, 2, 3]], &v, 4);
        assert!(!ax.patches.is_empty());
        // The band pieces partition the unit square: total area 1.
        let mut total = 0.0;
        for patch in &ax.patches {
            let verts = patch.path().vertices();
            let mut a = 0.0;
            for k in 0..verts.len() - 1 {
                let p = verts[k];
                let q = verts[k + 1];
                a += p[0] * q[1] - q[0] * p[1];
            }
            total += a.abs() / 2.0;
        }
        approx(total, 1.0);
    }

    #[test]
    fn tricontour_flat_field_or_bad_input_is_a_noop() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        let x = [0.0, 1.0, 0.0];
        let y = [0.0, 0.0, 1.0];
        // Flat field: no levels cross.
        ax.tricontour(&x, &y, &[[0, 1, 2]], &[2.0, 2.0, 2.0]);
        assert!(ax.lines.is_empty());
        // Mismatched values length: nothing drawn, no panic.
        ax.tricontourf(&x, &y, &[[0, 1, 2]], &[1.0]);
        assert!(ax.patches.is_empty());
    }
}
