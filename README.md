# rizzma

A Rust reimplementation of the good parts of **matplotlib / pyplot**, with first-class
**WebAssembly** support.

The object model mirrors matplotlib (Figure → Axes → Artists) behind a thin pyplot-style
stateful facade, with a single `Renderer` trait driving multiple targets: a `tiny-skia`
raster backend (PNG), an SVG emitter, and a browser `<canvas>` backend — the same figure,
rendered anywhere.

## Gallery

One figure per Tier-1 plot type, auto-rendered from
[`crates/rizzma-figure/examples/gallery.rs`](crates/rizzma-figure/examples/gallery.rs) on
every push to `main` and published to the `gh-pages` branch (so these images never live in
`main`'s history). Browse them all at the
[gallery page](https://orbitalcommons.github.io/rizzma/).

<!-- Images are served from the orphan gh-pages branch; they appear after the Gallery
     workflow runs on main. -->

| | | |
|:-:|:-:|:-:|
| ![plot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_plot.png) | ![scatter](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_scatter.png) | ![bar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_bar.png) |
| `plot` | `scatter` | `bar` |
| ![barh](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_barh.png) | ![hist](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_hist.png) | ![fill_between](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_fill_between.png) |
| `barh` | `hist` | `fill_between` |
| ![step](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_step.png) | ![errorbar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_errorbar.png) | ![reference lines](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_reflines.png) |
| `step` | `errorbar` | reference lines / spans |
| ![imshow](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_imshow.png) | ![legend + colorbar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_legend_colorbar.png) | ![stem](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stem.png) |
| `imshow` | `legend` + `colorbar` | `stem` |
| ![stairs](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stairs.png) | ![stackplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stackplot.png) | ![broken_barh](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_broken_barh.png) |
| `stairs` | `stackplot` | `broken_barh` |
| ![pcolormesh](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_pcolormesh.png) | ![boxplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_boxplot.png) | ![mathtext](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_mathtext.png) |
| `pcolormesh` | `boxplot` | mathtext title |
| ![contour](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_contour.png) | ![eventplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_eventplot.png) | ![fill_betweenx](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_fill_betweenx.png) |
| `contour` | `eventplot` | `fill_betweenx` |
| ![ecdf](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_ecdf.png) | | |
| `ecdf` | | |

Regenerate locally with `cargo run -p rizzma-figure --example gallery` (writes
`target/gallery_*.png`).

## Workspace layout

| Crate | Responsibility |
|-------|----------------|
| `rizzma` | Umbrella / public facade (re-exports, name reservation) |
| `rizzma-core` | cbook-style utils, typed `RcParams`, `Path`/`Bbox`/`Affine2D`, transform graph, color |
| `rizzma-render` | The `Renderer` trait + `GraphicsContext`/`Paint` (backend-agnostic) |
| `rizzma-skia` | Reference raster backend over `tiny-skia` → PNG |
| `rizzma-text` | Font sourcing + text layout/metrics (`cosmic-text`) |
| `rizzma-artist` | Artist scene tree + `Line2D`, `Patch`, markers, collections |
| `rizzma-axis` | Ticker, scales, units, dates, `Axis`/`Tick`/`Spine` |
| `rizzma-figure` | `GridSpec`, `Figure`, `Axes`, layout engines, legend, colorbar |
| `rizzma-plot` | Axes plotting methods (`plot`/`scatter`/`bar`/`hist`/…) |
| `rizzma-pyplot` | Stateful pyplot-style facade |
| `rizzma-svg` | SVG vector backend |
| `rizzma-wasm` | Canvas backend + DOM event bridge |
| `rizzma-mathtext` | TeX-subset math layout (later) |
| `rizzma-3d` | mplot3d-equivalent (later) |

## Design docs

See `design/`:

- [`01-architecture.md`](design/01-architecture.md) — overall architecture & Rust/wasm mapping
- [`02-plot-types.md`](design/02-plot-types.md) — catalogue of plot types, tiered by priority
- [`03-foundational-components.md`](design/03-foundational-components.md) — the engine room + crate mapping
- [`04-implementation-plan.md`](design/04-implementation-plan.md) — milestones and the PR DAG

## Development

The toolchain is pinned in `rust-toolchain.toml` (current stable). Common commands:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three on every PR. `main` accepts **squash merges only**.

## License

Licensed under either of MIT or Apache-2.0 at your option.
