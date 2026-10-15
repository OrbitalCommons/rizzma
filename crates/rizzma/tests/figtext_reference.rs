//! `Figure::text` against matplotlib: for each anchor, the inked pixels of
//! the rendered string must land where matplotlib's do, to within a pixel, at
//! 100 and 200 DPI. The reference boxes come from `fixtures/figtext_mpl.py`.

use rizzma::{Figure, HorizontalAlignment, VerticalAlignment};

/// rizzma size (px at 100 DPI) and string the fixture was rendered with.
const SIZE_PX: f64 = 20.0;
const TEXT: &str = "HELL";
const REFERENCE: &str = include_str!("fixtures/figtext_mpl.txt");

/// One fixture line: `dpi x y ha va rotation x0 y0 x1 y1`.
struct Case {
    dpi: f64,
    x: f64,
    y: f64,
    ha: HorizontalAlignment,
    va: VerticalAlignment,
    rotation: f64,
    ink: [u32; 4],
}

fn cases() -> Vec<Case> {
    REFERENCE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f[i].parse::<f64>().expect("number");
            Case {
                dpi: num(0),
                x: num(1),
                y: num(2),
                ha: match f[3] {
                    "left" => HorizontalAlignment::Left,
                    "center" => HorizontalAlignment::Center,
                    "right" => HorizontalAlignment::Right,
                    other => panic!("ha {other}"),
                },
                va: match f[4] {
                    "top" => VerticalAlignment::Top,
                    "center" => VerticalAlignment::Center,
                    "baseline" => VerticalAlignment::Baseline,
                    "bottom" => VerticalAlignment::Bottom,
                    other => panic!("va {other}"),
                },
                rotation: num(5),
                ink: [6, 7, 8, 9].map(|i| f[i].parse().expect("pixel")),
            }
        })
        .collect()
}

/// Inclusive bounding box `[x0, y0, x1, y1]` (top-left origin) of the pixels
/// darker than mid-gray.
fn ink_box(fig: &Figure) -> [u32; 4] {
    let r = fig.render();
    let pm = r.pixmap();
    let mut ink = [u32::MAX, u32::MAX, 0, 0];
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            let p = pm.pixel(x, y).expect("in bounds");
            let gray = (u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) / 3;
            if gray < 128 {
                ink = [ink[0].min(x), ink[1].min(y), ink[2].max(x), ink[3].max(y)];
            }
        }
    }
    ink
}

#[test]
fn figure_text_lands_where_matplotlib_puts_it() {
    let cases = cases();
    assert_eq!(cases.len(), 12, "fixture covers 6 anchors at 2 DPIs");
    let mut failures = Vec::new();
    for case in cases {
        let mut fig = Figure::new(4.0, 3.0).with_dpi(case.dpi);
        fig.text(case.x, case.y, TEXT)
            .set_size(SIZE_PX)
            .set_ha(case.ha)
            .set_va(case.va)
            .set_rotation(case.rotation);
        let got = ink_box(&fig);
        let off: Vec<i64> = (0..4)
            .map(|i| i64::from(got[i]) - i64::from(case.ink[i]))
            .collect();
        if off.iter().any(|d| d.abs() > 1) {
            failures.push(format!(
                "dpi {} ({}, {}) {:?}/{:?} rot {}: rizzma {got:?} vs matplotlib {:?}",
                case.dpi, case.x, case.y, case.ha, case.va, case.rotation, case.ink
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
