//! Fonts and text layout for rizzma.
//!
//! A pluggable font source backed by **embedded** fonts (so it works under
//! `wasm32` with no system font discovery) plus single-line text metrics. This
//! crate provides the analog of matplotlib's `get_text_width_height_descent`.
//!
//! Metrics are computed directly from the font tables via `ttf-parser`, which is
//! small, `no_std`-friendly, and wasm-clean. Glyph-outline extraction (text as
//! vector [`crate::core::Path`]s) lives in [`textpath`]; shaping and full layout
//! are deferred to a later phase.
//!
//! Math-aware text is represented as parsed [`TextRun`] spans. The span model is
//! render-independent: it classifies plain text, portable mathtext, and raw TeX,
//! preserving exact source text so backends can warn or pass through unsupported
//! spans without losing author intent.
//!
//! Build-order home: Phase 4 of `design/04-implementation-plan.md`.

pub mod font;
pub mod metrics;
pub mod span;
pub mod textpath;

pub use font::FontSource;
pub use metrics::TextExtent;
pub use span::{
    MathMode, TextFallbackAction, TextFallbackReason, TextFallbackWarning, TextRenderCapabilities,
    TextRun, TextSpan, TextSpanKind,
};
