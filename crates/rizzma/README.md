# rizzma

A Rust reimplementation of the good parts of **matplotlib / pyplot**, with first-class
**WebAssembly** support.

> **Status: early construction.** This release reserves the crate name on crates.io.
> The eventual public API will re-export the `rizzma-*` workspace crates: core
> geometry/color, the renderer seam (`tiny-skia` raster, SVG, and a wasm canvas backend),
> the artist scene graph, axis machinery, figures/layout, and a pyplot-style facade.

Design and roadmap live in the [project repository](https://github.com/OrbitalCommons/rizzma)
under `design/` — architecture, a catalogue of plot types, the foundational-components
breakdown, and a full implementation plan with a PR DAG.

## License

Licensed under either of MIT or Apache-2.0 at your option.
