//! Figure and layout scaffolding for rizzma.
//!
//! This crate provides [`GridSpec`] subplot geometry plus the integration core,
//! [`Figure`] and [`Axes`], that ties the artist, axis, text, and raster crates
//! together into "a line on labeled axes → PNG". [`GridSpec`] supplies the
//! figure-fraction arithmetic that positions a regular grid of cells and
//! resolves individual cells or multi-cell spans ([`SubplotSpec`]) to
//! rectangles. [`Axes::legend`] and [`Figure::colorbar`] add legend keys and
//! gradient colorbars, and [`Figure::to_svg`] proves the figure is
//! backend-agnostic. `SubFigure` and full layout engines are follow-ups.
//!
//! # Gallery
//!
//! One figure per Tier-1 plot type, rendered by
//! `cargo run -p rizzma-figure --example gallery` and published to the project's
//! `gh-pages` branch (the images are external, so this doc carries no binaries):
//!
//! | | | |
//! |:-:|:-:|:-:|
//! | ![plot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_plot.png) | ![scatter](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_scatter.png) | ![bar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_bar.png) |
//! | [`Axes::plot`] | [`Axes::scatter`] | [`Axes::bar`] |
//! | ![hist](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_hist.png) | ![fill_between](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_fill_between.png) | ![errorbar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_errorbar.png) |
//! | [`Axes::hist`] | [`Axes::fill_between`] | [`Axes::errorbar`] |
//! | ![imshow](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_imshow.png) | ![legend + colorbar](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_legend_colorbar.png) | ![reference lines](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_reflines.png) |
//! | [`Axes::imshow`] | [`Figure::colorbar`] | [`Axes::axhline`] etc. |
//! | ![stem](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stem.png) | ![stairs](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stairs.png) | ![stackplot](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_stackplot.png) |
//! | [`Axes::stem`] | [`Axes::stairs`] | [`Axes::stackplot`] |
//! | ![broken_barh](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_broken_barh.png) | ![pcolormesh](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_pcolormesh.png) | |
//! | [`Axes::broken_barh`] | [`Axes::pcolormesh`] | |
//!
//! Build-order home: Phase 7 of `design/04-implementation-plan.md`.

mod axes;
mod colorbar;
mod figure;
mod gridspec;
mod legend;
mod plotting;
mod plotting_area;
mod plotting_image;
mod plotting_mesh;
mod plotting_stats;
mod plotting_steps;
mod subplotspec;

pub use axes::Axes;
pub use figure::Figure;
pub use gridspec::GridSpec;
pub use subplotspec::SubplotSpec;
