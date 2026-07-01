//! Point markers in matplotlib's unit-marker space.
//!
//! A [`MarkerStyle`] holds a single unit-sized marker [`Path`] centered on the
//! origin and a [`filled`](MarkerStyle::is_filled) flag. The path lives in a
//! y-up unit space whose extents span roughly `[-0.5, 0.5]` on each axis,
//! matching matplotlib's unit markers, so the marker spans about `1.0` unit
//! across.
//!
//! The paths are meant to be fed to [`Renderer::draw_markers`](crate::render::Renderer::draw_markers), one marker
//! instance per data point: scale the unit path to the desired point size with
//! [`MarkerStyle::scaled`], then let the renderer translate a copy to each data
//! location.

use crate::artist::Path;
use crate::core::Affine2D;
use crate::core::path::PathCode;

/// Half the unit-marker span: markers are built to fit within `[-0.5, 0.5]`.
const HALF: f64 = 0.5;

/// The inner-radius fraction matplotlib uses for the star marker (`'*'`).
const STAR_INNER: f64 = 0.381_966;

/// A matplotlib-style point marker.
///
/// Wraps a unit-sized marker [`Path`] (centered on the origin, spanning roughly
/// `1.0` unit, extents about `[-0.5, 0.5]` per axis) together with whether the
/// marker is filled. Construct one from a matplotlib marker character with
/// [`MarkerStyle::from_char`].
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerStyle {
    /// The unit marker geometry, centered on the origin in y-up unit space.
    path: Path,
    /// Whether the marker is filled (`true`) or stroked only (`false`).
    filled: bool,
}

impl MarkerStyle {
    /// Build a [`MarkerStyle`] from a matplotlib marker character.
    ///
    /// Returns `None` for unrecognized characters. Supported markers:
    ///
    /// - `'o'` filled circle, `'.'` small filled point, `','` pixel.
    /// - `'s'` filled square.
    /// - `'^'`/`'v'`/`'<'`/`'>'` triangles (up/down/left/right).
    /// - `'D'` filled diamond, `'d'` thin diamond.
    /// - `'p'` pentagon, `'h'`/`'H'` hexagon, `'*'` filled star.
    /// - `'+'` plus and `'x'` cross, both unfilled and built from two crossed
    ///   line subpaths.
    #[must_use]
    pub fn from_char(c: char) -> Option<MarkerStyle> {
        let style = match c {
            'o' => MarkerStyle::filled(unit_circle()),
            '.' => MarkerStyle::filled(point_circle()),
            ',' => MarkerStyle::filled(pixel()),
            's' => MarkerStyle::filled(square()),
            '^' => MarkerStyle::unfilled(triangle(Triangle::Up)),
            'v' => MarkerStyle::unfilled(triangle(Triangle::Down)),
            '<' => MarkerStyle::unfilled(triangle(Triangle::Left)),
            '>' => MarkerStyle::unfilled(triangle(Triangle::Right)),
            'D' => MarkerStyle::filled(diamond(1.0)),
            'd' => MarkerStyle::unfilled(diamond(0.6)),
            'p' => MarkerStyle::unfilled(scaled_polygon(5)),
            'h' | 'H' => MarkerStyle::unfilled(scaled_polygon(6)),
            '*' => MarkerStyle::filled(star()),
            '+' => MarkerStyle::unfilled(plus()),
            'x' => MarkerStyle::unfilled(cross()),
            _ => return None,
        };
        Some(style)
    }

    /// The unit marker path, centered on the origin in y-up unit space.
    ///
    /// Feed a [`scaled`](Self::scaled) copy to [`Renderer::draw_markers`](crate::render::Renderer::draw_markers).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the marker is filled rather than stroked only.
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.filled
    }

    /// The marker path scaled by `size` (in points) about the origin.
    ///
    /// The unit marker spans about `1.0` unit, so `size` is the marker's
    /// device-space span. The result stays centered on the origin and is meant
    /// to be passed to [`Renderer::draw_markers`](crate::render::Renderer::draw_markers), which places one copy at each
    /// data point.
    #[must_use]
    pub fn scaled(&self, size: f64) -> Path {
        self.path.transformed(&Affine2D::from_scale(size, size))
    }

    /// A filled marker from a unit path.
    fn filled(path: Path) -> MarkerStyle {
        MarkerStyle { path, filled: true }
    }

    /// An unfilled (stroked-only) marker from a unit path.
    fn unfilled(path: Path) -> MarkerStyle {
        MarkerStyle {
            path,
            filled: false,
        }
    }
}

/// The four cardinal triangle orientations.
#[derive(Debug, Clone, Copy)]
enum Triangle {
    /// Apex pointing up (`'^'`).
    Up,
    /// Apex pointing down (`'v'`).
    Down,
    /// Apex pointing left (`'<'`).
    Left,
    /// Apex pointing right (`'>'`).
    Right,
}

/// The unit circle scaled to radius `0.5`.
fn unit_circle() -> Path {
    Path::unit_circle().transformed(&Affine2D::from_scale(HALF, HALF))
}

/// A small filled point: a circle of one-fifth the unit radius.
fn point_circle() -> Path {
    Path::unit_circle().transformed(&Affine2D::from_scale(HALF * 0.2, HALF * 0.2))
}

/// The pixel marker: a tiny square roughly one device unit across.
fn pixel() -> Path {
    let r = HALF * 0.1;
    Path::new(
        vec![[-r, -r], [r, -r], [r, r], [-r, r], [-r, -r]],
        Some(vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::ClosePoly,
        ]),
    )
}

/// A unit square centered on the origin spanning `[-0.5, 0.5]`.
fn square() -> Path {
    Path::new(
        vec![
            [-HALF, -HALF],
            [HALF, -HALF],
            [HALF, HALF],
            [-HALF, HALF],
            [-HALF, -HALF],
        ],
        Some(vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::ClosePoly,
        ]),
    )
}

/// A regular polygon inscribed in the radius-`0.5` circle, pointing up.
fn scaled_polygon(num_sides: usize) -> Path {
    Path::unit_regular_polygon(num_sides).transformed(&Affine2D::from_scale(HALF, HALF))
}

/// A five-pointed star inscribed in the radius-`0.5` circle.
fn star() -> Path {
    Path::unit_regular_star(5, STAR_INNER).transformed(&Affine2D::from_scale(HALF, HALF))
}

/// A diamond whose vertical extent is `0.5 * aspect` (a square rotated 45°).
///
/// `aspect` of `1.0` gives the full diamond (`'D'`); a smaller value narrows it
/// into the thin diamond (`'d'`).
fn diamond(aspect: f64) -> Path {
    let half_x = HALF * aspect;
    Path::new(
        vec![
            [0.0, -HALF],
            [half_x, 0.0],
            [0.0, HALF],
            [-half_x, 0.0],
            [0.0, -HALF],
        ],
        Some(vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::ClosePoly,
        ]),
    )
}

/// A cardinal triangle inscribed in the radius-`0.5` circle.
fn triangle(orientation: Triangle) -> Path {
    let deg = match orientation {
        Triangle::Up => 0.0,
        Triangle::Down => 180.0,
        Triangle::Left => 90.0,
        Triangle::Right => -90.0,
    };
    Path::unit_regular_polygon(3).transformed(&Affine2D::from_rotation_deg(deg).scale(HALF, HALF))
}

/// The plus marker: two crossed line subpaths, unfilled.
fn plus() -> Path {
    Path::new(
        vec![[-HALF, 0.0], [HALF, 0.0], [0.0, -HALF], [0.0, HALF]],
        Some(vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::MoveTo,
            PathCode::LineTo,
        ]),
    )
}

/// The cross (`'x'`) marker: two crossed diagonal line subpaths, unfilled.
fn cross() -> Path {
    Path::new(
        vec![[-HALF, -HALF], [HALF, HALF], [-HALF, HALF], [HALF, -HALF]],
        Some(vec![
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::MoveTo,
            PathCode::LineTo,
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "expected {b}, got {a}");
    }

    fn count_movetos(path: &Path) -> usize {
        path.codes()
            .map(|codes| codes.iter().filter(|&&c| c == PathCode::MoveTo).count())
            .unwrap_or(0)
    }

    #[test]
    fn circle_is_filled_and_unit_centered() {
        let m = MarkerStyle::from_char('o').unwrap();
        assert!(m.is_filled());
        let e = m.path().get_extents();
        approx((e.xmin() + e.xmax()) / 2.0, 0.0, 1e-9);
        approx((e.ymin() + e.ymax()) / 2.0, 0.0, 1e-9);
        approx(e.xmax() - e.xmin(), 1.0, 1e-9);
        approx(e.ymax() - e.ymin(), 1.0, 1e-9);
    }

    #[test]
    fn square_has_four_distinct_corners() {
        let m = MarkerStyle::from_char('s').unwrap();
        let mut corners: Vec<[f64; 2]> = m.path().vertices().to_vec();
        // Drop the closing duplicate of the first corner.
        corners.dedup();
        if corners.first() == corners.last() {
            corners.pop();
        }
        assert_eq!(corners.len(), 4, "square should have 4 distinct corners");
    }

    #[test]
    fn plus_is_unfilled_with_two_subpaths() {
        let m = MarkerStyle::from_char('+').unwrap();
        assert!(!m.is_filled());
        assert!(
            count_movetos(m.path()) >= 2,
            "plus should contain two subpaths"
        );
    }

    #[test]
    fn cross_is_unfilled_with_two_subpaths() {
        let m = MarkerStyle::from_char('x').unwrap();
        assert!(!m.is_filled());
        assert!(count_movetos(m.path()) >= 2);
    }

    #[test]
    fn triangle_up_apex_is_above_origin() {
        let m = MarkerStyle::from_char('^').unwrap();
        let e = m.path().get_extents();
        assert!(e.ymax() > 0.0, "apex should be above origin");
        // Apex height and base depth are within reason of each other.
        approx(e.ymax(), -e.ymin() * 2.0, 1e-6);
    }

    #[test]
    fn unknown_char_returns_none() {
        assert!(MarkerStyle::from_char('?').is_none());
    }

    #[test]
    fn scaled_doubles_extents() {
        let m = MarkerStyle::from_char('o').unwrap();
        let e1 = m.scaled(1.0).get_extents();
        let e2 = m.scaled(2.0).get_extents();
        approx(e2.xmax() - e2.xmin(), 2.0 * (e1.xmax() - e1.xmin()), 1e-9);
        approx(e2.ymax() - e2.ymin(), 2.0 * (e1.ymax() - e1.ymin()), 1e-9);
    }
}
