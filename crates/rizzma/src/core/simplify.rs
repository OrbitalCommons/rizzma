//! Display-space path simplification: matplotlib's `PathSimplifier`
//! (`src/path_converters.h`).
//!
//! A long noisy polyline packs many vertices into each pixel, and stroking
//! them one by one costs far more than the handful of pixels they cover. The
//! simplifier walks the vertices in display space building one segment at a
//! time: each next vertex whose perpendicular distance from the segment's
//! direction stays under the threshold is merged into it, keeping only the
//! farthest point reached forward and the farthest backward (so a column of
//! noise keeps its full vertical extent). When a vertex strays further, the
//! segment is emitted and a new one starts there. The result covers the same
//! pixels to within the threshold — matplotlib's default is 1/9 px — with a
//! small fraction of the vertices.
//!
//! This is a line-for-line port of matplotlib 3.11's algorithm, including its
//! handling of anti-parallel runs and of pen lifts (`MoveTo`) between pieces.

use super::path::PathCode;

/// Simplify a polyline already in display space (pixels).
///
/// `vertices` are paired with `codes` (`None` means one `MoveTo` then
/// `LineTo`s); only `MoveTo` and `LineTo` may appear. `threshold` is the
/// largest perpendicular deviation, in pixels, a merged vertex may have.
pub(crate) fn simplify(
    vertices: &[[f64; 2]],
    codes: Option<&[PathCode]>,
    threshold: f64,
) -> (Vec<[f64; 2]>, Vec<PathCode>) {
    let mut s = Simplifier::new(threshold * threshold);
    for (i, &[x, y]) in vertices.iter().enumerate() {
        let code = match codes {
            Some(codes) => codes[i],
            None if i == 0 => PathCode::MoveTo,
            None => PathCode::LineTo,
        };
        s.feed(code, x, y);
    }
    s.finish()
}

/// The simplifier's state between vertices; the fields follow matplotlib's
/// `m_*` members, the output vector stands in for its queue.
struct Simplifier {
    threshold2: f64,
    out_v: Vec<[f64; 2]>,
    out_c: Vec<PathCode>,
    moveto: bool,
    after_moveto: bool,
    clipped: bool,
    last: [f64; 2],
    orig: [f64; 2],
    orig_norm2: f64,
    forward_max2: f64,
    backward_max2: f64,
    last_forward_max: bool,
    last_backward_max: bool,
    next: [f64; 2],
    next_backward: [f64; 2],
    vec_start: [f64; 2],
}

impl Simplifier {
    fn new(threshold2: f64) -> Self {
        Self {
            threshold2,
            out_v: Vec::new(),
            out_c: Vec::new(),
            moveto: true,
            after_moveto: false,
            clipped: false,
            last: [0.0, 0.0],
            orig: [0.0, 0.0],
            orig_norm2: 0.0,
            forward_max2: 0.0,
            backward_max2: 0.0,
            last_forward_max: false,
            last_backward_max: false,
            next: [0.0, 0.0],
            next_backward: [0.0, 0.0],
            vec_start: [0.0, 0.0],
        }
    }

    fn emit(&mut self, code: PathCode, p: [f64; 2]) {
        self.out_c.push(code);
        self.out_v.push(p);
    }

    fn feed(&mut self, code: PathCode, x: f64, y: f64) {
        if self.moveto || code == PathCode::MoveTo {
            if self.orig_norm2 != 0.0 && !self.after_moveto {
                self.push(x, y);
            }
            self.after_moveto = true;
            self.last = [x, y];
            self.moveto = false;
            self.orig_norm2 = 0.0;
            self.backward_max2 = 0.0;
            self.clipped = true;
            return;
        }
        self.after_moveto = false;

        if self.orig_norm2 == 0.0 {
            if self.clipped {
                self.emit(PathCode::MoveTo, self.last);
                self.clipped = false;
            }
            self.orig = [x - self.last[0], y - self.last[1]];
            self.orig_norm2 = self.orig[0] * self.orig[0] + self.orig[1] * self.orig[1];
            self.forward_max2 = self.orig_norm2;
            self.backward_max2 = 0.0;
            self.last_forward_max = true;
            self.last_backward_max = false;
            self.vec_start = self.last;
            self.last = [x, y];
            self.next = [x, y];
            return;
        }

        // The perpendicular part of the vector from the segment's start to
        // this vertex, relative to the segment's original direction.
        let tot = [x - self.vec_start[0], y - self.vec_start[1]];
        let dot = self.orig[0] * tot[0] + self.orig[1] * tot[1];
        let para = [
            dot * self.orig[0] / self.orig_norm2,
            dot * self.orig[1] / self.orig_norm2,
        ];
        let perp = [tot[0] - para[0], tot[1] - para[1]];
        if perp[0] * perp[0] + perp[1] * perp[1] < self.threshold2 {
            let para2 = para[0] * para[0] + para[1] * para[1];
            self.last_forward_max = false;
            self.last_backward_max = false;
            if dot > 0.0 {
                if para2 > self.forward_max2 {
                    self.last_forward_max = true;
                    self.forward_max2 = para2;
                    self.next = [x, y];
                }
            } else if para2 > self.backward_max2 {
                self.last_backward_max = true;
                self.backward_max2 = para2;
                self.next_backward = [x, y];
            }
            self.last = [x, y];
            return;
        }
        self.push(x, y);
    }

    /// Emit the segment built so far and start the next at `(x, y)`.
    fn push(&mut self, x: f64, y: f64) {
        if self.backward_max2 > 0.0 {
            if self.last_forward_max {
                self.emit(PathCode::LineTo, self.next_backward);
                self.emit(PathCode::LineTo, self.next);
            } else {
                self.emit(PathCode::LineTo, self.next);
                self.emit(PathCode::LineTo, self.next_backward);
            }
        } else {
            self.emit(PathCode::LineTo, self.next);
        }
        if self.clipped {
            self.emit(PathCode::MoveTo, self.last);
        } else if !self.last_forward_max && !self.last_backward_max {
            self.emit(PathCode::LineTo, self.last);
        }

        self.orig = [x - self.last[0], y - self.last[1]];
        self.orig_norm2 = self.orig[0] * self.orig[0] + self.orig[1] * self.orig[1];
        self.forward_max2 = self.orig_norm2;
        self.last_forward_max = true;
        self.vec_start = *self.out_v.last().expect("push emits a vertex");
        self.last = [x, y];
        self.next = [x, y];
        self.backward_max2 = 0.0;
        self.last_backward_max = false;
        self.clipped = false;
    }

    /// Flush the segment in progress and return the simplified path.
    fn finish(mut self) -> (Vec<[f64; 2]>, Vec<PathCode>) {
        let code = |s: &Self| {
            if s.moveto || s.after_moveto {
                PathCode::MoveTo
            } else {
                PathCode::LineTo
            }
        };
        if self.orig_norm2 != 0.0 {
            let c = code(&self);
            self.emit(c, self.next);
            if self.backward_max2 > 0.0 {
                let c = code(&self);
                self.emit(c, self.next_backward);
            }
            self.moveto = false;
        }
        let c = code(&self);
        self.emit(c, self.last);
        (self.out_v, self.out_c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
        simplify(points, None, 1.0 / 9.0).0
    }

    #[test]
    fn collinear_runs_collapse_to_their_ends() {
        let line: Vec<[f64; 2]> = (0..100)
            .map(|i| [f64::from(i), 2.0 * f64::from(i)])
            .collect();
        let out = run(&line);
        assert_eq!(out.first(), Some(&[0.0, 0.0]));
        assert_eq!(out.last(), Some(&[99.0, 198.0]));
        assert!(out.len() <= 3, "{out:?}");
    }

    #[test]
    fn a_column_of_noise_keeps_its_full_extent() {
        // Vertical zig-zag at (almost) one x: the extremes must survive.
        let zig: Vec<[f64; 2]> = (0..200)
            .map(|i| {
                [
                    f64::from(i) * 1e-4,
                    if i % 2 == 0 {
                        0.0
                    } else {
                        10.0 + f64::from(i % 7)
                    },
                ]
            })
            .collect();
        let out = run(&zig);
        let (lo, hi) = out
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            });
        assert_eq!((lo, hi), (0.0, 16.0));
        assert!(out.len() < 20, "{} vertices", out.len());
    }

    #[test]
    fn corners_are_kept() {
        let corner = [
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 0.0],
            [20.0, 10.0],
            [20.0, 20.0],
        ];
        let out = run(&corner);
        assert!(out.contains(&[20.0, 0.0]), "{out:?}");
        assert_eq!(out.last(), Some(&[20.0, 20.0]));
    }

    /// Vertex for vertex against matplotlib's own `PathSimplifier`
    /// (`tests/fixtures/simplify_mpl.py`) on a noisy 20 000-point line with a
    /// NaN gap, split into runs the way `Line2D::path` lifts the pen.
    #[test]
    fn matches_matplotlib_vertex_for_vertex() {
        let floats = |bytes: &[u8]| -> Vec<f64> {
            bytes
                .chunks_exact(8)
                .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
                .collect()
        };
        let input = floats(include_bytes!("../../tests/fixtures/simplify_in.f64"));
        let want = floats(include_bytes!("../../tests/fixtures/simplify_out.f64"));
        let want_codes = include_bytes!("../../tests/fixtures/simplify_out_codes.u8");

        let points: Vec<[f64; 2]> = input.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
        let (mut vertices, mut codes) = (Vec::new(), Vec::new());
        for run in points
            .split(|[x, y]| !x.is_finite() || !y.is_finite())
            .filter(|run| run.len() >= 2)
        {
            vertices.extend_from_slice(run);
            codes.push(PathCode::MoveTo);
            codes.extend(std::iter::repeat_n(PathCode::LineTo, run.len() - 1));
        }
        let (v, c) = simplify(&vertices, Some(&codes), 1.0 / 9.0);

        assert_eq!(v.len(), want_codes.len());
        for (i, (p, code)) in v.iter().zip(&c).enumerate() {
            let want_code = if want_codes[i] == 1 {
                PathCode::MoveTo
            } else {
                PathCode::LineTo
            };
            assert_eq!(*code, want_code, "code {i}");
            assert_eq!(*p, [want[2 * i], want[2 * i + 1]], "vertex {i}");
        }
    }

    #[test]
    fn pen_lifts_survive() {
        let points = [[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [0.0, 5.0], [10.0, 5.0]];
        let codes = [
            PathCode::MoveTo,
            PathCode::LineTo,
            PathCode::LineTo,
            PathCode::MoveTo,
            PathCode::LineTo,
        ];
        let (v, c) = simplify(&points, Some(&codes), 1.0 / 9.0);
        let moves: Vec<[f64; 2]> = v
            .iter()
            .zip(&c)
            .filter(|(_, c)| **c == PathCode::MoveTo)
            .map(|(p, _)| *p)
            .collect();
        assert_eq!(moves, [[0.0, 0.0], [0.0, 5.0]]);
        assert_eq!(v.last(), Some(&[10.0, 5.0]));
    }
}
