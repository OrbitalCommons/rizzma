//! All-sky projections on a regular [`Axes`]: matplotlib's
//! `add_subplot(projection="mollweide")` / `"aitoff"`.
//!
//! [`Axes::set_sky_projection`] turns an axes into a map of the whole sky.
//! Data coordinates become `(longitude, latitude)` in **radians** and every
//! artist — lines, scatters, fills, meshes — projects through the
//! projection at draw time, so the ordinary plotting methods, colorbars and
//! legends all work on it. The rectangular frame and axes give way to the
//! projection's elliptical outline, degree labels along the equator and the
//! western edge, and (with [`Axes::grid`]) a graticule — matplotlib's
//! layout for these projections.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::core::{Affine2D, Bbox, Path};
use crate::figure::Axes;
use crate::figure::richtext::layout_rich_text;
use crate::figure::sky::{self, SkyProjection};
use crate::render::{GraphicsContext, Renderer};
use crate::text::FontSource;

/// Longitude tick spacing, matplotlib's geo-axes default.
const LON_STEP_DEG: i32 = 30;
/// Latitude tick spacing, matplotlib's geo-axes default.
const LAT_STEP_DEG: i32 = 15;
/// Meridian grid lines stop this far from the poles, where they would
/// crowd together (matplotlib's `longitude_cap`).
const MERIDIAN_CAP_DEG: f64 = 75.0;
/// Longitude labels sit this far above the equator, in pixels at 100 DPI.
const LON_LABEL_PAD: f64 = 4.0;
/// Latitude labels end this far left of the outline, in pixels at 100 DPI.
const LAT_LABEL_PAD: f64 = 8.0;

/// The tick values `-limit + step, …, limit - step` in degrees.
fn ticks(step: i32, limit: i32) -> impl Iterator<Item = i32> {
    (-limit + step..limit).step_by(step as usize)
}

impl Axes {
    /// Draw this axes as a map of the whole sky through `projection`
    /// (matplotlib's `add_subplot(projection="mollweide")`).
    ///
    /// Data coordinates become longitude and latitude in **radians**, with
    /// longitude increasing to the right across `[-π, π]`. Lines and
    /// polygon edges follow the projection's curves; meshes project their
    /// corners, so a [`pcolormesh_xy`](Axes::pcolormesh_xy) over a
    /// longitude/latitude grid fills the sky. The view always shows the
    /// whole sky with equal aspect, so limits and pan/zoom have no effect.
    ///
    /// The frame becomes the projection's elliptical outline, with degree
    /// labels along the equator and the western edge; [`Axes::grid`] adds
    /// the graticule and the tick-label visibility of each axis hides its
    /// labels.
    ///
    /// ![sky fill](https://raw.githubusercontent.com/OrbitalCommons/rizzma/gh-pages/gallery_sky_fill.png)
    ///
    /// ```
    /// use std::f64::consts::{FRAC_PI_2, PI};
    /// use rizzma::Figure;
    /// use rizzma::figure::Mappable;
    /// use rizzma::figure::sky::SkyProjection;
    ///
    /// // A 10° grid of cell edges and one value per cell, row-major by
    /// // latitude.
    /// let lon: Vec<f64> = (0..=36).map(|i| -PI + i as f64 * PI / 18.0).collect();
    /// let lat: Vec<f64> = (0..=18).map(|i| -FRAC_PI_2 + i as f64 * PI / 18.0).collect();
    /// let c: Vec<f64> = (0..18 * 36)
    ///     .map(|k| ((k % 36) as f64 / 6.0).sin() * ((k / 36) as f64 / 3.0).cos())
    ///     .collect();
    ///
    /// let mut fig = Figure::new(8.0, 4.5);
    /// let ax = fig.add_subplot(1, 1, 1);
    /// ax.set_sky_projection(SkyProjection::Mollweide).grid(true);
    /// ax.pcolormesh_xy(&lon, &lat, &c, "viridis");
    /// ax.colorbar(Mappable::Mesh(0));
    /// assert!(!fig.encode_png().unwrap().is_empty());
    /// ```
    pub fn set_sky_projection(&mut self, projection: SkyProjection) -> &mut Self {
        self.sky = Some(projection);
        self.aspect_equal = true;
        self.xlim = Some((-PI, PI));
        self.ylim = Some((-FRAC_PI_2, FRAC_PI_2));
        self
    }

    /// The sky projection set by [`Axes::set_sky_projection`], if any.
    #[must_use]
    pub fn sky_projection(&self) -> Option<SkyProjection> {
        self.sky
    }

    /// The projection's outline inscribed in the axes rectangle: both
    /// projections bound the sky with an ellipse filling their native
    /// extents.
    fn sky_outline(axes_px: &Bbox) -> Path {
        let (rx, ry) = (axes_px.width() / 2.0, axes_px.height() / 2.0);
        let (cx, cy) = (axes_px.xmin() + rx, axes_px.ymin() + ry);
        Path::unit_circle().transformed(&Affine2D::from_scale(rx, ry).translate(cx, cy))
    }

    /// Fill the sky's outline with the axes face color.
    pub(crate) fn draw_sky_background(&self, renderer: &mut dyn Renderer, axes_px: &Bbox) {
        renderer.draw_path(
            &GraphicsContext::new(),
            &Self::sky_outline(axes_px),
            &Affine2D::identity(),
            Some(self.facecolor),
        );
    }

    /// The graticule, when the grid is on: parallels across the whole sky,
    /// meridians stopping short of the poles. Drawn beneath the data, like
    /// a cartesian grid.
    pub(crate) fn draw_sky_grid(
        &self,
        renderer: &mut dyn Renderer,
        sky: SkyProjection,
        td: &Affine2D,
    ) {
        let mut stroke =
            |points: [[f64; 2]; 2], (color, width): (crate::core::color::Rgba, f64)| {
                let gc = GraphicsContext::new()
                    .with_stroke(color)
                    .with_line_width(width);
                let path = sky::project_path(sky, &Path::from_polyline(&points));
                renderer.draw_path(&gc, &path, td, None);
            };
        if let Some(style) = self.xaxis.grid_stroke() {
            let cap = MERIDIAN_CAP_DEG.to_radians();
            for lon in ticks(LON_STEP_DEG, 180) {
                let lon = f64::from(lon).to_radians();
                stroke([[lon, -cap], [lon, cap]], style);
            }
        }
        if let Some(style) = self.yaxis.grid_stroke() {
            for lat in ticks(LAT_STEP_DEG, 90) {
                let lat = f64::from(lat).to_radians();
                stroke([[-PI, lat], [PI, lat]], style);
            }
        }
    }

    /// The outline (when the frame is on) and the degree labels: longitudes
    /// just above the equator, latitudes right-aligned off the western edge.
    pub(crate) fn draw_sky_decorations(
        &self,
        renderer: &mut dyn Renderer,
        sky: SkyProjection,
        axes_px: &Bbox,
        td: &Affine2D,
        font: &FontSource,
    ) {
        if !self.axis_visible {
            return;
        }
        if self.frame {
            let gc = GraphicsContext::new()
                .with_stroke(self.edgecolor)
                .with_line_width(self.linewidth);
            renderer.draw_path(
                &gc,
                &Self::sky_outline(axes_px),
                &Affine2D::identity(),
                None,
            );
        }
        let s = renderer.decoration_scale();
        let to_px = |lon: f64, lat: f64| {
            let (x, y) = sky::project(sky, lon, lat);
            td.transform_point((x, y))
        };
        let mut label =
            |text: &str, color, size: f64, place: &dyn Fn(f64, f64, f64) -> (f64, f64)| {
                let rich = layout_rich_text(font, text, size * s);
                let (x, y) = place(rich.width, rich.ascent, rich.descent);
                let shift = Affine2D::from_translation(x, y);
                for path in &rich.paths {
                    renderer.draw_path(
                        &GraphicsContext::new(),
                        &path.transformed(&shift),
                        &Affine2D::identity(),
                        Some(color),
                    );
                }
            };
        if self.xaxis.tick_labels_visible() {
            let (color, size) = (self.xaxis.color(), self.xaxis.tick_label_size());
            for lon in ticks(LON_STEP_DEG, 180) {
                let (px, py) = to_px(f64::from(lon).to_radians(), 0.0);
                label(&format!("{lon}°"), color, size, &|w, _, descent| {
                    (px - w / 2.0, py + LON_LABEL_PAD * s + descent)
                });
            }
        }
        if self.yaxis.tick_labels_visible() {
            let (color, size) = (self.yaxis.color(), self.yaxis.tick_label_size());
            for lat in ticks(LAT_STEP_DEG, 90) {
                let (px, py) = to_px(-PI, f64::from(lat).to_radians());
                label(&format!("{lat}°"), color, size, &|w, ascent, descent| {
                    (px - LAT_LABEL_PAD * s - w, py - (ascent - descent) / 2.0)
                });
            }
        }
    }

    /// The room the sky decorations need outside the axes rectangle
    /// `(left, right, bottom, top)`: the latitude labels off the western
    /// edge. Longitude labels lie inside the outline.
    pub(crate) fn sky_layout_insets(&self, font: &FontSource, s: f64) -> (f64, f64, f64, f64) {
        if !self.axis_visible || !self.yaxis.tick_labels_visible() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let size = self.yaxis.tick_label_size() * s;
        let widest = ticks(LAT_STEP_DEG, 90)
            .map(|lat| layout_rich_text(font, &format!("{lat}°"), size).width)
            .fold(0.0, f64::max);
        (LAT_LABEL_PAD * s + widest, 0.0, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graticule_ticks_match_matplotlib() {
        let lon: Vec<i32> = ticks(LON_STEP_DEG, 180).collect();
        assert_eq!(lon.first(), Some(&-150));
        assert_eq!(lon.last(), Some(&150));
        assert_eq!(lon.len(), 11);
        let lat: Vec<i32> = ticks(LAT_STEP_DEG, 90).collect();
        assert_eq!((lat[0], lat[lat.len() - 1], lat.len()), (-75, 75, 11));
    }
}
