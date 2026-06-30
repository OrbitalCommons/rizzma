//! Renders one figure per Tier-1 plot type into `target/gallery_*.png`.
//! A quick visual catalogue of what rizzma can draw today.

use std::f64::consts::{PI, TAU};

use rizzma_core::color::Rgba;
use rizzma_figure::Figure;

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
        .collect()
}

fn main() {
    // 1. plot (line)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(0.0, TAU, 200);
        let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.plot(&x, &y);
        ax.set_title("plot");
        ax.set_xlabel("x");
        ax.set_ylabel("sin(x)");
        fig.save_png("target/gallery_plot.png").unwrap();
    }

    // 2. scatter (colormapped)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let t = linspace(0.0, 6.0 * PI, 240);
        let x: Vec<f64> = t.iter().map(|a| a * a.cos()).collect();
        let y: Vec<f64> = t.iter().map(|a| a * a.sin()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.scatter_mapped(&x, &y, &t, "viridis");
        ax.set_title("scatter");
        fig.save_png("target/gallery_scatter.png").unwrap();
    }

    // 3. bar
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let h = [3.0, 7.0, 2.0, 5.0, 4.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.bar(&x, &h);
        ax.set_title("bar");
        fig.save_png("target/gallery_bar.png").unwrap();
    }

    // 4. barh
    {
        let mut fig = Figure::new(5.0, 3.5);
        let y = [0.0, 1.0, 2.0, 3.0];
        let w = [2.0, 5.0, 3.0, 6.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.barh(&y, &w);
        ax.set_title("barh");
        fig.save_png("target/gallery_barh.png").unwrap();
    }

    // 5. hist
    {
        let mut fig = Figure::new(5.0, 3.5);
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = || {
            // xorshift64* → uniform in [0,1)
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
        };
        // Two clusters (sum-of-uniforms ≈ gaussian).
        let mut data = Vec::new();
        for _ in 0..240 {
            let g = (next() + next() + next() + next()) / 4.0;
            data.push(g * 2.0 - 1.5);
        }
        for _ in 0..160 {
            let g = (next() + next() + next() + next()) / 4.0;
            data.push(g * 2.0 + 1.0);
        }
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.hist(&data, 24);
        ax.set_title("hist");
        fig.save_png("target/gallery_hist.png").unwrap();
    }

    // 6. fill_between
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(0.0, TAU, 120);
        let y1: Vec<f64> = x.iter().map(|v| v.sin()).collect();
        let y2: Vec<f64> = x.iter().map(|v| 0.3 * (2.0 * v).sin()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.fill_between(&x, &y1, &y2);
        ax.plot(&x, &y1);
        ax.plot(&x, &y2);
        ax.set_title("fill_between");
        fig.save_png("target/gallery_fill_between.png").unwrap();
    }

    // 7. step
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let y = [1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 2.0, 4.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.step(&x, &y);
        ax.set_title("step");
        fig.save_png("target/gallery_step.png").unwrap();
    }

    // 8. errorbar
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(0.0, 10.0, 10);
        let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
        let yerr: Vec<f64> = x.iter().map(|v| 0.1 + 0.1 * v.cos().abs()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.errorbar(&x, &y, &yerr);
        ax.set_title("errorbar");
        fig.save_png("target/gallery_errorbar.png").unwrap();
    }

    // 9. reference lines & spans
    {
        let mut fig = Figure::new(5.0, 3.5);
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        ax.axhspan(1.0, 2.0);
        ax.axvspan(7.5, 8.5);
        ax.axhline(5.0);
        ax.axvline(3.0);
        ax.hlines(&[8.0], 1.0, 9.0);
        ax.vlines(&[6.0], 1.0, 9.0);
        ax.set_title("axhline / axvline / spans / hlines / vlines");
        fig.save_png("target/gallery_reflines.png").unwrap();
    }

    // 10. imshow
    {
        let mut fig = Figure::new(5.0, 3.8);
        let (nr, nc) = (80usize, 100usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for c in 0..nc {
                let yy = (r as f64 / nr as f64 - 0.5) * 12.0;
                let xx = (c as f64 / nc as f64 - 0.5) * 12.0;
                data[r * nc + c] = (xx * xx + yy * yy).sqrt().sin();
            }
        }
        let ax = fig.add_axes(0.13, 0.13, 0.80, 0.78);
        ax.imshow(&data, nr, nc);
        ax.set_title("imshow");
        fig.save_png("target/gallery_imshow.png").unwrap();
    }

    // 11. legend + colorbar
    {
        let mut fig = Figure::new(6.0, 4.0);
        let x = linspace(0.0, TAU, 200);
        let s: Vec<f64> = x.iter().map(|v| v.sin()).collect();
        let c: Vec<f64> = x.iter().map(|v| v.cos()).collect();
        {
            let ax = fig.add_axes(0.12, 0.13, 0.72, 0.76);
            ax.plot(&x, &s);
            ax.plot(&x, &c);
            ax.legend(vec![
                (Rgba::from_hex("#1f77b4").unwrap(), "sin(x)".into()),
                (Rgba::from_hex("#ff7f0e").unwrap(), "cos(x)".into()),
            ]);
            ax.set_title("legend + colorbar");
        }
        fig.colorbar("viridis", -1.0, 1.0);
        fig.save_png("target/gallery_legend_colorbar.png").unwrap();
    }

    // 12. stem
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(0.0, TAU, 24);
        let y: Vec<f64> = x.iter().map(|v| v.sin() * (-0.2 * v).exp()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.stem(&x, &y);
        ax.set_title("stem");
        fig.save_png("target/gallery_stem.png").unwrap();
    }

    // 13. stairs
    {
        let mut fig = Figure::new(5.0, 3.5);
        let values = [1.0, 3.0, 2.0, 4.0, 3.5, 2.5, 4.5, 3.0];
        let edges = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.stairs(&values, &edges);
        ax.set_title("stairs");
        fig.save_png("target/gallery_stairs.png").unwrap();
    }

    // 14. pcolormesh
    {
        let mut fig = Figure::new(5.0, 3.8);
        let (nr, nc) = (20usize, 20usize);
        let mut c = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = r as f64 / nr as f64 * TAU;
                let xx = col as f64 / nc as f64 * TAU;
                c[r * nc + col] = xx.sin() * yy.cos();
            }
        }
        let ax = fig.add_axes(0.13, 0.13, 0.80, 0.78);
        ax.pcolormesh(&c, nr, nc);
        ax.set_title("pcolormesh");
        fig.save_png("target/gallery_pcolormesh.png").unwrap();
    }

    // 15. stackplot
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(0.0, TAU, 80);
        let a: Vec<f64> = x.iter().map(|v| 1.0 + 0.5 * v.sin()).collect();
        let b: Vec<f64> = x.iter().map(|v| 1.5 + 0.5 * (2.0 * v).cos()).collect();
        let c: Vec<f64> = x.iter().map(|v| 1.0 + 0.3 * (0.5 * v).sin()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.stackplot(&x, &[&a, &b, &c]);
        ax.set_title("stackplot");
        fig.save_png("target/gallery_stackplot.png").unwrap();
    }

    // 16. broken_barh
    {
        let mut fig = Figure::new(5.0, 3.5);
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.broken_barh(&[(1.0, 3.0), (5.0, 2.0), (8.0, 4.0)], (10.0, 4.0));
        ax.broken_barh(&[(2.0, 4.0), (7.5, 3.0)], (20.0, 4.0));
        ax.set_title("broken_barh");
        fig.save_png("target/gallery_broken_barh.png").unwrap();
    }

    // 17. boxplot
    {
        let mut fig = Figure::new(5.0, 3.5);
        // Four datasets with different spreads and a couple of clear outliers.
        let tight = [4.0, 4.5, 5.0, 5.0, 5.5, 6.0];
        let wide = [1.0, 3.0, 5.0, 7.0, 9.0, 11.0];
        let skewed = [2.0, 2.5, 3.0, 3.5, 4.0, 9.0];
        let with_outlier = [5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 16.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.boxplot(&[&tight, &wide, &skewed, &with_outlier]);
        ax.set_title("boxplot");
        fig.save_png("target/gallery_boxplot.png").unwrap();
    }

    // 18. mathtext title (`$...$` math in an Axes title)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(-3.0, 3.0, 200);
        let y: Vec<f64> = x.iter().map(|v| v * v + 0.5).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.plot(&x, &y);
        ax.set_title("$y = x^2 + \\frac{1}{2}$");
        fig.save_png("target/gallery_mathtext.png").unwrap();
    }

    // 19. contour
    {
        let mut fig = Figure::new(5.0, 3.8);
        let (nr, nc) = (30usize, 30usize);
        let mut z = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = r as f64 / (nr - 1) as f64 * TAU;
                let xx = col as f64 / (nc - 1) as f64 * TAU;
                z[r * nc + col] = xx.sin() * yy.cos();
            }
        }
        let ax = fig.add_axes(0.13, 0.13, 0.80, 0.78);
        ax.contour(&z, nr, nc);
        ax.set_title("contour");
        fig.save_png("target/gallery_contour.png").unwrap();
    }

    // 20. eventplot
    {
        let mut fig = Figure::new(5.0, 3.5);
        // Three neuron-like spike trains at increasing rates.
        let row0: Vec<f64> = (0..8).map(|i| i as f64 * 1.2 + 0.3).collect();
        let row1: Vec<f64> = (0..14).map(|i| i as f64 * 0.7 + 0.1).collect();
        let row2: Vec<f64> = (0..20).map(|i| i as f64 * 0.5).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.eventplot(&[&row0, &row1, &row2]);
        ax.set_title("eventplot");
        fig.save_png("target/gallery_eventplot.png").unwrap();
    }

    // 21. fill_betweenx
    {
        let mut fig = Figure::new(5.0, 3.5);
        let y = linspace(0.0, TAU, 120);
        let x1: Vec<f64> = y.iter().map(|v| v.sin()).collect();
        let x2: Vec<f64> = y.iter().map(|v| v.sin() + 1.0 + 0.3 * v.cos()).collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.fill_betweenx(&y, &x1, &x2);
        ax.set_title("fill_betweenx");
        fig.save_png("target/gallery_fill_betweenx.png").unwrap();
    }

    // 22. ecdf
    {
        let mut fig = Figure::new(5.0, 3.5);
        // A lumpy sample whose empirical CDF rises in clear steps.
        let data = [
            0.2, 0.5, 0.5, 0.9, 1.1, 1.1, 1.4, 1.8, 2.0, 2.3, 2.3, 2.7, 3.0, 3.0, 3.4, 3.9, 4.2,
            4.2, 4.8, 5.0,
        ];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.ecdf(&data);
        ax.set_title("ecdf");
        fig.save_png("target/gallery_ecdf.png").unwrap();
    }

    // 23. matshow
    {
        let mut fig = Figure::new(5.0, 3.8);
        let (nr, nc) = (8usize, 8usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for c in 0..nc {
                data[r * nc + c] = (r as f64 - c as f64).abs();
            }
        }
        let ax = fig.add_axes(0.13, 0.13, 0.80, 0.78);
        ax.matshow(&data, nr, nc);
        ax.set_title("matshow");
        fig.save_png("target/gallery_matshow.png").unwrap();
    }

    // 24. spy
    {
        let mut fig = Figure::new(5.0, 3.8);
        let (nr, nc) = (16usize, 16usize);
        let mut data = vec![0.0; nr * nc];
        // A banded sparsity pattern: main diagonal plus two off-diagonals.
        for r in 0..nr {
            for c in 0..nc {
                if r == c || r.abs_diff(c) == 3 {
                    data[r * nc + c] = 1.0;
                }
            }
        }
        let ax = fig.add_axes(0.13, 0.13, 0.80, 0.78);
        ax.spy(&data, nr, nc);
        ax.set_title("spy");
        fig.save_png("target/gallery_spy.png").unwrap();
    }

    // 25. hist2d
    {
        let mut fig = Figure::new(5.0, 3.8);
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut next = || {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
        };
        // A correlated blob (sum-of-uniforms ≈ gaussian) for a clear 2D density.
        let mut x = Vec::new();
        let mut y = Vec::new();
        for _ in 0..2000 {
            let gx = (next() + next() + next() + next()) / 4.0;
            let gy = (next() + next() + next() + next()) / 4.0;
            let a = gx * 4.0 - 2.0;
            x.push(a);
            y.push(0.6 * a + (gy * 2.0 - 1.0));
        }
        let ax = fig.add_axes(0.14, 0.14, 0.78, 0.76);
        ax.hist2d(&x, &y, 30);
        ax.set_title("hist2d");
        fig.save_png("target/gallery_hist2d.png").unwrap();
    }

    // 26. pie
    {
        // A square figure so the equal-aspect pie reads as a clean circle.
        let mut fig = Figure::new(4.0, 4.0);
        let ax = fig.add_axes(0.1, 0.1, 0.8, 0.8);
        ax.pie(&[35.0, 25.0, 20.0, 15.0, 5.0]);
        ax.set_title("pie");
        fig.save_png("target/gallery_pie.png").unwrap();
    }

    // 27. violinplot
    {
        let mut fig = Figure::new(5.0, 3.5);
        // Four deterministic groups with distinct shapes: a tight cluster, a
        // wide spread, a bimodal set, and a right-skewed set. Built from
        // closed-form sine/quadratic perturbations so there is no RNG.
        let tight: Vec<f64> = (0..60)
            .map(|k| 5.0 + 0.6 * (k as f64 * 0.7).sin())
            .collect();
        let wide: Vec<f64> = (0..60)
            .map(|k| 5.0 + 3.0 * (k as f64 * 0.21).sin())
            .collect();
        let bimodal: Vec<f64> = (0..60)
            .map(|k| {
                let lobe = if k % 2 == 0 { 3.0 } else { 7.0 };
                lobe + 0.8 * (k as f64 * 0.5).sin()
            })
            .collect();
        let skewed: Vec<f64> = (0..60)
            .map(|k| {
                let t = k as f64 / 59.0;
                3.0 + 6.0 * t * t
            })
            .collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.violinplot(&[&tight, &wide, &bimodal, &skewed], None);
        ax.set_title("violinplot");
        fig.save_png("target/gallery_violinplot.png").unwrap();
    }

    // 28. hexbin
    {
        let mut fig = Figure::new(5.0, 3.8);
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
        };
        // Two overlapping correlated blobs (sum-of-uniforms ~ gaussian) so the
        // hexagons fill in densely and the viridis gradient is visible.
        let mut x = Vec::new();
        let mut y = Vec::new();
        for _ in 0..4000 {
            let gx = (next() + next() + next() + next()) / 4.0;
            let gy = (next() + next() + next() + next()) / 4.0;
            let a = gx * 4.0 - 2.0;
            x.push(a - 1.0);
            y.push(0.7 * a + (gy * 2.0 - 1.0) - 0.5);
        }
        for _ in 0..3000 {
            let gx = (next() + next() + next() + next()) / 4.0;
            let gy = (next() + next() + next() + next()) / 4.0;
            x.push(gx * 3.0 + 1.0);
            y.push(gy * 3.0 + 1.0);
        }
        let ax = fig.add_axes(0.14, 0.14, 0.78, 0.76);
        ax.hexbin(&x, &y, 30);
        ax.set_title("hexbin");
        fig.save_png("target/gallery_hexbin.png").unwrap();
    }

    // 29. grouped_bar
    {
        let mut fig = Figure::new(5.0, 3.5);
        // Three series across four groups, with visibly distinct heights.
        let s0 = [3.0, 6.0, 4.0, 7.0];
        let s1 = [5.0, 2.0, 8.0, 3.0];
        let s2 = [4.0, 5.0, 2.0, 6.0];
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.grouped_bar(&[&s0, &s1, &s2]);
        ax.set_title("grouped_bar");
        fig.save_png("target/gallery_grouped_bar.png").unwrap();
    }

    // 30. loglog
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(1.0, 1000.0, 120);
        let y: Vec<f64> = x.iter().map(|v| v * v).collect();
        let ax = fig.add_axes(0.17, 0.16, 0.76, 0.72);
        ax.loglog(&x, &y);
        ax.set_xlim(1.0, 1000.0);
        ax.set_ylim(1.0, 1_000_000.0);
        ax.set_title("loglog");
        ax.set_xlabel("x");
        ax.set_ylabel("$x^2$");
        fig.save_png("target/gallery_loglog.png").unwrap();
    }

    // 31. quiver (rotational vector field u = -y, v = x over a grid)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let coords = linspace(-3.0, 3.0, 7);
        let (mut x, mut y, mut u, mut v) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for &gy in &coords {
            for &gx in &coords {
                x.push(gx);
                y.push(gy);
                u.push(-gy);
                v.push(gx);
            }
        }
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.quiver(&x, &y, &u, &v);
        ax.set_title("quiver");
        fig.save_png("target/gallery_quiver.png").unwrap();
    }

    // 32. streamplot (rotational field u = -y, v = x: concentric circles)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let coords = linspace(-3.0, 3.0, 25);
        let nx = coords.len();
        let ny = coords.len();
        let mut u = vec![0.0; nx * ny];
        let mut v = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                u[j * nx + i] = -coords[j];
                v[j * nx + i] = coords[i];
            }
        }
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.streamplot(&coords, &coords, &u, &v);
        ax.set_title("streamplot");
        fig.save_png("target/gallery_streamplot.png").unwrap();
    }

    // A 6x6 grid of vertices over [0, 1]^2, each cell split into two
    // triangles, shared by the triplot and tripcolor cases below.
    let grid = 6;
    let coords = linspace(0.0, 1.0, grid);
    let mut vx = Vec::new();
    let mut vy = Vec::new();
    for &gy in &coords {
        for &gx in &coords {
            vx.push(gx);
            vy.push(gy);
        }
    }
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    for r in 0..grid - 1 {
        for col in 0..grid - 1 {
            let v00 = r * grid + col;
            let v01 = r * grid + col + 1;
            let v10 = (r + 1) * grid + col;
            let v11 = (r + 1) * grid + col + 1;
            triangles.push([v00, v01, v11]);
            triangles.push([v00, v11, v10]);
        }
    }

    // 33. triplot (wireframe of the triangulated unit-square grid)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.triplot(&vx, &vy, &triangles);
        ax.set_title("triplot");
        fig.save_png("target/gallery_triplot.png").unwrap();
    }

    // 34. tripcolor (flat shading by a radial field over the same mesh)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let values: Vec<f64> = vx
            .iter()
            .zip(&vy)
            .map(|(&x, &y)| (x - 0.5).hypot(y - 0.5))
            .collect();
        let ax = fig.add_axes(0.15, 0.15, 0.80, 0.74);
        ax.tripcolor(&vx, &vy, &triangles, &values);
        ax.set_title("tripcolor");
        fig.save_png("target/gallery_tripcolor.png").unwrap();
    }

    // 35. symlog (cubic spanning negative, zero, and positive tails)
    {
        let mut fig = Figure::new(5.0, 3.5);
        let x = linspace(-100.0, 100.0, 161);
        let y: Vec<f64> = x.iter().map(|v| v * v * v).collect();
        let ax = fig.add_axes(0.17, 0.16, 0.76, 0.72);
        ax.plot(&x, &y);
        ax.set_xscale_symlog(10.0, 1.0)
            .set_yscale_symlog(10.0, 1.0)
            .set_xlim(-100.0, 100.0)
            .set_ylim(-1_000_000.0, 1_000_000.0);
        ax.set_title("symlog");
        ax.set_xlabel("x");
        ax.set_ylabel("$x^3$");
        fig.save_png("target/gallery_symlog.png").unwrap();
    }

    println!("wrote target/gallery_*.png");
}
