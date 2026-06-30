//! The [`Collection`] artist: a batched scatter of markers at data offsets.
//!
//! Mirrors matplotlib's `PathCollection` for the scatter case. A single unit
//! marker [`Path`] (origin-centered, e.g. from [`MarkerStyle`](crate::MarkerStyle))
//! is stamped once per offset, each placement scaled by a per-point size and
//! painted with a per-point face and edge color. The `facecolors`, `edgecolors`,
//! and `sizes` vectors broadcast: a length-1 vector applies to every point, and
//! a length-N vector applies element-wise (indexed modulo its length), matching
//! matplotlib's broadcasting.

use rizzma_core::{Affine2D, Bbox, Path, color::Rgba};
use rizzma_render::{GraphicsContext, Renderer};

use crate::{Artist, MarkerStyle};

/// matplotlib's default scatter face color (`C0`, a muted blue), used by
/// [`Collection::scatter`] when no explicit face colors are given.
const DEFAULT_FACECOLOR: Rgba = Rgba::new(0.121_568_63, 0.466_666_67, 0.705_882_35, 1.0);

/// The default marker size (in points) used by [`Collection::scatter`].
const DEFAULT_SIZE: f64 = 6.0;

/// A batched scatter of markers at data-space offsets.
///
/// Each entry of `offsets` (in data coordinates) places one copy of the unit
/// `marker` path, scaled by the matching entry of `sizes` and filled/stroked
/// with the matching `facecolors`/`edgecolors`. The color and size vectors
/// broadcast (see the module docs).
///
/// Drawing currently fans out to one [`Renderer::draw_path`] per point; a future
/// optimization is a single batched `draw_path_collection`.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// Marker positions in data space.
    offsets: Vec<[f64; 2]>,
    /// The unit marker path, origin-centered, spanning roughly `1.0` unit.
    marker: Path,
    /// Per-point marker scales (in points); broadcast over `offsets`.
    sizes: Vec<f64>,
    /// Per-point fill colors; broadcast over `offsets`.
    facecolors: Vec<Rgba>,
    /// Per-point edge (stroke) colors; broadcast over `offsets`. Empty means no
    /// edge is drawn.
    edgecolors: Vec<Rgba>,
    /// Edge width in points.
    linewidth: f64,
    /// Whether the collection is drawn.
    visible: bool,
    /// Stacking order; higher draws on top.
    zorder: f64,
}

impl Collection {
    /// Construct a scatter [`Collection`] from data-space `offsets`.
    ///
    /// Defaults mirror matplotlib's `scatter`: a filled circle (`'o'`) marker,
    /// size `6.0`, a muted-blue face, no edge, `1.0`-point edge width, visible,
    /// and zorder `1.0`.
    #[must_use]
    pub fn scatter(offsets: Vec<[f64; 2]>) -> Self {
        let marker = MarkerStyle::from_char('o')
            .expect("'o' is a known marker")
            .path()
            .clone();
        Self {
            offsets,
            marker,
            sizes: vec![DEFAULT_SIZE],
            facecolors: vec![DEFAULT_FACECOLOR],
            edgecolors: Vec::new(),
            linewidth: 1.0,
            visible: true,
            zorder: 1.0,
        }
    }

    /// Set the unit marker path, returning `self` for chaining.
    #[must_use]
    pub fn with_marker(mut self, marker: Path) -> Self {
        self.marker = marker;
        self
    }

    /// Set the per-point marker sizes (in points), returning `self` for
    /// chaining. The sizes broadcast over the offsets.
    #[must_use]
    pub fn with_sizes(mut self, sizes: Vec<f64>) -> Self {
        self.sizes = sizes;
        self
    }

    /// Set the per-point fill colors, returning `self` for chaining. The colors
    /// broadcast over the offsets.
    #[must_use]
    pub fn with_facecolors(mut self, facecolors: Vec<Rgba>) -> Self {
        self.facecolors = facecolors;
        self
    }

    /// Set the per-point edge colors, returning `self` for chaining. An empty
    /// vector means no edge is drawn; otherwise the colors broadcast over the
    /// offsets.
    #[must_use]
    pub fn with_edgecolors(mut self, edgecolors: Vec<Rgba>) -> Self {
        self.edgecolors = edgecolors;
        self
    }

    /// Set the edge width in points, returning `self` for chaining.
    #[must_use]
    pub fn linewidth(mut self, linewidth: f64) -> Self {
        self.linewidth = linewidth;
        self
    }

    /// Set the stacking order, returning `self` for chaining.
    #[must_use]
    pub fn with_zorder(mut self, zorder: f64) -> Self {
        self.zorder = zorder;
        self
    }

    /// Set whether the collection is drawn, returning `self` for chaining.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// The fill color for point `i`, broadcasting over `facecolors`.
    ///
    /// Returns `None` only when `facecolors` is empty (no fill).
    fn facecolor_at(&self, i: usize) -> Option<Rgba> {
        broadcast(&self.facecolors, i)
    }

    /// The edge color for point `i`, broadcasting over `edgecolors`.
    ///
    /// Returns `None` when `edgecolors` is empty (no edge).
    fn edgecolor_at(&self, i: usize) -> Option<Rgba> {
        broadcast(&self.edgecolors, i)
    }

    /// The marker size for point `i`, broadcasting over `sizes`.
    ///
    /// Falls back to [`DEFAULT_SIZE`] when `sizes` is empty.
    fn size_at(&self, i: usize) -> f64 {
        broadcast(&self.sizes, i).unwrap_or(DEFAULT_SIZE)
    }
}

/// Index `values` at `i` modulo its length, broadcasting a length-1 vector to
/// every index. Returns `None` for an empty vector.
fn broadcast<T: Copy>(values: &[T], i: usize) -> Option<T> {
    if values.is_empty() {
        None
    } else {
        Some(values[i % values.len()])
    }
}

impl Artist for Collection {
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        if !self.visible {
            return;
        }
        for (i, &[x, y]) in self.offsets.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let size = self.size_at(i);
            // Scale the unit marker about the origin; it is then translated to
            // the data point and mapped through the axes transform.
            let marker_path = self.marker.transformed(&Affine2D::from_scale(size, size));
            let point_transform = Affine2D::from_translation(x, y).then(transform);
            let edge = self.edgecolor_at(i);
            let gc = GraphicsContext {
                line_width: self.linewidth,
                stroke: edge,
                ..GraphicsContext::new()
            };
            renderer.draw_path(&gc, &marker_path, &point_transform, self.facecolor_at(i));
        }
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        let mut xmin = f64::INFINITY;
        let mut ymin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        let mut any = false;
        for &[x, y] in &self.offsets {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x);
            ymax = ymax.max(y);
            any = true;
        }
        if any {
            Some(Bbox::from_extents(xmin, ymin, xmax, ymax))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Renderer`] that records, per `draw_path` call, the resolved
    /// device-space translation alongside the fill and stroke colors.
    #[derive(Default)]
    struct MockRenderer {
        calls: Vec<Call>,
    }

    /// One recorded `draw_path` invocation.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Call {
        translation: (f64, f64),
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
    }

    impl Renderer for MockRenderer {
        fn draw_path(
            &mut self,
            gc: &GraphicsContext,
            _path: &Path,
            transform: &Affine2D,
            fill: Option<Rgba>,
        ) {
            let [.., e, f] = transform.matrix();
            self.calls.push(Call {
                translation: (e, f),
                fill,
                stroke: gc.stroke,
            });
        }

        fn canvas_size(&self) -> (f64, f64) {
            (100.0, 100.0)
        }
    }

    #[test]
    fn scatter_draws_once_per_offset() {
        let offsets = vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]];
        let coll = Collection::scatter(offsets);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls.len(), 5);
    }

    #[test]
    fn single_facecolor_broadcasts_to_all() {
        let offsets = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let coll = Collection::scatter(offsets).with_facecolors(vec![Rgba::RED]);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        let fills: Vec<_> = r.calls.iter().map(|c| c.fill).collect();
        assert_eq!(fills, vec![Some(Rgba::RED); 3]);
    }

    #[test]
    fn per_point_facecolors_apply_elementwise() {
        let offsets = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let coll =
            Collection::scatter(offsets).with_facecolors(vec![Rgba::RED, Rgba::GREEN, Rgba::BLUE]);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        let fills: Vec<_> = r.calls.iter().map(|c| c.fill).collect();
        assert_eq!(
            fills,
            vec![Some(Rgba::RED), Some(Rgba::GREEN), Some(Rgba::BLUE)]
        );
    }

    #[test]
    fn edgecolors_drive_the_stroke() {
        let offsets = vec![[0.0, 0.0], [1.0, 0.0]];
        let coll = Collection::scatter(offsets).with_edgecolors(vec![Rgba::BLACK]);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        let strokes: Vec<_> = r.calls.iter().map(|c| c.stroke).collect();
        assert_eq!(strokes, vec![Some(Rgba::BLACK), Some(Rgba::BLACK)]);
    }

    #[test]
    fn no_edgecolors_means_no_stroke() {
        let offsets = vec![[0.0, 0.0]];
        let coll = Collection::scatter(offsets);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls[0].stroke, None);
    }

    #[test]
    fn draw_places_markers_at_offsets() {
        let offsets = vec![[10.0, 20.0], [30.0, 40.0]];
        let coll = Collection::scatter(offsets);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        let translations: Vec<_> = r.calls.iter().map(|c| c.translation).collect();
        assert_eq!(translations, vec![(10.0, 20.0), (30.0, 40.0)]);
    }

    #[test]
    fn invisible_collection_draws_nothing() {
        let coll = Collection::scatter(vec![[0.0, 0.0]]).with_visible(false);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        assert!(r.calls.is_empty());
    }

    #[test]
    fn with_zorder_sets_trait_zorder() {
        let coll = Collection::scatter(vec![[0.0, 0.0]]).with_zorder(9.0);
        assert_eq!(coll.zorder(), 9.0);
    }

    #[test]
    fn nan_offset_is_skipped_when_drawing() {
        let offsets = vec![[0.0, 0.0], [f64::NAN, 1.0], [2.0, 2.0]];
        let coll = Collection::scatter(offsets);
        let mut r = MockRenderer::default();
        coll.draw(&mut r, &Affine2D::identity());
        assert_eq!(r.calls.len(), 2);
    }

    #[test]
    fn data_extents_covers_offsets_ignoring_nan() {
        let offsets = vec![[1.0, -1.0], [f64::NAN, 5.0], [3.0, 4.0]];
        let coll = Collection::scatter(offsets);
        let e = coll.data_extents().expect("non-empty");
        assert_eq!(e.xmin(), 1.0);
        assert_eq!(e.xmax(), 3.0);
        assert_eq!(e.ymin(), -1.0);
        assert_eq!(e.ymax(), 4.0);
    }

    #[test]
    fn data_extents_empty_is_none() {
        let coll = Collection::scatter(vec![]);
        assert!(coll.data_extents().is_none());
    }

    #[test]
    fn default_zorder_is_one() {
        let coll = Collection::scatter(vec![[0.0, 0.0]]);
        // The inherent `zorder` setter shadows the trait getter, so the trait
        // method is called explicitly here.
        assert_eq!(Artist::zorder(&coll), 1.0);
    }
}
