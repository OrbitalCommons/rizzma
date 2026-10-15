//! Colormaps of your own reach every colormapped artist: a hand-rolled
//! value-to-color function keeps its exact colors, a `ListedColormap`
//! matches matplotlib's, a registered name works where only names are
//! taken, and builtin names render as before. References come from
//! `fixtures/custom_cmaps_mpl.py`.

use rizzma::Figure;
use rizzma::core::color::{CmapSpec, Colormap, ListedColormap, Rgba, register_colormap, viridis};
use rizzma::figure::Mappable;

/// OrbitalCommons/starfield's B−V color, `HSLColor(0.7 − 0.7·t, 0.65,
/// 0.45)`, as plotters 0.3.7 converts HSL to RGB (before its rounding to
/// 8 bits, which drawing does).
fn starfield_bv(t: f64) -> Rgba {
    let (h, s, l) = (0.7 - 0.7 * t, 0.65, 0.45);
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Rgba::rgb(channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
}

/// The plotters colors at the 256 lookup-table points, `t = i / 255`.
fn starfield_reference() -> Vec<[u8; 3]> {
    include_str!("fixtures/starfield_hsl.txt")
        .lines()
        .map(|l| {
            let v: Vec<u8> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        })
        .collect()
}

#[test]
fn from_fn_keeps_the_hand_rolled_colors_exactly() {
    let ramp = ListedColormap::from_fn(starfield_bv);
    let want = starfield_reference();
    assert_eq!(want.len(), 256);
    for (i, expected) in want.iter().enumerate() {
        let got = ramp.colors[i].to_u8_array();
        assert_eq!(&got[..3], expected, "entry {i}");
    }
}

#[test]
fn a_scatter_draws_the_hand_rolled_colors_with_a_colorbar() {
    // One marker per lookup-table entry, valued so it samples that entry.
    let (mut x, mut y, mut c) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..256 {
        x.push(f64::from(i % 16));
        y.push(f64::from(i / 16));
        c.push(f64::from(i) / 255.0);
    }
    let mut fig = Figure::new(6.0, 4.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.scatter_mapped(&x, &y, &c, ListedColormap::from_fn(starfield_bv));
    ax.colorbar(Mappable::Collection(0));
    let (rgb, w, _) = fig.to_rgb8();
    let want = starfield_reference();
    for (i, expected) in want.iter().enumerate() {
        let (px, py) = fig.data_to_pixel(0, x[i], y[i]).expect("axes 0");
        let at = (py as usize * w as usize + px as usize) * 3;
        assert_eq!(&rgb[at..at + 3], expected, "marker {i}");
    }
    let scale = fig.axes()[0]
        .color_scale_of(Mappable::Collection(0))
        .expect("the colorbar describes the scatter");
    assert!(matches!(scale.cmap, CmapSpec::Custom(_)));
}

#[test]
fn a_listed_colormap_matches_matplotlib() {
    const N: usize = 16;
    let hex = |s| Rgba::from_hex(s).unwrap();
    let colors = [
        hex("#440154"),
        hex("#3b528b"),
        hex("#21918c"),
        hex("#5ec962"),
        hex("#fde725"),
    ];
    let field: Vec<f64> = (0..N * N).map(|k| (k % 37) as f64 / 36.0).collect();
    let mut fig = Figure::new(N as f64 * 4.0 / 100.0, N as f64 * 4.0 / 100.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.imshow(&field, N, N)
        .cmap(ListedColormap::from_colors(&colors))
        .vmin(0.0)
        .vmax(1.0);
    ax.set_xlim(0.0, N as f64).set_ylim(0.0, N as f64);
    let (rgb, w, _) = fig.to_rgb8();

    let png = include_bytes!("fixtures/listed5.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&png[..]));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let channels = info.color_type.samples();
    for (i, want) in buf.chunks(channels).take(N * N).enumerate() {
        let (r, c) = (i / N, i % N);
        let at = ((4 * r + 2) * w as usize + 4 * c + 2) * 3;
        for ch in 0..3 {
            let d = i16::from(rgb[at + ch]) - i16::from(want[ch]);
            assert!(
                d.abs() <= 1,
                "cell ({r}, {c}): {:?} vs {:?}",
                &rgb[at..at + 3],
                &want[..3]
            );
        }
    }
}

#[test]
fn a_builtin_map_as_a_value_renders_as_by_name() {
    let draw = |cmap: CmapSpec| {
        let data: Vec<f64> = (0..64).map(f64::from).collect();
        let mut fig = Figure::new(2.0, 1.0);
        let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
        ax.set_axis_off();
        ax.imshow(&data, 1, 64).cmap(cmap);
        fig.encode_png().unwrap()
    };
    assert_eq!(draw("viridis".into()), draw(viridis().into()));
}

#[test]
fn a_registered_name_draws_its_map() {
    assert!(register_colormap(
        "starfield_bv",
        ListedColormap::from_fn(starfield_bv)
    ));
    let draw = |cmap: CmapSpec| {
        let data: Vec<f64> = (0..64).map(f64::from).collect();
        let mut fig = Figure::new(2.0, 1.0);
        let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
        ax.set_axis_off();
        ax.imshow(&data, 1, 64).cmap(cmap);
        fig.encode_png().unwrap()
    };
    let by_value = draw(ListedColormap::from_fn(starfield_bv).into());
    assert_eq!(draw("starfield_bv".into()), by_value);
    assert_ne!(draw("starfield_bv_r".into()), by_value);
    let reversed = ListedColormap::from_fn(starfield_bv).reversed();
    assert_eq!(draw("starfield_bv_r".into()), draw(reversed.into()));
    assert!(rizzma::core::color::colormap("starfield_bv").is_some());
    assert!(!register_colormap(
        "magma",
        ListedColormap::from_fn(starfield_bv)
    ));
    assert!(!register_colormap(
        "lut:x",
        ListedColormap::from_fn(starfield_bv)
    ));
    assert!(!register_colormap(
        "x_r",
        ListedColormap::from_fn(starfield_bv)
    ));
    assert!(!register_colormap("other", "viridis"));
    assert_eq!(
        ListedColormap::from_fn(starfield_bv)
            .sample(0.0)
            .to_u8_array()[..3],
        starfield_reference()[0]
    );
}
