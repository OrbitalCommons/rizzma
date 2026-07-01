//! Compatibility re-export for math-aware single-line text layout.
//!
//! The implementation lives in [`crate::mathtext::richtext`] so lower-level
//! the axis module can render formatter output through the same
//! mathtext path as figure titles.

pub use crate::mathtext::{RichText, layout_rich_text};
