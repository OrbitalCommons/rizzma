# rizzma

A Rust reimplementation of the good parts of **matplotlib / pyplot**, with first-class
**WebAssembly** support.

The object model mirrors matplotlib (Figure → Axes → Artists) behind a thin pyplot-style
stateful facade, with a single `Renderer` trait driving multiple targets: a `tiny-skia`
raster backend (PNG), an SVG emitter, and a browser `<canvas>` backend — the same figure,
rendered anywhere.

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
