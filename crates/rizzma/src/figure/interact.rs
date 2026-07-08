//! Host-agnostic interaction tools: wheel zoom, drag pan, hover, home reset.
//!
//! [`Interactor`] owns a [`Figure`] and consumes [`Event`]s, mutating axes
//! limits through the same public API (`set_xlim`/`set_ylim`) a user would
//! call. It contains no DOM or backend types, so every behavior is testable
//! natively by synthesizing events.
//!
//! Pan and zoom operate in **scale-transformed space** (via each axes' data
//! scale), so log/symlog/asinh/logit axes pan and zoom the way the eye
//! expects — a fixed pixel drag shifts a fixed number of decades on a log
//! axis — with no per-scale special cases here.

use crate::figure::Figure;
use crate::figure::event::{Event, MouseButton};

/// Per-wheel-detent zoom factor: one line of scroll scales the view by 1.1.
const ZOOM_BASE: f64 = 1.1;

/// Clamp on the per-event zoom factor so a trackpad momentum burst cannot
/// teleport the view.
const ZOOM_FACTOR_RANGE: (f64, f64) = (0.5, 2.0);

/// What an event did, so the host knows whether to repaint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// Nothing changed; no repaint needed.
    Unchanged,
    /// Axes limits (or the canvas) changed; schedule a repaint.
    NeedsRedraw,
    /// The cursor is over axes `axes` at data coordinates `(x, y)`.
    Hover {
        /// Index of the hovered axes.
        axes: usize,
        /// Cursor x in data coordinates.
        x: f64,
        /// Cursor y in data coordinates.
        y: f64,
    },
}

/// One axes' explicit `(xlim, ylim)` limit pairs.
type AxesLimits = ((f64, f64), (f64, f64));

/// An in-progress left-button pan drag.
#[derive(Clone, Copy)]
struct Drag {
    /// The axes being panned.
    axes: usize,
    /// The grabbed point in **scale-transformed** coordinates; kept under the
    /// cursor for the duration of the drag.
    anchor: (f64, f64),
}

/// Interaction state machine over an owned [`Figure`].
pub struct Interactor {
    /// The figure being interacted with.
    fig: Figure,
    /// Per-axes `(xlim, ylim)` captured before the first limit-changing
    /// interaction, restored by double-click.
    home: Option<Vec<AxesLimits>>,
    /// The active pan drag, if any.
    drag: Option<Drag>,
}

impl Interactor {
    /// Wrap `fig` for interaction.
    #[must_use]
    pub fn new(fig: Figure) -> Self {
        Self {
            fig,
            home: None,
            drag: None,
        }
    }

    /// A shared reference to the wrapped figure (for rendering and readouts).
    #[must_use]
    pub fn figure(&self) -> &Figure {
        &self.fig
    }

    /// A mutable reference to the wrapped figure.
    ///
    /// The captured home limits are kept; if you restructure the axes, the
    /// next double-click restores the limits captured before the change.
    pub fn figure_mut(&mut self) -> &mut Figure {
        &mut self.fig
    }

    /// Unwrap back into the figure.
    #[must_use]
    pub fn into_figure(self) -> Figure {
        self.fig
    }

    /// Consume one event, returning what the host should do next.
    pub fn handle(&mut self, ev: Event) -> Outcome {
        match ev {
            Event::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => self.begin_pan(x, y),
            Event::MouseDown { .. } => Outcome::Unchanged,
            Event::MouseMove { x, y } => match self.drag {
                Some(_) => self.continue_pan(x, y),
                None => self.hover(x, y),
            },
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            }
            | Event::Leave => {
                self.drag = None;
                Outcome::Unchanged
            }
            Event::MouseUp { .. } => Outcome::Unchanged,
            Event::Wheel { x, y, dy } => self.zoom(x, y, dy),
            Event::DoubleClick { x, y } => self.go_home(x, y),
            // The figure's size in inches is fixed; a host resize just needs a
            // repaint at the (unchanged) figure size.
            Event::Resize { .. } => Outcome::NeedsRedraw,
        }
    }

    /// The cursor position in scale-transformed coordinates for `axes`,
    /// **without** the inside-the-axes check (a captured drag may leave the
    /// axes rectangle).
    fn pixel_to_scaled(&self, axes: usize, px: f64, py: f64) -> Option<(f64, f64)> {
        let ax = self.fig.axes().get(axes)?;
        let (fig_w, fig_h) = self.fig.size_px();
        let (_rect, td) = ax.pixel_rect_and_trans_data_in(
            fig_w,
            fig_h,
            self.fig.xlim_override_for(axes),
            self.fig.layout_rect_for(axes, fig_w, fig_h),
        );
        let inv = td.inverted()?;
        Some(inv.transform_point((px, fig_h - py)))
    }

    /// The current effective limits of `axes` in scale-transformed space, as
    /// `((sx_lo, sx_hi), (sy_lo, sy_hi))`.
    ///
    /// Reads the *scale-limited* limits (the draw-time values), so stored
    /// limits that sit outside the scale's domain — e.g. a log axis whose raw
    /// limits went non-positive — are repaired before being transformed,
    /// instead of mapping to non-finite scaled coordinates.
    fn scaled_limits(&self, axes: usize) -> Option<((f64, f64), (f64, f64))> {
        let ax = self.fig.axes().get(axes)?;
        let ((xlo, xhi), (ylo, yhi)) = ax.scale_limited_effective_limits();
        let t = ax.data_to_scaled();
        let [sx_lo, sy_lo] = t.map_point(xlo, ylo);
        let [sx_hi, sy_hi] = t.map_point(xhi, yhi);
        Some(((sx_lo, sx_hi), (sy_lo, sy_hi)))
    }

    /// Apply scale-transformed limits back to `axes` as explicit data limits,
    /// returning whether they were stored.
    ///
    /// The inverse transform can saturate at the scale's domain edges — a
    /// logit inverse rounding to exactly `0.0`/`1.0`, a log inverse
    /// underflowing to `0.0` or overflowing to `inf` — so candidates are
    /// rejected when non-finite and clamped to the scale's domain before
    /// `set_xlim`/`set_ylim`. One extreme wheel or drag therefore cannot
    /// poison the axes for every later interaction.
    fn set_scaled_limits(&mut self, axes: usize, sx: (f64, f64), sy: (f64, f64)) -> bool {
        let ax = &mut self.fig.axes_mut()[axes];
        let t = ax.data_to_scaled();
        let [xlo, ylo] = t.inverse_point(sx.0, sy.0);
        let [xhi, yhi] = t.inverse_point(sx.1, sy.1);
        if ![xlo, xhi, ylo, yhi].iter().all(|v| v.is_finite()) {
            return false;
        }
        let ((xlo, xhi), (ylo, yhi)) = ax.clamp_limits_to_scale((xlo, xhi), (ylo, yhi));
        ax.set_xlim(xlo, xhi);
        ax.set_ylim(ylo, yhi);
        true
    }

    /// Capture every axes' current effective limits as "home", once.
    fn capture_home(&mut self) {
        if self.home.is_none() {
            self.home = Some(
                self.fig
                    .axes()
                    .iter()
                    .map(|ax| ax.effective_limits())
                    .collect(),
            );
        }
    }

    fn begin_pan(&mut self, x: f64, y: f64) -> Outcome {
        let Some(axes) = self.fig.axes_at(x, y) else {
            return Outcome::Unchanged;
        };
        let Some(anchor) = self.pixel_to_scaled(axes, x, y) else {
            return Outcome::Unchanged;
        };
        self.capture_home();
        self.drag = Some(Drag { axes, anchor });
        Outcome::Unchanged
    }

    fn continue_pan(&mut self, x: f64, y: f64) -> Outcome {
        let Some(Drag { axes, anchor }) = self.drag else {
            return Outcome::Unchanged;
        };
        let Some(under) = self.pixel_to_scaled(axes, x, y) else {
            return Outcome::Unchanged;
        };
        let Some((sx, sy)) = self.scaled_limits(axes) else {
            return Outcome::Unchanged;
        };
        // Shift limits so the grabbed point lands back under the cursor.
        let (dx, dy) = (anchor.0 - under.0, anchor.1 - under.1);
        if dx == 0.0 && dy == 0.0 {
            return Outcome::Unchanged;
        }
        if self.set_scaled_limits(axes, (sx.0 + dx, sx.1 + dx), (sy.0 + dy, sy.1 + dy)) {
            Outcome::NeedsRedraw
        } else {
            Outcome::Unchanged
        }
    }

    fn zoom(&mut self, x: f64, y: f64, dy: f64) -> Outcome {
        let Some(axes) = self.fig.axes_at(x, y) else {
            return Outcome::Unchanged;
        };
        let Some((cx, cy)) = self.pixel_to_scaled(axes, x, y) else {
            return Outcome::Unchanged;
        };
        let Some((sx, sy)) = self.scaled_limits(axes) else {
            return Outcome::Unchanged;
        };
        let k = ZOOM_BASE
            .powf(dy)
            .clamp(ZOOM_FACTOR_RANGE.0, ZOOM_FACTOR_RANGE.1);
        if k == 1.0 {
            return Outcome::Unchanged;
        }
        self.capture_home();
        // Scale each limit about the cursor's scaled coordinate, so the data
        // point under the cursor stays under the cursor.
        let nx = (cx - (cx - sx.0) * k, cx + (sx.1 - cx) * k);
        let ny = (cy - (cy - sy.0) * k, cy + (sy.1 - cy) * k);
        if self.set_scaled_limits(axes, nx, ny) {
            Outcome::NeedsRedraw
        } else {
            Outcome::Unchanged
        }
    }

    fn go_home(&mut self, x: f64, y: f64) -> Outcome {
        let Some(axes) = self.fig.axes_at(x, y) else {
            return Outcome::Unchanged;
        };
        let Some(limits) = self.home.as_ref().and_then(|h| h.get(axes).copied()) else {
            return Outcome::Unchanged;
        };
        let ((xlo, xhi), (ylo, yhi)) = limits;
        let ax = &mut self.fig.axes_mut()[axes];
        ax.set_xlim(xlo, xhi);
        ax.set_ylim(ylo, yhi);
        Outcome::NeedsRedraw
    }

    fn hover(&self, x: f64, y: f64) -> Outcome {
        let Some(axes) = self.fig.axes_at(x, y) else {
            return Outcome::Unchanged;
        };
        match self.fig.pixel_to_data(axes, x, y) {
            Some((dx, dy)) => Outcome::Hover { axes, x: dx, y: dy },
            None => Outcome::Unchanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4x3 in @ 100 dpi figure with one axes and a line, explicit limits.
    fn fixture() -> Interactor {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.plot(&[0.0, 5.0, 10.0], &[0.0, 5.0, 10.0]);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        Interactor::new(fig)
    }

    fn limits(it: &Interactor) -> ((f64, f64), (f64, f64)) {
        it.figure().axes()[0].effective_limits()
    }

    fn assert_close(a: f64, b: f64, tol: f64, what: &str) {
        assert!((a - b).abs() < tol, "{what}: expected {b}, got {a}");
    }

    #[test]
    fn wheel_zoom_keeps_cursor_data_point_fixed() {
        let mut it = fixture();
        let (px, py) = (150.0, 120.0);
        let before = it.figure().pixel_to_data(0, px, py).expect("inside axes");

        assert_eq!(
            it.handle(Event::Wheel {
                x: px,
                y: py,
                dy: -1.0
            }),
            Outcome::NeedsRedraw
        );

        let after = it.figure().pixel_to_data(0, px, py).expect("still inside");
        assert_close(after.0, before.0, 1e-9, "x under cursor");
        assert_close(after.1, before.1, 1e-9, "y under cursor");

        // And the view actually zoomed in (span shrank).
        let ((xlo, xhi), _) = limits(&it);
        assert!(xhi - xlo < 10.0, "zoom in must shrink the x span");
    }

    #[test]
    fn wheel_zoom_on_log_axes_keeps_cursor_point_fixed() {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.set_xscale_log(10.0);
        ax.plot(&[1.0, 10.0, 1000.0], &[0.0, 5.0, 10.0]);
        ax.set_xlim(1.0, 1000.0);
        ax.set_ylim(0.0, 10.0);
        let mut it = Interactor::new(fig);

        let (px, py) = (200.0, 150.0);
        let before = it.figure().pixel_to_data(0, px, py).expect("inside axes");
        it.handle(Event::Wheel {
            x: px,
            y: py,
            dy: 2.0,
        });
        let after = it.figure().pixel_to_data(0, px, py).expect("still inside");

        assert_close(
            after.0,
            before.0,
            1e-9 * before.0.abs(),
            "log x under cursor",
        );
        assert_close(after.1, before.1, 1e-9, "y under cursor");
        // Limits stay positive on the log axis.
        let ((xlo, _), _) = limits(&it);
        assert!(xlo > 0.0, "log axis lower limit must stay positive");
    }

    #[test]
    fn pan_roundtrip_restores_limits() {
        let mut it = fixture();
        let start = limits(&it);

        it.handle(Event::MouseDown {
            x: 200.0,
            y: 150.0,
            button: MouseButton::Left,
        });
        assert_eq!(
            it.handle(Event::MouseMove { x: 240.0, y: 130.0 }),
            Outcome::NeedsRedraw
        );
        it.handle(Event::MouseMove { x: 200.0, y: 150.0 });
        it.handle(Event::MouseUp {
            x: 200.0,
            y: 150.0,
            button: MouseButton::Left,
        });

        let end = limits(&it);
        assert_close(end.0.0, start.0.0, 1e-9, "xlo");
        assert_close(end.0.1, start.0.1, 1e-9, "xhi");
        assert_close(end.1.0, start.1.0, 1e-9, "ylo");
        assert_close(end.1.1, start.1.1, 1e-9, "yhi");
    }

    #[test]
    fn pan_on_log_axis_shifts_decades_not_data_units() {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.set_xscale_log(10.0);
        ax.plot(&[1.0, 10.0, 100.0], &[0.0, 1.0, 2.0]);
        ax.set_xlim(1.0, 100.0);
        ax.set_ylim(0.0, 2.0);
        let mut it = Interactor::new(fig);

        it.handle(Event::MouseDown {
            x: 200.0,
            y: 150.0,
            button: MouseButton::Left,
        });
        it.handle(Event::MouseMove { x: 100.0, y: 150.0 });

        // A pure-x pixel shift on a log axis multiplies both limits by the
        // same factor: the ratio (decade span) is preserved.
        let ((xlo, xhi), _) = limits(&it);
        assert!(xlo > 0.0 && xhi > xlo);
        assert_close(xhi / xlo, 100.0, 1e-6, "decade span preserved");
        assert!(xlo > 1.0, "dragging left pans toward larger x");
    }

    #[test]
    fn double_click_restores_home_after_zoom_and_pan() {
        let mut it = fixture();
        let home = limits(&it);

        it.handle(Event::Wheel {
            x: 150.0,
            y: 120.0,
            dy: -3.0,
        });
        it.handle(Event::MouseDown {
            x: 200.0,
            y: 150.0,
            button: MouseButton::Left,
        });
        it.handle(Event::MouseMove { x: 260.0, y: 90.0 });
        it.handle(Event::MouseUp {
            x: 260.0,
            y: 90.0,
            button: MouseButton::Left,
        });
        assert_ne!(limits(&it), home, "interaction must have changed the view");

        assert_eq!(
            it.handle(Event::DoubleClick { x: 200.0, y: 150.0 }),
            Outcome::NeedsRedraw
        );
        let end = limits(&it);
        assert_close(end.0.0, home.0.0, 1e-9, "xlo home");
        assert_close(end.0.1, home.0.1, 1e-9, "xhi home");
        assert_close(end.1.0, home.1.0, 1e-9, "ylo home");
        assert_close(end.1.1, home.1.1, 1e-9, "yhi home");
    }

    #[test]
    fn events_outside_all_axes_are_unchanged() {
        let mut it = fixture();
        let start = limits(&it);
        // (5, 5) is in the figure margin, outside the axes rect.
        assert_eq!(
            it.handle(Event::Wheel {
                x: 5.0,
                y: 5.0,
                dy: -1.0
            }),
            Outcome::Unchanged
        );
        assert_eq!(
            it.handle(Event::MouseDown {
                x: 5.0,
                y: 5.0,
                button: MouseButton::Left
            }),
            Outcome::Unchanged
        );
        assert_eq!(
            it.handle(Event::MouseMove { x: 6.0, y: 6.0 }),
            Outcome::Unchanged
        );
        assert_eq!(
            it.handle(Event::DoubleClick { x: 5.0, y: 5.0 }),
            Outcome::Unchanged
        );
        assert_eq!(limits(&it), start);
    }

    #[test]
    fn hover_reports_data_coordinates() {
        let mut it = fixture();
        let (px, py) = it
            .figure()
            .data_to_pixel(0, 5.0, 5.0)
            .expect("axes 0 exists");
        match it.handle(Event::MouseMove { x: px, y: py }) {
            Outcome::Hover { axes, x, y } => {
                assert_eq!(axes, 0);
                assert_close(x, 5.0, 1e-9, "hover x");
                assert_close(y, 5.0, 1e-9, "hover y");
            }
            other => panic!("expected Hover, got {other:?}"),
        }
    }

    #[test]
    fn repeated_extreme_logit_interaction_keeps_limits_in_domain() {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        let x: Vec<f64> = (0..50).map(|i| f64::from(i) * 0.2 - 5.0).collect();
        let p: Vec<f64> = x.iter().map(|v| 1.0 / (1.0 + (-v).exp())).collect();
        ax.logity(&x, &p);
        ax.set_xlim(-5.0, 5.0);
        ax.set_ylim(0.001, 0.999);
        let mut it = Interactor::new(fig);

        // Hammer the view: repeated max-rate zoom-outs toward the saturating
        // tails, interleaved with hard pans up and down.
        for round in 0..60 {
            it.handle(Event::Wheel {
                x: 200.0,
                y: 40.0,
                dy: 8.0,
            });
            it.handle(Event::MouseDown {
                x: 200.0,
                y: 150.0,
                button: MouseButton::Left,
            });
            let target_y = if round % 2 == 0 { -400.0 } else { 700.0 };
            it.handle(Event::MouseMove {
                x: 200.0,
                y: target_y,
            });
            it.handle(Event::MouseUp {
                x: 200.0,
                y: target_y,
                button: MouseButton::Left,
            });

            let (_, (ylo, yhi)) = limits(&it);
            assert!(
                ylo.is_finite() && yhi.is_finite(),
                "round {round}: logit limits went non-finite: ({ylo}, {yhi})"
            );
            assert!(
                ylo > 0.0 && yhi < 1.0 && ylo < yhi,
                "round {round}: logit limits left (0, 1): ({ylo}, {yhi})"
            );
        }
        // The view must still respond to interaction afterwards.
        assert_eq!(
            it.handle(Event::Wheel {
                x: 200.0,
                y: 150.0,
                dy: -1.0
            }),
            Outcome::NeedsRedraw
        );
    }

    #[test]
    fn repeated_extreme_log_interaction_keeps_limits_positive() {
        let mut fig = Figure::new(4.0, 3.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.set_xscale_log(10.0);
        ax.plot(&[1.0, 10.0, 100.0], &[0.0, 1.0, 2.0]);
        ax.set_xlim(1.0, 100.0);
        ax.set_ylim(0.0, 2.0);
        let mut it = Interactor::new(fig);

        for round in 0..80 {
            // Zoom out hard at the left edge, then drag far right — both push
            // the lower limit toward log-underflow territory.
            it.handle(Event::Wheel {
                x: 45.0,
                y: 150.0,
                dy: 8.0,
            });
            it.handle(Event::MouseDown {
                x: 100.0,
                y: 150.0,
                button: MouseButton::Left,
            });
            it.handle(Event::MouseMove { x: 900.0, y: 150.0 });
            it.handle(Event::MouseUp {
                x: 900.0,
                y: 150.0,
                button: MouseButton::Left,
            });

            let ((xlo, xhi), _) = limits(&it);
            assert!(
                xlo.is_finite() && xhi.is_finite(),
                "round {round}: log limits went non-finite: ({xlo}, {xhi})"
            );
            assert!(
                xlo > 0.0 && xlo < xhi,
                "round {round}: log limits left the domain: ({xlo}, {xhi})"
            );
        }
        assert_eq!(
            it.handle(Event::Wheel {
                x: 200.0,
                y: 150.0,
                dy: -1.0
            }),
            Outcome::NeedsRedraw
        );
    }

    #[test]
    fn drag_survives_leaving_the_axes_rect() {
        let mut it = fixture();
        it.handle(Event::MouseDown {
            x: 100.0,
            y: 150.0,
            button: MouseButton::Left,
        });
        // Drag far outside the axes (pointer capture keeps events flowing).
        assert_eq!(
            it.handle(Event::MouseMove { x: 500.0, y: 150.0 }),
            Outcome::NeedsRedraw
        );
        // Leave cancels the drag; further moves are hover/unchanged.
        it.handle(Event::Leave);
        assert_eq!(
            it.handle(Event::MouseMove { x: 500.0, y: 150.0 }),
            Outcome::Unchanged
        );
    }
}
