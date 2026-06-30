//! The [`Axes::imshow`] raster-display helper.
//!
//! Mirrors matplotlib's `Axes.imshow`: it builds an [`AxesImage`] from a
//! row-major scalar array and stores it on the axes, returning a mutable handle
//! whose `cmap`/`vmin`/`vmax` setters tune the colorization in place. Images are
//! drawn beneath the other artists (see [`Axes::draw`](super::Axes::draw)) and
//! their data-space extent participates in autoscaling.

use rizzma_artist::AxesImage;

use crate::Axes;

impl Axes {
    /// Display row-major scalar `data` (`nrows x ncols`) as a colormapped image.
    ///
    /// The image defaults to matplotlib's `imshow` conventions: extent `(0,
    /// ncols, 0, nrows)`, `origin="upper"` (data row `0` at the top),
    /// `vmin`/`vmax` set to the data min/max, and the `viridis` colormap. Tune
    /// the returned handle with [`AxesImage::cmap`], [`AxesImage::vmin`],
    /// [`AxesImage::vmax`], or [`AxesImage::set_extent`], e.g.
    /// `ax.imshow(&data, h, w).cmap("gray").vmin(0.0).vmax(1.0)`.
    ///
    /// # Panics
    ///
    /// Panics if `data.len()` is not exactly `nrows * ncols`.
    pub fn imshow(&mut self, data: &[f64], nrows: usize, ncols: usize) -> &mut AxesImage {
        let image = AxesImage::new(data.to_vec(), nrows, ncols);
        self.images.push(image);
        self.images.last_mut().expect("just pushed an image")
    }
}

#[cfg(test)]
mod tests {
    use crate::Axes;
    use rizzma_core::Bbox;

    #[test]
    fn imshow_sets_data_limits_to_extent() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        // A 2-row, 3-column image; default extent is (0, 3, 0, 2).
        ax.imshow(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], 2, 3);
        let limits = ax.data_limits().expect("image provides data limits");
        assert_eq!(limits.xmin(), 0.0);
        assert_eq!(limits.xmax(), 3.0);
        assert_eq!(limits.ymin(), 0.0);
        assert_eq!(limits.ymax(), 2.0);
    }

    #[test]
    fn imshow_handle_tunes_colorization_in_place() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.imshow(&[0.0, 1.0, 2.0, 3.0], 2, 2)
            .cmap("gray")
            .vmin(0.0)
            .vmax(10.0);
        assert_eq!(ax.images.len(), 1);
        assert_eq!(ax.images[0].clim(), (0.0, 10.0));
    }

    #[test]
    fn imshow_explicit_extent_drives_limits() {
        let mut ax = Axes::new(Bbox::from_extents(0.0, 0.0, 1.0, 1.0));
        ax.imshow(&[0.0; 4], 2, 2).set_extent([-2.0, 6.0, 1.0, 9.0]);
        let limits = ax.data_limits().expect("image provides data limits");
        assert_eq!((limits.xmin(), limits.xmax()), (-2.0, 6.0));
        assert_eq!((limits.ymin(), limits.ymax()), (1.0, 9.0));
    }
}
