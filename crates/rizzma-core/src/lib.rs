//! Core primitives for rizzma.
//!
//! cbook-style utilities, typed `RcParams`, geometry (`Path`, `Bbox`, `Affine2D`),
//! the transform graph, and color/colormaps/normalization.
//!
//! Build-order home: Phases 0–2 of `design/04-implementation-plan.md`.

pub mod affine;
pub mod bbox;
pub mod path;

pub use affine::Affine2D;
pub use bbox::Bbox;
pub use path::{Path, PathCode, PathSegment};
