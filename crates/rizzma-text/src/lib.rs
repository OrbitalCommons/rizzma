//! Fonts and text layout for rizzma.
//!
//! A pluggable font source (embedded fonts for wasm, `fontdb` discovery on native) plus
//! cosmic-text-backed metrics and layout. Provides `get_text_width_height_descent` and
//! the glyph geometry the renderer draws.
//!
//! Build-order home: Phase 4 of `design/04-implementation-plan.md`.
