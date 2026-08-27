//! A declarative animation timeline, evaluable at any time `t`.
//!
//! An animation driven by a host callback cannot be serialized — it is code, and
//! the whole point of a portable figure is that it travels without its process.
//! So an animated figure carries a **timeline**: keyframed tracks over the same
//! mutation vocabulary the interactive session already uses, sampled at a time
//! rather than replayed as a sequence.
//!
//! That distinction is what makes the controls meaningful. Because
//! [`Timeline::apply`] is a pure function of `t`, seeking is exact rather than
//! approximate, a paused figure is a full-quality render of that instant rather
//! than a held frame, and the same `t` produces the same pixels on every host
//! and in ten years' time. A frame sequence would be a video, and video already
//! exists and is smaller.
//!
//! ```
//! use rizzma::Figure;
//! use rizzma::portable::{Timeline, Track};
//!
//! let mut fig = Figure::new(4.0, 3.0);
//! let ax = fig.add_subplot(1, 1, 1);
//! ax.plot(&[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0]);
//!
//! // Two keyframes: a flat line rises into a ramp over two seconds.
//! let mut timeline = Timeline::new(2.0);
//! timeline.push(Track::line_y(0, 0, vec![0.0, 2.0], vec![
//!     vec![0.0, 0.0, 0.0],
//!     vec![0.0, 1.0, 2.0],
//! ])?);
//! fig.set_timeline(timeline);
//!
//! fig.seek(1.0)?;                       // halfway: the midpoint of the two
//! assert_eq!(fig.axes()[0].line_data(0).unwrap().1, vec![0.0, 0.5, 1.0]);
//! # Ok::<(), rizzma::PortableError>(())
//! ```

use serde::{Deserialize, Serialize};

use super::PortableError;

/// How values between two keyframes are sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interp {
    /// Hold the earlier keyframe until the next one is reached.
    Step,
    /// Interpolate element-wise between the bracketing keyframes.
    #[default]
    Linear,
}

/// What a [`Track`] animates.
///
/// A closed set, and deliberately the same verbs a live session already drives
/// (`set_line_data`, `set_collection_offsets`, `set_image_data`, `set_xlim`,
/// `set_ylim`) — so an animated figure exercises the paths that were already
/// there rather than a second, animation-only pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The x samples of a line.
    LineX {
        /// Index of the axes within the figure.
        axes: usize,
        /// Index of the line within that axes.
        index: usize,
    },
    /// The y samples of a line.
    LineY {
        /// Index of the axes within the figure.
        axes: usize,
        /// Index of the line within that axes.
        index: usize,
    },
    /// Scatter marker positions, as flattened `[x0, y0, x1, y1, …]`.
    Offsets {
        /// Index of the axes within the figure.
        axes: usize,
        /// Index of the collection within that axes.
        index: usize,
    },
    /// The samples of a colormapped image, row-major.
    ImageData {
        /// Index of the axes within the figure.
        axes: usize,
        /// Index of the image within that axes.
        index: usize,
    },
    /// The x view limits, as `[min, max]`.
    ///
    /// A scrolling window — an oscilloscope sweep — is a two-keyframe linear
    /// track here, not hundreds of frames of duplicated samples.
    Xlim {
        /// Index of the axes within the figure.
        axes: usize,
    },
    /// The y view limits, as `[min, max]`.
    Ylim {
        /// Index of the axes within the figure.
        axes: usize,
    },
}

impl Target {
    /// The axes this target addresses.
    #[must_use]
    pub fn axes(self) -> usize {
        match self {
            Target::LineX { axes, .. }
            | Target::LineY { axes, .. }
            | Target::Offsets { axes, .. }
            | Target::ImageData { axes, .. }
            | Target::Xlim { axes }
            | Target::Ylim { axes } => axes,
        }
    }
}

/// One animated property: keyframe times, and the values at each.
///
/// `values` is `times.len()` frames laid end to end, every frame the same width.
/// That width is fixed for the life of the track, which is what lets evaluation
/// be an element-wise lerp rather than a resampling problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// What this track animates.
    pub target: Target,
    /// Keyframe times in seconds, strictly increasing.
    pub times: Vec<f64>,
    /// Frames laid end to end: `times.len() * stride` values.
    pub values: Vec<f64>,
    /// Elements per frame.
    pub stride: usize,
    /// How values between keyframes are sampled.
    pub interp: Interp,
}

impl Track {
    /// Build a track from per-keyframe frames, each of which must be the same
    /// length.
    ///
    /// # Errors
    ///
    /// [`PortableError::Unsupported`] if `times` is empty, is not strictly
    /// increasing, disagrees in length with `frames`, or if the frames are
    /// ragged — all of which would make evaluation ill-defined rather than
    /// merely odd.
    pub fn new(
        target: Target,
        times: Vec<f64>,
        frames: Vec<Vec<f64>>,
        interp: Interp,
    ) -> Result<Track, PortableError> {
        if times.is_empty() {
            return Err(PortableError::Unsupported(
                "an animation track needs at least one keyframe".to_string(),
            ));
        }
        if times.len() != frames.len() {
            return Err(PortableError::Unsupported(format!(
                "track has {} keyframe times but {} frames",
                times.len(),
                frames.len()
            )));
        }
        if !times.windows(2).all(|w| w[0] < w[1]) {
            return Err(PortableError::Unsupported(
                "keyframe times must be strictly increasing".to_string(),
            ));
        }
        let stride = frames[0].len();
        if let Some(bad) = frames.iter().position(|f| f.len() != stride) {
            return Err(PortableError::Unsupported(format!(
                "frame {bad} has {} values but frame 0 has {stride}; every frame \
                 of a track must be the same width",
                frames[bad].len()
            )));
        }
        Ok(Track {
            target,
            times,
            values: frames.concat(),
            stride,
            interp,
        })
    }

    /// A linearly interpolated track over a line's y samples.
    ///
    /// # Errors
    ///
    /// As [`Track::new`].
    pub fn line_y(
        axes: usize,
        index: usize,
        times: Vec<f64>,
        frames: Vec<Vec<f64>>,
    ) -> Result<Track, PortableError> {
        Track::new(Target::LineY { axes, index }, times, frames, Interp::Linear)
    }

    /// A linearly interpolated track over a line's x samples.
    ///
    /// # Errors
    ///
    /// As [`Track::new`].
    pub fn line_x(
        axes: usize,
        index: usize,
        times: Vec<f64>,
        frames: Vec<Vec<f64>>,
    ) -> Result<Track, PortableError> {
        Track::new(Target::LineX { axes, index }, times, frames, Interp::Linear)
    }

    /// A scrolling x window: the view sweeps from `from` to `to` over the whole
    /// timeline. Two keyframes, not a frame per sample.
    ///
    /// # Errors
    ///
    /// As [`Track::new`].
    pub fn xlim_sweep(
        axes: usize,
        duration: f64,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<Track, PortableError> {
        Track::new(
            Target::Xlim { axes },
            vec![0.0, duration],
            vec![vec![from.0, from.1], vec![to.0, to.1]],
            Interp::Linear,
        )
    }

    /// The number of keyframes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.times.len()
    }

    /// Whether the track has no keyframes. Always false for a constructed
    /// track; present because clippy asks for it beside [`Track::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// The values this track takes at time `t`, clamped to the keyframe range.
    ///
    /// Before the first keyframe the first frame holds; after the last, the
    /// last holds. Extrapolating would invent data the author never drew.
    #[must_use]
    pub fn sample(&self, t: f64) -> Vec<f64> {
        let frame = |i: usize| &self.values[i * self.stride..(i + 1) * self.stride];
        let last = self.times.len() - 1;
        if t <= self.times[0] {
            return frame(0).to_vec();
        }
        if t >= self.times[last] {
            return frame(last).to_vec();
        }
        // Index of the keyframe at or before `t`.
        let hi = self.times.partition_point(|&k| k <= t);
        let lo = hi - 1;
        if self.interp == Interp::Step {
            return frame(lo).to_vec();
        }
        let span = self.times[hi] - self.times[lo];
        let u = if span > 0.0 {
            (t - self.times[lo]) / span
        } else {
            0.0
        };
        let (a, b) = (frame(lo), frame(hi));
        a.iter().zip(b).map(|(x, y)| x + (y - x) * u).collect()
    }
}

/// An animation: a duration, and the tracks evaluated against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    /// Length of the animation in seconds.
    pub duration: f64,
    /// Whether playback wraps at `duration`.
    #[serde(rename = "loop")]
    pub looping: bool,
    /// The animated properties.
    pub tracks: Vec<Track>,
}

impl Timeline {
    /// An empty looping timeline of `duration` seconds.
    #[must_use]
    pub fn new(duration: f64) -> Timeline {
        Timeline {
            duration,
            looping: true,
            tracks: Vec::new(),
        }
    }

    /// Add a track, returning `self` for chaining.
    pub fn push(&mut self, track: Track) -> &mut Self {
        self.tracks.push(track);
        self
    }

    /// Set whether playback loops, returning `self` for chaining.
    #[must_use]
    pub fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Whether this timeline animates anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Normalize `t` into the timeline: wrapped when looping, clamped when not.
    ///
    /// A non-finite `t` becomes `0.0` rather than propagating: a nonsense clock
    /// should not produce a nonsense figure.
    ///
    /// `t == duration` maps to `duration`, not to `0.0`, even when looping — so
    /// seeking to the end of an animation shows its end. Wrapping begins
    /// strictly *past* the duration, which is where a playback clock crosses
    /// anyway, so continuous playback is unaffected.
    #[must_use]
    pub fn normalize(&self, t: f64) -> f64 {
        if !t.is_finite() || self.duration <= 0.0 {
            return 0.0;
        }
        if self.looping && t > self.duration {
            t.rem_euclid(self.duration)
        } else {
            t.clamp(0.0, self.duration)
        }
    }
}
