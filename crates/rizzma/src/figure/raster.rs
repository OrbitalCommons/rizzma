//! Raster export in the pixel layouts other libraries consume.
//!
//! [`Figure::render`] yields tiny-skia's premultiplied RGBA. Windows,
//! terminal-graphics protocols, GUI textures and image crates each want their
//! own layout, so [`Figure::to_rgba8`], [`Figure::to_rgb8`],
//! [`Figure::render_into`] and [`Figure::render_into_u32`] convert once, into a
//! fresh or caller-owned buffer.
//!
//! The opaque layouts ([`PixelFormat::Rgb8`], [`PixelFormat::Bgrx8`],
//! [`PixelFormat::Xrgb8888`] and `render_into_u32`) composite any transparency
//! over the figure's face color made opaque, as matplotlib does when saving a
//! format without alpha.

use crate::core::color::Rgba;
use crate::figure::Figure;

/// A pixel layout for [`Figure::render_into`]. Rows run top to bottom, pixels
/// left to right, with no padding between rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    /// 3 bytes per pixel, `R G B`, composited over the face color — for the
    /// `image` crate's `RgbImage`, PDF embedding, BMP and terminal graphics.
    Rgb8,
    /// 4 bytes per pixel, `R G B A` with straight alpha — the bytes a PNG
    /// stores, and what browser `ImageData` and most GPU textures expect.
    Rgba8,
    /// 4 bytes per pixel, `R G B A` with premultiplied alpha — tiny-skia's own
    /// layout, copied without conversion.
    Rgba8Premul,
    /// 4 bytes per pixel, `B G R X` with `X = 255`, composited over the face
    /// color — plotters' `BGRXPixel`, Windows DIBs and many framebuffers.
    Bgrx8,
    /// 4 bytes per pixel, each a native-endian `u32` holding `0x00RRGGBB`,
    /// composited over the face color — a `u32` framebuffer (minifb,
    /// softbuffer) viewed as bytes. With a `&mut [u32]` buffer, use
    /// [`Figure::render_into_u32`] instead.
    Xrgb8888,
}

impl PixelFormat {
    /// Bytes each pixel occupies in this layout.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb8 => 3,
            PixelFormat::Rgba8
            | PixelFormat::Rgba8Premul
            | PixelFormat::Bgrx8
            | PixelFormat::Xrgb8888 => 4,
        }
    }
}

/// A caller-supplied buffer whose length does not match the raster:
/// `expected` is `width * height * bytes_per_pixel` elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferSizeError {
    /// The length the raster needs.
    pub expected: usize,
    /// The length supplied.
    pub actual: usize,
}

impl std::fmt::Display for BufferSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "raster buffer holds {} elements but the figure needs {}",
            self.actual, self.expected
        )
    }
}

impl std::error::Error for BufferSizeError {}

impl Figure {
    /// The `(width, height)` in pixels of the raster [`render`](Figure::render)
    /// and the methods below produce.
    #[must_use]
    pub fn raster_size(&self) -> (u32, u32) {
        let (w, h) = self.size_px();
        (w as u32, h as u32)
    }

    /// Render to straight-alpha RGBA bytes, returning `(pixels, width,
    /// height)`: the same bytes PNG export stores, without encoding them.
    ///
    /// ```
    /// use rizzma::Figure;
    /// let mut fig = Figure::new(2.0, 1.5);
    /// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// let (rgba, w, h) = fig.to_rgba8();
    /// assert_eq!(rgba.len(), (w * h * 4) as usize);
    /// // A Slint texture: SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&rgba, w, h)
    /// // A wasm canvas:   ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), w, h)
    /// ```
    #[must_use]
    pub fn to_rgba8(&self) -> (Vec<u8>, u32, u32) {
        self.to_buffer(PixelFormat::Rgba8)
    }

    /// Render to RGB bytes composited over the face color, returning
    /// `(pixels, width, height)`.
    ///
    /// ```
    /// use rizzma::Figure;
    /// let mut fig = Figure::new(2.0, 1.5);
    /// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// let (rgb, w, h) = fig.to_rgb8();
    /// assert_eq!(rgb.len(), (w * h * 3) as usize);
    /// // The image crate: image::RgbImage::from_raw(w, h, rgb)
    /// //   -> DynamicImage for kitty / ratatui terminal graphics or PDF embedding.
    /// ```
    #[must_use]
    pub fn to_rgb8(&self) -> (Vec<u8>, u32, u32) {
        self.to_buffer(PixelFormat::Rgb8)
    }

    /// Render into a caller-owned byte buffer in `format`. The buffer must
    /// hold exactly `width * height * format.bytes_per_pixel()` bytes, with
    /// `(width, height)` from [`raster_size`](Figure::raster_size); a buffer of
    /// any other length is left untouched and reported as an error.
    ///
    /// # Errors
    ///
    /// Returns [`BufferSizeError`] when `buf` has the wrong length.
    ///
    /// ```
    /// use rizzma::{Figure, PixelFormat};
    /// let mut fig = Figure::new(2.0, 1.5);
    /// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// let (w, h) = fig.raster_size();
    ///
    /// // A BGRX framebuffer, as plotters' BitMapBackend::<BGRXPixel> drew into.
    /// let mut bgrx = vec![0u8; (w * h) as usize * PixelFormat::Bgrx8.bytes_per_pixel()];
    /// fig.render_into(&mut bgrx, PixelFormat::Bgrx8)?;
    ///
    /// // RGB for a BMP data URL in a wasm client.
    /// let mut rgb = vec![0u8; (w * h * 3) as usize];
    /// fig.render_into(&mut rgb, PixelFormat::Rgb8)?;
    ///
    /// // A wrong-sized buffer is an error, not a panic.
    /// assert!(fig.render_into(&mut rgb[1..], PixelFormat::Rgb8).is_err());
    /// # Ok::<(), rizzma::BufferSizeError>(())
    /// ```
    pub fn render_into(&self, buf: &mut [u8], format: PixelFormat) -> Result<(), BufferSizeError> {
        let (w, h) = self.raster_size();
        let expected = w as usize * h as usize * format.bytes_per_pixel();
        if buf.len() != expected {
            return Err(BufferSizeError {
                expected,
                actual: buf.len(),
            });
        }
        let renderer = self.render();
        let background = opaque(self.facecolor());
        let pixels = renderer.pixmap().pixels();
        let chunks = buf.chunks_exact_mut(format.bytes_per_pixel());
        for (out, &p) in chunks.zip(pixels) {
            match format {
                PixelFormat::Rgba8 => {
                    let c = p.demultiply();
                    out.copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
                }
                PixelFormat::Rgba8Premul => {
                    out.copy_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
                }
                PixelFormat::Rgb8 => out.copy_from_slice(&over(p, background)),
                PixelFormat::Bgrx8 => {
                    let [r, g, b] = over(p, background);
                    out.copy_from_slice(&[b, g, r, 255]);
                }
                PixelFormat::Xrgb8888 => {
                    out.copy_from_slice(&xrgb(over(p, background)).to_ne_bytes());
                }
            }
        }
        Ok(())
    }

    /// Render into a caller-owned `u32` framebuffer, one `0x00RRGGBB` pixel per
    /// element, composited over the face color — the layout minifb and
    /// softbuffer windows take. The buffer must hold exactly `width * height`
    /// elements.
    ///
    /// # Errors
    ///
    /// Returns [`BufferSizeError`] when `buf` has the wrong length.
    ///
    /// ```
    /// use rizzma::Figure;
    /// let mut fig = Figure::new(2.0, 1.5);
    /// fig.add_subplot(1, 1, 1).plot(&[0.0, 1.0], &[0.0, 1.0]);
    /// let (w, h) = fig.raster_size();
    /// let mut frame = vec![0u32; (w * h) as usize];
    /// fig.render_into_u32(&mut frame)?;
    /// // window.update_with_buffer(&frame, w as usize, h as usize)   (minifb)
    /// # Ok::<(), rizzma::BufferSizeError>(())
    /// ```
    pub fn render_into_u32(&self, buf: &mut [u32]) -> Result<(), BufferSizeError> {
        let (w, h) = self.raster_size();
        let expected = w as usize * h as usize;
        if buf.len() != expected {
            return Err(BufferSizeError {
                expected,
                actual: buf.len(),
            });
        }
        let renderer = self.render();
        let background = opaque(self.facecolor());
        for (out, &p) in buf.iter_mut().zip(renderer.pixmap().pixels()) {
            *out = xrgb(over(p, background));
        }
        Ok(())
    }

    /// A fresh buffer in `format`, with the raster size.
    fn to_buffer(&self, format: PixelFormat) -> (Vec<u8>, u32, u32) {
        let (w, h) = self.raster_size();
        let mut buf = vec![0; w as usize * h as usize * format.bytes_per_pixel()];
        self.render_into(&mut buf, format)
            .expect("buffer sized from raster_size");
        (buf, w, h)
    }
}

/// `color` as opaque 8-bit RGB, alpha discarded.
fn opaque(color: Rgba) -> [u8; 3] {
    let [r, g, b, _] = color.to_u8_array();
    [r, g, b]
}

/// The premultiplied pixel `p` composited over the opaque `background`.
fn over(p: tiny_skia::PremultipliedColorU8, background: [u8; 3]) -> [u8; 3] {
    let rest = 255 - u16::from(p.alpha());
    let blend = |c: u8, bg: u8| {
        let v = u16::from(c) + (rest * u16::from(bg) + 127) / 255;
        v.min(255) as u8
    };
    [
        blend(p.red(), background[0]),
        blend(p.green(), background[1]),
        blend(p.blue(), background[2]),
    ]
}

/// `[r, g, b]` packed as `0x00RRGGBB`.
fn xrgb([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}
