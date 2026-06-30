//! Figure and layout scaffolding for rizzma.
//!
//! This crate currently provides [`GridSpec`] subplot geometry: the
//! figure-fraction arithmetic that positions a regular grid of cells and
//! resolves individual cells or multi-cell spans ([`SubplotSpec`]) to
//! rectangles. `Figure`/`SubFigure`, the `Axes` base, layout engines,
//! `Legend`, and `Colorbar` are follow-ups.
//!
//! Build-order home: Phase 7 of `design/04-implementation-plan.md`.

mod gridspec;
mod subplotspec;

pub use gridspec::GridSpec;
pub use subplotspec::SubplotSpec;
