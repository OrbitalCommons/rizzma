//! Target-agnostic input events for interactive figures.
//!
//! Hosts (the wasm DOM bridge, tests, any future embedder) translate their
//! native input into [`Event`] and feed it to an
//! [`Interactor`](crate::figure::Interactor). All positions are **top-down
//! logical figure pixels** — the same space [`Figure::pixel_to_data`] and
//! [`Figure::data_to_pixel`] operate in, with the origin at the canvas
//! top-left and *no* HiDPI scaling applied (a host presenting at
//! `devicePixelRatio > 1` divides device pixels by the scale first).
//!
//! Because the whole pipeline is top-down, there is no y-flip anywhere in the
//! event path (matplotlib's WebAgg flips because its display space is y-up;
//! rizzma's pixel APIs are already top-down).

use crate::figure::Figure;

/// A mouse button, remapped from the host's numbering by the event source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// The primary button (DOM button `0`).
    Left,
    /// The middle/wheel button (DOM button `1`).
    Middle,
    /// The secondary button (DOM button `2`).
    Right,
}

/// An input event in top-down logical figure pixels.
///
/// Wheel deltas are normalized by the source to "lines" (one detent ≈ 1.0),
/// with `dy > 0` meaning zoom *out* (scroll toward the user), matching the
/// DOM's sign convention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A button was pressed at `(x, y)`.
    MouseDown {
        /// Cursor x in logical figure pixels.
        x: f64,
        /// Cursor y in logical figure pixels (top-down).
        y: f64,
        /// Which button went down.
        button: MouseButton,
    },
    /// A button was released at `(x, y)`.
    MouseUp {
        /// Cursor x in logical figure pixels.
        x: f64,
        /// Cursor y in logical figure pixels (top-down).
        y: f64,
        /// Which button came up.
        button: MouseButton,
    },
    /// The cursor moved to `(x, y)`.
    MouseMove {
        /// Cursor x in logical figure pixels.
        x: f64,
        /// Cursor y in logical figure pixels (top-down).
        y: f64,
    },
    /// The wheel scrolled by `dy` normalized lines while the cursor was at
    /// `(x, y)`.
    Wheel {
        /// Cursor x in logical figure pixels.
        x: f64,
        /// Cursor y in logical figure pixels (top-down).
        y: f64,
        /// Normalized wheel delta in lines; positive scrolls down (zoom out).
        dy: f64,
    },
    /// The primary button was double-clicked at `(x, y)`.
    DoubleClick {
        /// Cursor x in logical figure pixels.
        x: f64,
        /// Cursor y in logical figure pixels (top-down).
        y: f64,
    },
    /// The cursor left the canvas.
    Leave,
    /// The host canvas was resized to `(width_px, height_px)` logical pixels.
    Resize {
        /// New width in logical pixels.
        width_px: f64,
        /// New height in logical pixels.
        height_px: f64,
    },
}

impl Figure {
    /// The index of the topmost axes whose pixel rectangle contains the
    /// **top-down logical pixel** `(px, py)`, or `None` when the point is over
    /// no axes.
    ///
    /// "Topmost" is the last-added axes, matching draw order (later axes paint
    /// over earlier ones).
    #[must_use]
    pub fn axes_at(&self, px: f64, py: f64) -> Option<usize> {
        let (fig_w_px, fig_h_px) = self.size_px();
        // Axes rects are stored in y-up display pixels; un-flip the query point.
        let display_y = fig_h_px - py;
        self.axes()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, ax)| {
                let (rect, _) = ax.pixel_rect_and_trans_data(fig_w_px, fig_h_px);
                rect.contains_point(px, display_y)
            })
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_at_hits_the_right_axes_and_misses_gaps() {
        // 4x2 in @ 100 dpi -> 400x200 px. Two axes side by side.
        let mut fig = Figure::new(4.0, 2.0);
        fig.add_axes(0.1, 0.1, 0.35, 0.8); // x px [40, 180]
        fig.add_axes(0.55, 0.1, 0.35, 0.8); // x px [220, 360]

        assert_eq!(fig.axes_at(100.0, 100.0), Some(0));
        assert_eq!(fig.axes_at(300.0, 100.0), Some(1));
        // The gap between them, and points outside the canvas, hit nothing.
        assert_eq!(fig.axes_at(200.0, 100.0), None);
        assert_eq!(fig.axes_at(-5.0, 100.0), None);
        assert_eq!(fig.axes_at(100.0, 500.0), None);
    }

    #[test]
    fn axes_at_prefers_the_topmost_overlapping_axes() {
        let mut fig = Figure::new(2.0, 2.0);
        fig.add_axes(0.1, 0.1, 0.8, 0.8);
        fig.add_axes(0.3, 0.3, 0.4, 0.4); // inset, drawn on top
        // Center of the canvas is inside both; the inset (later) wins.
        assert_eq!(fig.axes_at(100.0, 100.0), Some(1));
        // A point only in the outer axes resolves to it.
        assert_eq!(fig.axes_at(30.0, 100.0), Some(0));
    }
}
