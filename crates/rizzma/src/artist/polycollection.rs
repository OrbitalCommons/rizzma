//! The [`PolyCollection`] artist: many filled polygons, each with its own
//! face color.
//!
//! Mirrors matplotlib's `PolyCollection` with an array set on it: one
//! closed data-space polygon per item, colored from per-item values through
//! a colormap and norm (see
//! [`Axes::poly_collection`](crate::figure::Axes::poly_collection)). Sky
//! pixelizations such as HEALPix, survey footprints and Voronoi cells are
//! drawn this way.

use std::borrow::Cow;

use crate::core::color::{ColorScale, Rgba};
use crate::core::{Affine2D, Bbox, Path};
use crate::render::{GraphicsContext, Renderer};

use crate::artist::Artist;

/// Many filled polygons in data space, each with its own face color.
///
/// Construct with [`PolyCollection::new`] from the polygons' vertices and
/// one face color per polygon. Defaults, matching matplotlib's
/// `PolyCollection`: no edge, `1.0`-point edge width, zorder `1.0`, visible.
#[derive(Debug, Clone, PartialEq)]
pub struct PolyCollection {
    /// Every polygon's vertices, back to back.
    vertices: Vec<[f64; 2]>,
    /// Where each polygon starts in `vertices`; polygon `i` runs to the
    /// next start (or the end).
    starts: Vec<usize>,
    /// One fill color per polygon.
    facecolors: Vec<Rgba>,
    /// Shared edge (stroke) color; `None` draws no edge.
    edgecolor: Option<Rgba>,
    /// Edge width in points (used only with an edge color).
    linewidth: f64,
    /// Stacking order; higher draws on top.
    zorder: f64,
    /// Whether the collection is drawn.
    visible: bool,
    /// The mapping the face colors came from, for a colorbar; `None` when
    /// the colors were given directly.
    color_scale: Option<ColorScale>,
}

impl PolyCollection {
    /// Build a collection from `polygons` (each a list of data-space
    /// vertices, closed implicitly) and one face color per polygon.
    ///
    /// # Panics
    ///
    /// Panics if `facecolors.len()` differs from `polygons.len()`, or a
    /// polygon is empty or has a vertex that is not finite.
    #[must_use]
    pub fn new(polygons: &[Vec<[f64; 2]>], facecolors: Vec<Rgba>) -> Self {
        assert_eq!(
            facecolors.len(),
            polygons.len(),
            "PolyCollection: {} face colors for {} polygons",
            facecolors.len(),
            polygons.len()
        );
        let mut vertices = Vec::with_capacity(polygons.iter().map(Vec::len).sum());
        let mut starts = Vec::with_capacity(polygons.len());
        for polygon in polygons {
            assert!(
                !polygon.is_empty() && polygon.iter().flatten().all(|v| v.is_finite()),
                "PolyCollection: every polygon needs vertices, all finite"
            );
            starts.push(vertices.len());
            vertices.extend_from_slice(polygon);
        }
        Self {
            vertices,
            starts,
            facecolors,
            edgecolor: None,
            linewidth: 1.0,
            zorder: 1.0,
            visible: true,
            color_scale: None,
        }
    }

    /// The number of polygons.
    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Whether the collection holds no polygons.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The vertices of polygon `i`.
    ///
    /// # Panics
    ///
    /// Panics if `i >= self.len()`.
    #[must_use]
    pub fn polygon(&self, i: usize) -> &[[f64; 2]] {
        let end = self
            .starts
            .get(i + 1)
            .copied()
            .unwrap_or(self.vertices.len());
        &self.vertices[self.starts[i]..end]
    }

    /// The face color of each polygon.
    #[must_use]
    pub fn facecolors(&self) -> &[Rgba] {
        &self.facecolors
    }

    /// The color mapping the face colors came from, when the collection was
    /// colormapped from data.
    #[must_use]
    pub fn color_scale(&self) -> Option<&ColorScale> {
        self.color_scale.as_ref()
    }

    /// Record the color mapping the face colors came from.
    pub(crate) fn with_color_scale(mut self, scale: ColorScale) -> Self {
        self.color_scale = Some(scale);
        self
    }

    /// Stroke every polygon's outline in `edgecolor` (`None` for no edge).
    pub fn set_edgecolor(&mut self, edgecolor: Option<Rgba>) -> &mut Self {
        self.edgecolor = edgecolor;
        self
    }

    /// Set the edge width, in points.
    pub fn set_linewidth(&mut self, linewidth: f64) -> &mut Self {
        self.linewidth = linewidth;
        self
    }

    /// Set the stacking order; higher draws on top.
    pub fn set_zorder(&mut self, zorder: f64) -> &mut Self {
        self.zorder = zorder;
        self
    }

    /// Show or hide the collection.
    pub fn set_visible(&mut self, visible: bool) -> &mut Self {
        self.visible = visible;
        self
    }

    /// Draw each polygon, its closed path passed through `map` (a scale or
    /// projection) before `transform`.
    pub(crate) fn draw_mapped(
        &self,
        renderer: &mut dyn Renderer,
        transform: &Affine2D,
        map: impl Fn(&Path) -> Cow<'_, Path>,
    ) {
        if !self.visible {
            return;
        }
        let gc = GraphicsContext {
            line_width: self.linewidth,
            stroke: self.edgecolor,
            ..GraphicsContext::new()
        };
        for (i, &color) in self.facecolors.iter().enumerate() {
            let polygon = self.polygon(i);
            let Some(&first) = polygon.first() else {
                continue;
            };
            let mut ring = polygon.to_vec();
            ring.push(first);
            let path = Path::from_polyline(&ring);
            renderer.draw_path(&gc, &map(&path), transform, Some(color));
        }
    }
}

impl Artist for PolyCollection {
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        self.draw_mapped(renderer, transform, |path| Cow::Borrowed(path));
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        let first = self.vertices.first()?;
        let (mut lo, mut hi) = (*first, *first);
        for &[x, y] in &self.vertices {
            lo = [lo[0].min(x), lo[1].min(y)];
            hi = [hi[0].max(x), hi[1].max(y)];
        }
        Some(Bbox::from_extents(lo[0], lo[1], hi[0], hi[1]))
    }
}

#[cfg(feature = "portable")]
impl PolyCollection {
    /// Flatten this collection into its wire form: every polygon's vertices
    /// banked back to back, a NaN pair between polygons.
    pub(crate) fn to_portable(
        &self,
        bank: &mut crate::portable::data::BankWriter,
    ) -> crate::portable::spec::PolyCollectionSpec {
        let mut pairs = Vec::with_capacity(self.vertices.len() + self.len());
        for i in 0..self.len() {
            if i > 0 {
                pairs.push([f64::NAN; 2]);
            }
            pairs.extend_from_slice(self.polygon(i));
        }
        let scale = crate::portable::spec::FlatScale::of(self.color_scale.as_ref());
        crate::portable::spec::PolyCollectionSpec {
            vertices: bank.push_pairs(&pairs),
            facecolors: self.facecolors.clone(),
            edgecolor: self.edgecolor,
            linewidth: self.linewidth,
            zorder: self.zorder,
            visible: self.visible,
            cmap: scale.cmap,
            norm: scale.norm,
            bad: scale.bad,
            over: scale.over,
            under: scale.under,
        }
    }

    /// Reconstruct a collection from its wire form during import.
    pub(crate) fn from_portable(
        spec: &crate::portable::spec::PolyCollectionSpec,
        reader: &crate::portable::data::BankReader<'_>,
    ) -> Result<PolyCollection, crate::portable::PortableError> {
        let pairs = reader.pairs(&spec.vertices)?;
        let (mut vertices, mut starts) = (Vec::with_capacity(pairs.len()), Vec::new());
        let mut open = false;
        for p in pairs {
            if p[0].is_nan() && p[1].is_nan() {
                open = false;
                continue;
            }
            if !(p[0].is_finite() && p[1].is_finite()) {
                return Err(crate::portable::PortableError::Malformed(
                    "polygon vertices must be finite".to_owned(),
                ));
            }
            if !open {
                starts.push(vertices.len());
                open = true;
            }
            vertices.push(p);
        }
        if starts.len() != spec.facecolors.len() {
            return Err(crate::portable::PortableError::Malformed(format!(
                "polygon collection carries {} polygons but {} face colors",
                starts.len(),
                spec.facecolors.len()
            )));
        }
        Ok(PolyCollection {
            vertices,
            starts,
            facecolors: spec.facecolors.clone(),
            edgecolor: spec.edgecolor,
            linewidth: spec.linewidth,
            zorder: spec.zorder,
            visible: spec.visible,
            color_scale: crate::portable::spec::FlatScale {
                cmap: spec.cmap.clone(),
                norm: spec.norm.clone(),
                bad: spec.bad,
                over: spec.over,
                under: spec.under,
            }
            .into_scale()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64) -> Vec<[f64; 2]> {
        vec![[x, 0.0], [x + 1.0, 0.0], [x + 1.0, 1.0], [x, 1.0]]
    }

    #[test]
    fn polygons_keep_their_vertices_and_extents() {
        let polys = PolyCollection::new(&[square(0.0), square(3.0)], vec![Rgba::BLACK; 2]);
        assert_eq!(polys.len(), 2);
        assert_eq!(polys.polygon(1), square(3.0).as_slice());
        let extents = polys.data_extents().expect("extents");
        assert_eq!((extents.xmin(), extents.xmax()), (0.0, 4.0));
        assert!(
            PolyCollection::new(&[], Vec::new())
                .data_extents()
                .is_none()
        );
    }

    #[test]
    #[should_panic(expected = "2 face colors for 1 polygons")]
    fn mismatched_colors_panic() {
        let _ = PolyCollection::new(&[square(0.0)], vec![Rgba::BLACK; 2]);
    }
}
