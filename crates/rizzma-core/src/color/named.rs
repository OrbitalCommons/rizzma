//! Named colors and the [`to_rgba`] color-spec parser.
//!
//! Mirrors matplotlib's color name tables and `matplotlib.colors.to_rgba`.
//! Three name families are supported: the single-letter base colors
//! (`b g r c m y k w`), the Tableau `tab:` set, and the ~148-entry CSS4
//! named-color dictionary. In addition to names, [`to_rgba`] understands
//! `#`-hex strings, grayscale float strings such as `"0.5"`, the literal
//! `"none"` (fully transparent), and the property-cycle references
//! `"C0"..="C9"`.

use super::Rgba;

/// The base single-letter colors (`b g r c m y k w`).
///
/// Values mirror matplotlib's `BASE_COLORS` table.
pub const BASE_COLORS: &[(&str, Rgba)] = &[
    ("b", Rgba::new(0.0, 0.0, 1.0, 1.0)),
    ("g", Rgba::new(0.0, 0.5, 0.0, 1.0)),
    ("r", Rgba::new(1.0, 0.0, 0.0, 1.0)),
    ("c", Rgba::new(0.0, 0.75, 0.75, 1.0)),
    ("m", Rgba::new(0.75, 0.0, 0.75, 1.0)),
    ("y", Rgba::new(0.75, 0.75, 0.0, 1.0)),
    ("k", Rgba::new(0.0, 0.0, 0.0, 1.0)),
    ("w", Rgba::new(1.0, 1.0, 1.0, 1.0)),
];

/// The Tableau `tab:` colors, as `(name, hex)` pairs.
///
/// Values mirror matplotlib's `TABLEAU_COLORS` table; the hex strings are the
/// same ten colors that make up the default property cycle.
pub const TABLEAU_COLORS: &[(&str, &str)] = &[
    ("tab:blue", "#1f77b4"),
    ("tab:orange", "#ff7f0e"),
    ("tab:green", "#2ca02c"),
    ("tab:red", "#d62728"),
    ("tab:purple", "#9467bd"),
    ("tab:brown", "#8c564b"),
    ("tab:pink", "#e377c2"),
    ("tab:gray", "#7f7f7f"),
    ("tab:olive", "#bcbd22"),
    ("tab:cyan", "#17becf"),
];

/// The default ten-color property cycle (matplotlib's `tab10`).
///
/// [`to_rgba`] resolves the `"C0".."C9"` cycle references against this list,
/// indexing modulo its length.
pub const DEFAULT_COLOR_CYCLE: &[&str] = &[
    "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
    "#bcbd22", "#17becf",
];

/// The CSS4 named colors, as `(name, hex)` pairs.
///
/// Copied verbatim from matplotlib's `CSS4_COLORS` dictionary.
pub const CSS4_COLORS: &[(&str, &str)] = &[
    ("aliceblue", "#F0F8FF"),
    ("antiquewhite", "#FAEBD7"),
    ("aqua", "#00FFFF"),
    ("aquamarine", "#7FFFD4"),
    ("azure", "#F0FFFF"),
    ("beige", "#F5F5DC"),
    ("bisque", "#FFE4C4"),
    ("black", "#000000"),
    ("blanchedalmond", "#FFEBCD"),
    ("blue", "#0000FF"),
    ("blueviolet", "#8A2BE2"),
    ("brown", "#A52A2A"),
    ("burlywood", "#DEB887"),
    ("cadetblue", "#5F9EA0"),
    ("chartreuse", "#7FFF00"),
    ("chocolate", "#D2691E"),
    ("coral", "#FF7F50"),
    ("cornflowerblue", "#6495ED"),
    ("cornsilk", "#FFF8DC"),
    ("crimson", "#DC143C"),
    ("cyan", "#00FFFF"),
    ("darkblue", "#00008B"),
    ("darkcyan", "#008B8B"),
    ("darkgoldenrod", "#B8860B"),
    ("darkgray", "#A9A9A9"),
    ("darkgreen", "#006400"),
    ("darkgrey", "#A9A9A9"),
    ("darkkhaki", "#BDB76B"),
    ("darkmagenta", "#8B008B"),
    ("darkolivegreen", "#556B2F"),
    ("darkorange", "#FF8C00"),
    ("darkorchid", "#9932CC"),
    ("darkred", "#8B0000"),
    ("darksalmon", "#E9967A"),
    ("darkseagreen", "#8FBC8F"),
    ("darkslateblue", "#483D8B"),
    ("darkslategray", "#2F4F4F"),
    ("darkslategrey", "#2F4F4F"),
    ("darkturquoise", "#00CED1"),
    ("darkviolet", "#9400D3"),
    ("deeppink", "#FF1493"),
    ("deepskyblue", "#00BFFF"),
    ("dimgray", "#696969"),
    ("dimgrey", "#696969"),
    ("dodgerblue", "#1E90FF"),
    ("firebrick", "#B22222"),
    ("floralwhite", "#FFFAF0"),
    ("forestgreen", "#228B22"),
    ("fuchsia", "#FF00FF"),
    ("gainsboro", "#DCDCDC"),
    ("ghostwhite", "#F8F8FF"),
    ("gold", "#FFD700"),
    ("goldenrod", "#DAA520"),
    ("gray", "#808080"),
    ("green", "#008000"),
    ("greenyellow", "#ADFF2F"),
    ("grey", "#808080"),
    ("honeydew", "#F0FFF0"),
    ("hotpink", "#FF69B4"),
    ("indianred", "#CD5C5C"),
    ("indigo", "#4B0082"),
    ("ivory", "#FFFFF0"),
    ("khaki", "#F0E68C"),
    ("lavender", "#E6E6FA"),
    ("lavenderblush", "#FFF0F5"),
    ("lawngreen", "#7CFC00"),
    ("lemonchiffon", "#FFFACD"),
    ("lightblue", "#ADD8E6"),
    ("lightcoral", "#F08080"),
    ("lightcyan", "#E0FFFF"),
    ("lightgoldenrodyellow", "#FAFAD2"),
    ("lightgray", "#D3D3D3"),
    ("lightgreen", "#90EE90"),
    ("lightgrey", "#D3D3D3"),
    ("lightpink", "#FFB6C1"),
    ("lightsalmon", "#FFA07A"),
    ("lightseagreen", "#20B2AA"),
    ("lightskyblue", "#87CEFA"),
    ("lightslategray", "#778899"),
    ("lightslategrey", "#778899"),
    ("lightsteelblue", "#B0C4DE"),
    ("lightyellow", "#FFFFE0"),
    ("lime", "#00FF00"),
    ("limegreen", "#32CD32"),
    ("linen", "#FAF0E6"),
    ("magenta", "#FF00FF"),
    ("maroon", "#800000"),
    ("mediumaquamarine", "#66CDAA"),
    ("mediumblue", "#0000CD"),
    ("mediumorchid", "#BA55D3"),
    ("mediumpurple", "#9370DB"),
    ("mediumseagreen", "#3CB371"),
    ("mediumslateblue", "#7B68EE"),
    ("mediumspringgreen", "#00FA9A"),
    ("mediumturquoise", "#48D1CC"),
    ("mediumvioletred", "#C71585"),
    ("midnightblue", "#191970"),
    ("mintcream", "#F5FFFA"),
    ("mistyrose", "#FFE4E1"),
    ("moccasin", "#FFE4B5"),
    ("navajowhite", "#FFDEAD"),
    ("navy", "#000080"),
    ("oldlace", "#FDF5E6"),
    ("olive", "#808000"),
    ("olivedrab", "#6B8E23"),
    ("orange", "#FFA500"),
    ("orangered", "#FF4500"),
    ("orchid", "#DA70D6"),
    ("palegoldenrod", "#EEE8AA"),
    ("palegreen", "#98FB98"),
    ("paleturquoise", "#AFEEEE"),
    ("palevioletred", "#DB7093"),
    ("papayawhip", "#FFEFD5"),
    ("peachpuff", "#FFDAB9"),
    ("peru", "#CD853F"),
    ("pink", "#FFC0CB"),
    ("plum", "#DDA0DD"),
    ("powderblue", "#B0E0E6"),
    ("purple", "#800080"),
    ("rebeccapurple", "#663399"),
    ("red", "#FF0000"),
    ("rosybrown", "#BC8F8F"),
    ("royalblue", "#4169E1"),
    ("saddlebrown", "#8B4513"),
    ("salmon", "#FA8072"),
    ("sandybrown", "#F4A460"),
    ("seagreen", "#2E8B57"),
    ("seashell", "#FFF5EE"),
    ("sienna", "#A0522D"),
    ("silver", "#C0C0C0"),
    ("skyblue", "#87CEEB"),
    ("slateblue", "#6A5ACD"),
    ("slategray", "#708090"),
    ("slategrey", "#708090"),
    ("snow", "#FFFAFA"),
    ("springgreen", "#00FF7F"),
    ("steelblue", "#4682B4"),
    ("tan", "#D2B48C"),
    ("teal", "#008080"),
    ("thistle", "#D8BFD8"),
    ("tomato", "#FF6347"),
    ("turquoise", "#40E0D0"),
    ("violet", "#EE82EE"),
    ("wheat", "#F5DEB3"),
    ("white", "#FFFFFF"),
    ("whitesmoke", "#F5F5F5"),
    ("yellow", "#FFFF00"),
    ("yellowgreen", "#9ACD32"),
];

/// Look up a base single-letter color by name (e.g. `"r"`).
fn lookup_base(name: &str) -> Option<Rgba> {
    BASE_COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

/// Look up a Tableau or CSS4 named color, returning its `Rgba` value.
fn lookup_named(name: &str) -> Option<Rgba> {
    let hex = TABLEAU_COLORS
        .iter()
        .chain(CSS4_COLORS.iter())
        .find(|(n, _)| *n == name)
        .map(|(_, h)| *h)?;
    Rgba::from_hex(hex)
}

/// Resolve a `"C0".."C9"`-style property-cycle reference.
fn lookup_cycle(spec: &str) -> Option<Rgba> {
    let idx = spec.strip_prefix('C')?;
    // Must be all ASCII digits (matplotlib accepts only digit suffixes here).
    if idx.is_empty() || !idx.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: usize = idx.parse().ok()?;
    let hex = DEFAULT_COLOR_CYCLE[n % DEFAULT_COLOR_CYCLE.len()];
    Rgba::from_hex(hex)
}

/// Convert a color specification to an [`Rgba`].
///
/// Mirrors `matplotlib.colors.to_rgba`. The following spec forms are
/// recognized:
///
/// - `#`-hex strings in any length accepted by [`Rgba::from_hex`].
/// - The literal `"none"` (case-insensitive), which always maps to fully
///   transparent black, ignoring `alpha`.
/// - Base single-letter colors (`b g r c m y k w`).
/// - Tableau `tab:*` and CSS4 named colors (case-insensitive).
/// - Grayscale float strings in `0.0..=1.0`, e.g. `"0.5"`, mapping to an
///   opaque gray.
/// - Property-cycle references `"C0".."C9"`, resolved against
///   [`DEFAULT_COLOR_CYCLE`].
///
/// If `alpha` is `Some`, it overrides the alpha channel of the resolved color
/// (except for `"none"`). Returns [`None`] if `spec` is not a recognized form,
/// or if `alpha` is outside `0.0..=1.0`.
#[must_use]
pub fn to_rgba(spec: &str, alpha: Option<f64>) -> Option<Rgba> {
    if let Some(a) = alpha
        && !(0.0..=1.0).contains(&a)
    {
        return None;
    }
    // "none" is always fully transparent and ignores alpha.
    if spec.eq_ignore_ascii_case("none") {
        return Some(Rgba::TRANSPARENT);
    }

    let base = resolve_spec(spec)?;
    Some(match alpha {
        Some(a) => base.with_alpha(a),
        None => base,
    })
}

/// Resolve a spec to its `Rgba` ignoring the `alpha` override.
fn resolve_spec(spec: &str) -> Option<Rgba> {
    if spec.starts_with('#') {
        return Rgba::from_hex(spec);
    }
    if let Some(c) = lookup_base(spec) {
        return Some(c);
    }
    if let Some(c) = lookup_named(spec) {
        return Some(c);
    }
    // Named colors are matched case-insensitively as a fallback.
    let lower = spec.to_ascii_lowercase();
    if lower != spec
        && let Some(c) = lookup_named(&lower)
    {
        return Some(c);
    }
    if let Some(c) = lookup_cycle(spec) {
        return Some(c);
    }
    // Grayscale float string such as "0.5".
    if let Ok(v) = spec.parse::<f64>()
        && (0.0..=1.0).contains(&v)
    {
        return Some(Rgba::rgb(v, v, v));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_form() {
        assert_eq!(to_rgba("#ff0000", None), Some(Rgba::RED));
        assert_eq!(to_rgba("#abc", None), Rgba::from_hex("#aabbcc"));
    }

    #[test]
    fn base_colors() {
        assert_eq!(to_rgba("r", None), Some(Rgba::new(1.0, 0.0, 0.0, 1.0)));
        assert_eq!(to_rgba("g", None), Some(Rgba::new(0.0, 0.5, 0.0, 1.0)));
        assert_eq!(to_rgba("k", None), Some(Rgba::BLACK));
        assert_eq!(to_rgba("w", None), Some(Rgba::WHITE));
    }

    #[test]
    fn tableau_colors() {
        assert_eq!(to_rgba("tab:blue", None), Rgba::from_hex("#1f77b4"));
        assert_eq!(to_rgba("tab:cyan", None), Rgba::from_hex("#17becf"));
    }

    #[test]
    fn css4_colors() {
        assert_eq!(to_rgba("rebeccapurple", None), Rgba::from_hex("#663399"));
        // Case-insensitive.
        assert_eq!(to_rgba("RebeccaPurple", None), Rgba::from_hex("#663399"));
        assert_eq!(to_rgba("white", None), Some(Rgba::WHITE));
    }

    #[test]
    fn grayscale_string() {
        assert_eq!(to_rgba("0.5", None), Some(Rgba::rgb(0.5, 0.5, 0.5)));
        assert_eq!(to_rgba("0", None), Some(Rgba::rgb(0.0, 0.0, 0.0)));
        assert_eq!(to_rgba("1", None), Some(Rgba::rgb(1.0, 1.0, 1.0)));
        // Out of range -> None.
        assert_eq!(to_rgba("1.5", None), None);
    }

    #[test]
    fn cycle_reference() {
        assert_eq!(to_rgba("C0", None), Rgba::from_hex("#1f77b4"));
        assert_eq!(to_rgba("C9", None), Rgba::from_hex("#17becf"));
        // Wraps modulo the cycle length.
        assert_eq!(to_rgba("C10", None), to_rgba("C0", None));
    }

    #[test]
    fn alpha_override() {
        let c = to_rgba("r", Some(0.25)).unwrap();
        assert_eq!(c, Rgba::new(1.0, 0.0, 0.0, 0.25));
        // Out-of-range alpha rejected.
        assert_eq!(to_rgba("r", Some(2.0)), None);
    }

    #[test]
    fn none_is_transparent() {
        assert_eq!(to_rgba("none", None), Some(Rgba::TRANSPARENT));
        // alpha is ignored for "none".
        assert_eq!(to_rgba("none", Some(1.0)), Some(Rgba::TRANSPARENT));
        assert_eq!(to_rgba("NONE", None), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn unknown_spec() {
        assert_eq!(to_rgba("not-a-color", None), None);
        assert_eq!(to_rgba("", None), None);
    }

    #[test]
    fn css4_table_is_complete() {
        assert_eq!(CSS4_COLORS.len(), 148);
    }
}
