//! Wasm/browser target for rizzma.
//!
//! A canvas `Renderer` (blit a tiny-skia `Pixmap` via `putImageData`, or Canvas2D-native
//! ops) plus the DOM event bridge (`mousemove`/`mousedown`/`keydown`/`resize` with the
//! y-flip and button remap). "Canvas is just another backend."
//!
//! Build-order home: Phase 9 of `design/04-implementation-plan.md`.
