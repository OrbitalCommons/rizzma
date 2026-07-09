//! # Scientific communication reflects on the scientist, and your figures should carry the same *rizzma* as your ideas.
//!
//! ## rizzma
//!
//! A single-crate Rust reimplementation of the good parts of matplotlib / pyplot,
//! with first-class WebAssembly support.
//!
//! Everything lives in one publishable crate. The former workspace crates are now
//! namespaced modules — [`figure`], [`pyplot`], [`axis`], [`artist`], [`mathtext`],
//! the backends ([`skia`], [`svg`], [`pdf`]), and [`mplot3d`] — so you can reach any
//! part of the API through a single dependency.
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
//! ## Live demos
//!
//! On docs.rs every cell below is a real canvas driven by this crate compiled
//! to WebAssembly — animated from Rust each frame, and interactive: wheel to
//! zoom at the cursor, drag to pan, double-click to reset. (Elsewhere the
//! cells fall back to static gallery images. More at
//! <https://orbitalcommons.github.io/rizzma/demo/>.)
//!
//! <div class="rizzma-live-grid">
//! <div class="rizzma-live" data-demo="beats">
//!
//! ![plot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_plot.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="interference">
//!
//! ![interference](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_fill_between.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="rose">
//!
//! ![rose](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_polar.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="spectro">
//!
//! ![spectrogram](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_imshow.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="galaxy">
//!
//! ![galaxy](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_scatter.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="surface3d">
//!
//! ![surface3d](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_surface3d.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="ripples">
//!
//! ![ripples](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_pcolormesh.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="lorenz">
//!
//! ![lorenz](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_streamplot.png)
//!
//! </div>
//! <div class="rizzma-live" data-demo="scope">
//!
//! ![oscilloscope](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_oscilloscope.png)
//!
//! </div>
//! </div>
//!
//! ## Features
//!
//! The core plotting stack plus the SVG/PDF/raster backends are always compiled.
//! The optional leaf modules are enabled by default but can be turned off:
//!
//! - `plot3d` — the [`mplot3d`] 3D plotting module.
//! - `pyplot` — the stateful [`pyplot`] facade.
//! - `wasm` — the [`wasm`] browser bridge.
//!
//! Building with `--no-default-features` yields the core, figure, and all backends
//! without 3D, pyplot, or wasm.
//!
//! ## Colormaps
//!
//! The default colormap is `bgyw` (CET-L09), a perceptually uniform linear map
//! from Peter Kovesi's CET collection: its lightness rises at a constant rate,
//! so equal steps in your data read as equal steps of contrast on screen. The
//! full set of Kovesi maps featured in the paper ships with the crate,
//! organized by lightness profile — linear, diverging, rainbow, cyclic, and
//! isoluminant; see [`core::color::cmap`] for the taxonomy and when to reach
//! for each class. The classic vendor maps (`jet`, `hot`, `hsv`, `rainbow`)
//! hide and invent features, so they live behind the
//! [`core::color::misleading`] module and a `misleading:` name prefix — usable,
//! but never by accident.
//!
//! Reference: Peter Kovesi. *Good Colour Maps: How to Design Them.*
//! [arXiv:1509.03700 \[cs.GR\] 2015](https://arxiv.org/abs/1509.03700).

pub mod artist;
pub mod axis;
pub mod core;
pub mod figure;
pub mod mathtext;
pub mod pdf;
pub mod render;
pub mod skia;
pub mod svg;
pub mod text;

#[cfg(feature = "plot3d")]
pub mod mplot3d;
#[cfg(feature = "pyplot")]
pub mod pyplot;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use figure::{Axes, Figure, GridSpec, PolarAxes, SkyAxes, SkyProjection, SubplotSpec};

/// The version of this crate, from `CARGO_PKG_VERSION`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
