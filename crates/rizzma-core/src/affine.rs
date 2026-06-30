//! 2D affine transforms.
//!
//! [`Affine2D`] mirrors matplotlib's `Affine2D`, storing the upper two rows of a
//! 3×3 homogeneous matrix
//!
//! ```text
//! [ a  c  e ]
//! [ b  d  f ]
//! [ 0  0  1 ]
//! ```
//!
//! as the six values `(a, b, c, d, e, f)`. A point `(x, y)` is transformed to
//! `(a*x + c*y + e, b*x + d*y + f)`.

/// A 2D affine transformation.
///
/// The transform is stored as the six non-trivial entries of the 3×3 matrix
///
/// ```text
/// [ a  c  e ]
/// [ b  d  f ]
/// [ 0  0  1 ]
/// ```
///
/// matching the memory layout described by matplotlib's `Affine2D.from_values`.
/// Applying the transform to a point `(x, y)` yields
/// `(a*x + c*y + e, b*x + d*y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2D {
    /// Row 0, column 0 (x scaling component).
    a: f64,
    /// Row 1, column 0 (y shear / rotation component).
    b: f64,
    /// Row 0, column 1 (x shear / rotation component).
    c: f64,
    /// Row 1, column 1 (y scaling component).
    d: f64,
    /// Row 0, column 2 (x translation).
    e: f64,
    /// Row 1, column 2 (y translation).
    f: f64,
}

impl Default for Affine2D {
    /// The identity transform. See [`Affine2D::identity`].
    fn default() -> Self {
        Self::identity()
    }
}

impl Affine2D {
    /// Construct an [`Affine2D`] directly from the six matrix entries
    /// `(a, b, c, d, e, f)`, laid out as
    ///
    /// ```text
    /// [ a  c  e ]
    /// [ b  d  f ]
    /// [ 0  0  1 ]
    /// ```
    #[must_use]
    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// The identity transform, which leaves every point unchanged.
    #[must_use]
    pub const fn identity() -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }

    /// A pure translation by `(tx, ty)`.
    #[must_use]
    pub const fn from_translation(tx: f64, ty: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// A pure scaling by `sx` along x and `sy` along y.
    #[must_use]
    pub const fn from_scale(sx: f64, sy: f64) -> Self {
        Self::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// A counter-clockwise rotation by `theta_rad` radians about the origin.
    #[must_use]
    pub fn from_rotation(theta_rad: f64) -> Self {
        let (s, co) = theta_rad.sin_cos();
        Self::new(co, s, -s, co, 0.0, 0.0)
    }

    /// A counter-clockwise rotation by `deg` degrees about the origin.
    #[must_use]
    pub fn from_rotation_deg(deg: f64) -> Self {
        Self::from_rotation(deg.to_radians())
    }

    /// A skew (shear) by the angles `x_rad` and `y_rad` (in radians) along the
    /// x- and y-axes respectively, matching matplotlib's `Affine2D.skew`.
    #[must_use]
    pub fn from_skew(x_rad: f64, y_rad: f64) -> Self {
        Self::new(1.0, y_rad.tan(), x_rad.tan(), 1.0, 0.0, 0.0)
    }

    /// Compose two transforms so that `self` is applied first and `other`
    /// second.
    ///
    /// In matrix terms the result has matrix `other.matrix * self.matrix`,
    /// matching matplotlib's `a + b` (which applies `a` then `b`). For a point
    /// `p`, `self.then(&other).transform_point(p)` equals
    /// `other.transform_point(self.transform_point(p))`.
    #[must_use]
    pub fn then(&self, other: &Affine2D) -> Affine2D {
        // result = other * self
        Affine2D {
            a: other.a * self.a + other.c * self.b,
            b: other.b * self.a + other.d * self.b,
            c: other.a * self.c + other.c * self.d,
            d: other.b * self.c + other.d * self.d,
            e: other.a * self.e + other.c * self.f + other.e,
            f: other.b * self.e + other.d * self.f + other.f,
        }
    }

    /// Return `self` followed by a translation by `(tx, ty)`.
    ///
    /// This post-multiplies in the same sense as matplotlib's in-place
    /// `translate`: the translation is applied after the existing transform.
    #[must_use]
    pub fn translate(&self, tx: f64, ty: f64) -> Self {
        self.then(&Self::from_translation(tx, ty))
    }

    /// Return `self` followed by a scaling by `(sx, sy)`.
    ///
    /// Post-multiplies like matplotlib's in-place `scale`.
    #[must_use]
    pub fn scale(&self, sx: f64, sy: f64) -> Self {
        self.then(&Self::from_scale(sx, sy))
    }

    /// Return `self` followed by a counter-clockwise rotation of `theta_rad`
    /// radians about the origin.
    ///
    /// Post-multiplies like matplotlib's in-place `rotate`.
    #[must_use]
    pub fn rotate(&self, theta_rad: f64) -> Self {
        self.then(&Self::from_rotation(theta_rad))
    }

    /// Return `self` followed by a counter-clockwise rotation of `deg` degrees
    /// about the origin.
    ///
    /// Post-multiplies like matplotlib's in-place `rotate_deg`.
    #[must_use]
    pub fn rotate_deg(&self, deg: f64) -> Self {
        self.then(&Self::from_rotation_deg(deg))
    }

    /// Return `self` followed by a skew by `(x_rad, y_rad)` radians.
    ///
    /// Post-multiplies like matplotlib's in-place `skew`.
    #[must_use]
    pub fn skew(&self, x_rad: f64, y_rad: f64) -> Self {
        self.then(&Self::from_skew(x_rad, y_rad))
    }

    /// Transform a single point `(x, y)`.
    ///
    /// Returns `(a*x + c*y + e, b*x + d*y + f)`.
    #[must_use]
    pub fn transform_point(&self, p: (f64, f64)) -> (f64, f64) {
        let (x, y) = p;
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Transform a slice of points, returning a new vector.
    #[must_use]
    pub fn transform_points(&self, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
        points.iter().map(|&p| self.transform_point(p)).collect()
    }

    /// The determinant of the linear (2×2) part of the transform, `a*d - b*c`.
    #[must_use]
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// The inverse transform, or `None` when the transform is singular
    /// (its [`determinant`](Self::determinant) is zero).
    #[must_use]
    pub fn inverted(&self) -> Option<Affine2D> {
        let det = self.determinant();
        if det == 0.0 {
            return None;
        }
        let inv_det = 1.0 / det;
        let a = self.d * inv_det;
        let b = -self.b * inv_det;
        let c = -self.c * inv_det;
        let d = self.a * inv_det;
        // Inverse translation: -A^{-1} * (e, f).
        let e = -(a * self.e + c * self.f);
        let f = -(b * self.e + d * self.f);
        Some(Affine2D { a, b, c, d, e, f })
    }

    /// Whether this transform equals the identity exactly.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    /// The six matrix entries in the order `[a, b, c, d, e, f]`.
    #[must_use]
    pub const fn matrix(&self) -> [f64; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }
}

impl std::ops::Mul for Affine2D {
    type Output = Affine2D;

    /// Matrix multiplication `self * rhs`.
    ///
    /// As a transform this applies `rhs` first and then `self` (the opposite
    /// argument order of [`then`](Affine2D::then)).
    fn mul(self, rhs: Affine2D) -> Affine2D {
        rhs.then(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "expected {b}, got {a}");
    }

    fn approx_pt(p: (f64, f64), q: (f64, f64)) {
        approx(p.0, q.0);
        approx(p.1, q.1);
    }

    #[test]
    fn identity_leaves_points() {
        let id = Affine2D::identity();
        assert!(id.is_identity());
        approx_pt(id.transform_point((3.0, -7.0)), (3.0, -7.0));
        approx(id.determinant(), 1.0);
    }

    #[test]
    fn rotation_deg_90_maps_x_to_y() {
        let r = Affine2D::from_rotation_deg(90.0);
        approx_pt(r.transform_point((1.0, 0.0)), (0.0, 1.0));
        approx_pt(r.transform_point((0.0, 1.0)), (-1.0, 0.0));
    }

    #[test]
    fn translation_and_scale() {
        let t = Affine2D::from_translation(2.0, 3.0);
        approx_pt(t.transform_point((1.0, 1.0)), (3.0, 4.0));
        let s = Affine2D::from_scale(2.0, 4.0);
        approx_pt(s.transform_point((1.0, 1.0)), (2.0, 4.0));
    }

    #[test]
    fn composition_order_translate_then_scale() {
        // Apply translate first, then scale: (1,1) -> (3,4) -> (6,8).
        let composed = Affine2D::from_translation(2.0, 3.0).scale(2.0, 2.0);
        approx_pt(composed.transform_point((1.0, 1.0)), (6.0, 8.0));

        // `then` matches sequential application.
        let t = Affine2D::from_translation(2.0, 3.0);
        let s = Affine2D::from_scale(2.0, 2.0);
        let chained = t.then(&s);
        approx_pt(
            chained.transform_point((1.0, 1.0)),
            s.transform_point(t.transform_point((1.0, 1.0))),
        );
    }

    #[test]
    fn scale_then_translate_differs() {
        // Scale first then translate: (1,1) -> (2,2) -> (4,5).
        let composed = Affine2D::from_scale(2.0, 2.0).translate(2.0, 3.0);
        approx_pt(composed.transform_point((1.0, 1.0)), (4.0, 5.0));
    }

    #[test]
    fn mul_is_reverse_of_then() {
        let t = Affine2D::from_translation(2.0, 3.0);
        let s = Affine2D::from_scale(2.0, 2.0);
        // s * t applies t first then s == t.then(&s).
        let by_mul = s * t;
        let by_then = t.then(&s);
        approx_pt(
            by_mul.transform_point((1.0, 1.0)),
            by_then.transform_point((1.0, 1.0)),
        );
    }

    #[test]
    fn inverted_round_trips() {
        let m = Affine2D::from_translation(2.0, 3.0)
            .scale(2.0, 4.0)
            .rotate_deg(30.0);
        let inv = m.inverted().expect("invertible");
        let p = (1.7, -2.3);
        approx_pt(inv.transform_point(m.transform_point(p)), p);
        approx_pt(m.transform_point(inv.transform_point(p)), p);
    }

    #[test]
    fn singular_has_no_inverse() {
        let singular = Affine2D::from_scale(0.0, 1.0);
        assert!(singular.inverted().is_none());
        approx(singular.determinant(), 0.0);
    }

    #[test]
    fn skew_matches_matplotlib_form() {
        let x = 0.3_f64;
        let y = -0.2_f64;
        let sk = Affine2D::from_skew(x, y);
        // x' = x + tan(x_rad) * y, y' = tan(y_rad) * x + y.
        approx_pt(
            sk.transform_point((1.0, 2.0)),
            (1.0 + x.tan() * 2.0, y.tan() * 1.0 + 2.0),
        );
    }

    #[test]
    fn transform_points_batch() {
        let s = Affine2D::from_scale(2.0, 3.0);
        let out = s.transform_points(&[(1.0, 1.0), (2.0, 0.0)]);
        approx_pt(out[0], (2.0, 3.0));
        approx_pt(out[1], (4.0, 0.0));
    }

    #[test]
    fn matrix_round_trip() {
        let m = Affine2D::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        assert_eq!(m.matrix(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }
}
