//! The [`pie`](Axes::pie) plotting method on [`Axes`].
//!
//! Mirrors matplotlib's `Axes.pie` in its default geometry: slices are sized in
//! proportion to their values, the first slice starts at the top (90°), and
//! slices sweep counter-clockwise. Each slice is a [`Patch::wedge`] of unit
//! radius centered at the origin, drawn with the next property-cycle color and a
//! thin white edge. The axes are made circular via
//! [`set_aspect_equal`](Axes::set_aspect_equal) and stripped of frame/ticks via
//! [`set_axis_off`](Axes::set_axis_off).

use crate::artist::Patch;
use crate::core::color::Rgba;

use crate::figure::Axes;

/// Starting angle of the first slice, in degrees: matplotlib's default top
/// (12 o'clock) position.
const PIE_START_DEG: f64 = 90.0;

/// Edge width in points for the thin white separators between slices.
const PIE_EDGE_WIDTH: f64 = 1.0;

/// Half-extent of the square data window around the unit-radius pie, leaving a
/// small margin so the circle is not clipped by the axes edge.
const PIE_HALF_EXTENT: f64 = 1.15;

impl Axes {
    /// Draw a pie chart of `values`, one wedge per value sized by its fraction
    /// of the total.
    ///
    /// Following matplotlib's defaults, the first slice starts at 90° (the top)
    /// and slices sweep counter-clockwise. Each wedge has unit radius, is
    /// centered at the origin, takes the next property-cycle color, and is
    /// outlined with a thin white edge. The call also makes the axes
    /// equal-aspect (so the pie reads as a circle), turns the axis off (no
    /// frame, ticks, or tick labels), and sets symmetric limits so the pie is
    /// centered.
    ///
    /// Negative or zero totals are not meaningful for a pie; non-positive values
    /// simply contribute zero-width (or reversed) wedges.
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// ax.pie(&[1.0, 2.0, 3.0]);
    /// ```
    // TODO: support slice labels and `autopct` percentage annotations.
    pub fn pie(&mut self, values: &[f64]) -> &mut Self {
        let total: f64 = values.iter().sum();
        let mut theta = PIE_START_DEG;
        for &value in values {
            let fraction = if total != 0.0 { value / total } else { 0.0 };
            let sweep = fraction * 360.0;
            let theta1 = theta;
            let theta2 = theta + sweep;
            let color = self.next_cycle_color();
            self.add_patch(
                Patch::wedge([0.0, 0.0], 1.0, theta1, theta2)
                    .facecolor(Some(color))
                    .edgecolor(Some(Rgba::WHITE))
                    .linewidth(PIE_EDGE_WIDTH),
            );
            theta = theta2;
        }
        self.set_aspect_equal();
        self.set_axis_off();
        self.set_xlim(-PIE_HALF_EXTENT, PIE_HALF_EXTENT);
        self.set_ylim(-PIE_HALF_EXTENT, PIE_HALF_EXTENT);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bbox;

    /// Recover a wedge's angular span (degrees) from its first and last arc
    /// vertices about the origin. The path is `center, arc…, center`, so the
    /// arc endpoints are the second and second-to-last vertices.
    fn wedge_span_deg(patch: &Patch) -> f64 {
        let verts = patch.path().vertices();
        let start = verts[1];
        let end = verts[verts.len() - 2];
        let a0 = start[1].atan2(start[0]).to_degrees();
        let a1 = end[1].atan2(end[0]).to_degrees();
        let mut span = a1 - a0;
        while span < 0.0 {
            span += 360.0;
        }
        span
    }

    #[test]
    fn pie_creates_proportional_wedges() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.pie(&[1.0, 2.0, 3.0]);
        assert_eq!(ax.patches.len(), 3);
        // 1:2:3 of 360° -> 60°, 120°, 180°.
        let spans: Vec<f64> = ax.patches.iter().map(wedge_span_deg).collect();
        assert!((spans[0] - 60.0).abs() < 1e-6, "got {}", spans[0]);
        assert!((spans[1] - 120.0).abs() < 1e-6, "got {}", spans[1]);
        assert!((spans[2] - 180.0).abs() < 1e-6, "got {}", spans[2]);
    }

    #[test]
    fn pie_configures_equal_aspect_and_hidden_axis() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.pie(&[1.0, 1.0]);
        assert!(ax.aspect_equal);
        assert!(!ax.axis_visible);
    }
}
