//! Artist scene graph and drawable primitives for rizzma.
//!
//! Defines the [`Artist`] trait every drawable implements plus the concrete
//! primitives [`Line2D`] and [`Patch`]. An [`Artist`] knows how to
//! [`draw`](Artist::draw) itself into a [`Renderer`] given a data-to-device
//! [`Affine2D`], and reports a [`zorder`](Artist::zorder),
//! [`visibility`](Artist::visible), and optional data-space
//! [`extents`](Artist::data_extents) for future autoscaling.
//!
//! `hatch` is deferred to follow-up work; this crate currently ships the trait
//! plus `Line2D`, the `Patch` shape hierarchy, point [`MarkerStyle`]s, the
//! batched scatter [`Collection`], the colormapped raster [`AxesImage`], and a
//! small [`draw_artists`] scene helper.
//!
//! Build-order home: Phase 5 of `design/04-implementation-plan.md`.

mod collection;
mod image;
mod line;
mod marker;
mod patch;

pub use collection::Collection;
pub use image::AxesImage;
pub use line::Line2D;
pub use marker::MarkerStyle;
pub use patch::Patch;

pub use rizzma_core::{Affine2D, Bbox, Path, color::Rgba};
pub use rizzma_render::{CapStyle, GraphicsContext, JoinStyle, Renderer};

/// A drawable scene element.
///
/// Mirrors matplotlib's `Artist`: every visual is an [`Artist`] that draws
/// itself through a [`Renderer`] using a data-to-device [`Affine2D`]. The trait
/// is deliberately minimal — only [`draw`](Artist::draw) is required, and the
/// rest carry matplotlib-flavored defaults.
pub trait Artist {
    /// Draw this artist into `renderer`, mapping its data-space geometry to
    /// device space with `transform`.
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D);

    /// The stacking order; higher values draw on top.
    ///
    /// Defaults to `2.0`, matplotlib's `Line2D` default zorder.
    fn zorder(&self) -> f64 {
        2.0
    }

    /// Whether this artist should be drawn at all.
    fn visible(&self) -> bool {
        true
    }

    /// The data-space bounding box of this artist, for future autoscaling.
    ///
    /// Returns `None` when the artist has no finite extent.
    fn data_extents(&self) -> Option<Bbox> {
        None
    }
}

/// Draw `artists` into `renderer` in ascending [`zorder`](Artist::zorder),
/// skipping any that are not [`visible`](Artist::visible).
///
/// The sort is stable, so artists sharing a zorder draw in their original
/// order, matching matplotlib's behavior.
///
// TODO: arena/Figure ownership — this flat slice is a stepping stone to the
// Figure-owned arena tree that will own and order artists in a later PR.
pub fn draw_artists(artists: &[&dyn Artist], renderer: &mut dyn Renderer, transform: &Affine2D) {
    let mut order: Vec<usize> = (0..artists.len())
        .filter(|&i| artists[i].visible())
        .collect();
    order.sort_by(|&a, &b| {
        artists[a]
            .zorder()
            .partial_cmp(&artists[b].zorder())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in order {
        artists[i].draw(renderer, transform);
    }
}
