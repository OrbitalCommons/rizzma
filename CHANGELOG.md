# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/).

`rizzma` is a single crate. Bumping the version on a push to `main` triggers the
publish workflow (`.github/workflows/publish.yml`), which publishes it to crates.io.

## [1.2.1] - 2026-07-09

### Added
- **Live wasm demos in the crate docs**: docs.rs pages now mount real
  interactive canvases (wheel zoom, drag pan, double-click reset) where the
  docs previously showed static gallery images, via an injected
  `--html-in-header` loader that lazily imports the wasm bundle from the
  gh-pages `demo/` deployment. The static images remain as fallbacks
  everywhere the bundle cannot load. First two: an interactive line plot on
  the `wasm` module docs, and streaming x-linked strips on
  `Axes::oscilloscope`.
- The interactive demo site now deploys permanently to
  <https://orbitalcommons.github.io/rizzma/demo/> from the Gallery
  workflow, and the gallery gains an `oscilloscope` case.
- `cargo xtask serve-www` sends `Access-Control-Allow-Origin: *`, matching
  GitHub Pages, so locally built rustdoc can exercise the live-docs path.

## [1.2.0] - 2026-07-08

### Added
- **Ten capability upgrades** from the matplotlib-parity triage: the classic
  colormaps (`magma`, `inferno`, `plasma`, `cividis`, `RdBu`, `coolwarm`,
  all with `_r`), `Patch::circle`/`rectangle`/`arc` constructors,
  `Axes::text` and `Axes::annotate` with leader arrows, `tricontour` /
  `tricontourf` over triangulations, gouraud-shaded `pcolormesh`,
  `colorbar_at` placement with horizontal orientation, `SkyAxes` with
  aitoff and mollweide projections, `twinx` twins plus a secondary linear
  x axis, 3D quiver arrows and billboard text labels, and live line-data
  updates for wasm sessions (`set_line_data`).
- **Kovesi CET colormaps** (arXiv:1509.03700): 15 perceptually uniform maps
  embedded as compact 8-bit tables — linear (`cet_l01`, `cet_l03` "fire",
  `cet_l05`, `cet_l09`, `cet_l10`), diverging (`cet_d01`, `cet_d04`,
  `cet_d07`, `cet_d11`), the equalized rainbow `cet_r2`, cyclic
  `cet_c1`/`cet_c2`/`cet_c3`/`cet_c5`, and isoluminant `cet_i1`, each with
  `_r` variants. **The default colormap is now `bgyw` (CET-L09)** for every
  artist that previously hardcoded viridis. The classic vendor maps (`jet`,
  `hot`, `hsv`, `rainbow`) ship quarantined in `core::color::misleading` and
  only resolve with an explicit `misleading:` registry prefix; module docs
  explain the perceptual failure modes with the paper citation.
- **`sharex` linked axes** (`Figure::sharex`, `WasmFigure.sharex`): a
  follower mirrors its leader's x-limits at draw time, interactive pan/zoom
  on either axes keeps the group's x in lockstep (y stays per-axes), and
  double-click home restores the whole group. Stacked pairs hide the upper
  axes' x tick labels (matplotlib's `label_outer`).
- **Oscilloscope axes style** (`Axes::oscilloscope`): a chart built for
  arbitrary — including sparkline-strip — sizes. Everything draws inside the
  frame: near-black CRT face, fixed 10×4-division phosphor graticule,
  phosphor trace cycle (green/amber/cyan/magenta), dim bezel, and in-frame
  corner readouts (y-max, y-min, x-span) that track the live limits. Scope
  axes autoscale flush in x.
- **Interactive wasm demo site** (`crates/rizzma/www`): six live canvases —
  styled lines with `set_line_data` animation, scatter, log-log, `sharex`
  subplots, a Rust-side cursor trail, and streaming oscilloscope strips —
  served by the new dependency-free `cargo xtask serve-www`.
- **`WasmSession::track_cursor`**: record hovered data positions into a line
  artist as a rolling trail, entirely on the Rust side of the boundary.
- Titles for the self-contained axes: `SkyAxes`, `PolarAxes`, and `Axes3D`
  gain `set_title`, drawn centered and DPI-scaled like 2D axes titles.
- `Figure::set_facecolor` (and the `WasmFigure` export) for in-place canvas
  background changes; `Axes::set_x_tick_labels_visible` /
  `Axis::set_tick_labels_visible` for label-outer control.
- Sticky autoscale edges: bars, histograms, stems, stairs, stackplots, and
  grouped bars sit exactly on their zero baseline; images and meshes stay
  flush with their live extents; line charts are flush in x.
- Gallery: images render at ≥1600 px for HiDPI docs, every demo's title
  leads with the feature it demonstrates, and the colormap showcase is
  grouped into CET / classics / quarantined sections.

### Changed
- **Tight layout**: `add_subplot` frames are derived from measured
  decoration extents (matplotlib's tight layout) instead of fixed margins —
  including end-tick-label overhang, column-aligned frame widths for
  stacked subplots, and matplotlib-equivalent padding (full pad at figure
  edges, one shared pad between neighbors). Explicit `add_axes` rects stay
  literal.
- Text, tick, marker, and arrow geometry scales with renderer DPI, so
  high-DPI renders are true scale-ups rather than tiny text on big canvases.

### Fixed
- **Artists now clip to the axes frame** on every backend (raster mask with
  caching, SVG `clipPath`, PDF clip operator): zooming or setting limits
  tighter than the data no longer spills lines, patches, meshes, or images
  across the figure.
- Dropping a `WasmSession` (explicitly or via GC) detaches its DOM
  listeners, so events on the canvas no longer throw
  "closure invoked recursively or after being dropped".
- Degenerate axis ranges are judged relative to magnitude, log/logit limits
  survive extreme interaction, and the wasm browser CI pins wasm-pack
  0.15.0.

## [1.1.2] - 2026-07-07

### Changed
- The README and docs.rs crate page now open with the project tagline:
  "Scientific communication reflects on the scientist, and your figures should
  carry the same *rizzma* as your ideas."

## [1.1.1] - 2026-07-07

### Fixed
- Interaction hardening (from codex's post-release review): pan/zoom limit
  candidates are now rejected when non-finite and clamped to the axis scale's
  domain before being stored, so an extreme wheel or drag on a log/logit axis
  can no longer poison the view; `pointercancel`/`lostpointercapture` cancel an
  in-progress drag (touch/pen interruptions); a failed `requestAnimationFrame`
  no longer wedges the redraw flag; and the device pixel ratio is re-read every
  frame, so browser zoom or moving between monitors re-renders at the right
  density.
- Degenerate axis ranges are now judged relative to the values' magnitude
  (matplotlib's `nonsingular` semantics): a deep-zoomed log axis at tiny
  magnitudes — e.g. `(1e-30, 1e-22)`, eight healthy decades — is no longer
  misclassified as zero-width and blown up to `(-0.5, 0.5)`.

## [1.1.0] - 2026-07-05

### Added
- **Interactive wasm plots** (design doc 06, W1–W6). `WasmFigure` is now a real
  JS plotting surface — `new(w, h)`, `add_axes`/`add_subplot`, `plot`,
  `plot_styled({color, lw, ls})`, `scatter`, titles/labels/limits, log scales,
  and `legend` — and `WasmFigure.bind(canvas_id)` returns a `WasmSession` with
  wheel zoom anchored at the cursor, left-drag pan (log-scale-aware), double-click
  home reset, and an `on_hover` callback, repainting through a
  `requestAnimationFrame`-coalesced loop.
- HiDPI rendering: `Figure::render_scaled` / `wasm::figure_to_rgba_scaled`
  render at `devicePixelRatio` × DPI (bit-identical to a double-DPI render) and
  the canvas is presented at logical CSS size, so browser plots are crisp on
  retina displays.
- Host-agnostic interaction core: `figure::{Event, MouseButton}` (top-down
  logical pixels, no y-flip anywhere), `Figure::axes_at` hit-testing, and
  `figure::Interactor`/`Outcome` — pure Rust, fully covered by native tests.
- CI: `wasm browser tests` job runs the new `wasm-pack test --headless
  --chrome` suite (canvas pixel readback + synthetic pointer/wheel event
  end-to-end), and a native per-frame render budget test guards interactive
  latency (~14 ms release at 1600×1200 vs the 16 ms design target).

## [1.0.1] - 2026-07-01

### Changed
- Titles, axis labels, and tick labels now render with matplotlib-grade
  typography. Text layout and measurement apply the font's `kern` table (the
  same pairwise kerning FreeType uses), so pairs like `Ta`/`AV`/`Wo` tuck in
  correctly. The y-axis label is now rotated 90° and centered along the axis
  (previously drawn horizontally and clipped off the canvas), tick and axis
  labels are placed against measured text extents, and `axes.titlepad`/
  `axes.labelpad`/`xtick.major.pad`/`ytick.major.pad` are configurable via
  `RcParams`.

## [1.0.0] - 2026-07-01

**rizzma is now a single crate.** The former 14-crate workspace is collapsed into
one publishable `rizzma` crate: each `rizzma-*` sub-crate becomes a module
(`rizzma::core`, `rizzma::figure`, `rizzma::axis`, `rizzma::mathtext`,
`rizzma::skia`/`svg`/`pdf`, …), and the optional pieces are default-on Cargo
features you can opt out of. `cargo add rizzma` gives you the full 2D + 3D library.

### Changed
- Collapsed the workspace into a single `rizzma` crate; the `rizzma-*` sub-crates
  are now modules of `rizzma` (most-used types re-exported at the crate root).
- Optional, default-on features: `plot3d` (3D axes), `pyplot` (the facade), and
  `wasm` (browser backend). `default-features = false` builds core 2D + PNG/SVG/PDF.

### Added
- `contourf` filled contours; polar `scatter`/`fill`; 3D `bar3d` and
  `plot_wireframe`; mathtext `\operatorname`/named operators and math styles
  (`\mathbb`/`\mathcal`/`\mathfrak`, with font-coverage fallback).

## [0.1.0] - 2026-06-30

The umbrella `rizzma` crate now **re-exports the full workspace API** behind one
import surface (`use rizzma::Figure;`, plus namespaced modules `rizzma::figure`,
`rizzma::pyplot`, `rizzma::axis`, `rizzma::mathtext`, `rizzma::mplot3d`, and the
`skia`/`svg`/`pdf` backends). The whole workspace moves to a unified `0.1.0`.

### Added
- Single-crate public API via the `rizzma` umbrella (re-exports every `rizzma-*`
  crate; common types `Figure`/`Axes`/`PolarAxes`/`GridSpec` flattened to the root).
- Nonlinear axis scales integrated into `Axes`: log/`loglog`/`semilog*`, symlog,
  logit, and date axes — all with mathtext-superscript tick labels.
- `rizzma-3d`: `Axes3D` with `plot3d`/`scatter3d`/`plot_surface` and depth-sorted
  rendering; `PolarAxes` polar plots; `rizzma-pdf` PDF backend and `Figure::save_pdf`.
- pyplot `savefig` selects PNG/SVG/PDF from the file extension.
- Extensive mathtext: fractions, radicals/nth-root, over/underline, binomials,
  matrices, accents, spacing, and style commands.

### Note
- Publishing `0.1.0` to crates.io additionally requires a `CARGO_REGISTRY_TOKEN`
  secret and publishing the member crates in dependency order; the workflow skips
  (with a warning) until the token is configured.

## [0.0.2] - 2026-06-30

This entry tracks the workspace's progress. The published `rizzma` crate still
reserves the name — its public re-export of the functional crates is pending —
so the headline features below ship in the in-tree `rizzma-*` crates.

### Added
- 32 plot types on `Axes`, each with a rendered gallery example: line, scatter,
  bar/barh, hist, fill_between/fill_betweenx, errorbar, step, stem, stairs,
  stackplot, broken_barh, imshow, pcolormesh, contour, boxplot, violinplot,
  hexbin, grouped_bar, pie, eventplot, ecdf, matshow, spy, hist2d, quiver,
  streamplot, reference lines/spans, legend, and colorbar.
- Logarithmic axes: `set_xscale_log`/`set_yscale_log`, `semilogx`/`semilogy`/
  `loglog`, log tick locators and formatters, and mathtext-rendered tick and
  axis labels (real superscripts, e.g. `10^6`).
- Portable mathtext layout engine (`rizzma-mathtext`): fractions, radicals,
  accents, large operators, delimiters, and TeX spacing commands.
- `rizzma-pyplot` stateful facade mirroring `import matplotlib.pyplot as plt`,
  covering the full set of plot methods plus `legend`/`colorbar`.
- Backends: `tiny-skia` raster (PNG), a hand-emitted SVG vector backend, and a
  browser `<canvas>`/wasm backend, all behind one `Renderer` trait.
- Date tick locators/formatters (`AutoDateLocator`, `AutoDateMinorLocator`,
  `DateFormatter`, `ConciseDateFormatter`).
- CI: gallery rendered to the `gh-pages` branch, a wasm size budget check, a
  strict gallery-link checker, and a publish-on-version-change workflow.

## [0.0.1] - 2026

### Added
- Initial release reserving the `rizzma` crate name on crates.io.
