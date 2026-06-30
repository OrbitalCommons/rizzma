//! Mathtext for rizzma.
//!
//! A scoped TeX-subset box-and-glue layout engine for `$...$` expressions, producing
//! glyph geometry the renderer draws. This is the portable deterministic render path:
//! frontend renderers can preserve raw TeX spans for host-side typesetting, while
//! native exports may later opt into an external TeX backend when explicitly enabled
//! and the required binaries are present.
//!
//! Build-order home: Phase 10 of `design/04-implementation-plan.md`.
