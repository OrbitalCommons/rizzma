//! Typed runtime configuration ([`RcParams`]), mirroring matplotlib's `rcParams`.
//!
//! matplotlib exposes ~300 string-keyed configuration entries. This module
//! captures a curated, strongly-typed subset of the most useful ones, grouped
//! by the area they configure (figure, axes, lines, ticks, font, and grid).
//! Field names follow matplotlib's dotted keys with dots replaced by
//! underscores (e.g. `figure.figsize` becomes [`RcParams::figure_figsize`]).
//!
//! [`RcParams::default`] (aliased as [`RcParams::matplotlib`]) returns
//! matplotlib's documented default values. The struct (de)serializes via serde,
//! so a configuration can be round-tripped through JSON or any other serde
//! format.

use crate::color::{DEFAULT_COLOR_CYCLE, Rgba};

/// The direction tick marks point relative to the axes.
///
/// Mirrors matplotlib's `xtick.direction` / `ytick.direction` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TickDirection {
    /// Ticks point outward, away from the plotting area (matplotlib's default).
    Out,
    /// Ticks point inward, into the plotting area.
    In,
    /// Ticks straddle the axis spine, pointing both ways.
    InOut,
}

/// Build the default ten-color property cycle (matplotlib's `tab10`).
///
/// Resolves the hex strings in [`DEFAULT_COLOR_CYCLE`] into [`Rgba`] values.
#[must_use]
fn default_prop_cycle() -> Vec<Rgba> {
    DEFAULT_COLOR_CYCLE
        .iter()
        .map(|hex| Rgba::from_hex(hex).expect("DEFAULT_COLOR_CYCLE entries are valid hex"))
        .collect()
}

/// A typed subset of matplotlib's `rcParams` runtime configuration.
///
/// Fields are grouped by the area they configure. Construct the matplotlib
/// defaults with [`RcParams::default`] or its alias [`RcParams::matplotlib`],
/// and override individual fields as needed. Use [`RcParams::merge`] to layer a
/// set of overrides on top of an existing configuration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RcParams {
    // --- figure ---
    /// Default figure size in inches, as `(width, height)` (`figure.figsize`).
    pub figure_figsize: (f64, f64),
    /// Figure resolution in dots per inch (`figure.dpi`).
    pub figure_dpi: f64,
    /// Figure background color (`figure.facecolor`).
    pub figure_facecolor: Rgba,
    /// Resolution in dots per inch used when saving figures (`savefig.dpi`).
    pub savefig_dpi: f64,

    // --- axes ---
    /// Axes background color (`axes.facecolor`).
    pub axes_facecolor: Rgba,
    /// Color of the axes spines (`axes.edgecolor`).
    pub axes_edgecolor: Rgba,
    /// Width of the axes spines in points (`axes.linewidth`).
    pub axes_linewidth: f64,
    /// Whether to show the grid by default (`axes.grid`).
    pub axes_grid: bool,
    /// Font size of the axes title in points (`axes.titlesize`).
    pub axes_titlesize: f64,
    /// Font size of the x/y axis labels in points (`axes.labelsize`).
    pub axes_labelsize: f64,
    /// The property cycle of colors used for successive plots
    /// (`axes.prop_cycle`); defaults to the ten-color `tab10` cycle.
    pub axes_prop_cycle: Vec<Rgba>,

    // --- lines ---
    /// Default line width in points (`lines.linewidth`).
    pub lines_linewidth: f64,
    /// Default marker size in points (`lines.markersize`).
    pub lines_markersize: f64,

    // --- ticks ---
    /// Font size of x-axis tick labels in points (`xtick.labelsize`).
    pub xtick_labelsize: f64,
    /// Font size of y-axis tick labels in points (`ytick.labelsize`).
    pub ytick_labelsize: f64,
    /// Length of major x-axis ticks in points (`xtick.major.size`).
    pub xtick_major_size: f64,
    /// Length of major y-axis ticks in points (`ytick.major.size`).
    pub ytick_major_size: f64,
    /// Direction x-axis ticks point (`xtick.direction`).
    pub xtick_direction: TickDirection,
    /// Direction y-axis ticks point (`ytick.direction`).
    pub ytick_direction: TickDirection,

    // --- font ---
    /// Default font size in points (`font.size`).
    pub font_size: f64,
    /// Default font family (`font.family`).
    pub font_family: String,

    // --- grid ---
    /// Grid line color (`grid.color`).
    pub grid_color: Rgba,
    /// Grid line width in points (`grid.linewidth`).
    pub grid_linewidth: f64,
    /// Grid line opacity (`grid.alpha`).
    pub grid_alpha: f64,
}

impl Default for RcParams {
    /// matplotlib's documented default `rcParams`.
    fn default() -> Self {
        Self {
            figure_figsize: (6.4, 4.8),
            figure_dpi: 100.0,
            figure_facecolor: Rgba::WHITE,
            savefig_dpi: 100.0,

            axes_facecolor: Rgba::WHITE,
            axes_edgecolor: Rgba::BLACK,
            axes_linewidth: 0.8,
            axes_grid: false,
            axes_titlesize: 12.0,
            axes_labelsize: 10.0,
            axes_prop_cycle: default_prop_cycle(),

            lines_linewidth: 1.5,
            lines_markersize: 6.0,

            xtick_labelsize: 10.0,
            ytick_labelsize: 10.0,
            xtick_major_size: 3.5,
            ytick_major_size: 3.5,
            xtick_direction: TickDirection::Out,
            ytick_direction: TickDirection::Out,

            font_size: 10.0,
            font_family: "DejaVu Sans".to_string(),

            // ~0.85 gray; built from 8-bit channels so it round-trips through
            // the hex serialization of `Rgba` exactly.
            grid_color: Rgba::from_u8(217, 217, 217, 255),
            grid_linewidth: 0.8,
            grid_alpha: 1.0,
        }
    }
}

impl RcParams {
    /// matplotlib's default configuration; an alias for [`RcParams::default`].
    #[must_use]
    pub fn matplotlib() -> Self {
        Self::default()
    }

    /// A dark-theme preset: dark figure and axes backgrounds with light
    /// foreground colors. Otherwise identical to [`RcParams::default`].
    #[must_use]
    pub fn dark() -> Self {
        // 8-bit-derived so the preset round-trips through hex serialization.
        let dark = Rgba::from_u8(31, 31, 31, 255);
        let light = Rgba::from_u8(230, 230, 230, 255);
        Self {
            figure_facecolor: dark,
            axes_facecolor: dark,
            axes_edgecolor: light,
            grid_color: Rgba::from_u8(77, 77, 77, 255),
            ..Self::default()
        }
    }

    /// Resolve the color at index `i` of [`Self::axes_prop_cycle`], wrapping
    /// modulo the cycle length. Returns [`None`] only if the cycle is empty.
    #[must_use]
    pub fn cycle_color(&self, i: usize) -> Option<Rgba> {
        let cycle = &self.axes_prop_cycle;
        if cycle.is_empty() {
            return None;
        }
        Some(cycle[i % cycle.len()])
    }

    /// Layer `overrides` on top of `self`, returning the combined configuration.
    ///
    /// Each field of the result is taken from `overrides` where it differs from
    /// the default, and from `self` otherwise. This makes it easy to express a
    /// sparse set of changes: build them on top of [`RcParams::default`] and
    /// merge them onto a base configuration.
    #[must_use]
    pub fn merge(&self, overrides: &RcParams) -> RcParams {
        let base = RcParams::default();
        // Field-by-field: prefer the override when it diverges from the default.
        let mut out = self.clone();
        macro_rules! merge_field {
            ($field:ident) => {
                if overrides.$field != base.$field {
                    out.$field = overrides.$field.clone();
                }
            };
        }
        merge_field!(figure_figsize);
        merge_field!(figure_dpi);
        merge_field!(figure_facecolor);
        merge_field!(savefig_dpi);
        merge_field!(axes_facecolor);
        merge_field!(axes_edgecolor);
        merge_field!(axes_linewidth);
        merge_field!(axes_grid);
        merge_field!(axes_titlesize);
        merge_field!(axes_labelsize);
        merge_field!(axes_prop_cycle);
        merge_field!(lines_linewidth);
        merge_field!(lines_markersize);
        merge_field!(xtick_labelsize);
        merge_field!(ytick_labelsize);
        merge_field!(xtick_major_size);
        merge_field!(ytick_major_size);
        merge_field!(xtick_direction);
        merge_field!(ytick_direction);
        merge_field!(font_size);
        merge_field!(font_family);
        merge_field!(grid_color);
        merge_field!(grid_linewidth);
        merge_field!(grid_alpha);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_documented_values() {
        let rc = RcParams::default();
        assert_eq!(rc.figure_figsize, (6.4, 4.8));
        assert_eq!(rc.figure_dpi, 100.0);
        assert_eq!(rc.savefig_dpi, 100.0);
        assert_eq!(rc.figure_facecolor, Rgba::WHITE);
        assert_eq!(rc.axes_facecolor, Rgba::WHITE);
        assert_eq!(rc.axes_edgecolor, Rgba::BLACK);
        assert_eq!(rc.axes_linewidth, 0.8);
        assert!(!rc.axes_grid);
        assert_eq!(rc.axes_titlesize, 12.0);
        assert_eq!(rc.axes_labelsize, 10.0);
        assert_eq!(rc.lines_linewidth, 1.5);
        assert_eq!(rc.lines_markersize, 6.0);
        assert_eq!(rc.xtick_labelsize, 10.0);
        assert_eq!(rc.ytick_labelsize, 10.0);
        assert_eq!(rc.xtick_major_size, 3.5);
        assert_eq!(rc.ytick_major_size, 3.5);
        assert_eq!(rc.xtick_direction, TickDirection::Out);
        assert_eq!(rc.font_size, 10.0);
        assert_eq!(rc.font_family, "DejaVu Sans");
        assert_eq!(rc.grid_linewidth, 0.8);
        assert_eq!(rc.grid_alpha, 1.0);
    }

    #[test]
    fn matplotlib_aliases_default() {
        assert_eq!(RcParams::matplotlib(), RcParams::default());
    }

    #[test]
    fn default_prop_cycle_has_ten_colors() {
        let rc = RcParams::default();
        assert_eq!(rc.axes_prop_cycle.len(), 10);
        // matplotlib's C0 is tab:blue, #1f77b4.
        assert_eq!(rc.axes_prop_cycle[0], Rgba::from_hex("#1f77b4").unwrap());
    }

    #[test]
    fn cycle_color_wraps() {
        let rc = RcParams::default();
        assert_eq!(rc.cycle_color(0), Rgba::from_hex("#1f77b4"));
        assert_eq!(rc.cycle_color(10), rc.cycle_color(0));
    }

    #[test]
    fn json_round_trip() {
        let rc = RcParams::default();
        let json = serde_json::to_string(&rc).expect("serialize");
        let back: RcParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rc, back);
    }

    #[test]
    fn dark_preset_round_trips() {
        let rc = RcParams::dark();
        assert_ne!(rc.figure_facecolor, Rgba::WHITE);
        let json = serde_json::to_string(&rc).expect("serialize");
        let back: RcParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rc, back);
    }

    #[test]
    fn merge_applies_overrides() {
        let base = RcParams::default();
        let overrides = RcParams {
            lines_linewidth: 3.0,
            axes_grid: true,
            ..RcParams::default()
        };
        let merged = base.merge(&overrides);
        assert_eq!(merged.lines_linewidth, 3.0);
        assert!(merged.axes_grid);
        // Untouched fields keep the base value.
        assert_eq!(merged.figure_dpi, base.figure_dpi);
    }
}
