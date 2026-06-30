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

    println!("wrote target/gallery_*.png");
}
