//! Compatibility re-export for math-aware single-line text layout.
//!
//! The implementation lives in [`rizzma_mathtext::richtext`] so lower-level
//! crates such as `rizzma-axis` can render formatter output through the same
//! mathtext path as figure titles.

pub use rizzma_mathtext::{RichText, layout_rich_text};
