//! What `rizzma = { version = "1", default-features = false }` has offered
//! since 1.13: raster output and date axes. CI builds and runs this with
//! `--no-default-features`, so a 1.x release cannot take them away (1.14.0
//! did, behind `raster` and `dates`; 1.14.1 restored them).

use rizzma::Figure;
use rizzma::axis::dates::date2num;

#[test]
fn raster_output_and_date_axes_need_no_features() {
    let day = |d: u32| {
        date2num(
            chrono::NaiveDate::from_ymd_opt(2026, 10, d)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
    };
    let mut fig = Figure::new(3.0, 2.0);
    let ax = fig.add_subplot(1, 1, 1);
    ax.plot(&[day(1), day(5), day(9)], &[1.0, 3.0, 2.0]);
    ax.set_xaxis_date();

    let png = fig.encode_png().expect("png");
    assert!(png.starts_with(b"\x89PNG"));
    let (rgba, w, h) = fig.to_rgba8();
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    assert!(fig.render().encode_png().is_ok());
    let dir = std::env::temp_dir().join("rizzma-no-default-features-api.png");
    fig.save_png(&dir).expect("save");
}
