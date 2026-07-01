//! [`streamplot`](Axes::streamplot): streamlines of a 2D vector field.
//!
//! Mirrors matplotlib's `streamplot` in a simplified form: a regular `nx x ny`
//! grid carries vector components `(u, v)`, and the field is integrated into a
//! set of evenly spaced streamlines. Each streamline is a [`Line2D`] polyline
//! with a single filled [`Patch`] arrowhead near its middle pointing along the
//! local flow, so both fold into autoscaling via [`data_limits`](Axes::data_limits).
//!
//! The field is sampled by bilinear interpolation. Streamlines are integrated
//! with RK4 in both directions from each seed, normalized to a roughly uniform
//! step in data units. A coarse occupancy grid keeps streamlines from
//! overlapping: a seed in an already-occupied cell is skipped, and integration
//! stops when a line enters a cell owned by a different line.

use crate::artist::{Line2D, Patch};

use crate::figure::Axes;

/// Side length (in cells) of the occupancy grid controlling streamline spacing.
const DENSITY: usize = 30;

/// Arrowhead length as a fraction of the larger domain span.
const HEAD_LENGTH_FRACTION: f64 = 0.025;

/// Arrowhead half-width as a fraction of the larger domain span.
const HEAD_HALF_WIDTH_FRACTION: f64 = 0.015;

/// Maximum integration steps per direction before a streamline is cut off.
const MAX_STEPS: usize = 1000;

/// A regular vector-field grid plus helpers for bilinear sampling.
struct Field<'a> {
    x: &'a [f64],
    y: &'a [f64],
    u: &'a [f64],
    v: &'a [f64],
    nx: usize,
    ny: usize,
}

impl Field<'_> {
    /// Bilinearly sample `(u, v)` at data point `(px, py)`, or `None` if the
    /// point is outside the grid bounds `[x[0], x[nx-1]] x [y[0], y[ny-1]]`.
    fn sample(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        let (x0, x1) = (self.x[0], self.x[self.nx - 1]);
        let (y0, y1) = (self.y[0], self.y[self.ny - 1]);
        if px < x0 || px > x1 || py < y0 || py > y1 {
            return None;
        }

        // Locate the cell [i, i+1] x [j, j+1] containing the point. The grids
        // are assumed monotonic increasing; a linear scan is fine for the
        // modest grids streamplot targets.
        let i = locate(self.x, px);
        let j = locate(self.y, py);

        let xl = self.x[i];
        let xr = self.x[i + 1];
        let yb = self.y[j];
        let yt = self.y[j + 1];

        // Cell-local fractional coordinates, guarded against zero-width cells.
        let tx = if xr > xl { (px - xl) / (xr - xl) } else { 0.0 };
        let ty = if yt > yb { (py - yb) / (yt - yb) } else { 0.0 };

        // Row-major with y as the outer index: component at (x[i], y[j]) is at
        // index j*nx + i (matching pcolormesh/imshow).
        let idx = |ii: usize, jj: usize| jj * self.nx + ii;
        let bilerp = |c: &[f64]| {
            let c00 = c[idx(i, j)];
            let c10 = c[idx(i + 1, j)];
            let c01 = c[idx(i, j + 1)];
            let c11 = c[idx(i + 1, j + 1)];
            let bottom = c00 + (c10 - c00) * tx;
            let top = c01 + (c11 - c01) * tx;
            bottom + (top - bottom) * ty
        };

        Some((bilerp(self.u), bilerp(self.v)))
    }
}

/// Index `i` of the cell `[grid[i], grid[i+1]]` containing `p`, clamped so
/// `i` is in `0..len-1` (assumes `grid.len() >= 2` and `p` in range).
fn locate(grid: &[f64], p: f64) -> usize {
    let n = grid.len();
    let mut i = 0;
    while i + 2 < n && grid[i + 1] < p {
        i += 1;
    }
    i
}

/// A coarse occupancy grid over the domain, used to space streamlines. Each
/// cell stores the id of its owning streamline (`0` means free).
struct Occupancy {
    cells: Vec<u32>,
    x0: f64,
    y0: f64,
    dx: f64,
    dy: f64,
    n: usize,
}

impl Occupancy {
    fn new(x0: f64, x1: f64, y0: f64, y1: f64) -> Self {
        let n = DENSITY;
        Occupancy {
            cells: vec![0; n * n],
            x0,
            y0,
            dx: (x1 - x0) / n as f64,
            dy: (y1 - y0) / n as f64,
            n,
        }
    }

    /// Cell index for a data point, clamped to the grid.
    fn cell(&self, px: f64, py: f64) -> (usize, usize) {
        let ci = (((px - self.x0) / self.dx) as isize).clamp(0, self.n as isize - 1) as usize;
        let cj = (((py - self.y0) / self.dy) as isize).clamp(0, self.n as isize - 1) as usize;
        (ci, cj)
    }

    /// Owner id of a cell (`0` means free).
    fn owner(&self, ci: usize, cj: usize) -> u32 {
        self.cells[cj * self.n + ci]
    }

    /// Mark a cell as owned by `id`.
    fn mark(&mut self, ci: usize, cj: usize, id: u32) {
        self.cells[cj * self.n + ci] = id;
    }
}

impl Axes {
    /// Draw streamlines of the 2D vector field `(u, v)` sampled on the regular
    /// grid `x` (length `nx`) by `y` (length `ny`).
    ///
    /// Mirrors matplotlib's `streamplot`. The axes `x` and `y` give the grid
    /// coordinates (assumed monotonic increasing and roughly uniform). The
    /// components `u` and `v` are `nx * ny` long, row-major with `y` as the
    /// outer index: the value at grid point `(x[i], y[j])` is `u[j * nx + i]`
    /// (matching [`pcolormesh`](Axes::pcolormesh) / [`imshow`](Axes::imshow)).
    /// Empty input, mismatched lengths (`u.len() != nx * ny`), or a grid with
    /// `nx < 2` or `ny < 2` draw nothing (never panic).
    ///
    /// The field is sampled by bilinear interpolation. Seeds are tried at the
    /// cells of a coarse `30 x 30` occupancy grid; from each free seed a
    /// streamline is grown by RK4 integration of the unit velocity (so spacing
    /// is roughly uniform regardless of speed) with step
    /// `h = 0.5 * min(cell_dx, cell_dy)`, integrating both forward (`+v`) and
    /// backward (`-v`) and concatenating into one polyline. A direction stops
    /// when it leaves the domain, enters a cell owned by a different
    /// streamline, stagnates (speed ~ 0), or exceeds 1000 steps. Each cell a
    /// line passes through is marked occupied, so lines do not overlap.
    ///
    /// Every streamline spanning more than a cell is drawn as a [`Line2D`]
    /// polyline in a single shared color (the next property-cycle color), with
    /// one filled triangular [`Patch`] arrowhead near its middle pointing along
    /// the local flow. The data limits expand to the grid bounds
    /// `[x[0], x[nx-1]] x [y[0], y[ny-1]]`. Returns `&mut Self`.
    ///
    /// ![streamplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_streamplot.png)
    ///
    /// ```
    /// use rizzma::core::Bbox;
    /// use rizzma::figure::Axes;
    ///
    /// let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
    /// // A uniform rightward field on a small grid.
    /// let x = [0.0, 1.0, 2.0];
    /// let y = [0.0, 1.0, 2.0];
    /// let u = [1.0; 9];
    /// let v = [0.0; 9];
    /// ax.streamplot(&x, &y, &u, &v);
    /// let limits = ax.data_limits().expect("streamplot contributes data limits");
    /// assert_eq!((limits.xmin(), limits.xmax()), (0.0, 2.0));
    /// assert_eq!((limits.ymin(), limits.ymax()), (0.0, 2.0));
    /// ```
    pub fn streamplot(&mut self, x: &[f64], y: &[f64], u: &[f64], v: &[f64]) -> &mut Self {
        let nx = x.len();
        let ny = y.len();
        if nx < 2 || ny < 2 || u.len() != nx * ny || v.len() != nx * ny {
            return self;
        }

        let field = Field { x, y, u, v, nx, ny };
        let (x0, x1) = (x[0], x[nx - 1]);
        let (y0, y1) = (y[0], y[ny - 1]);
        if x1 <= x0 || y1 <= y0 {
            return self;
        }

        // Integration step: half the smaller cell size, so curves stay smooth.
        let cell_dx = (x1 - x0) / (nx - 1) as f64;
        let cell_dy = (y1 - y0) / (ny - 1) as f64;
        let h = 0.5 * cell_dx.min(cell_dy);
        if h <= 0.0 {
            return self;
        }

        let span = (x1 - x0).max(y1 - y0);
        let min_len = cell_dx.min(cell_dy);

        let mut occ = Occupancy::new(x0, x1, y0, y1);
        let color = self.next_cycle_color();
        let mut next_id: u32 = 1;

        // Seed at every occupancy-cell center, in scan order.
        let seeds: Vec<(f64, f64)> = (0..occ.n)
            .flat_map(|cj| {
                (0..occ.n).map(move |ci| {
                    (
                        occ.x0 + (ci as f64 + 0.5) * occ.dx,
                        occ.y0 + (cj as f64 + 0.5) * occ.dy,
                    )
                })
            })
            .collect();

        for (sx, sy) in seeds {
            let (ci, cj) = occ.cell(sx, sy);
            if occ.owner(ci, cj) != 0 {
                continue;
            }
            let id = next_id;
            if let Some(points) = integrate(&field, &mut occ, sx, sy, h, id) {
                // Skip degenerate stubs shorter than a cell.
                if polyline_span(&points) < min_len {
                    continue;
                }
                next_id += 1;
                draw_streamline(self, color, &points, span);
            }
        }

        self.include_data_bbox(x0, y0, x1, y1);
        self
    }
}

/// Grow a streamline through `seed`, integrating forward then backward and
/// concatenating into one ordered polyline. Returns `None` if too short.
fn integrate(
    field: &Field,
    occ: &mut Occupancy,
    sx: f64,
    sy: f64,
    h: f64,
    id: u32,
) -> Option<Vec<[f64; 2]>> {
    // Claim the seed cell up front so the two directions share ownership.
    let (sci, scj) = occ.cell(sx, sy);
    occ.mark(sci, scj, id);

    let forward = trace(field, occ, sx, sy, h, id);
    let backward = trace(field, occ, sx, sy, -h, id);

    // `backward` runs seed -> ...; reverse it and drop the duplicated seed,
    // then append the forward trace so the result runs
    // tail-back -> seed -> tail-fwd.
    let mut points: Vec<[f64; 2]> = backward.into_iter().rev().collect();
    if !forward.is_empty() {
        // forward[0] is the seed, already present as the last of `points`.
        points.extend(forward.into_iter().skip(1));
    }

    if points.len() < 2 {
        return None;
    }
    Some(points)
}

/// Integrate one direction (`signed_h` carries the sign) from the seed with
/// RK4 on the unit velocity field, marking occupancy and stopping on exit,
/// collision with a different line, stagnation, or the step cap.
fn trace(
    field: &Field,
    occ: &mut Occupancy,
    sx: f64,
    sy: f64,
    signed_h: f64,
    id: u32,
) -> Vec<[f64; 2]> {
    let mut points = vec![[sx, sy]];
    let (mut px, mut py) = (sx, sy);

    for _ in 0..MAX_STEPS {
        match rk4_step(field, px, py, signed_h) {
            Some((nx, ny)) => {
                let (ci, cj) = occ.cell(nx, ny);
                let owner = occ.owner(ci, cj);
                if owner != 0 && owner != id {
                    // Entered another streamline's territory: stop cleanly.
                    break;
                }
                occ.mark(ci, cj, id);
                px = nx;
                py = ny;
                points.push([px, py]);
            }
            None => break,
        }
    }

    points
}

/// One RK4 step of the UNIT velocity field, returning the new position or
/// `None` if any stage leaves the domain or the field stagnates.
fn rk4_step(field: &Field, px: f64, py: f64, h: f64) -> Option<(f64, f64)> {
    let dir = |x: f64, y: f64| -> Option<(f64, f64)> {
        let (u, v) = field.sample(x, y)?;
        let mag = u.hypot(v);
        if mag <= f64::EPSILON {
            None
        } else {
            Some((u / mag, v / mag))
        }
    };

    let (k1x, k1y) = dir(px, py)?;
    let (k2x, k2y) = dir(px + 0.5 * h * k1x, py + 0.5 * h * k1y)?;
    let (k3x, k3y) = dir(px + 0.5 * h * k2x, py + 0.5 * h * k2y)?;
    let (k4x, k4y) = dir(px + h * k3x, py + h * k3y)?;

    let nx = px + (h / 6.0) * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
    let ny = py + (h / 6.0) * (k1y + 2.0 * k2y + 2.0 * k3y + k4y);
    Some((nx, ny))
}

/// Bounding-box diagonal span of a polyline, for the min-length filter.
fn polyline_span(points: &[[f64; 2]]) -> f64 {
    let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in points {
        xmin = xmin.min(p[0]);
        xmax = xmax.max(p[0]);
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    (xmax - xmin).hypot(ymax - ymin)
}

/// Add the streamline polyline and a mid-line arrowhead to the axes.
fn draw_streamline(ax: &mut Axes, color: crate::core::color::Rgba, points: &[[f64; 2]], span: f64) {
    let xs: Vec<f64> = points.iter().map(|p| p[0]).collect();
    let ys: Vec<f64> = points.iter().map(|p| p[1]).collect();
    ax.add_line(Line2D::new(xs, ys).with_color(color));

    // Arrowhead near the middle, pointing along the local flow direction.
    let mid = points.len() / 2;
    if mid + 1 < points.len() {
        let a = points[mid];
        let b = points[mid + 1];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let mag = dx.hypot(dy);
        if mag > f64::EPSILON {
            let dir = [dx / mag, dy / mag];
            let perp = [-dir[1], dir[0]];
            let hl = HEAD_LENGTH_FRACTION * span;
            let hw = HEAD_HALF_WIDTH_FRACTION * span;
            // Tip slightly ahead of the mid point along the flow.
            let tip = [a[0] + dir[0] * hl, a[1] + dir[1] * hl];
            let left = [a[0] + perp[0] * hw, a[1] + perp[1] * hw];
            let right = [a[0] - perp[0] * hw, a[1] - perp[1] * hw];
            let head = Patch::polygon(&[tip, left, right])
                .facecolor(Some(color))
                .edgecolor(Some(color));
            ax.add_patch(head);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bbox;

    /// An `nx x ny` grid on `[-3, 3]^2` filled from the component closure `f`.
    fn grid(
        nx: usize,
        ny: usize,
        f: impl Fn(f64, f64) -> (f64, f64),
    ) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let x: Vec<f64> = (0..nx)
            .map(|i| -3.0 + 6.0 * i as f64 / (nx - 1) as f64)
            .collect();
        let y: Vec<f64> = (0..ny)
            .map(|j| -3.0 + 6.0 * j as f64 / (ny - 1) as f64)
            .collect();
        let mut u = vec![0.0; nx * ny];
        let mut v = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (uu, vv) = f(x[i], y[j]);
                u[j * nx + i] = uu;
                v[j * nx + i] = vv;
            }
        }
        (x, y, u, v)
    }

    #[test]
    fn sample_outside_grid_is_none_and_corner_is_exact() {
        let (x, y, u, v) = grid(5, 5, |px, py| (px + 2.0 * py, 3.0 * px - py));
        let field = Field {
            x: &x,
            y: &y,
            u: &u,
            v: &v,
            nx: 5,
            ny: 5,
        };
        // Outside the grid returns None.
        assert!(field.sample(-3.5, 0.0).is_none());
        assert!(field.sample(0.0, 4.0).is_none());
        // At a grid point the bilinear value is the exact corner value.
        let (su, sv) = field.sample(x[2], y[3]).expect("inside grid");
        assert!((su - (x[2] + 2.0 * y[3])).abs() < 1e-12);
        assert!((sv - (3.0 * x[2] - y[3])).abs() < 1e-12);
    }

    #[test]
    fn uniform_field_yields_horizontal_streamlines() {
        // u = 1, v = 0 everywhere: every streamline must stay at constant y.
        let (x, y, u, v) = grid(10, 10, |_, _| (1.0, 0.0));
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.streamplot(&x, &y, &u, &v);
        assert!(!ax.lines.is_empty(), "expected streamlines");
        for line in &ax.lines {
            let pts = line.points();
            let y0 = pts[0][1];
            for p in &pts {
                assert!(
                    (p[1] - y0).abs() < 1e-6,
                    "line drifted in y: {} vs {y0}",
                    p[1]
                );
            }
        }
    }

    #[test]
    fn circular_field_keeps_constant_radius() {
        // u = -y, v = x: streamlines are concentric circles. Trace one line
        // directly and assert the radius stays within a tight band, which
        // catches integrator drift (a bad step spirals outward).
        let (x, y, u, v) = grid(25, 25, |px, py| (-py, px));
        let field = Field {
            x: &x,
            y: &y,
            u: &u,
            v: &v,
            nx: 25,
            ny: 25,
        };
        let mut occ = Occupancy::new(-3.0, 3.0, -3.0, 3.0);
        let h = 0.5 * (6.0 / 24.0);
        // Seed at radius 2 on the +x axis.
        let pts = trace(&field, &mut occ, 2.0, 0.0, h, 1);
        assert!(pts.len() > 50, "expected a long trace, got {}", pts.len());
        let r0 = (pts[0][0].powi(2) + pts[0][1].powi(2)).sqrt();
        for p in &pts {
            let r = (p[0].powi(2) + p[1].powi(2)).sqrt();
            assert!(
                (r - r0).abs() < 0.1,
                "radius drifted: {r} vs {r0} (integrator spiraling)"
            );
        }
    }

    #[test]
    fn empty_or_mismatched_input_draws_nothing() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.streamplot(&[], &[], &[], &[]);
        assert!(ax.lines.is_empty());
        assert!(ax.patches.is_empty());
        assert!(ax.data_limits().is_none());

        // Mismatched component lengths: nx*ny = 4 but u has 3 entries.
        let mut ax2 = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax2.streamplot(
            &[0.0, 1.0],
            &[0.0, 1.0],
            &[1.0, 1.0, 1.0],
            &[0.0, 0.0, 0.0, 0.0],
        );
        assert!(ax2.lines.is_empty());
        assert!(ax2.data_limits().is_none());

        // Too-small grid (nx < 2).
        let mut ax3 = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax3.streamplot(&[0.0], &[0.0, 1.0], &[1.0, 1.0], &[0.0, 0.0]);
        assert!(ax3.lines.is_empty());
        assert!(ax3.data_limits().is_none());
    }
}
