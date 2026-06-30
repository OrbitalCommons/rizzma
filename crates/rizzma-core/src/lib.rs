//! Core primitives for rizzma.
//!
//! cbook-style utilities, typed `RcParams`, geometry (`Path`, `Bbox`, `Affine2D`),
//! the transform graph, and color/colormaps/normalization.
//!
//! Build-order home: Phases 0–2 of `design/04-implementation-plan.md`.

pub mod affine;
pub mod bbox;
pub mod color;
pub mod path;
pub mod rcparams;

pub use affine::Affine2D;
pub use bbox::Bbox;
pub use color::{
    BoundaryNorm, Colormap, LinearNorm, LinearSegmentedColormap, ListedColormap, LogNorm,
    Normalize, PowerNorm, Rgba, colormap, to_rgba, to_rgba_array,
};
pub use path::{Path, PathCode, PathSegment};
pub use rcparams::{RcParams, TickDirection};
