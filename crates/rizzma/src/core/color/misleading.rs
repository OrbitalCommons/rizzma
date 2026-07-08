//! Colormaps that are known to misrepresent data — quarantined on purpose.
//!
//! The classic vendor maps in this module (`jet`, `hot`, `hsv`, `rainbow`)
//! were built as piecewise paths through RGB space with no regard for how the
//! result is *perceived*. Kovesi's analysis (the paper below) shows what that
//! costs:
//!
//! - **Perceptual flat spots**: extended runs where the colors barely change
//!   in lightness — jet's cyan-to-yellow span is the canonical example — can
//!   hide features as large as one tenth of the total data range.
//! - **False anomalies**: sharp kinks in the lightness profile (jet at cyan,
//!   yellow, and red; hot where each RGB ramp saturates) read as edges and
//!   contours in the rendered image where the data has none.
//! - **No perceptual ordering**: rainbow-style maps place yellow (and cyan)
//!   — the *lightest* colors — in the middle of the range, so the eye cannot
//!   rank two rendered values without consulting the colorbar.
//!
//! These maps remain ubiquitous, so rizzma ships them for compatibility — but
//! only behind this module and the `misleading:` registry prefix
//! ([`colormap("misleading:jet")`](crate::core::color::colormap) resolves;
//! `colormap("jet")` does not), so choosing one is always an explicit,
//! visible act. If you want the qualities these maps are chosen for, the
//! honest equivalents are [`cet_l03`](super::cmap::cet_l03) ("fire") in place
//! of `hot`, [`cet_r2`](super::cmap::cet_r2) in place of `jet`/`rainbow`, and
//! the cyclic [`cet_c1`](super::cmap::cet_c1)/[`cet_c2`](super::cmap::cet_c2)
//! in place of `hsv`.
//!
//! Reference: Peter Kovesi. *Good Colour Maps: How to Design Them.*
//! [arXiv:1509.03700 \[cs.GR\] 2015](https://arxiv.org/abs/1509.03700).
//! Segment data matches matplotlib's `_cm.py` renditions of the MATLAB
//! originals.

use super::cmap::LinearSegmentedColormap;

/// MATLAB's `jet`: blue-cyan-yellow-red rainbow.
///
/// The map Kovesi's test image indicts most thoroughly: perceptual flat spots
/// across cyan-green-yellow, false anomalies at cyan, yellow, and red, and a
/// lightness profile that rises and falls so the colors have no perceptual
/// ordering.
#[must_use]
pub fn jet() -> LinearSegmentedColormap {
    let red = [
        (0.0, 0.0, 0.0),
        (0.35, 0.0, 0.0),
        (0.66, 1.0, 1.0),
        (0.89, 1.0, 1.0),
        (1.0, 0.5, 0.5),
    ];
    let green = [
        (0.0, 0.0, 0.0),
        (0.125, 0.0, 0.0),
        (0.375, 1.0, 1.0),
        (0.64, 1.0, 1.0),
        (0.91, 0.0, 0.0),
        (1.0, 0.0, 0.0),
    ];
    let blue = [
        (0.0, 0.5, 0.5),
        (0.11, 1.0, 1.0),
        (0.34, 1.0, 1.0),
        (0.65, 0.0, 0.0),
        (1.0, 0.0, 0.0),
    ];
    LinearSegmentedColormap::new(&red, &green, &blue)
}

/// MATLAB's `hot`: black-red-yellow-white built from staggered RGB ramps.
///
/// Kovesi uses this map (his Fig. 6) to demonstrate perceptual contrast
/// equalization: its lightness gradient collapses where the red and green
/// channels saturate, producing flat spots and false edges. The equalized
/// alternative is [`cet_l03`](super::cmap::cet_l03).
#[must_use]
pub fn hot() -> LinearSegmentedColormap {
    let red = [(0.0, 0.0416, 0.0416), (0.365079, 1.0, 1.0), (1.0, 1.0, 1.0)];
    let green = [
        (0.0, 0.0, 0.0),
        (0.365079, 0.0, 0.0),
        (0.746032, 1.0, 1.0),
        (1.0, 1.0, 1.0),
    ];
    let blue = [(0.0, 0.0, 0.0), (0.746032, 0.0, 0.0), (1.0, 1.0, 1.0)];
    LinearSegmentedColormap::new(&red, &green, &blue)
}

/// The fully saturated HSV hue circle.
///
/// Nominally cyclic, but its perceptual contrast is wildly uneven (Kovesi's
/// Fig. 13): the light secondary colors — cyan, yellow, magenta — generate
/// false anomalies, and the circle is partitioned into red/green/blue thirds
/// that correspond to nothing in typical orientation or phase data. The
/// designed cyclic maps [`cet_c1`](super::cmap::cet_c1) and
/// [`cet_c2`](super::cmap::cet_c2) are the replacements.
#[must_use]
pub fn hsv() -> LinearSegmentedColormap {
    let red = [
        (0.0, 1.0, 1.0),
        (0.158730, 1.0, 1.0),
        (0.174603, 0.96875, 0.96875),
        (0.333333, 0.03125, 0.03125),
        (0.349206, 0.0, 0.0),
        (0.666667, 0.0, 0.0),
        (0.682540, 0.03125, 0.03125),
        (0.841270, 0.96875, 0.96875),
        (0.857143, 1.0, 1.0),
        (1.0, 1.0, 1.0),
    ];
    let green = [
        (0.0, 0.0, 0.0),
        (0.158730, 0.9375, 0.9375),
        (0.174603, 1.0, 1.0),
        (0.507937, 1.0, 1.0),
        (0.666667, 0.0625, 0.0625),
        (0.682540, 0.0, 0.0),
        (1.0, 0.0, 0.0),
    ];
    let blue = [
        (0.0, 0.0, 0.0),
        (0.333333, 0.0, 0.0),
        (0.349206, 0.0625, 0.0625),
        (0.507937, 1.0, 1.0),
        (0.841270, 1.0, 1.0),
        (0.857143, 0.9375, 0.9375),
        (1.0, 0.09375, 0.09375),
    ];
    LinearSegmentedColormap::new(&red, &green, &blue)
}

/// The purple-to-red spectral `rainbow` map.
///
/// A smooth analytic rainbow (r = |2t - ½|, g = sin πt, b = cos πt/2) with
/// the same structural defect as every rainbow: lightness peaks mid-map, so
/// values on either side of the peak are perceptually unordered, and the
/// near-isoluminant green-yellow span hides features. Kovesi's equalized
/// rainbow is [`cet_r2`](super::cmap::cet_r2).
#[must_use]
pub fn rainbow() -> LinearSegmentedColormap {
    let table: Vec<[f64; 3]> = (0..256)
        .map(|i| {
            let t = f64::from(i) / 255.0;
            [
                (2.0 * t - 0.5).abs().clamp(0.0, 1.0),
                (t * std::f64::consts::PI).sin(),
                (t * std::f64::consts::PI / 2.0).cos(),
            ]
        })
        .collect();
    LinearSegmentedColormap::from_rgb_table(&table)
}

/// The vendor maps by bare name (`"jet"`, `"hot"`, `"hsv"`, `"rainbow"`).
///
/// This is the resolver behind the registry's `misleading:` prefix; the
/// prefix — not this function — is the supported spelling.
#[must_use]
pub(super) fn colormap(name: &str) -> Option<LinearSegmentedColormap> {
    match name {
        "jet" => Some(jet()),
        "hot" => Some(hot()),
        "hsv" => Some(hsv()),
        "rainbow" => Some(rainbow()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color::{Colormap, colormap as registry};

    #[test]
    fn bare_vendor_names_do_not_resolve() {
        for name in ["jet", "hot", "hsv", "rainbow"] {
            assert!(
                registry(name).is_none(),
                "{name} must require the misleading: prefix"
            );
        }
    }

    #[test]
    fn prefixed_vendor_names_resolve_with_reversals() {
        for name in ["jet", "hot", "hsv", "rainbow"] {
            let prefixed = format!("misleading:{name}");
            assert!(registry(&prefixed).is_some(), "{prefixed} must resolve");
            let reversed = format!("misleading:{name}_r");
            assert!(registry(&reversed).is_some(), "{reversed} must resolve");
        }
        assert!(registry("misleading:nope").is_none());
    }

    #[test]
    fn jet_endpoints_and_midpoint() {
        let cm = jet();
        // Dark blue -> saturated cyan-adjacent middle -> dark red.
        let lo = cm.sample(0.0);
        assert!(lo.b > 0.4 && lo.r == 0.0);
        let hi = cm.sample(1.0);
        assert!(hi.r > 0.4 && hi.b == 0.0);
        // The infamous bright middle: green fully saturated.
        let mid = cm.sample(0.5);
        assert!(mid.g > 0.95);
    }

    #[test]
    fn hot_runs_black_to_white() {
        let cm = hot();
        let lo = cm.sample(0.0);
        assert!(lo.r < 0.05 && lo.g == 0.0 && lo.b == 0.0);
        let hi = cm.sample(1.0);
        assert!(hi.r > 0.99 && hi.g > 0.99 && hi.b > 0.99);
    }

    #[test]
    fn hsv_is_cyclic_in_hue() {
        let cm = hsv();
        // Both ends are red-dominant (the hue circle closes).
        let (lo, hi) = (cm.sample(0.0), cm.sample(1.0));
        assert!(lo.r > 0.99 && lo.g < 0.05);
        assert!(hi.r > 0.99 && hi.g < 0.05);
    }

    #[test]
    fn rainbow_runs_violet_to_red() {
        let cm = rainbow();
        let lo = cm.sample(0.0);
        assert!(lo.b > 0.99 && lo.r > 0.4 && lo.g < 0.05, "violet start");
        let hi = cm.sample(1.0);
        assert!(hi.r > 0.99 && hi.g < 0.05 && hi.b < 0.05, "red end");
    }
}
