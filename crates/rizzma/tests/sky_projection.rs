//! Sky-projection axes against matplotlib 3.11: a Mollweide `pcolormesh`
//! over a 2° longitude/latitude grid matches `projection="mollweide"` to
//! within one unit per channel inside every cell, away from the outline;
//! cell edges land within a few pixels' ownership of matplotlib's (Agg and
//! tiny-skia break ties along slanted edges differently). The reference
//! comes from `fixtures/sky_mollweide_mpl.py`.

use std::f64::consts::PI;

use rizzma::Figure;
use rizzma::figure::Mappable;
use rizzma::figure::sky::SkyProjection;

const W: usize = 600;
const H: usize = 300;

/// Cell edges every `step` degrees from `lo` to `hi`, in radians.
fn edges(lo: i32, hi: i32, step: i32) -> Vec<f64> {
    (lo..=hi)
        .step_by(step as usize)
        .map(|d| f64::from(d).to_radians())
        .collect()
}

/// The fixture field at the cell centers, row-major by latitude.
fn field(lon: &[f64], lat: &[f64]) -> Vec<f64> {
    let centers = |e: &[f64]| -> Vec<f64> { e.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect() };
    let (clon, clat) = (centers(lon), centers(lat));
    clat.iter()
        .flat_map(|&la| {
            clon.iter()
                .map(move |&lo| (2.0 * lo).sin() * la.cos() + 0.5 * (3.0 * la).sin())
        })
        .collect()
}

/// The reference canvas as RGB triples, top row first.
fn reference() -> Vec<[u8; 3]> {
    let png_bytes = include_bytes!("fixtures/sky_mollweide_mesh.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes[..]));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    assert_eq!((info.width, info.height), (W as u32, H as u32));
    let channels = info.color_type.samples();
    buf.chunks(channels)
        .take(W * H)
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}

fn mollweide_figure() -> Figure {
    let (lon, lat) = (edges(-180, 180, 2), edges(-90, 90, 2));
    let c = field(&lon, &lat);
    let mut fig = Figure::new(6.0, 3.0);
    let ax = fig.add_axes(0.05, 0.05, 0.9, 0.9);
    ax.set_sky_projection(SkyProjection::Mollweide)
        .set_x_tick_labels_visible(false);
    ax.yaxis_mut().set_tick_labels_visible(false);
    ax.pcolormesh_xy(&lon, &lat, &c, "viridis");
    fig
}

/// The 2° cell `(row, col)` under image point `(x, y)` (top-down pixels),
/// by inverting the Mollweide projection of the map filling the
/// 540x270 axes rect centered at (300, 150).
fn cell_at(x: f64, y: f64) -> (i32, i32) {
    let sqrt2 = std::f64::consts::SQRT_2;
    let (nx, ny) = (
        (x - 300.0) / 270.0 * 2.0 * sqrt2,
        (150.0 - y) / 135.0 * sqrt2,
    );
    let theta = (ny / sqrt2).clamp(-1.0, 1.0).asin();
    let lat = ((2.0 * theta + (2.0 * theta).sin()) / PI).asin();
    let lon = PI * nx / (2.0 * sqrt2 * theta.cos());
    let step = 2f64.to_radians();
    (
        ((lat + PI / 2.0) / step).floor() as i32,
        ((lon + PI) / step).floor() as i32,
    )
}

#[test]
fn mollweide_pcolormesh_matches_matplotlib() {
    let (rgb, w, h) = mollweide_figure().to_rgb8();
    assert_eq!((w as usize, h as usize), (W, H));
    let want = reference();

    let diff = |a: [u8; 3], b: [u8; 3]| (0..3).map(|ch| a[ch].abs_diff(b[ch])).max().unwrap_or(0);
    let ours = |i: usize| [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]];

    // The outline is the ellipse inscribed in the 540x270 axes rect; skip
    // pixels within 3 px of it, where the spine and the cells' clipped
    // edges draw.
    let (cx, cy, rx, ry) = (300.0, 150.0, 270.0, 135.0);
    let mut worst = (0u8, 0usize);
    let (mut compared, mut cores, mut agree) = (0usize, 0usize, 0usize);
    let mut displaced = Vec::new();
    for py in 3..H - 3 {
        for px in 3..W - 3 {
            let (dx, dy) = ((px as f64 + 0.5 - cx) / rx, (py as f64 + 0.5 - cy) / ry);
            // Distance to the outline, approximately, in pixels.
            let inside = (1.0 - dx.hypot(dy)) * ry.min(rx / (1.0 + 3.0 * dx * dx).sqrt());
            if inside < 3.0 {
                continue;
            }
            let i = py * W + px;
            compared += 1;
            let d = diff(ours(i), want[i]);
            agree += usize::from(d <= 1);
            // Wherever a cell edge falls, it falls within three pixels of
            // matplotlib's: some pixel that near matches this one. Within
            // 5° of the poles matplotlib swaps its Newton solve for a
            // series approximation and the rows thin to slivers, so the
            // caps and the rows bordering them are left out.
            let cell = cell_at(px as f64 + 0.5, py as f64 + 0.5);
            let polar = cell.0 < 4 || cell.0 > 85;
            let near = (py - 3..=py + 3)
                .any(|y| (px - 3..=px + 3).any(|x| diff(ours(y * W + x), want[i]) <= 1));
            if !near && !polar {
                displaced.push((px, py));
            }
            // A cell-core pixel: the centers of its whole 3x3 neighborhood
            // fall in one cell, so it lies a pixel or more inside that
            // cell, where which cell owns an edge pixel cannot matter.
            let core = (0..3).all(|ny| {
                (0..3).all(|nx| cell_at((px + nx) as f64 - 0.5, (py + ny) as f64 - 0.5) == cell)
            });
            if core {
                cores += 1;
                if d > worst.0 {
                    worst = (d, i);
                }
            }
        }
    }
    let at = worst.1;
    assert!(cores > 4_000, "only {cores} cell-core pixels compared");
    assert!(
        worst.0 <= 1,
        "cell-core pixel ({}, {}) off by {}: rizzma {:?} vs matplotlib {:?}",
        at % W,
        at / W,
        worst.0,
        ours(at),
        want[at]
    );
    // Along slanted cell edges, Agg and tiny-skia decide pixel ownership by
    // different rules, so a minority of edge pixels take the neighboring
    // cell's color; everywhere else the maps agree.
    let share = agree as f64 / compared as f64;
    assert!(share > 0.83, "only {share:.3} of {compared} pixels agree");
    assert!(
        displaced.is_empty(),
        "{} pixels match nothing within 3 px, first {:?}",
        displaced.len(),
        &displaced[..displaced.len().min(5)]
    );
}

#[test]
fn meshes_draw_on_log_axes() {
    // Three decade-wide columns on a log x axis come out equally wide.
    let mut fig = Figure::new(4.0, 2.0);
    let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
    ax.set_axis_off();
    ax.pcolormesh_xy(
        &[1.0, 10.0, 100.0, 1000.0],
        &[0.0, 1.0],
        &[0.0, 1.0, 2.0],
        "gray",
    );
    ax.set_xscale_log(10.0)
        .set_xlim(1.0, 1000.0)
        .set_ylim(0.0, 1.0);
    let (rgb, w, _) = fig.to_rgb8();
    let row: Vec<u8> = (0..w as usize)
        .map(|x| rgb[(100 * w as usize + x) * 3])
        .collect();
    // gray runs black → white across the three cells.
    let thirds = [row[66], row[200], row[333]];
    assert_eq!(thirds, [0, 128, 255], "cells {thirds:?}");
    let edge = row.iter().position(|&v| v != 0).expect("a lighter cell");
    assert!((132..=134).contains(&edge), "first edge at {edge}");
}

#[test]
fn sky_axes_take_a_colorbar() {
    let mut fig = mollweide_figure();
    let ax = &mut fig.axes_mut()[0];
    ax.colorbar(Mappable::Mesh(0));
    let bar = fig.colorbar_frame(0).expect("the sky axes has a colorbar");
    let map = fig.axes_frame(0).expect("the sky axes has a frame");
    assert!(map.xmax() < bar.xmin(), "map {map:?} overlaps bar {bar:?}");

    // The bar runs viridis from dark purple at the bottom to yellow at the
    // top (image rows run top-down).
    let (rgb, w, h) = fig.to_rgb8();
    let (w, h) = (w as f64, h as f64);
    let at = |fx: f64, fy: f64| {
        let (x, y) = ((fx * w) as usize, ((1.0 - fy) * h) as usize);
        let i = (y * w as usize + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    let cx = 0.5 * (bar.xmin() + bar.xmax());
    let (top, bottom) = (at(cx, bar.ymax() - 0.02), at(cx, bar.ymin() + 0.02));
    assert!(top[0] > 200 && top[1] > 200 && top[2] < 100, "top {top:?}");
    assert!(
        bottom[0] < 100 && bottom[1] < 60 && bottom[2] > 60,
        "bottom {bottom:?}"
    );
}

#[test]
fn sky_lines_follow_the_projection() {
    // A parallel at 60° drawn as one straight segment must bow along the
    // projection, not cut straight across: its middle sits at the projected
    // y of 60°, and its ends lie inside the outline.
    let mut fig = Figure::new(6.0, 3.0);
    let ax = fig.add_axes(0.05, 0.05, 0.9, 0.9);
    ax.set_sky_projection(SkyProjection::Aitoff).set_axis_off();
    ax.plot(&[-PI, PI], &[PI / 3.0, PI / 3.0]);
    let (rgb, w, _) = fig.to_rgb8();
    let ink_rows = |x: usize| -> Vec<usize> {
        (0..H)
            .filter(|&y| rgb[(y * w as usize + x) * 3] < 128)
            .collect()
    };
    // Aitoff maps latitude linearly along the central meridian, so 60° sits
    // two thirds of the 135 px half height above the equator at row 150.
    let mid = ink_rows(300);
    let expected = 150.0 - 135.0 * 2.0 / 3.0;
    assert!(
        mid.iter().any(|&y| (y as f64 + 0.5 - expected).abs() < 1.5),
        "middle ink rows {mid:?}, expected near {expected}"
    );
    // Toward the western edge Aitoff parallels bow up toward the pole; a
    // straight chord would stay on the middle row.
    let west = ink_rows(200);
    assert!(
        !west.is_empty() && west.iter().all(|&y| (y as f64) < expected - 5.0),
        "western ink rows {west:?} should sit above {expected}"
    );
}
