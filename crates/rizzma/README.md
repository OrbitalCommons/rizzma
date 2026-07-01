# rizzma

A Rust reimplementation of the good parts of **matplotlib / pyplot**, with first-class
**WebAssembly** support.

> **Status: early construction.** The API is usable but still evolving while the plot
> catalogue and backends fill out.

The crate is organized into focused modules that mirror the old workspace boundaries:
core geometry/color, the renderer seam, text and mathtext layout, artists, axis/ticker
machinery, figures/layout, SVG/PDF/Skia backends, 3D plotting, pyplot-style helpers, and
the wasm demo bindings.

Default features enable the main plotting surface:

- `plot3d`: 3D plotting helpers and artists.
- `pyplot`: a stateful pyplot-style facade.
- `wasm`: browser bindings for the wasm demo and canvas rendering path.

Design and roadmap live in the [project repository](https://github.com/OrbitalCommons/rizzma)
under `design/` — architecture, a catalogue of plot types, the foundational-components
breakdown, and a full implementation plan with a PR DAG.

## License

Licensed under either of MIT or Apache-2.0 at your option.
