//! The [`AxesImage`] artist: a colormapped raster of scalar data (`imshow`).
//!
//! Mirrors matplotlib's `AxesImage`. A row-major `nrows x ncols` array of `f64`
//! samples is normalized through a [`LinearNorm`] and colorized through a named
//! [`Colormap`], then blitted into the device target rectangle spanned by the
//! image's data-space `extent`. Resampling is nearest-neighbour, hand-rolled so
//! the crate takes on no new dependencies.

use crate::core::color::{
    Colormap, DEFAULT_COLORMAP, LinearNorm, Normalize, colormap, default_colormap,
};
use crate::core::{Affine2D, Bbox};
use crate::render::{GraphicsContext, Renderer};

use crate::artist::Artist;

/// A colormapped image of scalar data drawn over a data-space rectangle.
///
/// Construct with [`AxesImage::new`] from a row-major `nrows x ncols` data
/// array. By default the image occupies the data rectangle `(0, ncols, 0,
/// nrows)`, uses `vmin`/`vmax` equal to the data min/max, the default
/// colormap, and matplotlib's `origin="upper"` convention (data row `0` is
/// drawn at the *top* of the extent). Adjust via the builder setters before
/// adding the image to an axes.
#[derive(Debug, Clone, PartialEq)]
pub struct AxesImage {
    /// Row-major scalar samples, length `nrows * ncols`.
    data: Vec<f64>,
    /// Number of data rows.
    nrows: usize,
    /// Number of data columns.
    ncols: usize,
    /// Data-space extent `[x0, x1, y0, y1]` the image is drawn into.
    extent: [f64; 4],
    /// Lower normalization bound, mapped to colormap position `0.0`.
    vmin: f64,
    /// Upper normalization bound, mapped to colormap position `1.0`.
    vmax: f64,
    /// Name of the colormap to colorize through (see [`colormap`]).
    cmap_name: String,
    /// When `true`, data row `0` maps to the top of the extent
    /// (matplotlib's `origin="upper"`); when `false`, to the bottom.
    origin_upper: bool,
    /// Stacking order; higher draws on top. Images default to `0.0` so they
    /// draw beneath lines, patches, and collections.
    zorder: f64,
    /// Whether the image is drawn.
    visible: bool,
}

impl AxesImage {
    /// Construct an [`AxesImage`] from a row-major `nrows x ncols` data array.
    ///
    /// The data is interpreted row-major (`data[r * ncols + c]`). Defaults
    /// mirror matplotlib's `imshow`: extent `(0, ncols, 0, nrows)`,
    /// `origin="upper"`, `vmin`/`vmax` set to the finite data min/max, the
    /// default colormap, zorder `0.0`, and visible. When the data is empty or
    /// all-NaN, `vmin`/`vmax` fall back to `(0.0, 1.0)`.
    ///
    /// # Panics
    ///
    /// Panics if `data.len()` is not exactly `nrows * ncols`.
    #[must_use]
    pub fn new(data: Vec<f64>, nrows: usize, ncols: usize) -> Self {
        assert_eq!(
            data.len(),
            nrows * ncols,
            "AxesImage: data length {} must equal nrows * ncols = {}",
            data.len(),
            nrows * ncols
        );
        let (vmin, vmax) = data_min_max(&data);
        Self {
            data,
            nrows,
            ncols,
            extent: [0.0, ncols as f64, 0.0, nrows as f64],
            vmin,
            vmax,
            cmap_name: DEFAULT_COLORMAP.to_string(),
            origin_upper: true,
            zorder: 0.0,
            visible: true,
        }
    }

    /// Set the data-space extent `[x0, x1, y0, y1]`, returning `self`.
    #[must_use]
    pub fn with_extent(mut self, extent: [f64; 4]) -> Self {
        self.extent = extent;
        self
    }

    /// Set the colormap name (see [`colormap`]), returning `self`.
    #[must_use]
    pub fn with_cmap(mut self, cmap_name: impl Into<String>) -> Self {
        self.cmap_name = cmap_name.into();
        self
    }

    /// Set the lower normalization bound, returning `self`.
    #[must_use]
    pub fn with_vmin(mut self, vmin: f64) -> Self {
        self.vmin = vmin;
        self
    }

    /// Set the upper normalization bound, returning `self`.
    #[must_use]
    pub fn with_vmax(mut self, vmax: f64) -> Self {
        self.vmax = vmax;
        self
    }

    /// Choose the `origin` convention: `true` for matplotlib's `"upper"` (row
    /// `0` at the top), `false` for `"lower"`. Returns `self`.
    #[must_use]
    pub fn with_origin_upper(mut self, origin_upper: bool) -> Self {
        self.origin_upper = origin_upper;
        self
    }

    /// Set the stacking order, returning `self`.
    #[must_use]
    pub fn with_zorder(mut self, zorder: f64) -> Self {
        self.zorder = zorder;
        self
    }

    /// Set whether the image is drawn, returning `self`.
    #[must_use]
    pub fn set_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the colormap name in place, returning `&mut self` for chaining on a
    /// handle obtained from an axes (e.g. `ax.imshow(..).cmap("gray")`).
    pub fn cmap(&mut self, cmap_name: impl Into<String>) -> &mut Self {
        self.cmap_name = cmap_name.into();
        self
    }

    /// Set the lower normalization bound in place, returning `&mut self`.
    pub fn vmin(&mut self, vmin: f64) -> &mut Self {
        self.vmin = vmin;
        self
    }

    /// Set the upper normalization bound in place, returning `&mut self`.
    pub fn vmax(&mut self, vmax: f64) -> &mut Self {
        self.vmax = vmax;
        self
    }

    /// Set the data-space extent in place, returning `&mut self`.
    pub fn set_extent(&mut self, extent: [f64; 4]) -> &mut Self {
        self.extent = extent;
        self
    }

    /// The data-space extent `[x0, x1, y0, y1]`.
    #[must_use]
    pub fn extent(&self) -> [f64; 4] {
        self.extent
    }

    /// The `(vmin, vmax)` normalization bounds.
    #[must_use]
    pub fn clim(&self) -> (f64, f64) {
        (self.vmin, self.vmax)
    }

    /// Colorize the source cell at data row `row` (0 at the top when
    /// `origin_upper`) and column `col` into a straight RGBA8 quad.
    ///
    /// This is the per-cell color pipeline (`LinearNorm` then the named
    /// colormap) exposed for testing. Out-of-range indices return transparent.
    #[must_use]
    pub fn colorize_cell(&self, row: usize, col: usize) -> [u8; 4] {
        if row >= self.nrows || col >= self.ncols {
            return [0, 0, 0, 0];
        }
        let norm = LinearNorm::new(self.vmin, self.vmax);
        let cmap = resolve_cmap(&self.cmap_name);
        let value = self.data[row * self.ncols + col];
        colorize(value, &norm, cmap.as_ref())
    }
}

/// Resolve a colormap by name, falling back to the default map for unknown
/// names so a stray name never produces an all-transparent image.
fn resolve_cmap(name: &str) -> Box<dyn Colormap> {
    colormap(name).unwrap_or_else(|| Box::new(default_colormap()))
}

/// Colorize a single scalar `value` through `norm` then `cmap` into straight
/// RGBA8. A non-finite value yields a transparent pixel.
fn colorize(value: f64, norm: &dyn Normalize, cmap: &dyn Colormap) -> [u8; 4] {
    if !value.is_finite() {
        return [0, 0, 0, 0];
    }
    cmap.sample(norm.normalize(value)).to_u8_array()
}

/// The finite min and max of `data`, or `(0.0, 1.0)` when there is no finite
/// value (empty or all-NaN input).
fn data_min_max(data: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min <= max { (min, max) } else { (0.0, 1.0) }
}

impl Artist for AxesImage {
    /// Colorize the data and blit it into the device rectangle spanned by the
    /// image's `extent`.
    ///
    /// # Coordinate handling
    ///
    /// The extent's lower-left `(x0, y0)` and upper-right `(x1, y1)` corners are
    /// mapped through `transform` into device (y-up) pixels. Their min/max give
    /// the device target rectangle and its integer pixel size `w_px x h_px`. An
    /// RGBA8 buffer is filled in **top-row-first** order, which is the row order
    /// [`Renderer::draw_image`] expects (its top edge is device `y +
    /// height`).
    ///
    /// For each target pixel `(tx, ty)` — `ty = 0` being the top device row —
    /// the source cell is chosen by nearest-neighbour sampling of the
    /// fractional position across the grid. With `origin_upper` the top device
    /// row samples data row `0`; otherwise the top device row samples the last
    /// data row, so the image is flipped vertically. Each sampled value is
    /// colorized through [`LinearNorm`] then the named colormap. The finished
    /// buffer is handed to `draw_image` with its lower-left corner at the
    /// device target rectangle's `(x, y)` minimum.
    fn draw(&self, renderer: &mut dyn Renderer, transform: &Affine2D) {
        if !self.visible || self.nrows == 0 || self.ncols == 0 {
            return;
        }
        let [x0, x1, y0, y1] = self.extent;
        // Map the two opposing corners into device space; take min/max so the
        // target rectangle is well-formed regardless of extent orientation.
        let (dx0, dy0) = transform.transform_point((x0, y0));
        let (dx1, dy1) = transform.transform_point((x1, y1));
        let dev_xmin = dx0.min(dx1);
        let dev_xmax = dx0.max(dx1);
        let dev_ymin = dy0.min(dy1);
        let dev_ymax = dy0.max(dy1);

        let w_px = (dev_xmax - dev_xmin).round() as i64;
        let h_px = (dev_ymax - dev_ymin).round() as i64;
        if w_px <= 0 || h_px <= 0 {
            return;
        }
        let w_px = w_px as usize;
        let h_px = h_px as usize;

        let norm = LinearNorm::new(self.vmin, self.vmax);
        let cmap = resolve_cmap(&self.cmap_name);

        let mut rgba = vec![0u8; w_px * h_px * 4];
        for ty in 0..h_px {
            // Fractional vertical position in [0, 1): 0 at the top device row.
            let fy = (ty as f64 + 0.5) / h_px as f64;
            // Top device row corresponds to data row 0 when origin is upper.
            let src_row_frac = if self.origin_upper { fy } else { 1.0 - fy };
            let row = ((src_row_frac * self.nrows as f64) as usize).min(self.nrows - 1);
            for tx in 0..w_px {
                let fx = (tx as f64 + 0.5) / w_px as f64;
                let col = ((fx * self.ncols as f64) as usize).min(self.ncols - 1);
                let value = self.data[row * self.ncols + col];
                let [r, g, b, a] = colorize(value, &norm, cmap.as_ref());
                let i = (ty * w_px + tx) * 4;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = a;
            }
        }

        renderer.draw_image(
            &GraphicsContext::new(),
            dev_xmin,
            dev_ymin,
            &rgba,
            w_px,
            h_px,
        );
    }

    fn zorder(&self) -> f64 {
        self.zorder
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn data_extents(&self) -> Option<Bbox> {
        let [x0, x1, y0, y1] = self.extent;
        Some(Bbox::from_extents(
            x0.min(x1),
            y0.min(y1),
            x0.max(x1),
            y0.max(y1),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color::default_colormap;

    #[test]
    fn defaults_match_matplotlib() {
        let img = AxesImage::new(vec![1.0, 2.0, 3.0, 4.0], 2, 2);
        assert_eq!(img.extent(), [0.0, 2.0, 0.0, 2.0]);
        assert_eq!(img.clim(), (1.0, 4.0));
        assert!(img.origin_upper);
        assert_eq!(Artist::zorder(&img), 0.0);
    }

    #[test]
    fn min_cell_is_cmap_zero_and_max_cell_is_cmap_one() {
        // 2x2 with min = 0.0 (top-left) and max = 3.0 (bottom-right).
        let img = AxesImage::new(vec![0.0, 1.0, 2.0, 3.0], 2, 2);
        let cm = default_colormap();
        let lo = cm.sample(0.0).to_u8_array();
        let hi = cm.sample(1.0).to_u8_array();

        // Row 0, col 0 holds the minimum -> cmap(0), dark blue.
        assert_eq!(img.colorize_cell(0, 0), lo);
        // Row 1, col 1 holds the maximum -> cmap(1), near-white.
        assert_eq!(img.colorize_cell(1, 1), hi);

        // Sanity: cet_l09(0) is dark blue, cet_l09(1) is near-white.
        assert!(lo[0] < 80 && lo[2] > 100 && lo[1] < 30);
        assert!(hi[0] > 200 && hi[1] > 200 && hi[2] > 200);
    }

    #[test]
    fn out_of_range_cell_is_transparent() {
        let img = AxesImage::new(vec![0.0, 1.0, 2.0, 3.0], 2, 2);
        assert_eq!(img.colorize_cell(5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn data_extents_returns_extent_bbox() {
        let img = AxesImage::new(vec![0.0; 6], 2, 3).with_extent([-1.0, 5.0, 2.0, 8.0]);
        let e = img.data_extents().expect("image has extents");
        assert_eq!((e.xmin(), e.xmax()), (-1.0, 5.0));
        assert_eq!((e.ymin(), e.ymax()), (2.0, 8.0));
    }

    #[test]
    fn nonfinite_value_colorizes_transparent() {
        let norm = LinearNorm::new(0.0, 1.0);
        let cm = default_colormap();
        assert_eq!(colorize(f64::NAN, &norm, &cm), [0, 0, 0, 0]);
    }

    #[test]
    fn all_nan_data_falls_back_to_unit_clim() {
        let img = AxesImage::new(vec![f64::NAN; 4], 2, 2);
        assert_eq!(img.clim(), (0.0, 1.0));
    }
}
