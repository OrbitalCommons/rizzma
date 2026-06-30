//! Vector paths.
//!
//! [`Path`] mirrors matplotlib's `Path`: a sequence of 2D vertices paired with
//! an optional sequence of [`PathCode`]s describing how successive vertices are
//! joined (straight lines, quadratic/cubic Béziers, or a subpath close). When
//! the codes are absent the path is an implicit polyline.
//!
//! Curves are handled with a simple uniform/recursive Bézier subdivision; there
//! is no robust simplification or clipping yet.
//
// TODO: lyon/kurbo for robust simplify/clip.

use crate::affine::Affine2D;
use crate::bbox::Bbox;

/// The drawing instruction associated with a path vertex.
///
/// Mirrors matplotlib's path codes `MOVETO`/`LINETO`/`CURVE3`/`CURVE4`/
/// `CLOSEPOLY`. The control-point counts for the curve codes are:
///
/// - [`PathCode::CurveTo3`] (CURVE3): 1 control point + 1 end point.
/// - [`PathCode::CurveTo4`] (CURVE4): 2 control points + 1 end point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathCode {
    /// Pick up the pen and move to the given vertex, starting a new subpath.
    MoveTo,
    /// Draw a straight line from the current point to the given vertex.
    LineTo,
    /// Quadratic Bézier: 1 control point followed by 1 end point.
    CurveTo3,
    /// Cubic Bézier: 2 control points followed by 1 end point.
    CurveTo4,
    /// Close the current subpath back to its starting vertex.
    ClosePoly,
}

impl PathCode {
    /// The number of vertices consumed by this code, matching matplotlib's
    /// `NUM_VERTICES_FOR_CODE`.
    #[must_use]
    pub const fn num_vertices(self) -> usize {
        match self {
            PathCode::MoveTo | PathCode::LineTo | PathCode::ClosePoly => 1,
            PathCode::CurveTo3 => 2,
            PathCode::CurveTo4 => 3,
        }
    }
}

/// A single resolved drawing segment produced by [`Path::iter_segments`].
///
/// Unlike raw [`PathCode`]s, each segment already carries the vertices it needs
/// (curves bundle their control and end points), so consumers do not have to
/// track the path's running position to interpret a code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    /// Move to a point, starting a new subpath.
    MoveTo([f64; 2]),
    /// Straight line to a point.
    LineTo([f64; 2]),
    /// Quadratic Bézier with `(control, end)` points.
    Quad([f64; 2], [f64; 2]),
    /// Cubic Bézier with `(control1, control2, end)` points.
    Cubic([f64; 2], [f64; 2], [f64; 2]),
    /// Close the current subpath.
    Close,
}

/// Control constant for approximating a quarter circle arc with a cubic Bézier.
const CIRCLE_K: f64 = 0.552_284_749_8;

/// A vector path: vertices plus optional per-segment [`PathCode`]s.
///
/// Mirrors matplotlib's `Path`. When `codes` is `None` the path is interpreted
/// as an implicit polyline — the first vertex is a `MoveTo` and every remaining
/// vertex is a `LineTo`. When `codes` is `Some`, its length must equal the
/// number of vertices.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// The path vertices, including curve control points.
    vertices: Vec<[f64; 2]>,
    /// One code per vertex, or `None` for an implicit polyline.
    codes: Option<Vec<PathCode>>,
}

impl Path {
    /// Construct a path from `vertices` and optional `codes`.
    ///
    /// # Panics
    ///
    /// Panics if `codes` is `Some` and its length differs from `vertices`.
    #[must_use]
    pub fn new(vertices: Vec<[f64; 2]>, codes: Option<Vec<PathCode>>) -> Self {
        if let Some(codes) = &codes {
            assert_eq!(
                codes.len(),
                vertices.len(),
                "Path: codes length ({}) must match vertices length ({})",
                codes.len(),
                vertices.len()
            );
        }
        Self { vertices, codes }
    }

    /// Construct an implicit polyline path from a slice of points.
    ///
    /// The first point becomes a `MoveTo` and the rest `LineTo`s (`codes` is
    /// left `None`, matching matplotlib's polyline convention).
    #[must_use]
    pub fn from_polyline(points: &[[f64; 2]]) -> Self {
        Self {
            vertices: points.to_vec(),
            codes: None,
        }
    }

    /// The path vertices, including any curve control points.
    #[must_use]
    pub fn vertices(&self) -> &[[f64; 2]] {
        &self.vertices
    }

    /// The per-vertex codes, or `None` for an implicit polyline.
    #[must_use]
    pub fn codes(&self) -> Option<&[PathCode]> {
        self.codes.as_deref()
    }

    /// The unit rectangle from `(0, 0)` to `(1, 1)`, closed.
    ///
    /// Matches matplotlib's `Path.unit_rectangle`: five vertices tracing the
    /// square corners and returning to the origin.
    #[must_use]
    pub fn unit_rectangle() -> Self {
        let vertices = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::ClosePoly,
        ];
        Self::new(vertices, Some(codes))
    }

    /// The unit-radius circle centered at the origin.
    ///
    /// Approximated with four cubic Bézier arcs (one per quadrant) using the
    /// standard control constant `k = 0.5522847498`. This is a coarser but
    /// simpler approximation than matplotlib's eight-arc `Path.circle`.
    #[must_use]
    pub fn unit_circle() -> Self {
        let k = CIRCLE_K;
        // Start at (1, 0) and sweep counter-clockwise through each quadrant.
        let vertices = vec![
            [1.0, 0.0],
            // Quadrant 1: (1,0) -> (0,1).
            [1.0, k],
            [k, 1.0],
            [0.0, 1.0],
            // Quadrant 2: (0,1) -> (-1,0).
            [-k, 1.0],
            [-1.0, k],
            [-1.0, 0.0],
            // Quadrant 3: (-1,0) -> (0,-1).
            [-1.0, -k],
            [-k, -1.0],
            [0.0, -1.0],
            // Quadrant 4: (0,-1) -> (1,0).
            [k, -1.0],
            [1.0, -k],
            [1.0, 0.0],
        ];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
        ];
        Self::new(vertices, Some(codes))
    }

    /// A regular polygon with `num_sides` sides inscribed in the unit circle,
    /// centered at the origin and pointing up.
    ///
    /// Matches matplotlib's `Path.unit_regular_polygon`: the circumscribing
    /// circle has radius 1 and the first vertex is rotated by `pi/2`.
    ///
    /// # Panics
    ///
    /// Panics if `num_sides` is zero.
    #[must_use]
    pub fn unit_regular_polygon(num_sides: usize) -> Self {
        assert!(num_sides > 0, "unit_regular_polygon: num_sides must be > 0");
        let n = num_sides;
        let mut vertices = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let theta =
                (2.0 * std::f64::consts::PI / n as f64) * i as f64 + std::f64::consts::FRAC_PI_2;
            vertices.push([theta.cos(), theta.sin()]);
        }
        Self::closed_polyline(vertices)
    }

    /// A regular star with `num_points` points inscribed in the unit circle,
    /// centered at the origin and pointing up.
    ///
    /// Matches matplotlib's `Path.unit_regular_star`: the outer radius is 1 and
    /// the inner vertices lie at radius `inner_circle`.
    ///
    /// # Panics
    ///
    /// Panics if `num_points` is zero.
    #[must_use]
    pub fn unit_regular_star(num_points: usize, inner_circle: f64) -> Self {
        assert!(num_points > 0, "unit_regular_star: num_points must be > 0");
        let ns2 = num_points * 2;
        let mut vertices = Vec::with_capacity(ns2 + 1);
        for i in 0..=ns2 {
            let theta =
                (2.0 * std::f64::consts::PI / ns2 as f64) * i as f64 + std::f64::consts::FRAC_PI_2;
            // Odd indices are the inner points.
            let r = if i % 2 == 1 { inner_circle } else { 1.0 };
            vertices.push([r * theta.cos(), r * theta.sin()]);
        }
        Self::closed_polyline(vertices)
    }

    /// Build a closed path from polygon vertices: `MoveTo`, `LineTo`s, then a
    /// final `ClosePoly`.
    fn closed_polyline(vertices: Vec<[f64; 2]>) -> Self {
        let n = vertices.len();
        let mut codes = Vec::with_capacity(n);
        codes.push(PathCode::MoveTo);
        for _ in 1..n.saturating_sub(1) {
            codes.push(PathCode::LineTo);
        }
        if n > 1 {
            codes.push(PathCode::ClosePoly);
        }
        Self::new(vertices, Some(codes))
    }

    /// Iterate the path as resolved [`PathSegment`]s.
    ///
    /// For an implicit polyline (no codes) the first vertex yields a
    /// [`PathSegment::MoveTo`] and the rest [`PathSegment::LineTo`]s. Curve
    /// codes bundle their control and end points into [`PathSegment::Quad`] /
    /// [`PathSegment::Cubic`].
    pub fn iter_segments(&self) -> impl Iterator<Item = PathSegment> + '_ {
        SegmentIter {
            path: self,
            index: 0,
        }
    }

    /// Return a new path with every vertex transformed by `transform`.
    ///
    /// The codes are preserved unchanged.
    #[must_use]
    pub fn transformed(&self, transform: &Affine2D) -> Path {
        let vertices = self
            .vertices
            .iter()
            .map(|&[x, y]| {
                let (tx, ty) = transform.transform_point((x, y));
                [tx, ty]
            })
            .collect();
        Path {
            vertices,
            codes: self.codes.clone(),
        }
    }

    /// The axis-aligned bounding box of the path's vertices.
    ///
    /// This bounds the control and anchor points directly, so for paths
    /// containing Béziers it is an over-estimate of the true geometric extent
    /// (control points can lie outside the drawn curve). Returns
    /// [`Bbox::null`] when the path has no vertices.
    #[must_use]
    pub fn get_extents(&self) -> Bbox {
        if self.vertices.is_empty() {
            return Bbox::null();
        }
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for &[x, y] in &self.vertices {
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x);
            ymax = ymax.max(y);
        }
        Bbox::from_extents(xmin, ymin, xmax, ymax)
    }

    /// Flatten the path into polylines, one `Vec` per subpath.
    ///
    /// Béziers are subdivided until straight within `tolerance`. A new subpath
    /// begins at each `MoveTo`; `ClosePoly` appends the subpath's starting
    /// vertex and ends the subpath. The `tolerance` is a coarse flatness bound
    /// in the path's own units.
    #[must_use]
    pub fn flatten(&self, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
        let tol = tolerance.max(f64::EPSILON);
        let mut subpaths: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut current: Vec<[f64; 2]> = Vec::new();
        let mut start: Option<[f64; 2]> = None;
        let mut cur: Option<[f64; 2]> = None;

        for seg in self.iter_segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    if !current.is_empty() {
                        subpaths.push(std::mem::take(&mut current));
                    }
                    current.push(p);
                    start = Some(p);
                    cur = Some(p);
                }
                PathSegment::LineTo(p) => {
                    current.push(p);
                    cur = Some(p);
                }
                PathSegment::Quad(c, e) => {
                    let p0 = cur.unwrap_or(c);
                    flatten_quad(p0, c, e, tol, &mut current);
                    cur = Some(e);
                }
                PathSegment::Cubic(c1, c2, e) => {
                    let p0 = cur.unwrap_or(c1);
                    flatten_cubic(p0, c1, c2, e, tol, &mut current);
                    cur = Some(e);
                }
                PathSegment::Close => {
                    if let Some(s) = start {
                        current.push(s);
                        cur = Some(s);
                    }
                    if !current.is_empty() {
                        subpaths.push(std::mem::take(&mut current));
                    }
                    start = None;
                }
            }
        }
        if !current.is_empty() {
            subpaths.push(current);
        }
        subpaths
    }

    /// Whether `p` lies inside the path, using an even-odd ray cast.
    ///
    /// The path is first [`flatten`](Self::flatten)ed and each subpath treated
    /// as closed (its last vertex is implicitly joined to its first). This is an
    /// approximation: curves are tested against their flattened polylines and
    /// the winding rule is even-odd rather than non-zero.
    #[must_use]
    pub fn contains_point(&self, p: [f64; 2]) -> bool {
        let [px, py] = p;
        let mut inside = false;
        for sub in self.flatten(1e-6) {
            if sub.len() < 2 {
                continue;
            }
            let n = sub.len();
            for i in 0..n {
                let [xi, yi] = sub[i];
                let [xj, yj] = sub[(i + 1) % n];
                let intersects =
                    (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi;
                if intersects {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// Iterator yielding resolved [`PathSegment`]s for a [`Path`].
struct SegmentIter<'a> {
    path: &'a Path,
    index: usize,
}

impl Iterator for SegmentIter<'_> {
    type Item = PathSegment;

    fn next(&mut self) -> Option<PathSegment> {
        let verts = &self.path.vertices;
        if self.index >= verts.len() {
            return None;
        }
        match &self.path.codes {
            None => {
                let seg = if self.index == 0 {
                    PathSegment::MoveTo(verts[0])
                } else {
                    PathSegment::LineTo(verts[self.index])
                };
                self.index += 1;
                Some(seg)
            }
            Some(codes) => {
                let code = codes[self.index];
                let seg = match code {
                    PathCode::MoveTo => {
                        let s = PathSegment::MoveTo(verts[self.index]);
                        self.index += 1;
                        s
                    }
                    PathCode::LineTo => {
                        let s = PathSegment::LineTo(verts[self.index]);
                        self.index += 1;
                        s
                    }
                    PathCode::ClosePoly => {
                        self.index += 1;
                        PathSegment::Close
                    }
                    PathCode::CurveTo3 => {
                        let c = verts[self.index];
                        let e = verts[self.index + 1];
                        self.index += 2;
                        PathSegment::Quad(c, e)
                    }
                    PathCode::CurveTo4 => {
                        let c1 = verts[self.index];
                        let c2 = verts[self.index + 1];
                        let e = verts[self.index + 2];
                        self.index += 3;
                        PathSegment::Cubic(c1, c2, e)
                    }
                };
                Some(seg)
            }
        }
    }
}

/// Recursively subdivide a quadratic Bézier, pushing the end points of each
/// flat-enough segment (not including the start point `p0`).
fn flatten_quad(p0: [f64; 2], c: [f64; 2], p1: [f64; 2], tol: f64, out: &mut Vec<[f64; 2]>) {
    // Flatness: distance of control point from the chord p0->p1.
    if point_line_distance(c, p0, p1) <= tol {
        out.push(p1);
        return;
    }
    let p01 = midpoint(p0, c);
    let p12 = midpoint(c, p1);
    let mid = midpoint(p01, p12);
    flatten_quad(p0, p01, mid, tol, out);
    flatten_quad(mid, p12, p1, tol, out);
}

/// Recursively subdivide a cubic Bézier, pushing the end points of each
/// flat-enough segment (not including the start point `p0`).
fn flatten_cubic(
    p0: [f64; 2],
    c1: [f64; 2],
    c2: [f64; 2],
    p1: [f64; 2],
    tol: f64,
    out: &mut Vec<[f64; 2]>,
) {
    // Flatness: both control points close to the chord p0->p1.
    let d1 = point_line_distance(c1, p0, p1);
    let d2 = point_line_distance(c2, p0, p1);
    if d1.max(d2) <= tol {
        out.push(p1);
        return;
    }
    // de Casteljau split at t = 0.5.
    let p01 = midpoint(p0, c1);
    let p12 = midpoint(c1, c2);
    let p23 = midpoint(c2, p1);
    let p012 = midpoint(p01, p12);
    let p123 = midpoint(p12, p23);
    let mid = midpoint(p012, p123);
    flatten_cubic(p0, p01, p012, mid, tol, out);
    flatten_cubic(mid, p123, p23, p1, tol, out);
}

/// The midpoint of two points.
fn midpoint(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

/// Perpendicular distance from point `p` to the line through `a` and `b`.
///
/// Falls back to the point-to-`a` distance when `a` and `b` coincide.
fn point_line_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        let ex = p[0] - a[0];
        let ey = p[1] - a[1];
        return (ex * ex + ey * ey).sqrt();
    }
    let cross = (p[0] - a[0]) * dy - (p[1] - a[1]) * dx;
    cross.abs() / len2.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn unit_rectangle_is_closed_unit_square() {
        let p = Path::unit_rectangle();
        assert_eq!(p.vertices().len(), 5);
        assert_eq!(p.vertices()[0], [0.0, 0.0]);
        assert_eq!(p.vertices()[4], [0.0, 0.0]);
        let e = p.get_extents();
        approx(e.xmin(), 0.0);
        approx(e.ymin(), 0.0);
        approx(e.xmax(), 1.0);
        approx(e.ymax(), 1.0);
    }

    #[test]
    fn from_polyline_iterates_moveto_then_linetos() {
        let p = Path::from_polyline(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
        let segs: Vec<_> = p.iter_segments().collect();
        assert_eq!(
            segs,
            vec![
                PathSegment::MoveTo([0.0, 0.0]),
                PathSegment::LineTo([1.0, 0.0]),
                PathSegment::LineTo([1.0, 1.0]),
            ]
        );
    }

    #[test]
    fn transformed_by_scale_doubles_extents() {
        let p = Path::unit_rectangle();
        let scaled = p.transformed(&Affine2D::from_scale(2.0, 2.0));
        let e = scaled.get_extents();
        approx(e.xmin(), 0.0);
        approx(e.ymin(), 0.0);
        approx(e.xmax(), 2.0);
        approx(e.ymax(), 2.0);
    }

    #[test]
    fn iter_segments_yields_cubic() {
        let verts = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
        ];
        let p = Path::new(verts, Some(codes));
        let segs: Vec<_> = p.iter_segments().collect();
        assert_eq!(
            segs,
            vec![
                PathSegment::MoveTo([0.0, 0.0]),
                PathSegment::Cubic([0.0, 1.0], [1.0, 1.0], [1.0, 0.0]),
            ]
        );
    }

    #[test]
    fn iter_segments_yields_quad_and_close() {
        let verts = vec![[0.0, 0.0], [1.0, 1.0], [2.0, 0.0], [0.0, 0.0]];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::CurveTo3,
            PathCode::LineTo,
            PathCode::ClosePoly,
        ];
        let p = Path::new(verts, Some(codes));
        let segs: Vec<_> = p.iter_segments().collect();
        assert_eq!(
            segs,
            vec![
                PathSegment::MoveTo([0.0, 0.0]),
                PathSegment::Quad([1.0, 1.0], [2.0, 0.0]),
                PathSegment::Close,
            ]
        );
    }

    #[test]
    fn contains_point_centroid_and_outside() {
        let p = Path::unit_rectangle();
        assert!(p.contains_point([0.5, 0.5]));
        assert!(!p.contains_point([5.0, 5.0]));
        assert!(!p.contains_point([-1.0, 0.5]));
    }

    #[test]
    fn unit_circle_extents_are_unit() {
        let e = Path::unit_circle().get_extents();
        approx(e.xmin(), -1.0);
        approx(e.ymin(), -1.0);
        approx(e.xmax(), 1.0);
        approx(e.ymax(), 1.0);
    }

    #[test]
    fn unit_circle_contains_origin_not_far_point() {
        let c = Path::unit_circle();
        assert!(c.contains_point([0.0, 0.0]));
        assert!(c.contains_point([0.5, 0.5]));
        assert!(!c.contains_point([2.0, 0.0]));
    }

    #[test]
    fn unit_regular_polygon_triangle_points_up() {
        let p = Path::unit_regular_polygon(3);
        // 3 distinct corners + 1 closing duplicate + ClosePoly => 4 vertices.
        assert_eq!(p.vertices().len(), 4);
        // First vertex points straight up.
        approx(p.vertices()[0][0], 0.0);
        approx(p.vertices()[0][1], 1.0);
        assert!(p.contains_point([0.0, 0.0]));
    }

    #[test]
    fn unit_regular_star_inner_radius() {
        let s = Path::unit_regular_star(5, 0.5);
        // First (outer) vertex points up at radius 1.
        approx(s.vertices()[0][0], 0.0);
        approx(s.vertices()[0][1], 1.0);
        let e = s.get_extents();
        approx(e.ymax(), 1.0);
        assert!(s.contains_point([0.0, 0.0]));
    }

    #[test]
    #[should_panic(expected = "codes length")]
    fn new_rejects_mismatched_codes() {
        let _ = Path::new(vec![[0.0, 0.0], [1.0, 1.0]], Some(vec![PathCode::MoveTo]));
    }

    #[test]
    fn flatten_splits_subpaths() {
        // Two separate line subpaths.
        let verts = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::MoveTo,
            PathCode::LineTo,
        ];
        let p = Path::new(verts, Some(codes));
        let subs = p.flatten(1e-3);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], vec![[0.0, 0.0], [1.0, 0.0]]);
        assert_eq!(subs[1], vec![[2.0, 0.0], [3.0, 0.0]]);
    }

    #[test]
    fn flatten_cubic_approximates_endpoint() {
        let verts = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let codes = vec![
            PathCode::MoveTo,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
            PathCode::CurveTo4,
        ];
        let p = Path::new(verts, Some(codes));
        let subs = p.flatten(1e-3);
        assert_eq!(subs.len(), 1);
        let pts = &subs[0];
        assert_eq!(pts.first().copied(), Some([0.0, 0.0]));
        assert_eq!(pts.last().copied(), Some([1.0, 0.0]));
        assert!(pts.len() > 2, "curve should be subdivided");
    }
}
