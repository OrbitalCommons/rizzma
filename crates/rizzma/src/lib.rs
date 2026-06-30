//! # rizzma
//!
//! A Rust reimplementation of the good parts of matplotlib / pyplot, with first-class
//! WebAssembly support.
//!
//! **Status: early construction.** This release reserves the crate name. The eventual
//! public API will re-export the `rizzma-*` workspace crates (core geometry/color,
//! the renderer seam, artists, axes, figures, and a pyplot-style facade). Follow
//! development at <https://github.com/OrbitalCommons/rizzma>.

/// The version of this crate, from `CARGO_PKG_VERSION`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
