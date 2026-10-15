//! Raster export in caller pixel formats: `to_rgba8`, `to_rgb8`,
//! `render_into` and `render_into_u32`.

use rizzma::core::color::Rgba;
use rizzma::{BufferSizeError, Figure, PixelFormat};

/// A small figure with a line, a scatter and text, on `face`.
fn figure(face: Rgba) -> Figure {
    let mut fig = Figure::new(2.0, 1.5);
    fig.set_facecolor(face);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]);
    ax.scatter(&[0.5, 1.5], &[0.8, 0.2]);
    ax.set_title("raster");
    fig
}

/// Decode PNG bytes to straight RGBA8 with no transformations.
fn decode_png(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

#[test]
fn to_rgba8_matches_the_decoded_png_byte_for_byte() {
    // Opaque, translucent and fully transparent canvases: the latter two put
    // partial alpha everywhere, so demultiplication is exercised throughout.
    for face in [
        Rgba::WHITE,
        Rgba::new(0.2, 0.4, 0.9, 0.5),
        Rgba::new(1.0, 1.0, 1.0, 0.0),
    ] {
        let fig = figure(face);
        let (rgba, w, h) = fig.to_rgba8();
        assert_eq!((w, h), fig.raster_size());
        let png = decode_png(&fig.encode_png().unwrap());
        assert_eq!((rgba, w, h), png, "face {face:?}");
    }
}

#[test]
fn every_format_agrees_with_the_premultiplied_raster() {
    let fig = figure(Rgba::new(0.9, 0.9, 0.8, 0.6));
    let (w, h) = fig.raster_size();
    let n = (w * h) as usize;
    let face = [230u32, 230, 204];

    let mut premul = vec![0; n * 4];
    fig.render_into(&mut premul, PixelFormat::Rgba8Premul)
        .unwrap();
    assert_eq!(premul, fig.render().pixmap().data());

    let (rgb, ..) = fig.to_rgb8();
    let mut bgrx = vec![0; n * 4];
    fig.render_into(&mut bgrx, PixelFormat::Bgrx8).unwrap();
    let mut xrgb_bytes = vec![0; n * 4];
    fig.render_into(&mut xrgb_bytes, PixelFormat::Xrgb8888)
        .unwrap();
    let mut frame = vec![0u32; n];
    fig.render_into_u32(&mut frame).unwrap();

    for i in 0..n {
        let p = &premul[i * 4..i * 4 + 4];
        // Composited over the opaque face: premultiplied + (1 - a) * face.
        let expect: Vec<u8> = (0..3)
            .map(|c| {
                let rest = 255 - u32::from(p[3]);
                (u32::from(p[c]) + (rest * face[c] + 127) / 255).min(255) as u8
            })
            .collect();
        assert_eq!(&rgb[i * 3..i * 3 + 3], &expect[..], "pixel {i}");
        assert_eq!(
            &bgrx[i * 4..i * 4 + 4],
            &[expect[2], expect[1], expect[0], 255]
        );
        let packed = u32::from(expect[0]) << 16 | u32::from(expect[1]) << 8 | u32::from(expect[2]);
        assert_eq!(frame[i], packed);
        assert_eq!(&xrgb_bytes[i * 4..i * 4 + 4], &packed.to_ne_bytes());
    }
}

#[test]
fn opaque_formats_composite_over_the_face_color_made_opaque() {
    // A fully transparent red canvas with nothing drawn on it.
    let mut fig = Figure::new(0.5, 0.5);
    fig.set_facecolor(Rgba::new(1.0, 0.0, 0.0, 0.0));
    let (rgb, ..) = fig.to_rgb8();
    assert!(rgb.chunks(3).all(|p| p == [255, 0, 0]));
    let (rgba, ..) = fig.to_rgba8();
    assert!(rgba.chunks(4).all(|p| p[3] == 0));
}

#[test]
fn wrong_sized_buffers_are_errors_and_stay_untouched() {
    let fig = figure(Rgba::WHITE);
    let (w, h) = fig.raster_size();
    let n = (w * h) as usize;

    for format in [
        PixelFormat::Rgb8,
        PixelFormat::Rgba8,
        PixelFormat::Rgba8Premul,
        PixelFormat::Bgrx8,
        PixelFormat::Xrgb8888,
    ] {
        let expected = n * format.bytes_per_pixel();
        for len in [0, expected - 1, expected + 1] {
            let mut buf = vec![7u8; len];
            let err = fig.render_into(&mut buf, format).unwrap_err();
            assert_eq!(
                err,
                BufferSizeError {
                    expected,
                    actual: len
                }
            );
            assert!(buf.iter().all(|&b| b == 7));
        }
    }

    let mut short = vec![9u32; n - 1];
    let err = fig.render_into_u32(&mut short).unwrap_err();
    assert_eq!(
        err,
        BufferSizeError {
            expected: n,
            actual: n - 1
        }
    );
    assert!(short.iter().all(|&p| p == 9));
    assert!(err.to_string().contains(&n.to_string()));
}
