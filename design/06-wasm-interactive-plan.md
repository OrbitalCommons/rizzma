# Wasm Support Charter & Interactive Plotting Plan

> **Status:** implemented — W1–W6 landed 2026-07 (see the Unreleased CHANGELOG
> entry). Known deviations from the plan below: the DOM bridge omits a
> `ResizeObserver` (the figure's inch size is fixed, so a container resize
> changes nothing; `Event::Resize` exists for hosts that want to feed it
> manually), `grid` is not on the JS surface (there is no Rust `Axes::grid`
> yet, and the surface never runs ahead of the Rust API), and `legend(labels)`
> pairs labels with plotted lines by index.
>
> This document consolidates the wasm commitments that were previously
> scattered across `01-architecture.md` §11.6, `04-implementation-plan.md`
> Phase 9 (PR-44…46), `AGENTS.md`, and CI — and extends them into a concrete
> plan for **interactive plots in the browser** (hover, pan, zoom, reset).
>
> Written 2026-07-05 against `crates/rizzma` 1.0.1.

---

## 1. The charter, in one place

Rizzma is **wasm-first**. Concretely that means:

1. **Every crate compiles to `wasm32-unknown-unknown` or is `cfg`-gated out** of the
   wasm build. Enforced by CI (`cargo check --workspace --exclude xtask --target
   wasm32-unknown-unknown`). No C dependencies in the default build.
2. **One rendering codepath.** The browser is a *blit target*, not a renderer:
   tiny-skia rasterizes the figure entirely in Rust, the straight-alpha RGBA buffer
   is pushed to `<canvas>` via `putImageData`. Browser output is pixel-identical to
   PNG export, so the golden-image suite covers the browser for free.
3. **The OO layer is unaware of the target.** Artists draw through the `Renderer`
   trait; only the final blit differs per target. This is the property to protect
   above all others (per `01-architecture.md` §11.6).
4. **Size stays bounded.** CI enforces a wasm artifact budget
   (`WASM_SIZE_MAX_BYTES`, currently 2.5 MiB) via `cargo xtask wasm-size`.
5. **The only interactive target is the browser canvas.** No GUI toolkit backends,
   ever. Interactivity (this document's main subject) is designed once, in core
   Rust types, with the DOM as one thin event source.

### Explicit non-goals

- **Canvas2D-native renderer** (`fillText`/`Path2D` per draw call). It would fork
  the rendering path the whole design keeps singular. Revisit only if profiling
  proves the blit path can't hit the interaction budget (§7).
- **WebGL/WebGPU**, wasm threads/SIMD, `OffscreenCanvas` workers. Future options,
  not current scope; nothing below should preclude them.
- **Host-font text.** Text renders from embedded font outlines like every other
  target; wasm gets no special text path.

---

## 2. Where we are (1.0.1)

Implemented, in `crates/rizzma/src/wasm/mod.rs` and `crates/rizzma/www/`:

- `figure_to_rgba(&Figure) -> (Vec<u8>, u32, u32)` — target-agnostic render to
  straight RGBA8 (un-premultiplied for `ImageData`), tested on the native host.
- `WasmFigure` — a `#[wasm_bindgen]`-owned figure with `size`, `render(canvas_id)`
  (wasm-only blit), and `data_at(px, py)` (pixel → data readout).
- A demo page wiring `mousemove` to `data_at` for a hover readout.
- CI: wasm target check + size budget.

Gaps, in rough order of user-visible pain:

| Gap | Consequence today |
|---|---|
| No `devicePixelRatio` handling | Blurry plots on every HiDPI display |
| JS can only build `WasmFigure::sample()` | The wasm feature is a demo, not a backend |
| No event bridge / interaction tools | No pan, zoom, or reset; hover is hand-wired JS |
| No `wasm-pack test --headless` in CI | Nothing proves the bindgen boundary works in a browser |
| No per-frame perf budget | Interaction cost is unmeasured (matters once pan/zoom exists) |

The plan below closes these in order.

---

## 3. Phase W1 — HiDPI-correct rendering

**Problem.** The canvas backing store is sized to figure pixels at logical DPI;
browsers upscale the bitmap on HiDPI displays.

**Design.** Render at `dpi × scale` where `scale` is supplied by the host
(`window.devicePixelRatio`), then present at logical size:

```rust
/// Straight RGBA at `scale` × the figure's DPI. `scale = 1.0` is today's output.
pub fn figure_to_rgba_scaled(fig: &Figure, scale: f64) -> (Vec<u8>, u32, u32);
```

The wasm blit sets `canvas.width/height` to the scaled pixel size and
`canvas.style.width/height` to the logical CSS size. `WasmFigure::render` reads
`devicePixelRatio` itself; a `render_with_scale` escape hatch keeps it testable.

**The coordinate invariant:** `Figure`'s pixel APIs (`pixel_to_data`,
`data_to_pixel`, `axes_at`) and the core event layer operate in **top-down
logical figure pixels** (`size_px()`), before any DPR scaling — exactly the
space they already used. Backing-store (device) pixels exist only on the
render/blit side. The **DOM bridge alone** converts browser coordinates (CSS
pixels relative to the canvas) into logical figure pixels; with the canvas CSS
size pinned to the logical size this is the identity map, so JS callers never
think about DPR at all.

Scaling must multiply the effective DPI (so line widths, fonts, and markers scale
together), not just the pixmap dimensions. If `Figure` DPI is immutable at render
time, `figure_to_rgba_scaled` renders through a DPI-overriding render entry rather
than mutating the figure.

**Acceptance:** at `scale = 2`, output is bit-identical to rendering the same
figure `.with_dpi(2 × dpi)`; demo page is crisp on a retina display; `data_at`
round-trip test passes at non-unit scale.

---

## 4. Phase W2 — A real JS plotting surface

**Problem.** `WasmFigure::sample()` is the only constructor; a host page cannot
plot its own data.

**Design.** Mirror the Rust builder API across the bindgen boundary, keeping the
wasm layer a *thin forwarding shell* — no plotting logic lives in `wasm/`:

```rust
#[wasm_bindgen]
impl WasmFigure {
    #[wasm_bindgen(constructor)]
    pub fn new(width_in: f64, height_in: f64) -> WasmFigure;
    pub fn add_subplot(&mut self, nrows: usize, ncols: usize, index: usize) -> usize; // axes index
    pub fn plot(&mut self, axes: usize, x: &[f64], y: &[f64]);
    pub fn plot_styled(&mut self, axes: usize, x: &[f64], y: &[f64], style: JsValue); // {color, lw, ls, label, marker}
    pub fn scatter(&mut self, axes: usize, x: &[f64], y: &[f64]);
    pub fn set_title(&mut self, axes: usize, s: &str);
    pub fn set_xlabel(&mut self, axes: usize, s: &str);
    pub fn set_ylabel(&mut self, axes: usize, s: &str);
    pub fn set_xlim(&mut self, axes: usize, lo: f64, hi: f64);
    pub fn set_ylim(&mut self, axes: usize, lo: f64, hi: f64);
    pub fn legend(&mut self, axes: usize);
    pub fn grid(&mut self, axes: usize, on: bool);
}
```

Conventions:

- **Axes are indices** (`usize` into `Figure::axes`), matching the existing
  `pixel_to_data(axes_index, …)` convention. No JS-side axes objects to keep alive.
- **`&[f64]` accepts `Float64Array`-compatible input**; wasm-bindgen copies the
  typed array's contents into wasm memory at the boundary (it is *not*
  zero-copy), and the figure copies again into its own plot data. `Figure`
  must retain the data anyway, so the copies are fine; revisit bulk/borrowing
  APIs only if profiling shows W2 ingestion matters.
- **Styling via a plain JS object** (`serde-wasm-bindgen` or manual `Reflect`
  reads) rather than N positional-arg variants. Unknown keys are errors.
- Coverage target is **Tier-1 plot types** (line, scatter, bar, hist, imshow as
  they exist in the Rust API); grow the surface with the Rust API, never ahead
  of it.

**Acceptance:** the demo page builds its plot from JS data instead of
`sample()`; a headless browser test (W6) constructs, plots, renders, and reads
back a pixel.

---

## 5. Phase W3 — Event bridge (core types + DOM source)

This is chartered PR-45, expanded. The key decision: **event types live in core
rizzma, not in `wasm/`** — the interaction layer (W4) is then pure, host-agnostic
Rust, testable natively by synthesizing events.

### 5.1 Core types (`src/figure/event.rs` or `src/core/event.rs`)

```rust
pub enum MouseButton { Left, Middle, Right }

/// All positions are figure pixels, top-left origin, logical (pre-DPR) scale —
/// i.e. the same space `Figure::pixel_to_data` already accepts.
pub enum Event {
    MouseDown { x: f64, y: f64, button: MouseButton },
    MouseUp   { x: f64, y: f64, button: MouseButton },
    MouseMove { x: f64, y: f64 },
    Wheel     { x: f64, y: f64, dy: f64 },        // dy > 0 = zoom out
    DoubleClick { x: f64, y: f64 },
    Leave,
    Resize    { width_px: f64, height_px: f64 },  // logical px
}
```

Notes vs. matplotlib/WebAgg:

- matplotlib events carry bottom-up y; **rizzma standardizes on top-down figure
  pixels** everywhere (`pixel_to_data` already does), so there is *no y-flip
  inside core* — the flip WebAgg does is unnecessary because we never expose the
  bottom-up convention.
- DOM `wheel` deltas are normalized to "lines" (÷ ~120 for `deltaMode` pixel
  events) at the bridge, so core sees consistent magnitudes across browsers.
- Hit-testing (`which axes contains (x, y)?`) is a core helper
  (`Figure::axes_at(x, y) -> Option<usize>`), shared by hover and tools.

### 5.2 DOM source (wasm-only, in `wasm/`)

A `bind_events(canvas_id, closure)`-style layer that attaches
`pointerdown/up/move`, `wheel`, `dblclick`, `pointerleave`, and a
`ResizeObserver`, converts each DOM event to an `Event` (offset → logical figure
px via `getBoundingClientRect`, button remap, `preventDefault` on wheel), and
feeds the interactor (W4). Uses pointer events + `setPointerCapture` so drags
survive leaving the canvas. Closures are owned by the `WasmFigure`
(`Closure::wrap` stored in the struct) so lifetime is explicit — no `forget()`.

**Acceptance:** native unit tests drive `Event` sequences through the interactor;
a headless browser test dispatches synthetic `PointerEvent`s and observes limit
changes.

---

## 6. Phase W4 — Interaction tools (pan / zoom / hover / reset)

A small state machine, **pure Rust, no DOM types**, exercising only public
`Figure`/`Axes` API (`set_xlim`/`set_ylim`, `pixel_to_data`, `axes_at`):

```rust
pub struct Interactor {
    fig: Figure,
    home: Vec<(Range, Range)>,   // per-axes limits captured at first interaction
    drag: Option<DragState>,     // axes index + anchor data point
}

pub enum Outcome { Unchanged, NeedsRedraw, Hover { axes: usize, x: f64, y: f64 } }

impl Interactor {
    pub fn handle(&mut self, ev: Event) -> Outcome;
}
```

Tool semantics (matplotlib toolbar behavior, minus the toolbar):

- **Wheel zoom, anchored at the cursor.** Scale x/y limits about the data point
  under the cursor by `factor = 1.1^dy`. The point under the cursor stays under
  the cursor — the invariant users actually feel.
- **Drag pan (left button).** On `MouseDown` inside an axes, record the anchor
  *data* point; each `MouseMove` sets limits so the anchor stays under the
  cursor. Log-scale axes pan in transformed space (delegate to the axis `Scale`
  so this works for `log`/`asinh` without special cases here).
- **Double-click reset ("home").** Restore the limits captured before the first
  interaction on that axes.
- **Hover readout.** `MouseMove` outside a drag returns `Hover { … }`; the host
  (demo page) formats it. Replaces the demo's hand-wired `data_at` JS.
- Interacting sets explicit limits (autoscale stops fighting the user), exactly
  as `set_xlim` already implies.

Box-zoom (rubber band) is deferred: it requires an overlay/compositing story and
is the one tool that doesn't fit "re-render figure per frame". Revisit after W5
measurements.

**Acceptance (all native tests, no browser):** wheel-zoom keeps the cursor's
data point fixed within 1e-9; pan by (dx, dy) then (-dx, -dy) restores limits;
double-click restores home after arbitrary interaction; events outside all axes
are `Unchanged`.

---

## 7. Phase W5 — The render loop & perf budget

Interactivity turns rendering from once into per-frame. Design:

- **Coalesce redraws with `requestAnimationFrame`.** `NeedsRedraw` sets a dirty
  flag; the rAF callback renders at most once per frame, skipping when clean.
  Never render synchronously inside an event handler.
- **Budget — really two budgets.** (a) A **native render budget**, asserted in
  ordinary CI: a Tier-1 line plot at 800×600 logical, DPR 2 (1600×1200
  backing), through `figure_to_rgba_scaled`, must stay under **16 ms** (stretch
  < 8 ms for DPR-3 headroom). This is the "per-frame perf budget" PR-46
  chartered and never got. (b) The **browser render + blit** total, which a
  native bench cannot prove: `putImageData` of a 1600×1200 `ImageData` is
  small-but-nonzero and browser-dependent, so it is covered by the headless
  browser suite as a smoke path today, with real timing telemetry as a
  follow-up if interaction ever feels sluggish. Measured native median at ship
  time: ~14 ms release.
- **Non-integer DPR rounding.** At DPR 1.5/2.25 etc., backing dimensions are
  `trunc(size_px() * scale)` (the pixmap's integer size), and the blit sizes
  the canvas backing store from the returned buffer dimensions — never
  recomputed independently — so buffer and canvas cannot disagree by a pixel.
- **Known costs to attack if the budget fails**, in order: (1) the per-pixel
  un-premultiply loop — chunk it or precompute a LUT; (2) full-figure redraw —
  cache the static background (axes, gridlines, text) as a pixmap and re-render
  only data artists during drags; (3) WebAgg's diff-blit trick. None are built
  until measurement says so.

---

## 8. Phase W6 — CI: prove the boundary in a browser

- Add a `wasm-pack test --headless --chrome` job compiling the wasm-gated tests:
  construct via the W2 API, `render()` to a canvas, read back a pixel with
  `getImageData`, assert ink; dispatch synthetic pointer events end-to-end
  through W3→W4 and assert limits changed.
- Keep the existing size budget; W2 grows the surface, so watch it (the budget
  check already fails loudly).
- The perf budget (W5) runs as a native test with a generous CI multiplier.

---

## 9. PR breakdown

Continuing nothing — this supersedes PR-45/-46's remainder from
`04-implementation-plan.md`. Ordered; each lands green on its own.

| PR | Scope | Size | Depends | Acceptance |
|----|-------|------|---------|------------|
| **W1** | `figure_to_rgba_scaled` + DPR-aware blit + CSS sizing | S | — | scale-2 output ≡ double-DPI render; crisp demo on retina |
| **W2** | JS plotting surface (`new`/`add_subplot`/`plot`/`scatter`/labels/limits/legend/grid, styled via JS object) | M | — | demo builds its plot from JS data |
| **W3a** | Core `Event` enum + `Figure::axes_at` + native tests | S | — | hit-test and event plumbing tested natively |
| **W3b** | DOM bridge: pointer/wheel/dblclick/resize → `Event`, pointer capture, wheel normalization | M | W1, W3a | synthetic `PointerEvent`s reach the interactor |
| **W4** | `Interactor`: wheel-zoom-at-cursor, drag-pan (log-aware), double-click home, hover outcome | M | W3a | the four native invariant tests in §6 pass |
| **W5** | rAF-coalesced render loop in demo + native per-frame bench with budget | S | W1, W4 | < 16 ms @ 1600×1200; no redraw when clean |
| **W6** | `wasm-pack test --headless` CI job (render-readback + event e2e) | S | W2, W3b | job green in CI |

Rough sequencing: W1, W2, W3a can proceed in parallel; W3b/W4 after; W5/W6 close
it out. **M4's definition — "identical figure to PNG / SVG / browser canvas,
interactive" — is reached at W5.**

---

## 10. Risks & open questions

- **DPI plumbing (W1).** If overriding DPI at render time is invasive, an interim
  `Figure::with_dpi` clone-per-scale is acceptable for correctness, with the
  zero-copy path as follow-up. Do not ship non-integer-scale font hinting bugs;
  test at DPR 1, 1.5, 2.
- **Styled-plot options (W2).** The JS style object must not grow its own schema —
  it maps 1:1 onto whatever `Line2D`/`Axes` setters exist. If a style key has no
  Rust setter, the answer is a Rust PR first.
- **`ResizeObserver` semantics.** Resizing re-renders at the new logical size
  (figure inches fixed, canvas CSS size drives nothing yet). True responsive
  figures (inches derived from container) are out of scope here.
- **Event modifiers.** `Event` carries no `shift`/`ctrl`/`alt` state yet; box
  zoom or axis-constrained pan will need them, which means extending the enum's
  variants when they arrive. Accepted: the enum is crate-internal enough that
  the change is cheap, and speculative fields invite dead plumbing.
- **Wheel-delta chaos across browsers/trackpads.** Normalize at the bridge, and
  clamp per-event zoom factor to [1/2, 2] so a momentum-scroll burst can't teleport
  the view.
- **wasm-pack in CI.** Adds a headless Chrome dependency; keep it a separate job
  so the main `fmt + clippy + test` gate stays fast, mirroring the existing
  `wasm-size` job split.
