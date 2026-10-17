/**
 * rizzma-demos — the live wasm demos, shared by every page that shows them.
 *
 * The interactive demo site (`index.html`) mounts every entry of `demos`; the
 * crate docs (`docs-header.html`) import this module from the published demo
 * site and mount the entries named by `data-demo` cells in the rustdoc. One
 * definition means the two cannot drift, and the demo site is a superset of
 * the docs by construction. `tests/live_demos.rs` checks every docs cell
 * names a demo defined here.
 *
 * Published docs of every crate version load the latest copy of this module
 * (and of the wasm pkg) from gh-pages, so keep `build`'s signature stable and
 * never remove a demo the docs name.
 *
 * @module rizzma-demos
 */

// Shared animation clock: each animator is {host, tick(t)}; ticks only run
// while the host element is near the viewport.
let animators = [];
const visible = new WeakSet();
const animIo = new IntersectionObserver(
  (entries) => {
    for (const en of entries) {
      if (en.isIntersecting) visible.add(en.target);
      else visible.delete(en.target);
    }
  },
  { rootMargin: "100px" },
);
const t0 = performance.now();
setInterval(() => {
  const t = (performance.now() - t0) / 1000;
  for (const a of animators) {
    if (visible.has(a.host)) a.tick(t);
  }
}, 50);
const animate = (host, tick) => {
  animIo.observe(host);
  animators.push({ host, tick });
};

/** Detach every animator bound to `host` (call before freeing its session). */
export const unanimate = (host) => {
  animators = animators.filter((a) => a.host !== host);
  animIo.unobserve(host);
};

const F64 = Float64Array;
const linspace = (a, b, n) => {
  const out = new F64(n);
  for (let i = 0; i < n; i++) out[i] = a + ((b - a) * i) / (n - 1);
  return out;
};

/**
 * Demo builders: `build(mod, canvas, host, k) -> session`, where `mod` is the
 * initialized wasm pkg, `canvas` an attached `<canvas>` with an id, and
 * `host` the element whose visibility gates the animation. Keep the returned
 * session referenced (an unreferenced one is GC'd and goes inert); every
 * session / `WasmAxes3D` exposes `.free()`. `k` scales the figure size; data
 * resolution stays the same.
 */
export const demos = {
  // Two close frequencies beating, the pattern traveling with time.
  beats(mod, canvas, host, k = 1) {
    const n = 700;
    const xs = linspace(0, 16 * Math.PI, n);
    const ys = new F64(n);
    const fill = (phase) => {
      for (let i = 0; i < n; i++) {
        ys[i] = Math.sin(3 * xs[i] - phase) + Math.sin(3.35 * xs[i] - phase);
      }
    };
    fill(0);
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot(ax, xs, ys);
    fig.set_title(ax, "plot: two strings beating");
    fig.set_ylim(ax, -2.2, 2.2);
    const session = fig.bind(canvas.id);
    animate(host, (t) => {
      fill(t * 4);
      session.set_line_data(ax, 0, xs, ys);
    });
    return session;
  },

  // Two counter-propagating waves and their interference sum.
  interference(mod, canvas, host, k = 1) {
    const n = 500;
    const xs = linspace(0, 4 * Math.PI, n);
    const y1 = new F64(n);
    const y2 = new F64(n);
    const sum = new F64(n);
    const fill = (t) => {
      for (let i = 0; i < n; i++) {
        y1[i] = Math.sin(xs[i] - 2 * t) * 0.9;
        y2[i] = Math.sin(1.5 * xs[i] + 2 * t) * 0.7;
        sum[i] = y1[i] + y2[i];
      }
    };
    fill(0);
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot_styled(ax, xs, y1, { color: "tab:gray", lw: 1.0 });
    fig.plot_styled(ax, xs, y2, { color: "tab:gray", lw: 1.0 });
    fig.plot_styled(ax, xs, sum, { color: "tab:blue", lw: 2.0 });
    fig.set_title(ax, "counter-propagating waves");
    fig.set_ylim(ax, -2.0, 2.0);
    const session = fig.bind(canvas.id);
    animate(host, (t) => {
      fill(t);
      session.set_line_data(ax, 0, xs, y1);
      session.set_line_data(ax, 1, xs, y2);
      session.set_line_data(ax, 2, xs, sum);
    });
    return session;
  },

  // A rose curve morphing its petal count.
  rose(mod, canvas, host, k = 1) {
    const n = 720;
    const xs = new F64(n);
    const ys = new F64(n);
    const fill = (kk) => {
      for (let i = 0; i < n; i++) {
        const th = (2 * Math.PI * i) / (n - 1);
        const r = Math.cos(kk * th);
        xs[i] = r * Math.cos(th);
        ys[i] = r * Math.sin(th);
      }
    };
    fill(3);
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot_styled(ax, xs, ys, { color: "tab:purple", lw: 1.5 });
    fig.set_title(ax, "rose, petals in motion");
    fig.set_xlim(ax, -1.15, 1.15);
    fig.set_ylim(ax, -1.15, 1.15);
    const session = fig.bind(canvas.id);
    animate(host, (t) => {
      fill(4.5 + 2.5 * Math.sin(0.25 * t));
      session.set_line_data(ax, 0, xs, ys);
    });
    return session;
  },

  // A random two-tone signal streaming over a live spectrogram of itself,
  // sharex-linked. The x axis is a fixed trailing window ("seconds before
  // now"): explicit static limits mean the trace and image sit flush
  // against the frame, tick labels never grow or churn, and double-click
  // home restores exactly this view.
  spectro(mod, canvas, host, k = 1) {
    const fs = 80; // samples/sec
    const win = 64; // DFT window
    const bins = 24; // frequency rows, up to bins*fs/win = 30 Hz
    const cols = 120; // spectrogram columns (one per tick)
    const dt = 0.05; // ticker period
    const span = cols * dt; // seconds on screen
    let seed = 0xd5b;
    const rand = () => {
      seed ^= seed << 13;
      seed ^= seed >>> 17;
      seed ^= seed << 5;
      return ((seed >>> 0) % 10000) / 10000;
    };
    // The two tones random-walk their frequencies.
    let f1 = 8;
    let f2 = 22;
    let ph1 = 0;
    let ph2 = 0;
    const samples = [];
    const times = [];
    const grid = new F64(bins * cols);
    const hann = new F64(win);
    for (let j = 0; j < win; j++) {
      hann[j] = 0.5 - 0.5 * Math.cos((2 * Math.PI * j) / (win - 1));
    }

    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const top = fig.add_subplot(2, 1, 1);
    const bottom = fig.add_subplot(2, 1, 2);
    fig.plot(top, new F64(0), new F64(0));
    fig.set_title(top, "signal + live spectrogram");
    fig.set_xlim(top, -span, 0);
    fig.set_ylim(top, -2.6, 2.6);
    fig.imshow(bottom, grid, bins, cols, [-span, 0, 0, 30], "", 0, 1.1);
    fig.set_ylabel(bottom, "Hz");
    fig.set_xlabel(bottom, "t − now (s)");
    fig.sharex(bottom, top);
    const session = fig.bind(canvas.id);

    let lastT = 0;
    animate(host, (t) => {
      // Wander the tones, then extend the signal since the last tick.
      f1 = Math.min(14, Math.max(3, f1 + (rand() - 0.5) * 0.6));
      f2 = Math.min(28, Math.max(16, f2 + (rand() - 0.5) * 0.9));
      const steps = Math.max(1, Math.min(8, Math.round((t - lastT) * fs)));
      for (let j = 0; j < steps; j++) {
        ph1 += (2 * Math.PI * f1) / fs;
        ph2 += (2 * Math.PI * f2) / fs;
        samples.push(
          Math.sin(ph1) + 0.8 * Math.sin(ph2) + (rand() - 0.5) * 0.7,
        );
        times.push(t - (steps - 1 - j) / fs);
      }
      while (times.length > 1 && t - times[0] > span) {
        times.shift();
        samples.shift();
      }
      // Data slides through the fixed [-span, 0] window.
      const rel = new F64(times.length);
      for (let i = 0; i < times.length; i++) rel[i] = times[i] - t;
      session.set_line_data(top, 0, rel, F64.from(samples));

      // Newest spectrogram column: Hann-windowed DFT magnitudes of the
      // last `win` samples, high frequencies on top (row 0).
      grid.copyWithin(0, 1);
      for (let r = 0; r < bins; r++) grid[(r + 1) * cols - 1] = 0;
      if (samples.length >= win) {
        const o = samples.length - win;
        for (let kk = 1; kk <= bins; kk++) {
          let re = 0;
          let im = 0;
          for (let j = 0; j < win; j++) {
            const a = (2 * Math.PI * kk * j) / win;
            const s = samples[o + j] * hann[j];
            re += s * Math.cos(a);
            im -= s * Math.sin(a);
          }
          const row = bins - kk;
          grid[(row + 1) * cols - 1] = Math.hypot(re, im) / (win / 4);
        }
      }
      session.set_image_data(bottom, 0, grid, bins, cols, [-span, 0, 0, 30]);
      lastT = t;
    });
    return session;
  },

  // A two-armed spiral galaxy spinning about an out-of-plane axis: the
  // stars live in 3D (a thin disk), and each tick applies the full
  // rotation matrix M = Rz(precession) · Rx(inclination) · Rz(spin) and
  // projects orthographically to the screen. Fully interactive — it keeps
  // turning inside whatever view you zoom to.
  galaxy(mod, canvas, host, k = 1) {
    const stars = 900;
    const gx = new F64(stars);
    const gy = new F64(stars);
    const gz = new F64(stars);
    const rx = new F64(stars);
    const ry = new F64(stars);
    let seed = 0x5eed;
    const rand = () => {
      seed ^= seed << 13;
      seed ^= seed >>> 17;
      seed ^= seed << 5;
      return ((seed >>> 0) % 10000) / 10000;
    };
    for (let i = 0; i < stars; i++) {
      const arm = i % 2 === 0 ? 0 : Math.PI;
      const a = 0.4 + 3.4 * (i / stars);
      const r = 0.22 * a;
      gx[i] = r * Math.cos(2.1 * a + arm) + (rand() - 0.5) * 0.09;
      gy[i] = r * Math.sin(2.1 * a + arm) + (rand() - 0.5) * 0.09;
      // A thin disk, slightly thicker toward the core.
      gz[i] = ((rand() - 0.5) * 0.08) / (1 + 2.5 * r);
    }
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.scatter(ax, gx, gy);
    fig.set_title(ax, "scatter: zoom into the galaxy");
    fig.set_xlim(ax, -0.95, 0.95);
    fig.set_ylim(ax, -0.95, 0.95);
    const session = fig.bind(canvas.id);
    const ca = Math.cos(0.96); // ~55° inclination
    const sa = Math.sin(0.96);
    animate(host, (t) => {
      const th = 0.4 * t; // spin about the disk normal
      const ph = 0.06 * t; // slow precession of the projected axis
      const c1 = Math.cos(th);
      const s1 = Math.sin(th);
      const c2 = Math.cos(ph);
      const s2 = Math.sin(ph);
      // Screen rows of M = Rz(ph) · Rx(a) · Rz(th).
      const m00 = c2 * c1 - s2 * ca * s1;
      const m01 = -c2 * s1 - s2 * ca * c1;
      const m02 = s2 * sa;
      const m10 = s2 * c1 + c2 * ca * s1;
      const m11 = -s2 * s1 + c2 * ca * c1;
      const m12 = -c2 * sa;
      for (let i = 0; i < stars; i++) {
        rx[i] = m00 * gx[i] + m01 * gy[i] + m02 * gz[i];
        ry[i] = m10 * gx[i] + m11 * gy[i] + m12 * gz[i];
      }
      session.set_scatter_offsets(ax, 0, rx, ry);
    });
    return session;
  },

  // A spinning 3D sombrero surface, re-rendered each tick from a new
  // azimuth (WasmAxes3D renders full frames; no session machinery).
  surface3d(mod, canvas, host, k = 1) {
    const n = 32;
    const gx = linspace(-8, 8, n);
    const gz = new F64(n * n);
    for (let j = 0; j < n; j++) {
      for (let i = 0; i < n; i++) {
        const r = Math.hypot(gx[i], gx[j]);
        gz[j * n + i] = r < 1e-9 ? 1 : Math.sin(r) / r;
      }
    }
    const ax = new mod.WasmAxes3D(
      Math.round(360 * k),
      Math.round(270 * k),
      100,
    );
    ax.plot_surface(gx, gx, gz);
    ax.set_title("plot_surface: sombrero, spinning");
    ax.set_view(28, -60);
    ax.render(canvas.id);
    animate(host, (t) => {
      ax.set_view(28, -60 + 12 * t);
      ax.render(canvas.id);
    });
    return ax;
  },

  // A ripple tank: two point sources interfering, the field streamed
  // through imshow as a diverging-colormapped image.
  ripples(mod, canvas, host, k = 1) {
    const nr = 40;
    const nc = 54;
    const grid = new F64(nr * nc);
    const fill = (t) => {
      for (let r = 0; r < nr; r++) {
        for (let c = 0; c < nc; c++) {
          const x = (c + 0.5) / nc;
          const y = (r + 0.5) / nr;
          const r1 = Math.hypot(x - 0.32, y - 0.5);
          const r2 = Math.hypot(x - 0.68, y - 0.5);
          grid[r * nc + c] =
            Math.sin(38 * r1 - 5 * t) / (1 + 4 * r1) +
            Math.sin(38 * r2 - 5 * t) / (1 + 4 * r2);
        }
      }
    };
    fill(0);
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.imshow(ax, grid, nr, nc, [0, 1.35, 0, 1], "coolwarm", -1.4, 1.4);
    fig.set_title(ax, "imshow: a ripple tank");
    const session = fig.bind(canvas.id);
    animate(host, (t) => {
      fill(t);
      session.set_image_data(ax, 0, grid, nr, nc, [0, 1.35, 0, 1]);
    });
    return session;
  },

  // The Lorenz attractor traced live in the x-z plane.
  lorenz(mod, canvas, host, k = 1) {
    const keep = 2200;
    let x = 0.9;
    let y = 1.4;
    let z = 19;
    const xs = [];
    const zs = [];
    const step = () => {
      const dt = 0.005;
      const dx = 10 * (y - x);
      const dy = x * (28 - z) - y;
      const dz = x * y - (8 / 3) * z;
      x += dx * dt;
      y += dy * dt;
      z += dz * dt;
      xs.push(x);
      zs.push(z);
      if (xs.length > keep) {
        xs.shift();
        zs.shift();
      }
    };
    for (let i = 0; i < 400; i++) step();
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot_styled(ax, F64.from(xs), F64.from(zs), {
      color: "tab:orange",
      lw: 1.0,
    });
    fig.set_title(ax, "the Lorenz attractor, live");
    fig.set_xlim(ax, -23, 23);
    fig.set_ylim(ax, 0, 52);
    const session = fig.bind(canvas.id);
    animate(host, () => {
      for (let i = 0; i < 12; i++) step();
      session.set_line_data(ax, 0, F64.from(xs), F64.from(zs));
    });
    return session;
  },

  // Streaming oscilloscope strips, x-linked.
  scope(mod, canvas, host, k = 1) {
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    fig.set_facecolor("#0a0d0b");
    const strips = [
      fig.add_axes(0.0, 0.68, 1.0, 0.32),
      fig.add_axes(0.0, 0.34, 1.0, 0.32),
      fig.add_axes(0.0, 0.0, 1.0, 0.32),
    ];
    const empty = new F64(0);
    for (const s of strips) {
      fig.oscilloscope(s);
      fig.plot(s, empty, empty);
    }
    fig.sharex(strips[1], strips[0]);
    fig.sharex(strips[2], strips[0]);
    const session = fig.bind(canvas.id);

    let seed = 0xacab;
    const rand = () => {
      seed ^= seed << 13;
      seed ^= seed >>> 17;
      seed ^= seed << 5;
      return ((seed >>> 0) % 10000) / 10000;
    };
    const T = [];
    const C = [[], [], []];
    let drift = 0;
    animate(host, (t) => {
      drift += (rand() - 0.5) * 0.05;
      T.push(t);
      C[0].push(Math.sin(t * 2.1) * 0.4 + (rand() - 0.5) * 0.12);
      C[1].push(Math.cos(t * 1.3) * 0.3 + drift + (rand() - 0.5) * 0.12);
      C[2].push(18 + Math.sin(t * 0.7) * 3 + (rand() - 0.5) * 2);
      while (T.length > 1 && t - T[0] > 10) {
        T.shift();
        for (const c of C) c.shift();
      }
      const ts = F64.from(T);
      strips.forEach((s, i) => {
        session.set_line_data(s, 0, ts, F64.from(C[i]));
      });
    });
    return session;
  },

  // A damped oscillation under its decay envelope, the phase rolling.
  damped(mod, canvas, host, k = 1) {
    const n = 400;
    const xs = linspace(0, 12, n);
    const envelope = new F64(n);
    const ys = new F64(n);
    const fill = (phase) => {
      for (let i = 0; i < n; i++) {
        ys[i] = envelope[i] * Math.cos(2 * xs[i] + phase);
      }
    };
    for (let i = 0; i < n; i++) envelope[i] = Math.exp(-xs[i] / 4);
    fill(0);
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot(ax, xs, ys);
    fig.plot_styled(ax, xs, envelope, { color: "tab:orange", ls: "--", lw: 1.0 });
    fig.set_title(ax, "plot: damped oscillation");
    fig.set_xlabel(ax, "t");
    fig.set_ylabel(ax, "amplitude");
    fig.legend(ax, ["exp(-t/4)·cos(2t)", "envelope"]);
    const session = fig.bind(canvas.id);
    animate(host, (t) => {
      fill(2.4 * t);
      session.set_line_data(ax, 0, xs, ys);
    });
    return session;
  },

  // Log-log axes: Zipf's law.
  zipf(mod, canvas, host, k = 1) {
    const n = 240;
    const xs = linspace(1, 1000, n);
    const ys = new F64(n);
    for (let i = 0; i < n; i++) ys[i] = 1.0e6 / xs[i];
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot(ax, xs, ys);
    fig.set_xscale_log(ax, 10);
    fig.set_yscale_log(ax, 10);
    fig.set_xlim(ax, 1, 1000);
    fig.set_ylim(ax, 1.0e3, 1.0e6);
    fig.set_title(ax, "loglog: Zipf's law");
    fig.set_xlabel(ax, "word rank");
    fig.set_ylabel(ax, "frequency");
    return fig.bind(canvas.id);
  },

  // The pointer's trail, recorded on the Rust side: track_cursor mirrors
  // pointer positions into the line with no JS in the loop.
  trail(mod, canvas, host, k = 1) {
    const fig = new mod.WasmFigure(3.6 * k, 2.7 * k);
    const ax = fig.add_subplot(1, 1, 1);
    fig.plot(ax, new F64(0), new F64(0));
    fig.set_xlim(ax, 0.0, 10.0);
    fig.set_ylim(ax, 0.0, 10.0);
    fig.set_title(ax, "track_cursor: move your pointer here");
    fig.set_xlabel(ax, "x (data units)");
    fig.set_ylabel(ax, "y (data units)");
    const session = fig.bind(canvas.id);
    session.track_cursor(ax, 0, 400);
    return session;
  },
};
