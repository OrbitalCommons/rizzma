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
//! Build-order home: Phase 7 of `design/04-implementation-plan.md`.

mod axes;
mod colorbar;
mod figure;
mod gridspec;
mod legend;
mod plotting;
mod plotting_stats;
mod subplotspec;

pub use axes::Axes;
pub use figure::Figure;
pub use gridspec::GridSpec;
pub use subplotspec::SubplotSpec;
