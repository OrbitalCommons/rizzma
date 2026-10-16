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
use crate::render::JoinStyle;

/// Half the unit-marker span: markers are built to fit within `[-0.5, 0.5]`.
const HALF: f64 = 0.5;

/// The inner-radius fraction matplotlib uses for the star marker (`'*'`).
const STAR_INNER: f64 = 0.381_966;

/// When a marker's vertices snap to the pixel grid: matplotlib's
/// `MarkerStyle.get_snap_threshold`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Snap {
    /// From this size in pixels up.
    From(f64),
    /// When every segment is horizontal or vertical (Agg's `SNAP_AUTO`).
    Auto,
}

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
    /// How its outline's corners are joined when stroked.
    join: JoinStyle,
    /// When its vertices snap to the pixel grid.
    snap: Snap,
}

impl MarkerStyle {
    /// Build a [`MarkerStyle`] from a matplotlib marker character, with
    /// matplotlib's geometry, fill and stroke joins (`markers.py`).
    ///
    /// Returns `None` for unrecognized characters. Supported markers, all
    /// filled except `'+'` and `'x'`:
    ///
    /// - `'o'` circle, `'.'` point (half the circle's size), `','` pixel.
    /// - `'s'` square.
    /// - `'^'`/`'v'`/`'<'`/`'>'` triangles (up/down/left/right).
    /// - `'D'` diamond (the unit square turned 45°), `'d'` thin diamond.
    /// - `'p'` pentagon, `'h'`/`'H'` hexagon (flat sides vertical or
    ///   horizontal), `'*'` star.
    /// - `'+'` plus and `'x'` cross, each two crossed line subpaths.
    #[must_use]
    pub fn from_char(c: char) -> Option<MarkerStyle> {
        use JoinStyle::{Bevel, Miter, Round};
        let (path, join, snap) = match c {
            // matplotlib rounds a circle's joins, but a circle has no
            // corners to join: miter, the stroke default, draws it the same.
            'o' => (circle(HALF), Miter, Snap::From(f64::INFINITY)),
            '.' => (circle(HALF / 2.0), Miter, Snap::From(f64::INFINITY)),
            ',' => (pixel(), Round, Snap::Auto),
            's' => (square(), Miter, Snap::From(2.0)),
            '^' => (triangle(0.0), Miter, Snap::From(5.0)),
            'v' => (triangle(180.0), Miter, Snap::From(5.0)),
            '<' => (triangle(90.0), Miter, Snap::From(5.0)),
            '>' => (triangle(270.0), Miter, Snap::From(5.0)),
            'D' => (diamond(1.0), Miter, Snap::From(5.0)),
            'd' => (diamond(0.6), Miter, Snap::From(5.0)),
            'p' => (polygon(5, 0.0), Miter, Snap::From(5.0)),
            'h' => (polygon(6, 0.0), Miter, Snap::Auto),
            'H' => (polygon(6, 30.0), Miter, Snap::Auto),
            '*' => (star(), Bevel, Snap::From(5.0)),
            '+' => (plus(), Round, Snap::From(1.0)),
            'x' => (cross(), Round, Snap::From(3.0)),
            _ => return None,
        };
        Some(MarkerStyle {
            path,
            filled: !matches!(c, '+' | 'x'),
            join,
            snap,
        })
    }

    /// How the marker's outline corners are joined when stroked.
    pub(crate) fn join(&self) -> JoinStyle {
        self.join
    }

    /// When the marker's vertices snap to the pixel grid.
    pub(crate) fn snap(&self) -> Snap {
        self.snap
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
}

/// A closed polygon through `vertices`, back to the first.
fn closed(vertices: &[[f64; 2]]) -> Path {
    let mut points = vertices.to_vec();
    points.push(vertices[0]);
    let mut codes = vec![PathCode::LineTo; points.len()];
    codes[0] = PathCode::MoveTo;
    codes[points.len() - 1] = PathCode::ClosePoly;
    Path::new(points, Some(codes))
}

/// The unit circle scaled to `radius`.
fn circle(radius: f64) -> Path {
    Path::unit_circle().transformed(&Affine2D::from_scale(radius, radius))
}

/// The pixel marker: matplotlib's unit square shifted just short of
/// centered, so that Agg snaps it onto exactly one pixel.
fn pixel() -> Path {
    closed(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .transformed(&Affine2D::from_translation(-0.49999, -0.49999))
}

/// A unit square centered on the origin spanning `[-0.5, 0.5]`.
fn square() -> Path {
    closed(&[[-HALF, -HALF], [HALF, -HALF], [HALF, HALF], [-HALF, HALF]])
}

/// A regular polygon inscribed in the radius-`0.5` circle, a vertex up, then
/// turned `deg` counter-clockwise.
fn polygon(num_sides: usize, deg: f64) -> Path {
    Path::unit_regular_polygon(num_sides)
        .transformed(&Affine2D::from_scale(HALF, HALF).rotate_deg(deg))
}

/// A five-pointed star inscribed in the radius-`0.5` circle.
fn star() -> Path {
    Path::unit_regular_star(5, STAR_INNER).transformed(&Affine2D::from_scale(HALF, HALF))
}

/// The unit square turned 45° about its center, its corners `0.707` out,
/// then narrowed to `aspect` of its width: `1.0` for `'D'`, `0.6` for `'d'`.
fn diamond(aspect: f64) -> Path {
    closed(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).transformed(
        &Affine2D::from_translation(-HALF, -HALF)
            .rotate_deg(45.0)
            .scale(aspect, 1.0),
    )
}

/// matplotlib's triangle (apex `(0, 0.5)`, base corners `(±0.5, -0.5)`),
/// turned `deg` counter-clockwise.
fn triangle(deg: f64) -> Path {
    closed(&[[0.0, 1.0], [-1.0, -1.0], [1.0, -1.0]])
        .transformed(&Affine2D::from_scale(HALF, HALF).rotate_deg(deg))
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
    fn triangles_are_filled_and_fill_the_unit_box() {
        for (code, apex) in [
            ('^', [0.0, 0.5]),
            ('v', [0.0, -0.5]),
            ('<', [-0.5, 0.0]),
            ('>', [0.5, 0.0]),
        ] {
            let m = MarkerStyle::from_char(code).unwrap();
            assert!(m.is_filled(), "{code:?}");
            let p = m.path().vertices()[0];
            approx(p[0], apex[0], 1e-9);
            approx(p[1], apex[1], 1e-9);
            let e = m.path().get_extents();
            approx(e.width(), 1.0, 1e-9);
            approx(e.height(), 1.0, 1e-9);
        }
    }

    #[test]
    fn diamonds_are_the_unit_square_turned() {
        let half_diagonal = std::f64::consts::FRAC_1_SQRT_2;
        let e = MarkerStyle::from_char('D').unwrap().path().get_extents();
        approx(e.xmax(), half_diagonal, 1e-9);
        approx(e.ymax(), half_diagonal, 1e-9);
        let e = MarkerStyle::from_char('d').unwrap().path().get_extents();
        approx(e.xmax(), 0.6 * half_diagonal, 1e-9);
        approx(e.ymax(), half_diagonal, 1e-9);
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
