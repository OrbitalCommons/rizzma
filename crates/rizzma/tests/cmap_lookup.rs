//! Colormaps pick the lookup-table entry matplotlib does, `floor(t * N)`: at
//! every sample of `fixtures/cmap_lookup_mpl.txt` (each entry hit twice, bin
//! edges included) the color matches matplotlib 3.11 to within one unit per
//! channel — the gap between matplotlib truncating to 8 bits and rizzma
//! rounding. A neighbouring entry would be off by up to 5.

use rizzma::core::color::colormap;

const REFERENCE: &str = include_str!("fixtures/cmap_lookup_mpl.txt");

#[test]
fn colormap_lookup_matches_matplotlib() {
    let mut checked = 0;
    for line in REFERENCE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let cmap = colormap(f[0]).expect("known colormap");
        let t: f64 = f[1].parse().unwrap();
        let want: Vec<i16> = f[2..5].iter().map(|v| v.parse().unwrap()).collect();
        let got = cmap.sample(t).to_u8_array();
        for c in 0..3 {
            assert!(
                (i16::from(got[c]) - want[c]).abs() <= 1,
                "{} at t = {t}: rizzma {got:?} vs matplotlib {want:?}",
                f[0]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 2 * 513);
}
