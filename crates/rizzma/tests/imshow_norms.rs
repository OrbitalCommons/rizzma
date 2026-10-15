//! `imshow` with each norm against matplotlib 3.11: every cell of a 64x64
//! field spanning four decades of both signs — NaN holes and out-of-range
//! values included, drawn with bad/over/under colors — matches
//! `imshow(norm=...)` to within one unit per channel. References come from
//! `fixtures/imshow_norms_mpl.py`.

use rizzma::Figure;
use rizzma::core::color::{
    BoundaryNorm, CenteredNorm, LinearNorm, LogNorm, Norm, PowerNorm, Rgba, SymLogNorm,
    TwoSlopeNorm,
};

const N: usize = 64;

/// The fixture field, as `imshow_norms_mpl.py` builds it.
fn field() -> Vec<f64> {
    let mut data = Vec::with_capacity(N * N);
    for r in 0..N {
        for c in 0..N {
            let mag = 10f64.powf(-1.0 + 4.0 * c as f64 / (N - 1) as f64);
            let sign = if r < 40 { 1.0 } else { -1.0 };
            let v = if r % 9 == 4 && c % 7 == 3 {
                f64::NAN
            } else {
                sign * mag * (1.0 + 0.3 * (r as f64).sin())
            };
            data.push(v);
        }
    }
    data
}

fn norms() -> Vec<(&'static str, &'static str, Norm, &'static [u8])> {
    vec![
        (
            "linear",
            "viridis",
            LinearNorm::new(-200.0, 600.0).into(),
            include_bytes!("fixtures/imshow_norm_linear.png"),
        ),
        (
            "log",
            "viridis",
            LogNorm::new(0.3, 300.0).into(),
            include_bytes!("fixtures/imshow_norm_log.png"),
        ),
        (
            "power",
            "magma",
            PowerNorm::new(0.5, -100.0, 800.0).into(),
            include_bytes!("fixtures/imshow_norm_power.png"),
        ),
        (
            "twoslope",
            "coolwarm",
            TwoSlopeNorm::new(-300.0, 0.0, 900.0).into(),
            include_bytes!("fixtures/imshow_norm_twoslope.png"),
        ),
        (
            "centered",
            "coolwarm",
            CenteredNorm::new(50.0, 400.0).into(),
            include_bytes!("fixtures/imshow_norm_centered.png"),
        ),
        (
            "symlog",
            "RdBu",
            SymLogNorm::new(1.0, -500.0, 500.0).into(),
            include_bytes!("fixtures/imshow_norm_symlog.png"),
        ),
        (
            "boundary",
            "viridis",
            BoundaryNorm::new(
                vec![-100.0, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0, 1000.0],
                256,
            )
            .into(),
            include_bytes!("fixtures/imshow_norm_boundary.png"),
        ),
    ]
}

/// The reference cells as RGB triples, row 0 first.
fn reference(png_bytes: &[u8]) -> Vec<[u8; 3]> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    assert_eq!((info.width, info.height), (N as u32, N as u32));
    let channels = info.color_type.samples();
    buf.chunks(channels)
        .take(N * N)
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}

#[test]
fn imshow_norms_match_matplotlib() {
    let data = field();
    let hex = |s| Rgba::from_hex(s).unwrap();
    let mut failures = Vec::new();
    for (name, cmap, norm, png) in norms() {
        let mut fig = Figure::new(N as f64 * 4.0 / 100.0, N as f64 * 4.0 / 100.0);
        let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
        ax.set_axis_off();
        ax.imshow(&data, N, N)
            .cmap(cmap)
            .norm(norm)
            .bad(hex("#ff00ff"))
            .over(hex("#00ff00"))
            .under(hex("#ffff00"));
        ax.set_xlim(0.0, N as f64).set_ylim(0.0, N as f64);
        let (rgb, w, _) = fig.to_rgb8();
        assert_eq!(w as usize, N * 4);

        let want = reference(png);
        let mut worst = (0i16, 0usize);
        for (i, expected) in want.iter().enumerate() {
            let (r, c) = (i / N, i % N);
            let at = ((4 * r + 2) * w as usize + 4 * c + 2) * 3;
            for ch in 0..3 {
                let d = (i16::from(rgb[at + ch]) - i16::from(expected[ch])).abs();
                if d > worst.0 {
                    worst = (d, i);
                }
            }
        }
        if worst.0 > 1 {
            let (r, c) = (worst.1 / N, worst.1 % N);
            let at = ((4 * r + 2) * w as usize + 4 * c + 2) * 3;
            failures.push(format!(
                "{name}: cell ({r}, {c}) value {} off by {}: rizzma {:?} vs matplotlib {:?}",
                data[worst.1],
                worst.0,
                &rgb[at..at + 3],
                want[worst.1]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
