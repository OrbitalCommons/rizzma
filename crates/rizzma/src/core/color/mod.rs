//! Colors, colormaps, and normalization.
//!
//! [`Rgba`] stores straight (non-premultiplied) red, green, blue, and alpha
//! channels as `f64` in the range `0.0..=1.0`, mirroring matplotlib's internal
//! RGBA tuples. Conversions to and from 8-bit channels and CSS-style hex strings
//! are provided.
//!
//! The submodules add the higher-level color pipeline mirrored from
//! `matplotlib.colors`: named-color parsing ([`to_rgba`]), data normalization
//! ([`Normalize`] and friends), and colormaps ([`Colormap`] and friends). The
//! [`to_rgba_array`] helper ties a [`Normalize`] and a [`Colormap`] together
//! into the data-to-color pipeline used by scatter/imshow.

mod cet_data;
pub mod cmap;
mod cmap_data;
pub mod misleading;
pub mod named;
pub mod norm;

pub use cmap::{
    Colormap, DEFAULT_COLORMAP, LinearSegmentedColormap, ListedColormap, SegmentPoint,
    VIRIDIS_DATA, cet_c1, cet_c2, cet_c3, cet_c5, cet_d01, cet_d04, cet_d07, cet_d11, cet_i1,
    cet_l01, cet_l03, cet_l05, cet_l09, cet_l10, cet_r2, cividis, colormap, coolwarm,
    default_colormap, gray, inferno, magma, plasma, rdbu, viridis,
};
pub use named::{BASE_COLORS, CSS4_COLORS, DEFAULT_COLOR_CYCLE, TABLEAU_COLORS, to_rgba};
pub use norm::{BoundaryNorm, LinearNorm, LogNorm, Normalize, PowerNorm};

/// Map a data array to colors through a [`Normalize`] then a [`Colormap`].
///
/// This is the scatter/imshow color pipeline (matplotlib's `ScalarMappable`):
/// each datum is normalized to `[0, 1]` and the result sampled from `cmap`.
#[must_use]
pub fn to_rgba_array(data: &[f64], norm: &dyn Normalize, cmap: &dyn Colormap) -> Vec<Rgba> {
    data.iter()
        .map(|&v| cmap.sample(norm.normalize(v)))
        .collect()
}

/// An RGBA color with `f64` channels in the range `0.0..=1.0`.
///
/// Channels are stored straight (not premultiplied by alpha). Values outside the
/// `0.0..=1.0` range are accepted by the constructors but clamped when converting
/// to 8-bit representations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red channel, `0.0..=1.0`.
    pub r: f64,
    /// Green channel, `0.0..=1.0`.
    pub g: f64,
    /// Blue channel, `0.0..=1.0`.
    pub b: f64,
    /// Alpha (opacity) channel, `0.0..=1.0`.
    pub a: f64,
}

impl Rgba {
    /// Opaque black (`0, 0, 0, 1`).
    pub const BLACK: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque white (`1, 1, 1, 1`).
    pub const WHITE: Rgba = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Fully transparent (`0, 0, 0, 0`).
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    /// Opaque red (`1, 0, 0, 1`).
    pub const RED: Rgba = Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque green (`0, 1, 0, 1`).
    pub const GREEN: Rgba = Rgba {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque blue (`0, 0, 1, 1`).
    pub const BLUE: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    /// Construct an [`Rgba`] from straight `r`, `g`, `b`, `a` channels.
    #[must_use]
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    /// Construct an opaque [`Rgba`] (`a = 1.0`) from `r`, `g`, `b` channels.
    #[must_use]
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Construct an [`Rgba`] from 8-bit channels, mapping `0..=255` to `0.0..=1.0`.
    #[must_use]
    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
            a: f64::from(a) / 255.0,
        }
    }

    /// Convert to 8-bit channels, clamping to `0.0..=1.0` and rounding to nearest.
    #[must_use]
    pub fn to_u8_array(&self) -> [u8; 4] {
        [
            channel_to_u8(self.r),
            channel_to_u8(self.g),
            channel_to_u8(self.b),
            channel_to_u8(self.a),
        ]
    }

    /// Parse a CSS-style hex color string.
    ///
    /// Accepts a leading `#` followed by 3, 4, 6, or 8 hexadecimal digits:
    ///
    /// - `#rgb` and `#rrggbb` produce an opaque color (`a = 1.0`).
    /// - `#rgba` and `#rrggbbaa` include an explicit alpha channel.
    ///
    /// The short forms (`#rgb`, `#rgba`) expand each digit by duplication, so
    /// `#abc` is equivalent to `#aabbcc`. Returns [`None`] if the string is not a
    /// valid hex color of one of these lengths.
    #[must_use]
    pub fn from_hex(s: &str) -> Option<Rgba> {
        let hex = s.strip_prefix('#')?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let nib = |c: u8| u8::from_str_radix(&(c as char).to_string(), 16).ok();
        match hex.len() {
            3 | 4 => {
                let bytes = hex.as_bytes();
                let r = nib(bytes[0])?;
                let g = nib(bytes[1])?;
                let b = nib(bytes[2])?;
                let a = if hex.len() == 4 { nib(bytes[3])? } else { 0xF };
                // Expand each nibble by duplication: 0xA -> 0xAA.
                Some(Rgba::from_u8(r * 17, g * 17, b * 17, a * 17))
            }
            6 | 8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = if hex.len() == 8 {
                    u8::from_str_radix(&hex[6..8], 16).ok()?
                } else {
                    0xFF
                };
                Some(Rgba::from_u8(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Return a copy of this color with the alpha channel replaced by `alpha`.
    #[must_use]
    pub const fn with_alpha(&self, alpha: f64) -> Rgba {
        Rgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a: alpha,
        }
    }

    /// Format this color as an `#rrggbbaa` hex string.
    ///
    /// Channels are clamped to `0.0..=1.0` and rounded to 8 bits, matching
    /// [`Rgba::to_u8_array`]. The alpha channel is always included, so the
    /// result round-trips through [`Rgba::from_hex`].
    #[must_use]
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.to_u8_array();
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

impl serde::Serialize for Rgba {
    /// Serialize as an `#rrggbbaa` hex string (see [`Rgba::to_hex`]).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Rgba {
    /// Deserialize from any hex form accepted by [`Rgba::from_hex`].
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        Rgba::from_hex(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid hex color: {s:?}")))
    }
}

/// Clamp a `0.0..=1.0` channel and round it to the nearest 8-bit value.
fn channel_to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors() {
        assert_eq!(Rgba::rgb(0.1, 0.2, 0.3), Rgba::new(0.1, 0.2, 0.3, 1.0));
        assert_eq!(Rgba::BLACK, Rgba::rgb(0.0, 0.0, 0.0));
        assert_eq!(Rgba::WHITE, Rgba::rgb(1.0, 1.0, 1.0));
        assert_eq!(Rgba::TRANSPARENT, Rgba::new(0.0, 0.0, 0.0, 0.0));
        assert_eq!(Rgba::RED, Rgba::rgb(1.0, 0.0, 0.0));
        assert_eq!(Rgba::GREEN, Rgba::rgb(0.0, 1.0, 0.0));
        assert_eq!(Rgba::BLUE, Rgba::rgb(0.0, 0.0, 1.0));
    }

    #[test]
    fn u8_roundtrip() {
        let c = Rgba::from_u8(12, 34, 56, 78);
        assert_eq!(c.to_u8_array(), [12, 34, 56, 78]);
        assert_eq!(Rgba::WHITE.to_u8_array(), [255, 255, 255, 255]);
        assert_eq!(Rgba::TRANSPARENT.to_u8_array(), [0, 0, 0, 0]);
    }

    #[test]
    fn u8_rounds_to_nearest() {
        // 0.5 * 255 = 127.5 -> 128.
        assert_eq!(Rgba::new(0.5, 0.5, 0.5, 0.5).to_u8_array(), [128; 4]);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(
            Rgba::new(-1.0, 2.0, 0.5, 10.0).to_u8_array(),
            [0, 255, 128, 255]
        );
    }

    #[test]
    fn hex_long_opaque() {
        assert_eq!(Rgba::from_hex("#ff0000"), Some(Rgba::RED));
        assert_eq!(Rgba::from_hex("#00ff00"), Some(Rgba::GREEN));
        assert_eq!(Rgba::from_hex("#0000ff"), Some(Rgba::BLUE));
    }

    #[test]
    fn hex_with_alpha() {
        assert_eq!(Rgba::from_hex("#00000000"), Some(Rgba::TRANSPARENT));
        let c = Rgba::from_hex("#11223380").unwrap();
        assert_eq!(c.to_u8_array(), [0x11, 0x22, 0x33, 0x80]);
    }

    #[test]
    fn hex_short_forms_expand() {
        assert_eq!(Rgba::from_hex("#abc"), Rgba::from_hex("#aabbcc"));
        assert_eq!(Rgba::from_hex("#abcd"), Rgba::from_hex("#aabbccdd"));
        assert_eq!(Rgba::from_hex("#fff"), Some(Rgba::WHITE));
    }

    #[test]
    fn hex_roundtrip_via_u8() {
        let original = Rgba::from_u8(0x12, 0x34, 0x56, 0x78);
        let [r, g, b, a] = original.to_u8_array();
        let hex = format!("#{r:02x}{g:02x}{b:02x}{a:02x}");
        assert_eq!(Rgba::from_hex(&hex), Some(original));
    }

    #[test]
    fn hex_invalid() {
        assert_eq!(Rgba::from_hex("ff0000"), None);
        assert_eq!(Rgba::from_hex("#ff"), None);
        assert_eq!(Rgba::from_hex("#gggggg"), None);
        assert_eq!(Rgba::from_hex("#12345"), None);
        assert_eq!(Rgba::from_hex("#1234567"), None);
        assert_eq!(Rgba::from_hex("#"), None);
    }

    #[test]
    fn with_alpha_replaces_channel() {
        assert_eq!(Rgba::RED.with_alpha(0.5), Rgba::new(1.0, 0.0, 0.0, 0.5));
    }

    #[test]
    fn to_rgba_array_pipeline() {
        let data = [0.0, 0.5, 1.0];
        let norm = LinearNorm::new(0.0, 1.0);
        let cmap = gray();
        let colors = to_rgba_array(&data, &norm, &cmap);
        assert_eq!(colors.len(), 3);
        assert_eq!(colors[0], Rgba::BLACK);
        assert_eq!(colors[2], Rgba::WHITE);
        // Midpoint of a gray ramp is mid-gray (within one LUT step).
        assert!((colors[1].r - 0.5).abs() < 1.0 / 255.0);
    }
}
