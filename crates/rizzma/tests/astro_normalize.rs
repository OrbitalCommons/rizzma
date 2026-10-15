//! astropy-style normalization against astropy 8 and matplotlib 3.11
//! (`fixtures/astro/astro_mpl.py`): ZScale and percentile limits on a star
//! field, a nebula and an image with NaN holes; every stretch's forward
//! transform on a dense grid; and an `imshow` through
//! `ImageNormalize(ZScale, Asinh(0.1))`.

use rizzma::Figure;
use rizzma::core::color::{ImageNormalize, Interval, Stretch, ZScaleInterval};

const N: usize = 64;

fn floats(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks_exact(8)
        .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn image(name: &str) -> Vec<f64> {
    floats(match name {
        "star" => include_bytes!("fixtures/astro/star.f64"),
        "nebula" => include_bytes!("fixtures/astro/nebula.f64"),
        "holes" => include_bytes!("fixtures/astro/holes.f64"),
        other => panic!("no fixture {other}"),
    })
}

fn relative(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

#[test]
fn intervals_match_astropy() {
    let mut checked = 0;
    for line in include_str!("fixtures/astro/zscale.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        let interval = match f[0] {
            "zscale" => Interval::ZScale(ZScaleInterval::default()),
            "minmax" => Interval::MinMax,
            "percentile99.5" => Interval::Percentile(99.5),
            "asymmetric1-99.9" => Interval::AsymmetricPercentile(1.0, 99.9),
            other => panic!("unknown interval {other}"),
        };
        let want: (f64, f64) = (f[2].parse().unwrap(), f[3].parse().unwrap());
        let got = interval.limits(&image(f[1]));
        assert!(
            relative(got.0, want.0) <= 1e-6 && relative(got.1, want.1) <= 1e-6,
            "{} on {}: rizzma {got:?} vs astropy {want:?}",
            f[0],
            f[1]
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
}

#[test]
fn stretches_match_astropy_on_a_dense_grid() {
    let names: Vec<&str> = include_str!("fixtures/astro/stretches.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect();
    let curves = floats(include_bytes!("fixtures/astro/stretches.f64"));
    let points = 2001;
    assert_eq!(curves.len(), names.len() * points);
    for (k, name) in names.iter().enumerate() {
        let stretch = match *name {
            "linear" => Stretch::Linear {
                slope: 0.8,
                intercept: 0.1,
            },
            "sqrt" => Stretch::Sqrt,
            "power" => Stretch::Power(2.5),
            "log" => Stretch::Log(1000.0),
            "asinh" => Stretch::Asinh(0.1),
            "sinh" => Stretch::Sinh(1.0 / 3.0),
            "histeq" => Stretch::histeq(&image("star")),
            other => panic!("unknown stretch {other}"),
        };
        for i in 0..points {
            let x = i as f64 / (points - 1) as f64;
            let want = curves[k * points + i];
            let got = stretch.forward(x);
            assert!(
                (got - want).abs() <= 1e-12,
                "{name} at {x}: rizzma {got} vs astropy {want}"
            );
        }
    }
}

#[test]
fn zscale_asinh_imshow_matches_matplotlib() {
    let png = include_bytes!("fixtures/astro/render_zscale_asinh.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&png[..]));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();

    let mut fig = Figure::new(N as f64 * 4.0 / 100.0, N as f64 * 4.0 / 100.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.imshow(&image("star"), N, N)
        .cmap("gray")
        .norm(ImageNormalize::new(Interval::zscale(), Stretch::Asinh(0.1)));
    ax.set_xlim(0.0, N as f64).set_ylim(0.0, N as f64);
    let (rgb, w, _) = fig.to_rgb8();

    let mut worst = 0i16;
    for r in 0..N {
        for c in 0..N {
            let want = &buf[(r * N + c) * channels..][..3];
            let at = ((4 * r + 2) * w as usize + 4 * c + 2) * 3;
            for ch in 0..3 {
                worst = worst.max((i16::from(rgb[at + ch]) - i16::from(want[ch])).abs());
            }
        }
    }
    assert!(worst <= 1, "worst channel difference {worst}");
}
