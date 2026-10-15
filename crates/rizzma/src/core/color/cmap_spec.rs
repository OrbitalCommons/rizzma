//! Colormaps by name or by value.
//!
//! Every colormapped artist takes a [`CmapSpec`]: a registry name such as
//! `"viridis"`, or a colormap of your own ([`ListedColormap`],
//! [`LinearSegmentedColormap`], or [`ListedColormap::from_fn`] for a
//! hand-rolled value-to-color function). Anything that converts into one
//! works wherever a colormap is taken, so a bare name still does.
//! [`register_colormap`] gives a map of your own a name, for the code paths
//! that only take strings (the pyplot facade, the wasm bindings).

use std::sync::{Arc, Mutex};

use super::cmap::{Colormap, LinearSegmentedColormap, ListedColormap};
use super::mapping::resolve_cmap;

/// A colormap, by registry name or as a value.
#[derive(Debug, Clone, PartialEq)]
pub enum CmapSpec {
    /// A builtin or [registered](register_colormap) colormap name, resolved
    /// when drawn (an unknown name draws the default map, reported once).
    Named(String),
    /// A colormap of your own.
    Custom(Arc<ListedColormap>),
}

impl CmapSpec {
    /// The colormap to sample.
    #[must_use]
    pub fn resolve(&self) -> Box<dyn Colormap> {
        match self {
            CmapSpec::Named(name) => resolve_cmap(name),
            CmapSpec::Custom(cmap) => Box::new(Arc::clone(cmap)),
        }
    }

    /// The registry name, for a named colormap.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            CmapSpec::Named(name) => Some(name),
            CmapSpec::Custom(_) => None,
        }
    }
}

impl From<&str> for CmapSpec {
    fn from(name: &str) -> Self {
        CmapSpec::Named(name.to_owned())
    }
}

impl From<String> for CmapSpec {
    fn from(name: String) -> Self {
        CmapSpec::Named(name)
    }
}

impl From<&String> for CmapSpec {
    fn from(name: &String) -> Self {
        CmapSpec::Named(name.clone())
    }
}

impl From<ListedColormap> for CmapSpec {
    fn from(cmap: ListedColormap) -> Self {
        CmapSpec::Custom(Arc::new(cmap))
    }
}

impl From<Arc<ListedColormap>> for CmapSpec {
    fn from(cmap: Arc<ListedColormap>) -> Self {
        CmapSpec::Custom(cmap)
    }
}

impl From<LinearSegmentedColormap> for CmapSpec {
    fn from(cmap: LinearSegmentedColormap) -> Self {
        CmapSpec::Custom(Arc::new(cmap.to_listed()))
    }
}

/// Colormaps given names with [`register_colormap`].
static REGISTERED: Mutex<Vec<(String, Arc<ListedColormap>)>> = Mutex::new(Vec::new());

/// Give `cmap` the registry name `name`, so every API taking a colormap
/// name accepts it (with an `_r` reversed variant): matplotlib's
/// `matplotlib.colormaps.register`. Registering a name again replaces the
/// map. Returns `false`, registering nothing, when `name` is a builtin
/// colormap, starts with `misleading:` or `lut:`, or ends in `_r`.
///
/// Portable figures embed a registered map's colors, so they render where
/// it was never registered.
///
/// ```
/// use rizzma::core::color::{ListedColormap, Rgba, colormap, register_colormap};
///
/// let stoplight = ListedColormap::from_colors(&[Rgba::GREEN, Rgba::RED]);
/// assert!(register_colormap("stoplight", stoplight));
/// assert!(colormap("stoplight_r").is_some());
/// assert!(!register_colormap("viridis", ListedColormap::from_colors(&[Rgba::BLACK])));
/// ```
pub fn register_colormap(name: &str, cmap: impl Into<CmapSpec>) -> bool {
    let reserved = name.starts_with("misleading:")
        || name.starts_with(LUT_PREFIX)
        || name.ends_with("_r")
        || super::cmap::builtin_colormap(name).is_some();
    let CmapSpec::Custom(cmap) = cmap.into() else {
        return false;
    };
    if reserved {
        return false;
    }
    let Ok(mut registered) = REGISTERED.lock() else {
        return false;
    };
    // The reversed variant is stored alongside, so a lookup is a plain
    // match (and code that never registers carries no reversal).
    let reversed = Arc::new(cmap.reversed());
    for (key, map) in [(name.to_owned(), cmap), (format!("{name}_r"), reversed)] {
        match registered.iter_mut().find(|(known, _)| *known == key) {
            Some(entry) => entry.1 = map,
            None => registered.push((key, map)),
        }
    }
    true
}

/// The colormap registered as `name` (an `_r` variant included), if any.
pub(crate) fn registered_colormap(name: &str) -> Option<Arc<ListedColormap>> {
    let registered = REGISTERED.lock().ok()?;
    registered
        .iter()
        .find(|(known, _)| known == name)
        .map(|(_, cmap)| Arc::clone(cmap))
}

/// The wire prefix of an embedded colormap: `lut:`, then each color's
/// RGBA channels as the 16 hex digits of their `f64` bits — lossless, and
/// decoded without a base64 engine in the wasm runtime.
const LUT_PREFIX: &str = "lut:";

/// Hex digits per encoded channel.
#[cfg(feature = "portable")]
const CHANNEL_HEX: usize = 16;

#[cfg(feature = "portable")]
impl CmapSpec {
    /// The portable form: a builtin name as itself; a colormap of your own,
    /// or a registered name, as its embedded colors, so the figure renders
    /// where that map was never registered.
    pub(crate) fn to_wire(&self) -> String {
        use std::fmt::Write as _;
        let custom = match self {
            CmapSpec::Custom(cmap) => Arc::clone(cmap),
            CmapSpec::Named(name) => match registered_colormap(name) {
                Some(cmap) => cmap,
                None => return name.clone(),
            },
        };
        let mut wire =
            String::with_capacity(LUT_PREFIX.len() + custom.colors.len() * 4 * CHANNEL_HEX);
        wire.push_str(LUT_PREFIX);
        for c in &custom.colors {
            for v in [c.r, c.g, c.b, c.a] {
                let _ = write!(wire, "{:016x}", v.to_bits());
            }
        }
        wire
    }

    /// Read the portable form back; a malformed embedded map is an error.
    pub(crate) fn from_wire(wire: &str) -> Result<Self, crate::portable::PortableError> {
        let Some(encoded) = wire.strip_prefix(LUT_PREFIX) else {
            return Ok(CmapSpec::Named(wire.to_owned()));
        };
        let malformed = |why: &str| {
            crate::portable::PortableError::Malformed(format!("embedded colormap: {why}"))
        };
        if encoded.is_empty() || !encoded.is_ascii() || encoded.len() % (4 * CHANNEL_HEX) != 0 {
            return Err(malformed("not a whole number of hex RGBA f64 colors"));
        }
        let channels = encoded
            .as_bytes()
            .chunks_exact(CHANNEL_HEX)
            .map(|digits| {
                std::str::from_utf8(digits)
                    .ok()
                    .and_then(|d| u64::from_str_radix(d, 16).ok())
                    .map(f64::from_bits)
                    .ok_or_else(|| malformed("not hex"))
            })
            .collect::<Result<Vec<f64>, _>>()?;
        if !channels.iter().all(|v| (0.0..=1.0).contains(v)) {
            return Err(malformed("channels must lie in [0, 1]"));
        }
        let colors = channels
            .chunks_exact(4)
            .map(|c| super::Rgba::new(c[0], c[1], c[2], c[3]))
            .collect();
        Ok(CmapSpec::Custom(Arc::new(ListedColormap::new(colors))))
    }

    /// Whether the portable form embeds a colormap (schema 15).
    pub(crate) fn is_embedded_wire(wire: &str) -> bool {
        wire.starts_with(LUT_PREFIX)
    }
}
