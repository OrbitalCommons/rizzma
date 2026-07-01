//! The [`Patch`] artist: a filled and/or stroked closed shape.
//!
//! Mirrors matplotlib's `patches.Patch` hierarchy (`Rectangle`, `Polygon`,
//! `Circle`, `Ellipse`, `RegularPolygon`, `Wedge`) collapsed into a single type
//! parameterized by its data-space [`Path`]. The shape constructors bake the
//! position and size into the path's vertices, so a [`Patch`] carries data-space
//! geometry directly and is drawn by filling with its `facecolor` and stroking
//! with its `edgecolor`.

use crate::core::{Affine2D, Bbox, Path, color::Rgba};
use crate::render::{CapStyle, GraphicsContext, JoinStyle, Renderer};

use crate::artist::Artist;

/// matplotlib's default patch face color, a muted light blue (`C0` at the
/// lightened patch tint), used when [`Patch`] constructors are not given an
/// explicit face color.
const DEFAULT_FACECOLOR: Rgba = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);

/// Number of straight-line samples used to approximate the arc of a [`wedge`].
///
/// [`Patch::wedge`]: Patch::wedge
const WEDGE_ARC_SAMPLES: usize = 60;

/// A filled and/or stroked closed shape in data coordinates.
///
/// The `path` is stored in data space; the shape constructors
/// ([`rectangle`](Patch::rectangle), [`circle`](Patch::circle), …) bake the
/// requested position and size into it. Drawing fills the path with
/// `facecolor` (matplotlib's `rgbFace`) and strokes its edge with `edgecolor`.
///
/// Defaults mirror matplotlib: a muted light-blue face, opaque black edge,
/// `1.0`-point edge width, butt caps, miter joins, visible, and zorder `1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The shape outline in data space.
    path: Path,
    /// Fill color, or `None` for an unfilled patch.
    facecolor: Option<Rgba>,
    /// Edge (stroke) color, or `None` for no edge.
    edgecolor: Option<Rgba>,
    /// Edge width in points.
    linewidth: f64,
    /// Optional dash pattern as `(offset, on_off_lengths)` in points.
    dashes: Option<(f64, Vec<f64>)>,
    /// Line cap style for the edge.
    cap: CapStyle,
    /// Line join style for the edge.
    join: JoinStyle,
    /// Whether the patch is drawn.
    visible: bool,
    /// Stacking order; higher draws on top.
    zorder: f64,
}

impl Patch {
    /// Construct a [`Patch`] from a data-space `path` with matplotlib-ish
    /// defaults: a muted light-blue face, opaque black edge, `1.0`-point edge
    /// width, butt caps, miter joins, visible, and zorder `1.0`.
    #[must_use]
    pub fn new(path: Path) -> Self {
        Self {
            path,
            facecolor: Some(DEFAULT_FACECOLOR),
            edgecolor: Some(Rgba::BLACK),
            linewidth: 1.0,
            dashes: None,
            cap: CapStyle::Butt,
            join: JoinStyle::Miter,
            visible: true,
            zorder: 1.0,
        }
    }

    /// An axis-aligned rectangle with its lower-left corner at `(x, y)` and the
    /// given `w`idth and `h`eight, as a closed five-point polyline.
    #[must_use]
    pub fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Self {
        let points = [[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]];
        Self::new(Path::from_polyline(&points))
    }

    /// A closed polygon through `points` in data space.
    ///
    /// The polygon is closed by appending the first point when it does not
    /// already repeat, matching matplotlib's `Polygon(closed=True)`.
    #[must_use]
    pub fn polygon(points: &[[f64; 2]]) -> Self {
        let mut verts = points.to_vec();
        if let (Some(&first), Some(&last)) = (verts.first(), verts.last())
            && first != last
        {
            verts.push(first);
        }
        Self::new(Path::from_polyline(&verts))
    }

    /// A circle of radius `r` centered at `center`.
    ///
    /// Built from [`Path::unit_circle`] scaled by `r` and translated to
    /// `center`, so the data-space path traces the circle directly.
    #[must_use]
    pub fn circle(center: [f64; 2], r: f64) -> Self {
        let [cx, cy] = center;
        let transform = Affine2D::from_scale(r, r).translate(cx, cy);
        Self::new(Path::unit_circle().transformed(&transform))
    }

    /// An axis-aligned ellipse centered at `center` with the given full `width`
    /// and `height` (matching matplotlib's `Ellipse`, which takes diameters).
    #[must_use]
    pub fn ellipse(center: [f64; 2], width: f64, height: f64) -> Self {
        let [cx, cy] = center;
        let transform = Affine2D::from_scale(width / 2.0, height / 2.0).translate(cx, cy);
        Self::new(Path::unit_circle().transformed(&transform))
    }

    /// A regular polygon with `num_sides` sides, circumscribed by a circle of
    /// `radius` centered at `center` and rotated `rotation_rad` radians.
    ///
    /// # Panics
    ///
    /// Panics if `num_sides` is zero (via [`Path::unit_regular_polygon`]).
    #[must_use]
    pub fn regular_polygon(
        center: [f64; 2],
        num_sides: usize,
        radius: f64,
        rotation_rad: f64,
    ) -> Self {
        let [cx, cy] = center;
        let transform = Affine2D::from_rotation(rotation_rad)
            .scale(radius, radius)
            .translate(cx, cy);
        Self::new(Path::unit_regular_polygon(num_sides).transformed(&transform))
    }

    /// A filled circular sector ("pie slice") of radius `r` centered at
    /// `center`, sweeping from `theta1_deg` to `theta2_deg` (degrees,
    /// counter-clockwise).
    ///
    /// The path runs from the center out to the start of the arc, samples the
    /// arc, and closes back to the center.
    #[must_use]
    pub fn wedge(center: [f64; 2], r: f64, theta1_deg: f64, theta2_deg: f64) -> Self {
        let [cx, cy] = center;
        let theta1 = theta1_deg.to_radians();
        let theta2 = theta2_deg.to_radians();
        let mut verts = Vec::with_capacity(WEDGE_ARC_SAMPLES + 2);
        verts.push([cx, cy]);
        for i in 0..=WEDGE_ARC_SAMPLES {
            let t = theta1 + (theta2 - theta1) * (i as f64 / WEDGE_ARC_SAMPLES as f64);
            verts.push([cx + r * t.cos(), cy + r * t.sin()]);
        }
        verts.push([cx, cy]);
        Self::new(Path::from_polyline(&verts))
    }

    /// Set the fill color (or `None` for unfilled), returning `self` for
    /// chaining.
    #[must_use]
    pub fn facecolor(mut self, facecolor: Option<Rgba>) -> Self {
        self.facecolor = facecolor;
        self
    }

    /// Set the edge color (or `None` for no edge), returning `self` for
    /// chaining.
    #[must_use]
    pub fn edgecolor(mut self, edgecolor: Option<Rgba>) -> Self {
        self.edgecolor = edgecolor;
        self
    }

    /// Set the edge width in points, returning `self` for chaining.
    #[must_use]
    pub fn linewidth(mut self, linewidth: f64) -> Self {
        self.linewidth = linewidth;
        self
    }

    /// Set the dash pattern as `(offset, on_off_lengths)` in points, returning
    /// `self` for chaining.
    #[must_use]
    pub fn dashes(mut self, dashes: Option<(f64, Vec<f64>)>) -> Self {
        self.dashes = dashes;
        self
    }

    /// Set the stacking order, returning `self` for chaining.
    #[must_use]
    pub fn with_zorder(mut self, zorder: f64) -> Self {
        self.zorder = zorder;
        self
    }

    /// Set whether the patch is drawn, returning `self` for chaining.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// The data-space path traced by this patch.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Draw this patch's fill and stroke style against an already-built path.
    ///
    /// This lets an owning axes pre-transform nonlinear-scale geometry while
    /// reusing the patch's face, edge, width, dash, cap, and join settings.
    pub fn draw_path(&self, renderer: &mut dyn Renderer, path: &Path, transform: &Affine2D) {
        if !self.visible {
            return;
        }
        let gc = GraphicsContext {
            line_width: self.linewidth,
            dashes: self.dashes.clone(),
            cap: self.cap,
            join: self.join,
            stroke: self.edgecolor,
            ..GraphicsContext::new()
        };
        renderer.draw_path(&gc, path, transform, self.facecolor);
    }
}

impl Artist for Patch {
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        self.draw_path(renderer, &self.path, transform);
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        Some(self.path.get_extents())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that records, per `draw_path` call, the path's vertex
    /// count and the fill (face) color.
    #[derive(Default)]
    struct MockRenderer {
        calls: Vec<(usize, Option<Rgba>)>,
    }

    impl Renderer for MockRenderer {
        fn draw_path(
            &mut self,
            _gc: &GraphicsContext,
            path: &Path,
            _transform: &Affine2D,
            fill: Option<Rgba>,
        ) {
            self.calls.push((path.vertices().len(), fill));
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    #[test]
    fn red_rectangle_draws_once_with_fill() {
        let patch = Patch::rectangle(0.0, 0.0, 2.0, 2.0).facecolor(Some(Rgba::RED));
        let mut r = MockRenderer::default();
        patch.draw(&mut r, &Affine2D::identity());
        // A closed rectangle is five vertices, filled red.
        assert_eq!(r.calls, vec![(5, Some(Rgba::RED))]);
    }

    #[test]
    fn rectangle_data_extents() {
        let patch = Patch::rectangle(0.0, 0.0, 2.0, 2.0);
        let e = patch.data_extents().expect("non-empty");
        approx(e.xmin(), 0.0);
        approx(e.ymin(), 0.0);
        approx(e.xmax(), 2.0);
        approx(e.ymax(), 2.0);
    }

    #[test]
    fn rectangle_is_closed() {
        let patch = Patch::rectangle(1.0, 2.0, 3.0, 4.0);
        let verts = patch.path().vertices();
        assert_eq!(verts.len(), 5);
        assert_eq!(verts.first(), verts.last());
    }

    #[test]
    fn invisible_patch_draws_nothing() {
        let patch = Patch::rectangle(0.0, 0.0, 1.0, 1.0).with_visible(false);
        let mut r = MockRenderer::default();
        patch.draw(&mut r, &Affine2D::identity());
        assert!(r.calls.is_empty());
    }

    #[test]
    fn default_zorder_is_one() {
        let patch = Patch::rectangle(0.0, 0.0, 1.0, 1.0);
        assert_eq!(patch.zorder(), 1.0);
    }

    #[test]
    fn with_zorder_sets_trait_zorder() {
        let patch = Patch::rectangle(0.0, 0.0, 1.0, 1.0).with_zorder(7.0);
        assert_eq!(patch.zorder(), 7.0);
    }

    #[test]
    fn polygon_closes_open_input() {
        let patch = Patch::polygon(&[[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
        let verts = patch.path().vertices();
        // Three corners plus the appended closing vertex.
        assert_eq!(verts.len(), 4);
        assert_eq!(verts.first(), verts.last());
    }

    #[test]
    fn circle_extents_match_center_and_radius() {
        let patch = Patch::circle([50.0, 50.0], 30.0);
        let e = patch.data_extents().expect("non-empty");
        approx(e.xmin(), 20.0);
        approx(e.xmax(), 80.0);
        approx(e.ymin(), 20.0);
        approx(e.ymax(), 80.0);
    }

    #[test]
    fn ellipse_uses_full_width_and_height() {
        let patch = Patch::ellipse([0.0, 0.0], 4.0, 2.0);
        let e = patch.data_extents().expect("non-empty");
        approx(e.xmin(), -2.0);
        approx(e.xmax(), 2.0);
        approx(e.ymin(), -1.0);
        approx(e.ymax(), 1.0);
    }

    #[test]
    fn regular_polygon_square_has_four_distinct_corners() {
        // A square: 4 sides, no rotation.
        let patch = Patch::regular_polygon([0.0, 0.0], 4, 1.0, 0.0);
        let verts = patch.path().vertices();
        // unit_regular_polygon(4) yields 5 vertices (4 corners + closing dup).
        assert_eq!(verts.len(), 5);
        // The first four are the distinct corners.
        let corners = &verts[..4];
        for i in 0..corners.len() {
            for j in (i + 1)..corners.len() {
                let d = (corners[i][0] - corners[j][0]).hypot(corners[i][1] - corners[j][1]);
                assert!(d > 1e-6, "corners {i} and {j} coincide");
            }
        }
    }

    #[test]
    fn wedge_stays_within_bounding_quadrant() {
        // 0..90 degrees, radius 10, centered at origin: the sector lives in the
        // first quadrant, bounded by [0, 10] x [0, 10].
        let patch = Patch::wedge([0.0, 0.0], 10.0, 0.0, 90.0);
        let e = patch.data_extents().expect("non-empty");
        approx(e.xmin(), 0.0);
        approx(e.ymin(), 0.0);
        approx(e.xmax(), 10.0);
        approx(e.ymax(), 10.0);
        // Every vertex is inside the radius-10 disk (center plus arc samples).
        for &[x, y] in patch.path().vertices() {
            assert!(
                x.hypot(y) <= 10.0 + 1e-9,
                "vertex ({x}, {y}) outside radius"
            );
        }
    }
}
