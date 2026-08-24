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
use crate::figure::colorbar::Colorbar;
#[cfg(feature = "portable")]
use crate::figure::legend::{LegendEntry, LegendLocation};
#[cfg(feature = "portable")]
use crate::figure::plotting_contour::ContourLabelCandidate;
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
    pub(crate) generator: rizzma_portable::GeneratorRef,
    /// The renderer this artifact was authored against. The digest is
    /// provenance and a lookup key for a host's own registry — never an
    /// authorization; see the [`rizzma_portable`] crate docs.
    pub(crate) renderer: rizzma_portable::RendererRef,
    /// Layout, accessibility, and poster metadata a host can read without
    /// instantiating a renderer. Absent in schema 1 artifacts, which predate
    /// it, so it stays optional for as long as [`SCHEMA_MIN`] is 1.
    ///
    /// [`SCHEMA_MIN`]: super::SCHEMA_MIN
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) meta: Option<rizzma_portable::Meta>,
    /// The figure itself.
    pub(crate) figure: FigureSpec,
    /// Typed views into the binary chunk.
    pub(crate) accessors: Vec<Accessor>,
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
    pub(crate) axes: Vec<AxesSpec>,
    pub(crate) colorbars: Vec<Colorbar>,
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
    pub(crate) extra_data_bbox: Option<Bbox>,
    pub(crate) xaxis: AxisSpec,
    pub(crate) yaxis: AxisSpec,
    pub(crate) xaxis_hidden: bool,
    pub(crate) xlim_link: Option<usize>,
    pub(crate) secondary_x: Option<SecondaryXAxis>,
    pub(crate) title: Option<String>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) annotation_color: Rgba,
    pub(crate) contour_label_candidates: Vec<ContourLabelCandidate>,
    pub(crate) frame: bool,
    pub(crate) aspect_equal: bool,
    pub(crate) axis_visible: bool,
    pub(crate) scope: bool,
    pub(crate) span_lines: Vec<SpanLine>,
    pub(crate) span_rects: Vec<SpanRect>,
    pub(crate) legend: Vec<LegendEntry>,
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
    /// Reserved for per-artist legend labels (issue #286); always `None` today.
    pub(crate) label: Option<String>,
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
    /// Reserved for per-artist legend labels (issue #286); always `None` today.
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
    pub(crate) visible: bool,
    pub(crate) zorder: f64,
    /// Reserved for per-artist legend labels (issue #286); always `None` today.
    pub(crate) label: Option<String>,
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
