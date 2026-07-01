# rizzma

[![Crates.io](https://img.shields.io/crates/v/rizzma.svg)](https://crates.io/crates/rizzma)
[![Docs.rs](https://docs.rs/rizzma/badge.svg)](https://docs.rs/rizzma)
[![Downloads](https://img.shields.io/crates/d/rizzma.svg)](https://crates.io/crates/rizzma)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-orange.svg)](rust-toolchain.toml)
[![Edition 2024](https://img.shields.io/badge/edition-2024-93450a.svg)](Cargo.toml)
[![CI](https://github.com/OrbitalCommons/rizzma/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/OrbitalCommons/rizzma/actions/workflows/ci.yml)
[![Gallery](https://github.com/OrbitalCommons/rizzma/actions/workflows/gallery.yml/badge.svg?branch=main)](https://github.com/OrbitalCommons/rizzma/actions/workflows/gallery.yml)
[![Publish](https://github.com/OrbitalCommons/rizzma/actions/workflows/publish.yml/badge.svg?branch=main)](https://github.com/OrbitalCommons/rizzma/actions/workflows/publish.yml)
[![WASM](https://img.shields.io/badge/wasm-first-654ff0.svg)](crates/rizzma/src/wasm)
[![Renderer](https://img.shields.io/badge/renderers-PNG%20%7C%20SVG%20%7C%20Canvas-007d8a.svg)](crates)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-ff8300.svg)](AGENTS.md)

A Rust reimplementation of the good parts of **matplotlib / pyplot**, with first-class
**WebAssembly** support.

The object model mirrors matplotlib (Figure → Axes → Artists) behind a thin pyplot-style
stateful facade, with a single `Renderer` trait driving multiple targets: a `tiny-skia`
raster backend (PNG), an SVG emitter, and a browser `<canvas>` backend — the same figure,
rendered anywhere.

## Gallery

One figure per Tier-1 plot type, auto-rendered from
[`crates/rizzma/examples/gallery.rs`](crates/rizzma/examples/gallery.rs) on
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
| ![ecdf](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_ecdf.png) | ![matshow](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_matshow.png) | ![spy](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_spy.png) |
| `ecdf` | `matshow` | `spy` |
| ![hist2d](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_hist2d.png) | ![pie](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_pie.png) | ![violinplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_violinplot.png) |
| `hist2d` | `pie` | `violinplot` |
| ![hexbin](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_hexbin.png) | ![grouped_bar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_grouped_bar.png) | ![loglog](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_loglog.png) |
| `hexbin` | `grouped_bar` | `loglog` |
| ![quiver](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_quiver.png) | ![streamplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_streamplot.png) | ![triplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_triplot.png) |
| `quiver` | `streamplot` | `triplot` |
| ![tripcolor](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_tripcolor.png) | ![symlog](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_symlog.png) | ![logit](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_logit.png) |
| `tripcolor` | `symlog` | `logit` |
| ![dates](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_dates.png) | ![polar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar.png) | ![polar scatter](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar_scatter.png) |
| date axis | `polar` | `polar scatter` |
| ![polar fill](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar_fill.png) | ![contourf](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_contourf.png) | |
| `polar fill` | `contourf` | |

Regenerate locally with `cargo run -p rizzma --example gallery` (writes
`target/gallery_*.png`).

## Module layout

`rizzma` is a single publishable crate. Each area lives in its own module under
`crates/rizzma/src/`; the optional leaf modules are enabled by default and gated behind
the `plot3d`, `pyplot`, and `wasm` features.

| Module | Responsibility |
|--------|----------------|
| `core` | cbook-style utils, typed `RcParams`, `Path`/`Bbox`/`Affine2D`, transform graph, color |
| `render` | The `Renderer` trait + `GraphicsContext`/`Paint` (backend-agnostic) |
| `skia` | Reference raster backend over `tiny-skia` → PNG |
| `text` | Font sourcing + text layout/metrics |
| `artist` | Artist scene tree + `Line2D`, `Patch`, markers, collections |
| `axis` | Ticker, scales, units, dates, `Axis`/`Tick`/`Spine` |
| `figure` | `GridSpec`, `Figure`, `Axes`, layout engines, legend, colorbar, plotting methods |
| `pyplot` | Stateful pyplot-style facade (feature `pyplot`) |
| `svg` | SVG vector backend |
| `pdf` | PDF vector backend |
| `wasm` | Canvas backend + DOM event bridge (feature `wasm`) |
| `mathtext` | TeX-subset math layout |
| `mplot3d` | mplot3d-equivalent 3D plotting (feature `plot3d`) |

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
