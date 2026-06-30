# Log-Scale Axes Integration Design

This note scopes the first `loglog`/`semilogx`/`semilogy` integration step. It is
design-only: do not implement until both agents agree on the transform seam.

## Current State

- `rizzma-axis` already has numeric scale transforms (`LinearScale`, `LogScale`,
  `SymlogScale`, `LogitScale`), `LogLocator`, `LogFormatter`, and date locators.
- `rizzma-figure::Axes` still draws with a single affine `trans_data`:
  data coordinates are mapped directly to pixels by `Affine2D`.
- `rizzma-artist::Artist::draw` accepts only `&Affine2D`, and renderers only accept
  affine transforms. This is good for backend portability and should stay true.
- `Axis::draw` receives numeric limits and draws tick positions linearly inside the
  axes rectangle.
- Figure coordinate helpers (`Figure::data_at_pixel`, `Figure::pixel_to_data`) share
  the same affine path and explicitly TODO nonlinear scales.

Matplotlib's model is:

```text
data -> scale non-affine -> limits affine -> axes affine -> display
```

The important split is that the non-affine scale is resolved before the renderer sees
geometry; backends still render paths under an affine transform.

## Option A: Scale Then Affine In `Axes`

Add per-axis scale state to `Axes` and build a scaled coordinate domain at draw time:

```text
raw data -> x/y Scale::transform -> scaled data -> Affine2D -> pixels
```

Implementation shape:

- `Axes` owns `xscale` and `yscale` values, defaulting to linear.
- Effective raw limits remain user-facing data limits, e.g. `xlim = (1, 1000)`.
- Before building `trans_data`, map the raw limits through each scale to scaled limits,
  e.g. `(log10(1), log10(1000)) = (0, 3)`.
- For each artist path, build scaled geometry in `Axes::draw` and pass that scaled path
  plus the scaled-domain `Affine2D` to the renderer.
- For tick marks, locators run in raw data coordinates, but tick positions are scaled
  before converting to pixels.
- For tick labels, formatters receive raw tick values, not scaled values.
- For coordinate inversion, invert the affine into scaled space, then apply the inverse
  scale to recover raw data coordinates.

Pros:

- Keeps `Renderer` and `Artist::draw` affine-only.
- Matches matplotlib's affine/non-affine split conceptually.
- Centralizes scale semantics in `Axes`, where limits, ticks, spans, and coordinate
  inversion already meet.
- Avoids changing `rizzma-core::Affine2D` or adding a transform graph now.

Cons:

- `Axes` must transform geometry for every artist category it draws.
- The current `Artist::draw(&Affine2D)` call is insufficient for nonlinear axes unless
  the path is pre-scaled or the artist exposes transformable geometry.
- Images/meshes need explicit policy: rectilinear images can map their extent corners
  under monotonic scales, while quad meshes should scale every grid coordinate.
- Equal aspect must operate in scaled units, not raw data units.

Recommended implementation detail:

- Introduce an `AxisScale`/`ScaleSpec` enum at the `Axes` layer first, not in
  `rizzma-core`.
- Add private helpers:
  - `scale_x(value)`, `scale_y(value)`, and inverse equivalents.
  - `scaled_limits(raw_xlim, raw_ylim)`.
  - path/point conversion helpers used only inside `Axes::draw`.
- Keep the public `set_xlim`/`set_ylim` contract in raw data units.
- Add `set_xscale_log(base)`, `set_yscale_log(base)`, then convenience wrappers
  `semilogx`, `semilogy`, and `loglog` can call `plot` after setting scales.

## Option B: Push Scale Into The Data-To-Pixel Pipeline

Define a richer transform type, for example:

```rust
enum DataTransform {
    Affine(Affine2D),
    Scaled { x: Scale, y: Scale, affine: Affine2D },
}
```

Then thread it through artists and possibly renderers.

Pros:

- Artists can remain responsible for their own geometry if they accept a richer transform.
- This more closely resembles matplotlib's symbolic transform stack.
- Future projections or blended transforms have a more obvious home.

Cons:

- It changes a broad public seam (`Artist::draw`) and likely every artist crate.
- Renderers cannot consume nonlinear transforms directly, so every artist still needs a
  pre-render non-affine path step.
- It risks growing a transform graph before the library needs one.
- It is harder to keep the current small-PR velocity because every plot artist becomes
  part of the migration.

## Recommendation

Use Option A for the first log-axis integration.

That means nonlinear scale handling is an `Axes` responsibility for now. Artists and
renderers stay affine-only; `Axes` pre-scales data geometry and computes a scaled-domain
`Affine2D`. This keeps the shared public seams stable while still matching the core
matplotlib invariant that non-affine work happens before backend rendering.

Do not change `rizzma-render::Renderer`, `rizzma-artist::Artist`, or `rizzma-core` for
this step.

## Tick And Label Semantics

- Locators operate on raw data limits and produce raw data tick values.
- For log axes, default major locator should be `LogLocator::new(base)`.
- Default minor locator should be `LogLocator::minor(base)`.
- Default formatter should be `LogFormatterMathtext::new(base)` once that lands, so
  large powers can render as rich text.
- Axis drawing needs a scaled-position path:
  - tick value `v` is formatted as raw `v`;
  - tick position is `scale(v)`;
  - normalized position is computed against scaled limits.
- Grid lines and reference lines follow the same rule: raw semantic value, scaled
  display position.

## Limits And Autoscaling

- Data limits are collected in raw data units.
- Scale-specific limit guards happen after autoscale and explicit limits are resolved:
  - log axes reject or clamp non-positive bounds via the existing scale limit policy;
  - symlog/logit can follow the same pattern later.
- Margins for nonlinear scales should be applied in scaled space, otherwise a 5% margin
  around `[1, 1000]` is visually dominated by the upper endpoint.
- Reversed ranges should work by preserving raw limit order while computing scaled limits
  in the same order.

## Artist Categories

- Lines and patches: transform every path vertex through the relevant x/y scale before
  applying the scaled-domain affine.
- Collections: transform offsets through the scale; marker glyph size remains in device
  units as today.
- Quad meshes: transform every grid coordinate. This supports log pcolormesh without
  changing `QuadMesh`.
- Images: first implementation can transform the image extent corners for monotonic
  rectilinear log axes. True nonlinear image resampling is deferred.
- Spans/reference lines: construct raw endpoint geometry as today, then scale the
  relevant coordinates before draw.
- Text/title/richtext: unchanged for this step; tick-label richtext integration belongs
  in `Axis::draw` when it consumes the formatter output.

## Coordinate Inversion

Forward:

```text
raw data -> scale -> scaled affine -> pixels
```

Inverse:

```text
pixels -> inverse affine -> inverse scale -> raw data
```

`Figure::data_at_pixel` and `Figure::pixel_to_data` must use the same scaled-limits path
as `Axes::draw`, not a parallel reconstruction.

## Open Questions

- Where should the per-axis scale state live: directly on `Axes`, or inside the existing
  `Axis` values with accessors from `Axes`?
- Should `Axis::draw` receive raw limits plus a scale callback, or should `Axes` precompute
  scaled tick positions and pass a richer tick model?
- Should log margins be applied by `Axes::effective_limits` directly, or by a
  scale-aware `effective_scaled_limits` helper that leaves the existing linear helper
  untouched?
- What should the first image behavior be on log axes: transform extent only, or reject
  with a documented limitation until nonlinear image resampling exists?

## First Implementation PR Boundary

A safe first implementation PR should include:

- `Axes` x/y scale state and public log-scale setters.
- scaled-limit and coordinate inversion helpers.
- line/scatter/path-like artist scaling.
- x/y `Axis` tick placement using raw values but scaled positions.
- `semilogx`, `semilogy`, and `loglog` wrappers.
- tests for line geometry, tick positions/labels, reversed ranges, non-positive log limits,
  and pixel-to-data round-trip.

Leave images, pcolormesh, spans, and broader symlog/logit polish for follow-ups unless the
first implementation naturally covers them without broadening the seam.
