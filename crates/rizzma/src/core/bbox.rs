//! Axis-aligned bounding boxes.
//!
//! [`Bbox`] mirrors matplotlib's `Bbox`, storing two corner points
//! `(x0, y0)` and `(x1, y1)`. The stored corners are not required to be
//! min/max-ordered; accessors such as [`Bbox::xmin`] and [`Bbox::xmax`]
//! normalize on demand, matching matplotlib's behaviour.

use crate::core::affine::Affine2D;

/// An axis-aligned bounding box defined by two corner points.
///
/// The fields `(x0, y0)` and `(x1, y1)` are the box's defining corners. They
/// may be stored in any order: `x0` is not guaranteed to be less than `x1`.
/// Use [`xmin`](Self::xmin) / [`xmax`](Self::xmax) (and the y equivalents) for
/// min/max-normalized values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    /// X coordinate of the first defining corner.
    pub x0: f64,
    /// Y coordinate of the first defining corner.
    pub y0: f64,
    /// X coordinate of the second defining corner.
    pub x1: f64,
    /// Y coordinate of the second defining corner.
    pub y1: f64,
}

impl Bbox {
    /// Create a box from its four extents (the two corner points directly).
    ///
    /// Equivalent to matplotlib's `Bbox.from_extents`.
    #[must_use]
    pub const fn from_extents(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Create a box from a position `(x, y)` plus a `width` and `height`.
    ///
    /// `width` and `height` may be negative. Equivalent to matplotlib's
    /// `Bbox.from_bounds`.
    #[must_use]
    pub const fn from_bounds(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::from_extents(x, y, x + width, y + height)
    }

    /// The unit box from `(0, 0)` to `(1, 1)`.
    #[must_use]
    pub const fn unit() -> Self {
        Self::from_extents(0.0, 0.0, 1.0, 1.0)
    }

    /// The "null" box, from `(+inf, +inf)` to `(-inf, -inf)`.
    ///
    /// This collapsed sentinel matches matplotlib's `Bbox.null` and is used as
    /// a starting placeholder when accumulating data bounds.
    #[must_use]
    pub const fn null() -> Self {
        Self::from_extents(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        )
    }

    /// The smaller of the two x coordinates.
    #[must_use]
    pub fn xmin(&self) -> f64 {
        self.x0.min(self.x1)
    }

    /// The larger of the two x coordinates.
    #[must_use]
    pub fn xmax(&self) -> f64 {
        self.x0.max(self.x1)
    }

    /// The smaller of the two y coordinates.
    #[must_use]
    pub fn ymin(&self) -> f64 {
        self.y0.min(self.y1)
    }

    /// The larger of the two y coordinates.
    #[must_use]
    pub fn ymax(&self) -> f64 {
        self.y0.max(self.y1)
    }

    /// The signed width `x1 - x0`. May be negative if `x0 > x1`.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// The signed height `y1 - y0`. May be negative if `y0 > y1`.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// The signed `(width, height)` of the box.
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        (self.width(), self.height())
    }

    /// The first defining corner `(x0, y0)`.
    #[must_use]
    pub const fn p0(&self) -> (f64, f64) {
        (self.x0, self.y0)
    }

    /// The second defining corner `(x1, y1)`.
    #[must_use]
    pub const fn p1(&self) -> (f64, f64) {
        (self.x1, self.y1)
    }

    /// Whether the point `(x, y)` lies within the box (inclusive of edges),
    /// using min/max-normalized bounds.
    #[must_use]
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.xmin() && x <= self.xmax() && y >= self.ymin() && y <= self.ymax()
    }

    /// The smallest box that contains both `self` and `other`.
    ///
    /// The result is returned in min/max-normalized form.
    #[must_use]
    pub fn union(&self, other: &Bbox) -> Bbox {
        Bbox::from_extents(
            self.xmin().min(other.xmin()),
            self.ymin().min(other.ymin()),
            self.xmax().max(other.xmax()),
            self.ymax().max(other.ymax()),
        )
    }

    /// The intersection of `self` and `other`, or `None` if they do not
    /// overlap.
    ///
    /// The result is returned in min/max-normalized form.
    #[must_use]
    pub fn intersection(&self, other: &Bbox) -> Option<Bbox> {
        let x0 = self.xmin().max(other.xmin());
        let x1 = self.xmax().min(other.xmax());
        let y0 = self.ymin().max(other.ymin());
        let y1 = self.ymax().min(other.ymax());
        if x0 <= x1 && y0 <= y1 {
            Some(Bbox::from_extents(x0, y0, x1, y1))
        } else {
            None
        }
    }

    /// Expand the box around its center by the factors `sw` (width) and `sh`
    /// (height).
    ///
    /// Equivalent to matplotlib's `Bbox.expanded`.
    #[must_use]
    pub fn expanded(&self, sw: f64, sh: f64) -> Bbox {
        let w = self.width();
        let h = self.height();
        let dw = (sw * w - w) / 2.0;
        let dh = (sh * h - h) / 2.0;
        Bbox::from_extents(self.x0 - dw, self.y0 - dh, self.x1 + dw, self.y1 + dh)
    }

    /// Pad the box by `p` on all four sides (the box grows by `p` on each
    /// edge), matching matplotlib's `Bbox.padded` with a single pad value.
    #[must_use]
    pub fn padded(&self, p: f64) -> Bbox {
        Bbox::from_extents(self.x0 - p, self.y0 - p, self.x1 + p, self.y1 + p)
    }

    /// The axis-aligned bounding box of this box after applying `transform`.
    ///
    /// All four corners are transformed and the result is the min/max-
    /// normalized box that bounds them, matching matplotlib's
    /// `BboxBase.transformed` for affine transforms.
    #[must_use]
    pub fn transformed(&self, transform: &Affine2D) -> Bbox {
        let corners = [
            transform.transform_point((self.x0, self.y0)),
            transform.transform_point((self.x0, self.y1)),
            transform.transform_point((self.x1, self.y0)),
            transform.transform_point((self.x1, self.y1)),
        ];
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for (x, y) in corners {
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x);
            ymax = ymax.max(y);
        }
        Bbox::from_extents(xmin, ymin, xmax, ymax)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "expected {b}, got {a}");
    }

    #[test]
    fn from_bounds_and_extents() {
        let b = Bbox::from_bounds(1.0, 2.0, 3.0, 4.0);
        approx(b.x0, 1.0);
        approx(b.y0, 2.0);
        approx(b.x1, 4.0);
        approx(b.y1, 6.0);
        approx(b.width(), 3.0);
        approx(b.height(), 4.0);
    }

    #[test]
    fn unit_box() {
        let u = Bbox::unit();
        assert_eq!(u, Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        assert_eq!(u.size(), (1.0, 1.0));
        assert_eq!(u.p0(), (0.0, 0.0));
        assert_eq!(u.p1(), (1.0, 1.0));
    }

    #[test]
    fn null_sentinel_corners() {
        // Matches matplotlib: corners are (+inf, +inf) and (-inf, -inf), so the
        // raw stored extents are the infinite sentinels.
        let n = Bbox::null();
        assert_eq!(n.x0, f64::INFINITY);
        assert_eq!(n.y0, f64::INFINITY);
        assert_eq!(n.x1, f64::NEG_INFINITY);
        assert_eq!(n.y1, f64::NEG_INFINITY);
    }

    #[test]
    fn normalization_with_swapped_corners() {
        // x0 > x1 and y0 > y1.
        let b = Bbox::from_extents(5.0, 8.0, 1.0, 2.0);
        approx(b.xmin(), 1.0);
        approx(b.xmax(), 5.0);
        approx(b.ymin(), 2.0);
        approx(b.ymax(), 8.0);
        approx(b.width(), -4.0);
        approx(b.height(), -6.0);
        assert!(b.contains_point(3.0, 5.0));
        assert!(!b.contains_point(0.0, 5.0));
    }

    #[test]
    fn contains_point_edges() {
        let b = Bbox::unit();
        assert!(b.contains_point(0.0, 0.0));
        assert!(b.contains_point(1.0, 1.0));
        assert!(b.contains_point(0.5, 0.5));
        assert!(!b.contains_point(1.5, 0.5));
    }

    #[test]
    fn union_of_two() {
        let a = Bbox::from_extents(0.0, 0.0, 2.0, 2.0);
        let b = Bbox::from_extents(1.0, 1.0, 3.0, 4.0);
        let u = a.union(&b);
        assert_eq!(u, Bbox::from_extents(0.0, 0.0, 3.0, 4.0));
    }

    #[test]
    fn intersection_overlap_and_disjoint() {
        let a = Bbox::from_extents(0.0, 0.0, 2.0, 2.0);
        let b = Bbox::from_extents(1.0, 1.0, 3.0, 3.0);
        assert_eq!(
            a.intersection(&b),
            Some(Bbox::from_extents(1.0, 1.0, 2.0, 2.0))
        );

        let c = Bbox::from_extents(5.0, 5.0, 6.0, 6.0);
        assert_eq!(a.intersection(&c), None);
    }

    #[test]
    fn expanded_about_center() {
        let b = Bbox::from_extents(0.0, 0.0, 2.0, 2.0);
        let e = b.expanded(2.0, 2.0);
        // Doubling about center (1,1): from (-1,-1) to (3,3).
        assert_eq!(e, Bbox::from_extents(-1.0, -1.0, 3.0, 3.0));
    }

    #[test]
    fn padded_all_sides() {
        let b = Bbox::from_extents(1.0, 1.0, 2.0, 2.0);
        assert_eq!(b.padded(0.5), Bbox::from_extents(0.5, 0.5, 2.5, 2.5));
    }

    #[test]
    fn transformed_by_translation() {
        let b = Bbox::unit();
        let t = Affine2D::from_translation(2.0, 3.0);
        let out = b.transformed(&t);
        assert_eq!(out, Bbox::from_extents(2.0, 3.0, 3.0, 4.0));
    }

    #[test]
    fn transformed_by_rotation() {
        // Rotate the unit box 90 deg; bounding box becomes [-1,0]x[0,1].
        let b = Bbox::unit();
        let r = Affine2D::from_rotation_deg(90.0);
        let out = b.transformed(&r);
        approx(out.xmin(), -1.0);
        approx(out.xmax(), 0.0);
        approx(out.ymin(), 0.0);
        approx(out.ymax(), 1.0);
    }
}
