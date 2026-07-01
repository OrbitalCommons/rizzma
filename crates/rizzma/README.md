# rizzma

A Rust reimplementation of the good parts of **matplotlib / pyplot**, rendering one
figure to **PNG, SVG, and PDF** — plus a browser `<canvas>`/**WebAssembly** backend.

```rust
use rizzma::Figure;

let mut fig = Figure::new(6.0, 4.0);
let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
ax.plot(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 0.5, 1.5]);
ax.set_title("hello, rizzma");
fig.save_png("plot.png").unwrap();
```

## What's inside

`rizzma` is a single crate; the subsystems are modules, with the most-used types
(`Figure`, `Axes`, `PolarAxes`, `GridSpec`, `SubplotSpec`) re-exported at the crate root:

| Module | What it does |
|--------|--------------|
| `rizzma::figure` | `Figure` / `Axes` / `PolarAxes` — 40+ plot types (line, scatter, bar, hist, box/violin, hexbin, contour/contourf, pcolormesh, quiver, streamplot, pie, tri*, …) |
| `rizzma::axis` | ticks, scales (linear / log / symlog / logit), date axes |
| `rizzma::mathtext` | TeX-subset math layout for `$...$` labels and titles |
| `rizzma::artist` | `Line2D`, `Patch`, markers, quad meshes |
| `rizzma::core` | geometry, color, colormaps, transforms |
| `rizzma::render` + `rizzma::skia` / `rizzma::svg` / `rizzma::pdf` | the `Renderer` seam and the PNG / SVG / PDF backends |
| `rizzma::mplot3d` | 3D axes — `plot3d` / `scatter3d` / `plot_surface` / `bar3d` / `plot_wireframe` *(feature `plot3d`)* |
| `rizzma::pyplot` | stateful `plt.*`-style facade *(feature `pyplot`)* |
| `rizzma::wasm` | browser canvas backend *(feature `wasm`)* |

## Features

The `plot3d`, `pyplot`, and `wasm` modules are **on by default**. Opt out for a leaner
build (core 2D plotting + PNG/SVG/PDF):

```toml
rizzma = { version = "1", default-features = false }
```

## Gallery & docs

A rendered example of every plot type, plus the design docs, live in the
[project repository](https://github.com/OrbitalCommons/rizzma).

## License

Licensed under either of MIT or Apache-2.0 at your option.
