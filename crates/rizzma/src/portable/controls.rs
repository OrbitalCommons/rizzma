//! User-driven parameters: sliders over axes other than time (schema 4).
//!
//! A [`Timeline`](super::Timeline) is a pure function of a clock. A [`Control`]
//! is the same idea over an axis the *user* drives: a declared range, a
//! default, and keyframed [`Track`]s whose `times` are read as positions of the
//! control rather than seconds — the same struct, because nothing about a
//! keyframe axis is inherently temporal.
//!
//! A control may also carry [`Grid`]s, which bind the control *and* the clock
//! on one target: a lattice of frames over `(time, position)`, sampled
//! bilinearly. That is what lets a slider reshape a figure while it plays —
//! the wave keeps travelling while its wavelength changes under the user's
//! finger — without the event-order-dependent flicker of one axis overwriting
//! the other.
//!
//! Design: `design/12-controls.md`.

use serde::{Deserialize, Serialize};

use super::PortableError;
use super::timeline::{Interp, Target, Track};

/// Values on a `(time, position)` lattice, evaluable at any `(t, p)`.
///
/// `values` holds `times.len() * positions.len()` frames of `stride` elements:
/// time-major, so frame `(i, j)` — time index `i`, position index `j` — starts
/// at `(i * positions.len() + j) * stride`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    /// What this grid animates.
    pub target: Target,
    /// Keyframe times in seconds, strictly increasing.
    pub times: Vec<f64>,
    /// Keyframe positions of the owning control, strictly increasing.
    pub positions: Vec<f64>,
    /// `times.len() * positions.len() * stride` values, time-major.
    pub values: Vec<f64>,
    /// Elements per frame.
    pub stride: usize,
    /// How values between lattice points are sampled, on both axes.
    pub interp: Interp,
}

impl Grid {
    /// Build a grid from per-`(time, position)` frames: `frames[i][j]` is the
    /// frame at `times[i]`, `positions[j]`, and every frame must be the same
    /// length.
    ///
    /// # Errors
    ///
    /// [`PortableError::Unsupported`] if either axis is empty or not strictly
    /// increasing, if the frame lattice disagrees with the axes in shape, or
    /// if the frames are ragged.
    pub fn new(
        target: Target,
        times: Vec<f64>,
        positions: Vec<f64>,
        frames: Vec<Vec<Vec<f64>>>,
        interp: Interp,
    ) -> Result<Grid, PortableError> {
        for (name, axis) in [("times", &times), ("positions", &positions)] {
            if axis.is_empty() {
                return Err(PortableError::Unsupported(format!(
                    "a grid needs at least one keyframe on its {name} axis"
                )));
            }
            if !axis.windows(2).all(|w| w[0] < w[1]) {
                return Err(PortableError::Unsupported(format!(
                    "grid {name} must be strictly increasing"
                )));
            }
        }
        if frames.len() != times.len() {
            return Err(PortableError::Unsupported(format!(
                "grid has {} time keyframes but {} rows of frames",
                times.len(),
                frames.len()
            )));
        }
        let stride = frames[0].first().map_or(0, Vec::len);
        let mut values = Vec::with_capacity(times.len() * positions.len() * stride);
        for (i, row) in frames.iter().enumerate() {
            if row.len() != positions.len() {
                return Err(PortableError::Unsupported(format!(
                    "grid row {i} has {} frames but there are {} positions",
                    row.len(),
                    positions.len()
                )));
            }
            for (j, frame) in row.iter().enumerate() {
                if frame.len() != stride {
                    return Err(PortableError::Unsupported(format!(
                        "grid frame ({i}, {j}) has {} values but frame (0, 0) has \
                         {stride}; every frame of a grid must be the same width",
                        frame.len()
                    )));
                }
                values.extend_from_slice(frame);
            }
        }
        Ok(Grid {
            target,
            times,
            positions,
            values,
            stride,
            interp,
        })
    }

    /// The values this grid takes at time `t` and control position `p`,
    /// clamped to the lattice on both axes.
    #[must_use]
    pub fn sample(&self, t: f64, p: f64) -> Vec<f64> {
        let (ti, tj, tu) = bracket(&self.times, t, self.interp);
        let (pi, pj, pu) = bracket(&self.positions, p, self.interp);
        let cols = self.positions.len();
        let frame = |i: usize, j: usize| {
            let start = (i * cols + j) * self.stride;
            &self.values[start..start + self.stride]
        };
        let (a, b, c, d) = (frame(ti, pi), frame(ti, pj), frame(tj, pi), frame(tj, pj));
        (0..self.stride)
            .map(|k| {
                let lo = a[k] + (b[k] - a[k]) * pu;
                let hi = c[k] + (d[k] - c[k]) * pu;
                lo + (hi - lo) * tu
            })
            .collect()
    }
}

/// The bracketing keyframe indices for `x` in strictly increasing `axis`, and
/// the blend factor between them — `(lo, lo, 0.0)` under [`Interp::Step`] or
/// when clamped at either end.
fn bracket(axis: &[f64], x: f64, interp: Interp) -> (usize, usize, f64) {
    let last = axis.len() - 1;
    if x <= axis[0] {
        return (0, 0, 0.0);
    }
    if x >= axis[last] {
        return (last, last, 0.0);
    }
    let hi = axis.partition_point(|&k| k <= x);
    let lo = hi - 1;
    if interp == Interp::Step {
        return (lo, lo, 0.0);
    }
    let span = axis[hi] - axis[lo];
    let u = if span > 0.0 {
        (x - axis[lo]) / span
    } else {
        0.0
    };
    (lo, hi, u)
}

/// One user-driven parameter: a labelled range with a default, and the tracks
/// and grids evaluated against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    /// The label a host shows beside the slider.
    pub label: String,
    /// Smallest position, inclusive.
    pub min: f64,
    /// Largest position, inclusive.
    pub max: f64,
    /// The position the figure was authored at.
    pub default: f64,
    /// Snap increment from `min`; `None` is a continuous slider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Tracks whose `times` are positions of this control.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    /// Lattices over `(time, position of this control)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grids: Vec<Grid>,
}

impl Control {
    /// A continuous control over `[min, max]`, starting at `default`, with no
    /// drivers yet — [`push_track`](Control::push_track) and
    /// [`push_grid`](Control::push_grid) add them.
    ///
    /// # Errors
    ///
    /// [`PortableError::Unsupported`] if the range is not finite and
    /// increasing, or `default` lies outside it.
    pub fn new(label: &str, min: f64, max: f64, default: f64) -> Result<Control, PortableError> {
        if !(min.is_finite() && max.is_finite() && default.is_finite()) || min >= max {
            return Err(PortableError::Unsupported(format!(
                "control \"{label}\" needs a finite increasing range, got [{min}, {max}]"
            )));
        }
        if !(min..=max).contains(&default) {
            return Err(PortableError::Unsupported(format!(
                "control \"{label}\" defaults to {default}, outside [{min}, {max}]"
            )));
        }
        Ok(Control {
            label: label.to_string(),
            min,
            max,
            default,
            step: None,
            tracks: Vec::new(),
            grids: Vec::new(),
        })
    }

    /// Snap the control to multiples of `step` from `min`, returning `self`
    /// for chaining.
    ///
    /// # Errors
    ///
    /// [`PortableError::Unsupported`] if `step` is not finite and positive.
    pub fn with_step(mut self, step: f64) -> Result<Control, PortableError> {
        if !(step.is_finite() && step > 0.0) {
            return Err(PortableError::Unsupported(format!(
                "control \"{}\" needs a positive finite step, got {step}",
                self.label
            )));
        }
        self.step = Some(step);
        Ok(self)
    }

    /// Add a track driven by this control, returning `self` for chaining.
    pub fn push_track(&mut self, track: Track) -> &mut Self {
        self.tracks.push(track);
        self
    }

    /// Add a `(time, position)` grid, returning `self` for chaining.
    pub fn push_grid(&mut self, grid: Grid) -> &mut Self {
        self.grids.push(grid);
        self
    }

    /// Normalize a requested position: clamped into the range, snapped to
    /// `step` when one is set, and the default when non-finite — a nonsense
    /// slider should not produce a nonsense figure.
    #[must_use]
    pub fn normalize(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return self.default;
        }
        let clamped = value.clamp(self.min, self.max);
        match self.step {
            Some(step) => {
                (self.min + ((clamped - self.min) / step).round() * step).clamp(self.min, self.max)
            }
            None => clamped,
        }
    }
}
