//! SVG vector backend for rizzma.
//!
//! Implements `Renderer` by streaming primitives to SVG markup — content-addressed
//! `defs`/`use` for markers/glyphs, deferred clip and hatch defs — mirroring matplotlib's
//! SVG backend.
//!
//! Build-order home: Phase 9 of `design/04-implementation-plan.md`.
