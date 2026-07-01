//! # rizzma
//!
//! A Rust reimplementation of the good parts of matplotlib / pyplot, with first-class
//! WebAssembly support.
//!
//! This umbrella crate re-exports the `rizzma-*` workspace crates behind one import
//! surface, so downstream users depend on a single `rizzma` crate:
//!
//! ```
//! use rizzma::Figure;
//!
//! let mut fig = Figure::new(4.0, 3.0);
//! let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
//! ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
//! let png = fig.encode_png().expect("encode");
//! assert!(!png.is_empty());
//! ```
//!
//! The workspace crates are also reachable as namespaced modules — [`figure`],
//! [`pyplot`], [`axis`], [`artist`], [`mathtext`], the backends ([`skia`], [`svg`],
//! [`pdf`]), and [`mplot3d`] — so you can reach any part of the API without adding
//! the individual crates as dependencies.

// Namespaced re-exports of each workspace crate.
/// The mplot3d-equivalent 3D plotting crate ([`rizzma_3d`]).
pub use rizzma_3d as mplot3d;
pub use rizzma_artist as artist;
pub use rizzma_axis as axis;
pub use rizzma_core as core;
pub use rizzma_figure as figure;
pub use rizzma_mathtext as mathtext;
pub use rizzma_pdf as pdf;
pub use rizzma_pyplot as pyplot;
pub use rizzma_render as render;
pub use rizzma_skia as skia;
pub use rizzma_svg as svg;
pub use rizzma_text as text;
pub use rizzma_wasm as wasm;

// Flatten the most commonly used entry points to the crate root so `use rizzma::Figure`
// (and friends) works without reaching through the `figure` module.
pub use rizzma_figure::{Axes, Figure, GridSpec, PolarAxes, SubplotSpec};

/// The version of this crate, from `CARGO_PKG_VERSION`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
