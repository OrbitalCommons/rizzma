//! Reading an artifact's metadata without rendering it.
//!
//! A host has decisions to make *before* it draws anything, or instead of ever
//! drawing: how much space to reserve, what to announce to a screen reader,
//! which runtime to pick, and what to show when it cannot render at all.
//! [`inspect`] answers those from the head of the JSON chunk, never allocating
//! the binary payloads.
//!
//! That is the common case rather than an edge case: a transcript holding
//! hundreds of figures paints a poster and a card for nearly all of them and
//! renders one or two.

use serde::Deserialize;

use super::container;
use super::meta::ControlRef;
use super::{GeneratorRef, Limits, Meta, Metadata, PortableError, RendererRef};

/// The subset of the spec document inspection needs.
///
/// Deliberately **not** `deny_unknown_fields`: the inspector's job is metadata,
/// so it ignores the figure model and the accessor table rather than parsing
/// them. Full validation happens when the figure is actually built.
#[derive(Debug, Deserialize)]
struct Header {
    schema: u32,
    generator: GeneratorRef,
    renderer: RendererRef,
    #[serde(default)]
    meta: Option<Meta>,
    /// Presence only — inspection needs the timeline's existence for the
    /// schema-floor check, not its keyframes.
    #[serde(default)]
    timeline: Option<serde::de::IgnoredAny>,
    /// Manifest fields only: serde skips each control's tracks and grids
    /// rather than allocating them, keeping inspection proportional to the
    /// manifest, not the keyframe data.
    #[serde(default)]
    controls: Vec<ControlRef>,
    /// Only the schema-5 axis fields, for the schema-floor check; serde skips
    /// the rest of the figure model.
    #[serde(default)]
    figure: FigureHead,
}

/// The figure fields inspection reads: each axes' schema-5 styling markers.
#[derive(Debug, Default, Deserialize)]
struct FigureHead {
    #[serde(default)]
    suptitle_size: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    axes: Vec<AxesHead>,
}

/// Presence of an axes' title style, text size and minor-tick settings.
#[derive(Debug, Deserialize)]
struct AxesHead {
    #[serde(default)]
    title_style: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    text_size: Option<serde::de::IgnoredAny>,
    xaxis: AxisHead,
    yaxis: AxisHead,
}

/// Presence of an axis' minor-tick settings.
#[derive(Debug, Deserialize)]
struct AxisHead {
    #[serde(default)]
    minor: Option<serde::de::IgnoredAny>,
}

impl FigureHead {
    /// Mirrors [`uses_axis_styling`](super::spec::uses_axis_styling).
    fn uses_axis_styling(&self) -> bool {
        self.axes.iter().any(|axes| {
            axes.title_style.is_some() || axes.xaxis.minor.is_some() || axes.yaxis.minor.is_some()
        })
    }

    /// Mirrors [`uses_text_sizes`](super::spec::uses_text_sizes).
    fn uses_text_sizes(&self) -> bool {
        self.suptitle_size.is_some() || self.axes.iter().any(|axes| axes.text_size.is_some())
    }
}

/// Read an artifact's metadata without building a figure or rendering it.
///
/// Walks the chunk directory, parses only the head of the JSON chunk, and
/// never allocates the `BIN` payloads. Everything is checked against `limits`
/// first, because artifact bytes are attacker-influenced and the host is the
/// one that knows what is reasonable.
///
/// ```
/// use rizzma::Figure;
/// use rizzma::portable::{inspect, Limits};
///
/// let mut fig = Figure::new(6.4, 4.8);
/// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
/// let bytes = fig.to_portable()?;
///
/// let info = inspect(&bytes, &Limits::default())?;
/// let meta = info.meta.as_ref().expect("schema 2 carries meta");
/// assert_eq!((meta.width_px, meta.height_px), (640, 480));
/// assert!(info.renderable());
/// assert!(info.poster(&bytes).is_some());
/// # Ok::<(), rizzma::PortableError>(())
/// ```
///
/// # Errors
///
/// [`PortableError::Malformed`] for a structurally invalid artifact,
/// [`PortableError::Budget`] when `limits` are exceeded, and
/// [`PortableError::Json`] when the spec head will not parse. Note that an
/// *unsupported schema* is not an error here — it is reported as
/// [`Metadata::schema_supported`] being false, because a host still wants the
/// size and poster of a figure it cannot draw.
pub fn inspect(bytes: &[u8], limits: &Limits) -> Result<Metadata, PortableError> {
    let dir = container::directory(bytes, limits)?;

    let json = container::chunk(bytes, &dir, container::TAG_JSON).expect("directory checked JSON");
    if json.len() > limits.max_json_bytes {
        return Err(PortableError::Budget(format!(
            "JSON chunk is {} bytes, over the {} byte limit",
            json.len(),
            limits.max_json_bytes
        )));
    }
    let header: Header = serde_json::from_slice(json)?;

    if let Some(meta) = &header.meta {
        if let Some(poster) = &meta.poster {
            if poster.bytes > limits.max_poster_bytes {
                return Err(PortableError::Budget(format!(
                    "poster is {} bytes, over the {} byte limit",
                    poster.bytes, limits.max_poster_bytes
                )));
            }
            let actual = container::chunk(bytes, &dir, container::TAG_PSTR).map(<[u8]>::len);
            if actual != Some(poster.bytes) {
                return Err(PortableError::Malformed(format!(
                    "spec declares a {}-byte poster but the PSTR chunk holds {}",
                    poster.bytes,
                    actual.map_or_else(|| "nothing".to_string(), |n| n.to_string())
                )));
            }
        }
        let pixels = (meta.width_px as usize).saturating_mul(meta.height_px as usize);
        if pixels > limits.max_canvas_pixels {
            return Err(PortableError::Budget(format!(
                "figure is {}x{} = {pixels} pixels, over the {} pixel limit",
                meta.width_px, meta.height_px, limits.max_canvas_pixels
            )));
        }
    }

    // Schema is the compatibility decision — a host selects a runtime from
    // the declaration — so features must not be able to under-declare it.
    let required = super::required_schema(
        header.meta.is_some(),
        header.timeline.is_some(),
        !header.controls.is_empty(),
        header.figure.uses_axis_styling(),
        header.figure.uses_text_sizes(),
    );
    if header.schema < required {
        return Err(PortableError::Malformed(format!(
            "declares schema {} but carries features requiring schema {required}",
            header.schema
        )));
    }

    // Each control becomes host-materialized state — a persisted manifest
    // entry, a DOM slider — so the count and label size answer to their own
    // budgets, not to whatever fits in the JSON allowance.
    if header.controls.len() > limits.max_controls {
        return Err(PortableError::Budget(format!(
            "artifact declares {} controls, over the {} control limit",
            header.controls.len(),
            limits.max_controls
        )));
    }
    if let Some(long) = header
        .controls
        .iter()
        .find(|c| c.label.len() > limits.max_control_label_bytes)
    {
        return Err(PortableError::Budget(format!(
            "a control label is {} bytes, over the {} byte limit",
            long.label.len(),
            limits.max_control_label_bytes
        )));
    }

    for (i, c) in header.controls.iter().enumerate() {
        // The same range invariants import enforces, minus the track data the
        // header never parsed. A manifest a host would persist must not carry
        // a range that makes every slider position nonsense.
        if !(c.min.is_finite() && c.max.is_finite() && c.default.is_finite())
            || c.min >= c.max
            || !(c.min..=c.max).contains(&c.default)
        {
            return Err(PortableError::Malformed(format!(
                "control {i} range [{}, {}] with default {} is not finite, \
                 increasing, and containing",
                c.min, c.max, c.default
            )));
        }
        if let Some(step) = c.step.filter(|step| !(step.is_finite() && *step > 0.0)) {
            return Err(PortableError::Malformed(format!(
                "control {i} step {step} is not finite and positive"
            )));
        }
    }

    Ok(Metadata {
        schema: header.schema,
        schema_supported: (super::SCHEMA_MIN..=super::SCHEMA_VERSION).contains(&header.schema),
        generator: header.generator,
        renderer: header.renderer,
        meta: header.meta,
        controls: header.controls,
        chunks: dir,
        total_bytes: bytes.len(),
    })
}
