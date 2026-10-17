//! The wire model: strict serde mirrors of the semantic figure.
//!
//! These types are the schema of the portable-figure JSON chunk. They mirror
//! the live model (`Figure → Axes → Artists` plus the axis trait objects) as
//! plain data: every struct is `deny_unknown_fields`, every enum is closed, so
//! an artifact from a newer schema fails loudly instead of silently dropping
//! content. Small closed leaf types ([`Rgba`](crate::core::color::Rgba),
//! [`RcParams`](crate::core::RcParams), [`Bbox`](crate::core::Bbox), span and
//! annotation records, …) serialize in place; the mirrors here exist where the
//! live types hold trait objects, caches, or bulk data that belongs in the
//! binary chunk.
//!
//! Conversions live next to the types they read: each artist, the axis, the
//! axes, and the figure implement `to_portable` / `from_portable` in their own
//! modules where private fields are reachable. The trait-object closures
//! ([`Scale`](crate::axis::scale::Scale), [`Locator`](crate::axis::ticker::Locator),
//! [`Formatter`](crate::axis::ticker::Formatter)) are handled by the
//! `portable_spec` hook each concrete implementation provides; a type without
//! one (the closure-holding [`FuncFormatter`](crate::axis::ticker::FuncFormatter))
//! makes export fail loudly.

use serde::{Deserialize, Serialize};

use crate::axis::ticker::TickPrune;

// The artifact-level types below exist only with the `portable` feature; the
// three axis enums at the end of this file are always compiled, because the
// public `Scale`/`Locator`/`Formatter` hooks name them either way.
#[cfg(feature = "portable")]
use crate::axis::axis::AxisSide;
#[cfg(feature = "portable")]
use crate::core::color::Rgba;
#[cfg(feature = "portable")]
use crate::core::{Bbox, RcParams};
#[cfg(feature = "portable")]
use crate::figure::axes::{Annotation, SecondaryXAxis, SpanLine, SpanRect};
#[cfg(feature = "portable")]
use crate::figure::colorbar::FigureColorbar;
#[cfg(feature = "portable")]
use crate::figure::legend::{ArtistKind, LegendLocation};
#[cfg(feature = "portable")]
use crate::figure::plotting_contour::ContourLabelCandidate;
#[cfg(feature = "portable")]
use crate::figure::text::Text;
#[cfg(feature = "portable")]
use crate::render::{CapStyle, JoinStyle};

#[cfg(feature = "portable")]
use super::data::{Accessor, ArrF64, ArrU8};

/// The top-level JSON document of a portable figure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct PortableSpec {
    /// Wire-format schema version (see [`SCHEMA_VERSION`](super::SCHEMA_VERSION)).
    pub(crate) schema: u32,
    /// Provenance of the exporter.
    pub(crate) generator: crate::portable::GeneratorRef,
    /// The renderer this artifact was authored against. The digest is
    /// provenance and a lookup key for a host's own registry — never an
    /// authorization; see the [module docs](super).
    pub(crate) renderer: crate::portable::RendererRef,
    /// Layout, accessibility, and poster metadata a host can read without
    /// instantiating a renderer. Absent in schema 1 artifacts, which predate
    /// it, so it stays optional for as long as [`SCHEMA_MIN`] is 1.
    ///
    /// [`SCHEMA_MIN`]: super::SCHEMA_MIN
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) meta: Option<crate::portable::Meta>,
    /// The figure itself.
    pub(crate) figure: FigureSpec,
    /// The animation, when the figure has one. Absent in schema 1 and 2, which
    /// predate it, so it stays optional while those remain loadable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeline: Option<crate::portable::Timeline>,
    /// User-driven parameters, when the figure has any. Absent before schema
    /// 4, which introduced them, so the field stays defaulted while older
    /// artifacts remain loadable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) controls: Vec<crate::portable::Control>,
    /// Typed views into the binary chunk.
    pub(crate) accessors: Vec<Accessor>,
}

/// Whether any axes carries schema-5 styling: minor ticks or grid, or a
/// non-default title size/pad.
#[cfg(feature = "portable")]
pub(crate) fn uses_axis_styling(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.title_style.is_some() || axes.xaxis.minor.is_some() || axes.yaxis.minor.is_some()
    })
}

/// Whether the figure carries schema-6 text sizing: a figure-title size or an
/// axes text size.
#[cfg(feature = "portable")]
pub(crate) fn uses_text_sizes(figure: &FigureSpec) -> bool {
    figure.suptitle_size.is_some() || figure.axes.iter().any(|axes| axes.text_size.is_some())
}

/// Whether any axes carries schema-7 legend data: a scatter marker sample or a
/// recorded artist order.
#[cfg(feature = "portable")]
pub(crate) fn uses_legend_markers(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.artist_order.is_some() || axes.legend.iter().any(|entry| entry.marker.is_some())
    })
}

/// Whether the figure carries schema-8 figure text: free text or a figure
/// axis label.
#[cfg(feature = "portable")]
pub(crate) fn uses_figure_text(figure: &FigureSpec) -> bool {
    !figure.texts.is_empty() || figure.supxlabel.is_some() || figure.supylabel.is_some()
}

/// Whether any image carries schema-9 color mapping: an explicit norm or
/// bad/over/under colors.
#[cfg(feature = "portable")]
pub(crate) fn uses_image_norms(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.images.iter().any(|image| {
            image.norm.is_some()
                || image.bad.is_some()
                || image.over.is_some()
                || image.under.is_some()
        })
    })
}

/// Whether any axes carries schema-10 colorbar data: a colorbar, or the color
/// scale a mesh or collection was mapped through.
#[cfg(feature = "portable")]
pub(crate) fn uses_colorbars(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.colorbar.is_some()
            || axes.meshes.iter().any(|m| {
                m.cmap.is_some()
                    || m.norm.is_some()
                    || m.bad.is_some()
                    || m.over.is_some()
                    || m.under.is_some()
            })
            || axes.collections.iter().any(|c| {
                c.cmap.is_some()
                    || c.norm.is_some()
                    || c.bad.is_some()
                    || c.over.is_some()
                    || c.under.is_some()
            })
    })
}

/// Whether the figure's rc carries a schema-12 path-simplification setting
/// other than the default.
#[cfg(feature = "portable")]
pub(crate) fn uses_path_simplify(figure: &FigureSpec) -> bool {
    let default = RcParams::default();
    figure.rc.path_simplify != default.path_simplify
        || figure.rc.path_simplify_threshold != default.path_simplify_threshold
}

/// Whether any line, or legend sample, draws schema-17 markers.
#[cfg(feature = "portable")]
pub(crate) fn uses_line_markers(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.lines.iter().any(|l| l.marker.is_some())
            || axes.legend.iter().any(|e| e.line_marker.is_some())
    })
}

#[cfg(feature = "portable")]
fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

/// Whether any axis uses matplotlib's automatic log minor ticks or a log
/// formatter with a minimum exponent, or the rc sets one (schema 20).
#[cfg(feature = "portable")]
pub(crate) fn uses_log_ticks(figure: &FigureSpec) -> bool {
    let log_ticks = |axis: &AxisSpec| {
        let locators = std::iter::once(&axis.locator)
            .chain(axis.minor.as_ref().and_then(|m| m.locator.as_ref()));
        locators
            .into_iter()
            .any(|l| matches!(l, LocatorSpec::Log { auto: true, .. }))
            || matches!(
                axis.formatter,
                FormatterSpec::LogMathtext { min_exponent, .. } if min_exponent != 0
            )
    };
    figure.rc.axes_formatter_min_exponent != 0
        || figure
            .axes
            .iter()
            .any(|axes| log_ticks(&axes.xaxis) || log_ticks(&axes.yaxis))
}

#[cfg(feature = "portable")]
fn round_join() -> crate::render::JoinStyle {
    crate::render::JoinStyle::Round
}

#[cfg(feature = "portable")]
fn is_round_join(join: &crate::render::JoinStyle) -> bool {
    *join == crate::render::JoinStyle::Round
}

/// Whether any collection joins its edges other than round (schema 19).
#[cfg(feature = "portable")]
pub(crate) fn uses_collection_joins(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| {
        axes.collections
            .iter()
            .any(|c| c.join != crate::render::JoinStyle::Round)
    })
}

#[cfg(feature = "portable")]
fn is_line_axisbelow(axisbelow: &crate::core::rcparams::AxisBelow) -> bool {
    *axisbelow == crate::core::rcparams::AxisBelow::Line
}

/// Whether the figure places a grid other than matplotlib's default way
/// (schema 16): in its rc or on any axes.
#[cfg(feature = "portable")]
pub(crate) fn uses_axisbelow(figure: &FigureSpec) -> bool {
    use crate::core::rcparams::AxisBelow;
    figure.rc.axes_axisbelow != AxisBelow::Line
        || figure
            .axes
            .iter()
            .any(|axes| axes.axisbelow != AxisBelow::Line)
}

/// Whether any axes draws through a schema-13 sky projection.
#[cfg(feature = "portable")]
pub(crate) fn uses_sky_projection(figure: &FigureSpec) -> bool {
    figure.axes.iter().any(|axes| axes.sky.is_some())
}

/// Whether any artist is colored through a schema-11 astronomical
/// [`ImageNormalize`](crate::core::color::ImageNormalize).
#[cfg(feature = "portable")]
pub(crate) fn uses_image_normalize(figure: &FigureSpec) -> bool {
    use crate::core::color::Norm;
    let astro = |norm: &Option<Norm>| matches!(norm, Some(Norm::ImageNormalize(_)));
    figure.axes.iter().any(|axes| {
        axes.images.iter().any(|i| astro(&i.norm))
            || axes.meshes.iter().any(|m| astro(&m.norm))
            || axes.collections.iter().any(|c| astro(&c.norm))
    })
}

/// Wire mirror of [`Figure`](crate::figure::Figure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct FigureSpec {
    pub(crate) width_in: f64,
    pub(crate) height_in: f64,
    pub(crate) dpi: f64,
    pub(crate) facecolor: Rgba,
    pub(crate) rc: RcParams,
    pub(crate) suptitle: Option<String>,
    pub(crate) suptitle_color: Rgba,
    /// The figure title size, present only when it differs from the default.
    /// Absent before schema 6, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) suptitle_size: Option<f64>,
    /// The figure x label. Absent before schema 8, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supxlabel: Option<Text>,
    /// The figure y label. Absent before schema 8, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supylabel: Option<Text>,
    /// Free figure text. Absent before schema 8, which introduced it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) texts: Vec<Text>,
    pub(crate) axes: Vec<AxesSpec>,
    pub(crate) colorbars: Vec<FigureColorbar>,
}

/// Wire mirror of [`Axes`](crate::figure::Axes). Field-for-field with the live
/// struct; artist data is offloaded to accessors via the artist specs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct AxesSpec {
    pub(crate) position: Bbox,
    pub(crate) layout_envelope: Option<Bbox>,
    pub(crate) sticky_x: Vec<f64>,
    pub(crate) sticky_y: Vec<f64>,
    pub(crate) xlim: Option<(f64, f64)>,
    pub(crate) ylim: Option<(f64, f64)>,
    pub(crate) xinverted: bool,
    pub(crate) yinverted: bool,
    pub(crate) margins: f64,
    pub(crate) xscale: ScaleWire,
    pub(crate) yscale: ScaleWire,
    pub(crate) facecolor: Rgba,
    pub(crate) edgecolor: Rgba,
    pub(crate) linewidth: f64,
    pub(crate) title_color: Rgba,
    pub(crate) prop_cycle: Vec<Rgba>,
    pub(crate) prop_cycle_index: usize,
    pub(crate) legend_facecolor: Rgba,
    pub(crate) legend_edgecolor: Rgba,
    pub(crate) legend_labelcolor: Rgba,
    pub(crate) legend_title: Option<String>,
    pub(crate) legend_location: LegendLocation,
    pub(crate) lines: Vec<LineSpec>,
    pub(crate) patches: Vec<PatchSpec>,
    pub(crate) collections: Vec<CollectionSpec>,
    pub(crate) images: Vec<ImageSpec>,
    pub(crate) meshes: Vec<QuadMeshSpec>,
    /// The axes' polygon collections. Absent before schema 14, which
    /// introduced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) poly_collections: Vec<PolyCollectionSpec>,
    /// The axes' vector fields. Absent before schema 18, which introduced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) quivers: Vec<QuiverSpec>,
    pub(crate) extra_data_bbox: Option<Bbox>,
    pub(crate) xaxis: AxisSpec,
    pub(crate) yaxis: AxisSpec,
    pub(crate) xaxis_hidden: bool,
    pub(crate) xlim_link: Option<usize>,
    pub(crate) secondary_x: Option<SecondaryXAxis>,
    /// The axes' colorbar. Absent before schema 10, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) colorbar: Option<crate::figure::Colorbar>,
    pub(crate) title: Option<String>,
    /// `(size, pad)` of the title, present only when either differs from the
    /// default. Absent before schema 5, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) title_style: Option<(f64, f64)>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) annotation_color: Rgba,
    /// The size given to text added from now on, present only when it differs
    /// from the default. Absent before schema 6, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text_size: Option<f64>,
    pub(crate) contour_label_candidates: Vec<ContourLabelCandidate>,
    pub(crate) frame: bool,
    pub(crate) aspect_equal: bool,
    /// The all-sky projection the axes draws through. Absent before schema
    /// 13, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sky: Option<crate::figure::sky::SkyProjection>,
    /// Where the grid draws among the artists, present only when it is not
    /// the default. Absent before schema 16, which introduced it.
    #[serde(default, skip_serializing_if = "is_line_axisbelow")]
    pub(crate) axisbelow: crate::core::rcparams::AxisBelow,
    pub(crate) axis_visible: bool,
    pub(crate) scope: bool,
    pub(crate) span_lines: Vec<SpanLine>,
    pub(crate) span_rects: Vec<SpanRect>,
    pub(crate) legend: Vec<LegendEntrySpec>,
    /// The order the lines, patches and collections were added in, present
    /// only when it is not lines-then-patches-then-collections. Absent before
    /// schema 7, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) artist_order: Option<Vec<ArtistKind>>,
}

/// Wire mirror of a legend row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct LegendEntrySpec {
    pub(crate) color: Rgba,
    pub(crate) label: String,
    /// The one-marker sample of a scatter row. Absent before schema 7, which
    /// introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) marker: Option<CollectionSpec>,
    /// The marker shown in the middle of a marked line's sample. Absent
    /// before schema 17, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) line_marker: Option<crate::artist::line::LineMarker>,
}

/// Wire mirror of [`Axis`](crate::axis::axis::Axis), with the three trait
/// objects flattened to their closed specs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct AxisSpec {
    pub(crate) side: AxisSide,
    pub(crate) scale: ScaleWire,
    pub(crate) locator: LocatorSpec,
    pub(crate) formatter: FormatterSpec,
    pub(crate) label: Option<String>,
    pub(crate) color: Rgba,
    pub(crate) tick_length: f64,
    pub(crate) tick_width: f64,
    pub(crate) tick_label_size: f64,
    pub(crate) axis_label_size: f64,
    pub(crate) tick_label_pad: f64,
    pub(crate) axis_label_pad: f64,
    pub(crate) grid: bool,
    pub(crate) grid_color: Rgba,
    pub(crate) grid_linewidth: f64,
    pub(crate) grid_alpha: f64,
    pub(crate) tick_direction: crate::core::rcparams::TickDirection,
    pub(crate) tick_labels_visible: bool,
    /// Minor ticks and minor grid, present only when they differ from the
    /// defaults. Absent before schema 5, which introduced them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) minor: Option<MinorAxisSpec>,
}

/// Wire form of an axis' minor ticks and minor grid (schema 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct MinorAxisSpec {
    pub(crate) locator: Option<LocatorSpec>,
    pub(crate) tick_length: f64,
    pub(crate) tick_width: f64,
    pub(crate) grid: bool,
    pub(crate) grid_color: Rgba,
    pub(crate) grid_linewidth: f64,
    pub(crate) grid_alpha: f64,
}

/// Wire mirror of [`Line2D`](crate::artist::Line2D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct LineSpec {
    pub(crate) x: ArrF64,
    pub(crate) y: ArrF64,
    pub(crate) color: Rgba,
    pub(crate) linewidth: f64,
    pub(crate) dashes: Option<(f64, Vec<f64>)>,
    pub(crate) cap: CapStyle,
    pub(crate) join: JoinStyle,
    pub(crate) visible: bool,
    pub(crate) zorder: f64,
    /// Per-artist legend label, collected by `Axes::legend_auto`.
    pub(crate) label: Option<String>,
    /// The markers at the data points, present only when the line has them.
    /// Absent before schema 17, which introduced them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) marker: Option<crate::artist::line::LineMarker>,
}

/// Wire mirror of [`Patch`](crate::artist::Patch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct PatchSpec {
    pub(crate) path: PathSpec,
    pub(crate) facecolor: Option<Rgba>,
    pub(crate) edgecolor: Option<Rgba>,
    pub(crate) linewidth: f64,
    pub(crate) dashes: Option<(f64, Vec<f64>)>,
    pub(crate) cap: CapStyle,
    pub(crate) join: JoinStyle,
    pub(crate) visible: bool,
    pub(crate) zorder: f64,
    /// Per-artist legend label, collected by `Axes::legend_auto`.
    pub(crate) label: Option<String>,
}

/// Wire mirror of [`Collection`](crate::artist::Collection).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct CollectionSpec {
    /// Marker positions, flattened `[x0, y0, x1, y1, …]`.
    pub(crate) offsets: ArrF64,
    pub(crate) marker: PathSpec,
    pub(crate) sizes: ArrF64,
    pub(crate) facecolors: Vec<Rgba>,
    pub(crate) edgecolors: Vec<Rgba>,
    pub(crate) linewidth: f64,
    /// How the edges' corners join: written only when not round, the
    /// default. Absent before schema 19; those collections load round.
    #[serde(default = "round_join", skip_serializing_if = "is_round_join")]
    pub(crate) join: crate::render::JoinStyle,
    pub(crate) visible: bool,
    pub(crate) zorder: f64,
    /// Per-artist legend label, collected by `Axes::legend_auto`.
    pub(crate) label: Option<String>,
    /// The color mapping the face colors came from, flat as an image writes
    /// its own (see [`FlatScale`]). Absent before schema 10, which introduced
    /// it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cmap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) norm: Option<crate::core::color::Norm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bad: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) over: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) under: Option<Rgba>,
}

/// A mesh's or collection's optional [`ColorScale`] as the flat wire fields
/// it is written in: a colormap and norm present together, or neither (and
/// then no extreme colors).
///
/// [`ColorScale`]: crate::core::color::ColorScale
#[cfg(feature = "portable")]
pub(crate) struct FlatScale {
    pub(crate) cmap: Option<String>,
    pub(crate) norm: Option<crate::core::color::Norm>,
    pub(crate) bad: Option<Rgba>,
    pub(crate) over: Option<Rgba>,
    pub(crate) under: Option<Rgba>,
}

#[cfg(feature = "portable")]
impl FlatScale {
    /// The wire fields of `scale`.
    pub(crate) fn of(scale: Option<&crate::core::color::ColorScale>) -> Self {
        FlatScale {
            cmap: scale.map(|s| s.cmap.to_wire()),
            norm: scale.map(|s| s.norm.clone()),
            bad: scale.and_then(|s| s.extremes.bad),
            over: scale.and_then(|s| s.extremes.over),
            under: scale.and_then(|s| s.extremes.under),
        }
    }

    /// The scale these fields describe; a half-written one is malformed.
    pub(crate) fn into_scale(
        self,
    ) -> Result<Option<crate::core::color::ColorScale>, crate::portable::PortableError> {
        let extremes = crate::core::color::Extremes {
            bad: self.bad,
            over: self.over,
            under: self.under,
        };
        match (self.cmap, self.norm) {
            (Some(cmap), Some(norm)) => Ok(Some(crate::core::color::ColorScale {
                cmap: crate::core::color::CmapSpec::from_wire(&cmap)?,
                norm,
                extremes,
            })),
            (None, None) if extremes == crate::core::color::Extremes::default() => Ok(None),
            _ => Err(crate::portable::PortableError::Malformed(
                "a color scale needs both a colormap and a norm".to_owned(),
            )),
        }
    }
}

/// Wire mirror of [`AxesImage`](crate::artist::AxesImage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct ImageSpec {
    /// Row-major scalar samples, length `nrows * ncols`.
    pub(crate) data: ArrF64,
    pub(crate) nrows: usize,
    pub(crate) ncols: usize,
    pub(crate) extent: [f64; 4],
    pub(crate) vmin: f64,
    pub(crate) vmax: f64,
    pub(crate) cmap: String,
    /// An explicit norm. Absent before schema 9, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) norm: Option<crate::core::color::Norm>,
    /// The bad, over and under colors, each present only when set. Absent
    /// before schema 9, which introduced them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bad: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) over: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) under: Option<Rgba>,
    pub(crate) origin_upper: bool,
    pub(crate) zorder: f64,
    pub(crate) visible: bool,
    /// Reserved for per-artist legend labels (issue #286); always `None` today.
    pub(crate) label: Option<String>,
}

/// Wire mirror of [`QuadMesh`](crate::artist::QuadMesh).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct QuadMeshSpec {
    pub(crate) nrows: usize,
    pub(crate) ncols: usize,
    /// Grid corner coordinates, flattened `[x0, y0, x1, y1, …]`, row-major,
    /// `(nrows + 1) * (ncols + 1)` points.
    pub(crate) coordinates: ArrF64,
    pub(crate) facecolors: Vec<Rgba>,
    pub(crate) vertex_colors: Option<Vec<Rgba>>,
    pub(crate) edgecolor: Option<Rgba>,
    pub(crate) linewidth: f64,
    pub(crate) zorder: f64,
    pub(crate) visible: bool,
    /// Reserved for per-artist legend labels (issue #286); always `None` today.
    pub(crate) label: Option<String>,
    /// The color mapping the face colors came from, flat (see
    /// [`FlatScale`]). Absent before schema 10, which introduced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cmap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) norm: Option<crate::core::color::Norm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bad: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) over: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) under: Option<Rgba>,
}

/// Wire mirror of [`Quiver`](crate::figure::Quiver).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct QuiverSpec {
    pub(crate) x: ArrF64,
    pub(crate) y: ArrF64,
    pub(crate) u: ArrF64,
    pub(crate) v: ArrF64,
    pub(crate) options: crate::figure::QuiverOptions,
    pub(crate) zorder: f64,
    pub(crate) visible: bool,
}

/// Wire mirror of [`PolyCollection`](crate::artist::PolyCollection).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct PolyCollectionSpec {
    /// Every polygon's vertices, flattened `[x0, y0, x1, y1, …]`, with a
    /// NaN pair between consecutive polygons.
    pub(crate) vertices: ArrF64,
    pub(crate) facecolors: Vec<Rgba>,
    pub(crate) edgecolor: Option<Rgba>,
    pub(crate) linewidth: f64,
    pub(crate) zorder: f64,
    pub(crate) visible: bool,
    /// The color mapping the face colors came from, flat (see
    /// [`FlatScale`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cmap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) norm: Option<crate::core::color::Norm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bad: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) over: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) under: Option<Rgba>,
}

/// Whether any artist carries a schema-15 embedded colormap (a colormap of
/// the user's own, written out as its colors).
#[cfg(feature = "portable")]
pub(crate) fn uses_embedded_colormaps(figure: &FigureSpec) -> bool {
    use crate::core::color::CmapSpec;
    let embedded = |cmap: &Option<String>| cmap.as_deref().is_some_and(CmapSpec::is_embedded_wire);
    figure.axes.iter().any(|axes| {
        axes.images
            .iter()
            .any(|i| CmapSpec::is_embedded_wire(&i.cmap))
            || axes.meshes.iter().any(|m| embedded(&m.cmap))
            || axes.collections.iter().any(|c| embedded(&c.cmap))
            || axes.poly_collections.iter().any(|p| embedded(&p.cmap))
    })
}

/// Whether any axes carries schema-14 polygon collections.
#[cfg(feature = "portable")]
pub(crate) fn uses_poly_collections(figure: &FigureSpec) -> bool {
    figure
        .axes
        .iter()
        .any(|axes| !axes.poly_collections.is_empty())
}

/// Wire mirror of [`Path`](crate::core::Path): flattened vertices plus
/// optional per-vertex codes (see `code_to_u8` in `core::path`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "portable")]
pub(crate) struct PathSpec {
    /// Vertices, flattened `[x0, y0, x1, y1, …]`.
    pub(crate) verts: ArrF64,
    /// One code byte per vertex, or `None` for an implicit polyline.
    pub(crate) codes: Option<ArrU8>,
}

/// Closed wire form of the [`Scale`](crate::axis::scale::Scale) trait objects
/// and the axes-level [`ScaleSpec`](crate::figure::axes::ScaleSpec).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScaleWire {
    Linear,
    Log {
        base: f64,
    },
    Symlog {
        base: f64,
        linthresh: f64,
        linscale: f64,
    },
    Logit,
    Asinh {
        linear_width: f64,
    },
}

/// Closed wire form of every built-in [`Locator`](crate::axis::ticker::Locator).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LocatorSpec {
    MaxN {
        /// Maximum interval count; `None` is matplotlib's `nbins='auto'`.
        nbins: Option<usize>,
        /// The staircase-extended step multiples (stored as resolved, so the
        /// round trip is exact regardless of how they were built).
        extended_steps: Vec<f64>,
        integer: bool,
        symmetric: bool,
        min_n_ticks: usize,
        prune: TickPrune,
    },
    Auto,
    AutoMinor {
        subdivisions: Option<usize>,
    },
    Multiple {
        step: f64,
        offset: f64,
    },
    LinearN {
        numticks: usize,
    },
    Fixed {
        locs: Vec<f64>,
        nbins: Option<usize>,
    },
    Log {
        base: f64,
        subs: Vec<f64>,
        /// matplotlib's `subs='auto'`, written only when set (schema 20).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        auto: bool,
    },
    Symlog {
        base: f64,
        linthresh: f64,
        linear_ticks: usize,
    },
    Asinh {
        base: f64,
        linear_width: f64,
        linear_ticks: usize,
    },
    Logit {
        max_exponent: i32,
    },
    Index {
        base: f64,
        offset: f64,
    },
    Null,
    AutoDate {
        maxticks: usize,
    },
    AutoDateMinor {
        maxticks: usize,
    },
}

/// Closed wire form of every built-in serializable
/// [`Formatter`](crate::axis::ticker::Formatter).
/// [`FuncFormatter`](crate::axis::ticker::FuncFormatter) holds a closure and
/// has no wire form: exporting an axis using one fails loudly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum FormatterSpec {
    Scalar {
        /// `Some` when a fixed precision was chosen (`with_decimals` /
        /// `set_locs`); `None` for the default.
        decimals: Option<usize>,
    },
    Log {
        base: f64,
    },
    LogMathtext {
        base: f64,
        /// Below this power, labels are plain numbers; written only when not
        /// 0 (schema 20).
        #[serde(default, skip_serializing_if = "is_zero_i32")]
        min_exponent: i32,
    },
    Symlog {
        base: f64,
        linthresh: f64,
    },
    SymlogMathtext {
        base: f64,
        linthresh: f64,
    },
    Asinh {
        base: f64,
        linear_width: f64,
    },
    AsinhMathtext {
        base: f64,
        linear_width: f64,
    },
    Logit {
        max_exponent: i32,
    },
    LogitMathtext {
        max_exponent: i32,
    },
    Eng {
        unit: String,
        places: Option<usize>,
        separator: String,
    },
    Percent {
        xmax: f64,
        decimals: usize,
        symbol: String,
    },
    Null,
    Fixed {
        seq: Vec<String>,
    },
    Index {
        labels: Vec<String>,
    },
    FormatStr {
        fmt: String,
    },
    StrMethod {
        fmt: String,
    },
    Date {
        fmt: String,
    },
    ConciseDate,
}
