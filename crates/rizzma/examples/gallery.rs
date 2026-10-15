//! Renders one figure per Tier-1 plot type into `target/gallery_*.png`.
//! A quick visual catalogue of what rizzma can draw today — each case sticks
//! to its plot type but plots something worth looking at.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use chrono::NaiveDate;
use rizzma::artist::Patch;
use rizzma::axis::dates::date2num;
use rizzma::core::color::Rgba;
use rizzma::figure::{Figure, PolarAxes, SkyAxes, SkyProjection, TextBoxStyle};

/// Render docs/gallery images at publication-friendly resolution. The figure
/// dimensions stay in inches so layout proportions are unchanged; the DPI is
/// raised per figure so every generated gallery image is at least this wide.
const GALLERY_MIN_WIDTH_PX: f64 = 1600.0;
const GALLERY_SQUARE_PX: u32 = 1600;
const GALLERY_SQUARE_DPI: f64 = 320.0;
const GALLERY_SKY_WIDTH_PX: u32 = 1600;
const GALLERY_SKY_HEIGHT_PX: u32 = 850;
const GALLERY_SKY_DPI: f64 = 250.0;
const GALLERY_3D_WIDTH_PX: u32 = 1600;
const GALLERY_3D_HEIGHT_PX: u32 = 1344;
const GALLERY_3D_DPI: f64 = 320.0;

fn gallery_figure(width_in: f64, height_in: f64) -> Figure {
    Figure::new(width_in, height_in).with_dpi((GALLERY_MIN_WIDTH_PX / width_in).ceil())
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
        .collect()
}

fn date_num(year: i32, month: u32, day: u32) -> f64 {
    date2num(
        NaiveDate::from_ymd_opt(year, month, day)
            .expect("gallery date is valid")
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid"),
    )
}

/// Deterministic xorshift64* uniform in [0, 1) so the gallery is reproducible.
fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        ((s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// The first three colors of the default (tab10) prop cycle, for legends that
/// annotate cycle-colored artists.
fn c(i: usize) -> Rgba {
    let hexes = ["#1f77b4", "#ff7f0e", "#2ca02c", "#d62728"];
    Rgba::from_hex(hexes[i]).expect("cycle hex is valid")
}

fn main() {
    // 1. plot (line) — two close frequencies beating against each other.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(0.0, 8.0 * TAU, 900);
        let y: Vec<f64> = x
            .iter()
            .map(|t| (3.0 * t).sin() + (3.35 * t).sin())
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_title("plot: two guitar strings beating");
        ax.set_xlabel("time (s)");
        ax.set_ylabel("amplitude");
        fig.save_png("target/gallery_plot.png").unwrap();
    }

    // 2. scatter (colormapped) — a two-armed spiral galaxy with star jitter.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let mut noise = rng(0xBADC0FFEE0DDF00D);
        let (mut x, mut y, mut t) = (Vec::new(), Vec::new(), Vec::new());
        for arm in 0..2 {
            let phase = arm as f64 * PI;
            for i in 0..160 {
                let a = i as f64 / 159.0 * 1.6 * TAU;
                let r = 0.55 * a + 0.35;
                let spread = 0.12 + 0.06 * a;
                let jx = (noise() - 0.5) * spread;
                let jy = (noise() - 0.5) * spread;
                x.push(r * (a + phase).cos() + jx);
                y.push(r * (a + phase).sin() + jy);
                t.push(a);
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.scatter_mapped(&x, &y, &t, "bgyw");
        ax.set_title("scatter: a tidy little galaxy");
        fig.save_png("target/gallery_scatter.png").unwrap();
    }

    // 3. bar — commit activity by hour, featuring the infamous 2am spike.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x: Vec<f64> = (0..24).map(|h| h as f64).collect();
        let h: Vec<f64> = (0..24)
            .map(|hr| {
                let hr = hr as f64;
                let day = 22.0 * (-((hr - 10.5) / 2.6).powi(2)).exp();
                let evening = 9.0 * (-((hr - 15.5) / 2.0).powi(2)).exp();
                let two_am = 13.0 * (-((hr - 2.0) / 1.1).powi(2)).exp();
                1.0 + day + evening + two_am
            })
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.bar(&x, &h);
        ax.set_title("bar: commits by hour of day");
        ax.set_xlabel("hour");
        ax.set_ylabel("commits");
        fig.save_png("target/gallery_bar.png").unwrap();
    }

    // 4. barh — coffee consumed per release candidate.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let y = [0.0, 1.0, 2.0, 3.0];
        let w = [2.0, 5.0, 9.0, 14.0];
        let ax = fig.add_subplot(1, 1, 1);
        ax.barh(&y, &w);
        ax.set_title("barh: coffee per release candidate");
        ax.set_xlabel("cups");
        fig.save_png("target/gallery_barh.png").unwrap();
    }

    // 5. hist — minutes to find a missing semicolon: short mode, long tail.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let mut next = rng(0x2545F4914F6CDD1D);
        let mut data = Vec::new();
        for _ in 0..380 {
            // Exponential bulk: most are found fast…
            let minutes = -(1.0 - next()).ln() * 4.0;
            data.push(minutes.min(30.0));
        }
        for _ in 0..20 {
            // …and a haunted few take most of an hour.
            data.push(35.0 + next() * 20.0);
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.hist(&data, 28);
        ax.set_title("hist: minutes to find the semicolon");
        ax.set_xlabel("minutes");
        ax.set_ylabel("incidents");
        fig.save_png("target/gallery_hist.png").unwrap();
    }

    // 6. fill_between — a year of daily temperature range.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let mut wob = rng(0xC1DA7E5);
        let x = linspace(0.0, 12.0, 200);
        // Seasonal swing peaking mid-year (July ≈ month 6.5).
        let lo: Vec<f64> = x
            .iter()
            .map(|m| 3.0 - 9.0 * ((m - 6.5) / 12.0 * TAU).cos() + (wob() - 0.5) * 1.6)
            .collect();
        let hi: Vec<f64> = lo
            .iter()
            .zip(&x)
            .map(|(&l, &m)| l + 6.0 + 2.5 * ((m / 12.0) * TAU).sin().abs() + (wob() - 0.5) * 1.6)
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.fill_between(&x, &lo, &hi);
        ax.plot(&x, &lo);
        ax.plot(&x, &hi);
        ax.set_title("fill_between: daily temperature range");
        ax.set_xlabel("month");
        ax.set_ylabel("°C");
        fig.save_png("target/gallery_fill_between.png").unwrap();
    }

    // 7. step — the office thermostat wars, hour by hour.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x: Vec<f64> = (0..12).map(|h| h as f64).collect();
        let y = [
            21.0, 24.0, 20.0, 24.5, 19.5, 25.0, 19.0, 25.5, 18.5, 26.0, 22.0, 22.0,
        ];
        let ax = fig.add_subplot(1, 1, 1);
        ax.step(&x, &y);
        ax.set_title("step: the thermostat wars");
        ax.set_xlabel("hour");
        ax.set_ylabel("setpoint (°C)");
        fig.save_png("target/gallery_step.png").unwrap();
    }

    // 8. errorbar — measuring g with rising caffeine levels.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let mut jit = rng(0xDECAF);
        let x = linspace(0.0, 9.0, 10);
        let y: Vec<f64> = x
            .iter()
            .map(|&cups| 9.81 + (jit() - 0.5) * 0.05 * (1.0 + cups * 0.6))
            .collect();
        let yerr: Vec<f64> = x.iter().map(|&cups| 0.05 + 0.05 * cups).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.axhline(9.81);
        ax.errorbar(&x, &y, &yerr);
        ax.set_title("errorbar: measuring g vs. coffee intake");
        ax.set_xlabel("cups of coffee");
        ax.set_ylabel("g (m/s²)");
        fig.save_png("target/gallery_errorbar.png").unwrap();
    }

    // 9. reference lines & spans — an operating envelope, and you.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let ax = fig.add_subplot(1, 1, 1);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 10.0);
        ax.axhspan(0.0, 1.5); // too-cold band
        ax.axvspan(8.0, 10.0); // over-pressure band
        ax.axhline_with(8.5, Rgba::RED, 1.5); // temperature ceiling
        ax.axvline_with(1.0, Rgba::BLUE, 1.5); // minimum pressure
        ax.hlines(&[5.0], 1.0, 8.0); // nominal temperature
        ax.vlines(&[4.5], 1.5, 8.5); // nominal pressure
        ax.scatter(&[4.6], &[5.2]); // you are here
        ax.set_title("reference lines & spans: operating envelope");
        ax.set_xlabel("pressure");
        ax.set_ylabel("temperature");
        fig.save_png("target/gallery_reflines.png").unwrap();
    }

    // 10. imshow — two-source interference fringes.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (80usize, 100usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = (r as f64 / nr as f64 - 0.5) * 14.0;
                let xx = (col as f64 / nc as f64 - 0.5) * 14.0;
                let r1 = ((xx + 3.0).powi(2) + yy * yy).sqrt();
                let r2 = ((xx - 3.0).powi(2) + yy * yy).sqrt();
                data[r * nc + col] = (r1 * 1.8).sin() + (r2 * 1.8).sin();
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        // Signed field -> zero-centered diverging map.
        ax.imshow(&data, nr, nc)
            .cmap("coolwarm")
            .vmin(-2.0)
            .vmax(2.0);
        ax.set_title("imshow: two-source interference (coolwarm)");
        fig.save_png("target/gallery_imshow.png").unwrap();
    }

    // 11. legend + colorbar — predator–prey cycles, a quarter out of phase.
    {
        let mut fig = gallery_figure(6.0, 4.0);
        let x = linspace(0.0, 24.0, 400);
        let hares: Vec<f64> = x
            .iter()
            .map(|t| 3.2 + 2.2 * (t * TAU / 10.0).sin() * (1.0 + 0.15 * (t / 6.0).sin()))
            .collect();
        let lynxes: Vec<f64> = x
            .iter()
            .map(|t| 2.4 + 1.5 * ((t - 2.5) * TAU / 10.0).sin())
            .collect();
        {
            let ax = fig.add_axes(0.12, 0.13, 0.72, 0.76);
            ax.plot(&x, &hares);
            ax.plot(&x, &lynxes);
            ax.legend_with_title(
                vec![(c(0), "hares (k)".into()), (c(1), "lynxes (k)".into())],
                "Series",
            );
            ax.set_title("legend + colorbar: predator–prey cycles");
            ax.set_xlabel("year");
            ax.set_ylabel("population");
        }
        fig.colorbar("bgyw", 0.0, 6.0);
        fig.save_png("target/gallery_legend_colorbar.png").unwrap();
    }

    // 12. stem — a struck bell ringing down.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(0.0, 3.0 * TAU, 40);
        let y: Vec<f64> = x
            .iter()
            .map(|t| (2.0 * t).sin() * (-0.25 * t).exp())
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.stem(&x, &y);
        ax.set_title("stem: struck-bell impulse response");
        ax.set_xlabel("t (s)");
        ax.set_ylabel("amplitude");
        fig.save_png("target/gallery_stem.png").unwrap();
    }

    // 13. stairs — the elevation profile of a commute with exactly one hill.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let values = [
            12.0, 14.0, 15.0, 22.0, 48.0, 74.0, 60.0, 28.0, 16.0, 13.0, 12.0, 11.0,
        ];
        let edges: Vec<f64> = (0..=12).map(|k| k as f64 * 0.5).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.stairs(&values, &edges);
        ax.set_title("stairs: commute elevation profile");
        ax.set_xlabel("km");
        ax.set_ylabel("m above sea level");
        fig.save_png("target/gallery_stairs.png").unwrap();
    }

    // 14. pcolormesh — standing waves in a square drumhead.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (28usize, 28usize);
        let mut cdata = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = r as f64 / (nr - 1) as f64 * PI;
                let xx = col as f64 / (nc - 1) as f64 * PI;
                // (3,2) mode of a square membrane.
                cdata[r * nc + col] = (3.0 * xx).sin() * (2.0 * yy).sin();
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.pcolormesh(&cdata, nr, nc);
        ax.set_title("pcolormesh: drumhead mode (3, 2)");
        fig.save_png("target/gallery_pcolormesh.png").unwrap();
    }

    // 15. stackplot — where the workday actually goes.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(0.0, 10.0, 60); // sprint days
        let deep_work: Vec<f64> = x.iter().map(|d| (3.4 - 0.22 * d).max(0.7)).collect();
        let meetings: Vec<f64> = x
            .iter()
            .map(|d| 1.2 + 0.30 * d + 0.3 * (d * 2.0).sin())
            .collect();
        let slack: Vec<f64> = x.iter().map(|d| 1.0 + 0.12 * d).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.stackplot(&x, &[&deep_work, &meetings, &slack]);
        ax.legend(vec![
            (c(0), "deep work".into()),
            (c(1), "meetings".into()),
            (c(2), "chat".into()),
        ]);
        ax.set_title("stackplot: where the workday goes");
        ax.set_xlabel("sprint day");
        ax.set_ylabel("hours");
        fig.save_png("target/gallery_stackplot.png").unwrap();
    }

    // 16. broken_barh — a CI timeline with one suspicious gap.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let ax = fig.add_subplot(1, 1, 1);
        // Lane 1: build stages back-to-back.
        ax.broken_barh(&[(0.0, 3.0), (3.2, 2.0), (5.4, 1.4)], (10.0, 4.0));
        // Lane 2: tests, with a gap where the flaky one got retried.
        ax.broken_barh(&[(1.0, 4.0), (8.0, 3.5)], (20.0, 4.0));
        ax.set_title("broken_barh: CI timeline with a retry gap");
        ax.set_xlabel("minutes");
        fig.save_png("target/gallery_broken_barh.png").unwrap();
    }

    // 17. boxplot — time-to-first-review across four repos.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let well_run = [1.0, 1.5, 2.0, 2.0, 2.5, 3.0];
        let normal = [1.0, 3.0, 5.0, 7.0, 9.0, 11.0];
        let backlogged = [2.0, 2.5, 3.0, 3.5, 4.0, 9.0];
        let cursed = [5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 40.0];
        let ax = fig.add_subplot(1, 1, 1);
        ax.boxplot(&[&well_run, &normal, &backlogged, &cursed]);
        ax.set_title("boxplot: hours to first PR review");
        ax.set_ylabel("hours");
        fig.save_png("target/gallery_boxplot.png").unwrap();
    }

    // 18. mathtext title — sinc, labeled in math.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(-6.0 * PI, 6.0 * PI, 481);
        let y: Vec<f64> = x
            .iter()
            .map(|&v| if v.abs() < 1e-12 { 1.0 } else { v.sin() / v })
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_title("mathtext: $y = \\frac{\\sin(x)}{x}$");
        fig.save_png("target/gallery_mathtext.png").unwrap();
    }

    // 19. contour — a mountain saddle between two peaks.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (40usize, 40usize);
        let mut z = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = -2.5 + r as f64 / (nr - 1) as f64 * 5.0;
                let xx = -2.5 + col as f64 / (nc - 1) as f64 * 5.0;
                let peak_a = (-((xx - 1.1).powi(2) + (yy - 0.8).powi(2))).exp();
                let peak_b = 0.9 * (-((xx + 1.1).powi(2) + (yy + 0.8).powi(2))).exp();
                let bowl = -0.12 * (xx * xx + yy * yy);
                z[r * nc + col] = peak_a + peak_b + bowl;
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.contour(&z, nr, nc);
        ax.clabel();
        ax.set_title("contour: two peaks and a saddle");
        fig.save_png("target/gallery_contour.png").unwrap();
    }

    // 20. eventplot — one bar of drum & bass as a spike raster.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        // Hi-hat on every 16th, snare on 2 and 4, kick syncopated.
        let hihat: Vec<f64> = (0..16).map(|i| i as f64 * 0.25).collect();
        let snare = vec![1.0, 3.0];
        let kick = vec![0.0, 0.75, 2.5, 3.25, 3.75];
        let ax = fig.add_subplot(1, 1, 1);
        ax.eventplot(&[&kick, &snare, &hihat]);
        ax.set_title("eventplot: one bar of drum & bass");
        ax.set_xlabel("beat");
        fig.save_png("target/gallery_eventplot.png").unwrap();
    }

    // 21. fill_betweenx — a lazy meandering river, width = flow.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let y = linspace(0.0, 10.0, 200);
        let center: Vec<f64> = y
            .iter()
            .map(|v| 1.5 * (v * 0.8).sin() + 0.4 * (v * 2.1).sin())
            .collect();
        let width: Vec<f64> = y
            .iter()
            .map(|v| 0.5 + 0.25 * (v * 0.5).sin() + 0.04 * v)
            .collect();
        let left: Vec<f64> = center.iter().zip(&width).map(|(&m, &w)| m - w).collect();
        let right: Vec<f64> = center.iter().zip(&width).map(|(&m, &w)| m + w).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.fill_betweenx(&y, &left, &right);
        ax.set_title("fill_betweenx: a lazy river");
        ax.set_xlabel("east (km)");
        ax.set_ylabel("downstream (km)");
        fig.save_png("target/gallery_fill_betweenx.png").unwrap();
    }

    // 22. ecdf — how long TODO(urgent) comments actually live.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        // Days until fixed: a few same-day, lumps at "next sprint" and "next
        // quarter", and the immortals.
        let data = [
            0.5, 1.0, 1.0, 2.0, 3.0, 7.0, 7.0, 8.0, 14.0, 14.0, 15.0, 30.0, 31.0, 45.0, 60.0, 90.0,
            90.0, 180.0, 365.0, 730.0,
        ];
        let ax = fig.add_subplot(1, 1, 1);
        ax.ecdf(&data);
        ax.set_title("ecdf: lifetime of a TODO(urgent)");
        ax.set_xlabel("days until fixed");
        ax.set_ylabel("fraction fixed");
        fig.save_png("target/gallery_ecdf.png").unwrap();
    }

    // 23. matshow — the times table, as heat.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (12usize, 12usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                data[r * nc + col] = ((r + 1) * (col + 1)) as f64;
            }
        }
        let ax = fig.add_axes(0.11, 0.13, 0.70, 0.78);
        ax.matshow(&data, nr, nc);
        ax.set_title("matshow: the 12×12 times table");
        // Dedicated colorbar column labeled with the real value range.
        fig.colorbar_at((0.86, 0.13, 0.035, 0.78), "bgyw", 1.0, 144.0);
        fig.save_png("target/gallery_matshow.png").unwrap();
    }

    // 24. spy — who reviews whose PRs (everyone pings the maintainer).
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (16usize, 16usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let selfie = r == col;
                let maintainer = r == 3 || col == 3;
                let sprinkle = (r * 7 + col * 5) % 13 == 0;
                if selfie || maintainer || sprinkle {
                    data[r * nc + col] = 1.0;
                }
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.spy(&data, nr, nc);
        ax.set_title("spy: who reviews whom");
        fig.save_png("target/gallery_spy.png").unwrap();
    }

    // 25. hist2d — espresso intake vs. typing speed, suspiciously correlated.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let mut next = rng(0x9E3779B97F4A7C15);
        let mut x = Vec::new();
        let mut y = Vec::new();
        for _ in 0..2500 {
            let gx = (next() + next() + next() + next()) / 4.0;
            let gy = (next() + next() + next() + next()) / 4.0;
            let a = gx * 4.0 - 2.0;
            x.push(a);
            y.push(0.6 * a + (gy * 2.0 - 1.0));
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.hist2d(&x, &y, 30);
        ax.set_title("hist2d: espresso vs. typing speed");
        ax.set_xlabel("espresso (z-score)");
        ax.set_ylabel("wpm (z-score)");
        fig.save_png("target/gallery_hist2d.png").unwrap();
    }

    // 26. pie — the classic.
    {
        let mut fig = gallery_figure(4.0, 4.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.pie(&[75.0, 25.0]);
        ax.set_title("pie: resemblance to Pac-Man");
        fig.save_png("target/gallery_pie.png").unwrap();
    }

    // 27. violinplot — a field guide to distributions.
    {
        let mut fig = gallery_figure(5.0, 3.5);
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
        let ax = fig.add_subplot(1, 1, 1);
        ax.violinplot(&[&tight, &wide, &bimodal, &skewed], None);
        ax.set_title("violinplot: tight, wide, bimodal, skewed");
        fig.save_png("target/gallery_violinplot.png").unwrap();
    }

    // 28. hexbin — GPS pings around the two lunch spots.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let mut next = rng(0x243F6A8885A308D3);
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
        let ax = fig.add_subplot(1, 1, 1);
        ax.hexbin(&x, &y, 30);
        ax.set_title("hexbin: lunchtime GPS pings");
        ax.set_xlabel("blocks east");
        ax.set_ylabel("blocks north");
        fig.save_png("target/gallery_hexbin.png").unwrap();
    }

    // 29. grouped_bar — estimated vs. actual vs. shipped, four sprints.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let estimated = [8.0, 8.0, 8.0, 8.0]; // estimates are remarkably stable
        let actual = [11.0, 9.5, 13.0, 10.5];
        let shipped = [5.0, 6.5, 4.0, 7.0];
        let ax = fig.add_subplot(1, 1, 1);
        ax.grouped_bar(&[&estimated, &actual, &shipped]);
        ax.legend(vec![
            (c(0), "estimated".into()),
            (c(1), "actual".into()),
            (c(2), "shipped".into()),
        ]);
        ax.set_title("grouped_bar: sprint arithmetic");
        ax.set_ylabel("story points");
        fig.save_png("target/gallery_grouped_bar.png").unwrap();
    }

    // 30. loglog — Zipf's law: frequency falls as 1/rank.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(1.0, 1000.0, 240);
        let y: Vec<f64> = x.iter().map(|r| 1.0e6 / r).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.loglog(&x, &y);
        ax.set_xlim(1.0, 1000.0);
        ax.set_ylim(1.0e3, 1.0e6);
        ax.set_title("loglog: Zipf's law, eight decades");
        ax.set_xlabel("word rank");
        ax.set_ylabel("frequency");
        fig.save_png("target/gallery_loglog.png").unwrap();
    }

    // 31. quiver — wind field spiraling into a low.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let coords = linspace(-3.0, 3.0, 9);
        let (mut x, mut y, mut u, mut v) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for &gy in &coords {
            for &gx in &coords {
                x.push(gx);
                y.push(gy);
                u.push(-gy - 0.35 * gx);
                v.push(gx - 0.35 * gy);
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.quiver(&x, &y, &u, &v);
        ax.set_title("quiver: wind around a low");
        fig.save_png("target/gallery_quiver.png").unwrap();
    }

    // 32. streamplot — the same cyclone, ridden by streamlines.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let coords = linspace(-3.0, 3.0, 25);
        let nx = coords.len();
        let ny = coords.len();
        let mut u = vec![0.0; nx * ny];
        let mut v = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                u[j * nx + i] = -coords[j] - 0.25 * coords[i];
                v[j * nx + i] = coords[i] - 0.25 * coords[j];
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.streamplot(&coords, &coords, &u, &v);
        ax.set_title("streamplot: spiraling into the drain");
        fig.save_png("target/gallery_streamplot.png").unwrap();
    }

    // A 7x7 grid of vertices over [0, 1]^2 with deterministic jitter on the
    // interior nodes so the mesh looks like a real unstructured one; each cell
    // is split into two triangles. Shared by triplot and tripcolor below.
    let grid = 7;
    let coords = linspace(0.0, 1.0, grid);
    let mut vx = Vec::new();
    let mut vy = Vec::new();
    for (r, &gy) in coords.iter().enumerate() {
        for (col, &gx) in coords.iter().enumerate() {
            let interior = r > 0 && r < grid - 1 && col > 0 && col < grid - 1;
            let (jx, jy) = if interior {
                (
                    0.045 * ((r * 13 + col * 7) as f64).sin(),
                    0.045 * ((r * 5 + col * 11) as f64).cos(),
                )
            } else {
                (0.0, 0.0)
            };
            vx.push(gx + jx);
            vy.push(gy + jy);
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

    // 33. triplot (wireframe of the jittered triangulated grid)
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let ax = fig.add_subplot(1, 1, 1);
        ax.triplot(&vx, &vy, &triangles);
        ax.set_title("triplot: a budget finite-element mesh");
        fig.save_png("target/gallery_triplot.png").unwrap();
    }

    // 34. tripcolor (heat spreading from a point on the same mesh)
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let values: Vec<f64> = vx
            .iter()
            .zip(&vy)
            .map(|(&x, &y)| (-8.0 * ((x - 0.35).powi(2) + (y - 0.6).powi(2))).exp())
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.tripcolor(&vx, &vy, &triangles, &values);
        ax.set_title("tripcolor: heat from a soldering iron");
        fig.save_png("target/gallery_tripcolor.png").unwrap();
    }

    // 35. symlog (cubic spanning negative, zero, and positive tails)
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(-100.0, 100.0, 161);
        let y: Vec<f64> = x.iter().map(|v| v * v * v).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_xscale_symlog(10.0, 1.0)
            .set_yscale_symlog(10.0, 1.0)
            .set_xlim(-100.0, 100.0)
            .set_ylim(-1_000_000.0, 1_000_000.0);
        ax.set_title("symlog: a leveraged trader's net worth");
        ax.set_xlabel("conviction");
        ax.set_ylabel("net worth");
        fig.save_png("target/gallery_symlog.png").unwrap();
    }

    // 36. logit (sigmoid with probability-tail ticks)
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(-7.0, 7.0, 161);
        let y: Vec<f64> = x.iter().map(|v| 1.0 / (1.0 + (-v).exp())).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.logity(&x, &y);
        ax.set_xlim(-7.0, 7.0).set_ylim(0.001, 0.999);
        ax.set_title("logit: P(demo works) vs. rehearsal");
        ax.set_xlabel("rehearsal days (relative to enough)");
        ax.set_ylabel("P(demo works)");
        fig.save_png("target/gallery_logit.png").unwrap();
    }

    // 37. asinh (linear near zero with logarithmic tails)
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(-100.0, 100.0, 201);
        let y: Vec<f64> = x.iter().map(|v| v * v * v).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_yscale_asinh(1.0)
            .set_xlim(-100.0, 100.0)
            .set_ylim(-1_000_000.0, 1_000_000.0);
        ax.set_title("asinh: linear near zero, log in the tails");
        ax.set_xlabel("x");
        ax.set_ylabel("$x^3$");
        fig.save_png("target/gallery_asinh.png").unwrap();
    }

    // 38. date axis — coffee ramps into the May release, then the crash.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = [
            date_num(2026, 1, 1),
            date_num(2026, 2, 1),
            date_num(2026, 3, 1),
            date_num(2026, 4, 1),
            date_num(2026, 5, 1),
            date_num(2026, 6, 1),
        ];
        let y = [2.1, 2.4, 3.2, 4.4, 5.6, 2.0];
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_xaxis_date();
        ax.set_xlim(date_num(2026, 1, 1), date_num(2026, 6, 1));
        ax.set_title("date axis: coffee per day");
        ax.set_xlabel("2026");
        ax.set_ylabel("cups/day");
        fig.save_png("target/gallery_dates.png").unwrap();
    }

    // 39. polar (6-petal rose r = |cos(3θ)|)
    {
        let theta: Vec<f64> = (0..=720).map(|i| i as f64 * TAU / 720.0).collect();
        let r: Vec<f64> = theta.iter().map(|t| (3.0 * t).cos().abs()).collect();
        let mut ax = PolarAxes::new();
        ax.plot(&theta, &r);
        ax.set_title("polar plot: rose r = |cos 3θ|");
        ax.save_png(
            "target/gallery_polar.png",
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_DPI,
        )
        .unwrap();
    }

    // 40. polar scatter (sunflower phyllotaxis: golden-angle seed spiral)
    {
        let n = 220;
        let golden = PI * (3.0 - 5.0_f64.sqrt());
        let theta: Vec<f64> = (0..n).map(|i| i as f64 * golden).collect();
        let r: Vec<f64> = (0..n)
            .map(|i| (i as f64 + 0.5).sqrt() / (n as f64).sqrt())
            .collect();
        let mut ax = PolarAxes::new();
        ax.scatter(&theta, &r);
        ax.set_title("polar scatter: sunflower phyllotaxis");
        ax.save_png(
            "target/gallery_polar_scatter.png",
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_DPI,
        )
        .unwrap();
    }

    // 41. polar fill (filled 8-petal rose r = |cos(4θ)|)
    {
        let theta: Vec<f64> = (0..=1440).map(|i| i as f64 * TAU / 1440.0).collect();
        let r: Vec<f64> = theta.iter().map(|t| (4.0 * t).cos().abs()).collect();
        let mut ax = PolarAxes::new();
        ax.fill(&theta, &r);
        ax.set_title("polar fill: eight-petal rose");
        ax.save_png(
            "target/gallery_polar_fill.png",
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_PX,
            GALLERY_SQUARE_DPI,
        )
        .unwrap();
    }

    // 42. contourf (an island chain, filled by elevation band)
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (60usize, 60usize);
        let mut z = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let yy = -3.0 + r as f64 / (nr - 1) as f64 * 6.0;
                let xx = -3.0 + col as f64 / (nc - 1) as f64 * 6.0;
                let volcano = (-((xx - 1.2).powi(2) + (yy - 0.9).powi(2))).exp();
                let atoll = 0.75 * (-((xx + 1.3).powi(2) + (yy + 1.0).powi(2)) / 1.5).exp();
                let islet = 0.5 * (-((xx - 0.4).powi(2) * 2.0 + (yy + 1.6).powi(2) * 2.0)).exp();
                z[r * nc + col] = volcano + atoll + islet - 0.18;
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.contourf(&z, nr, nc);
        ax.set_title("contourf: island chain by elevation");
        fig.save_png("target/gallery_contourf.png").unwrap();
    }

    // 43. patches — shape overlays in data coordinates: an aperture ring on a
    // star, a dashed region-of-interest box, and a measurement arc.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (60usize, 80usize);
        let mut data = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                // A bright PSF-ish star plus a faint neighbor.
                let star = |cx: f64, cy: f64, amp: f64, sigma: f64| {
                    let d2 = (col as f64 - cx).powi(2) + (r as f64 - cy).powi(2);
                    amp * (-d2 / (2.0 * sigma * sigma)).exp()
                };
                data[r * nc + col] =
                    star(30.0, 28.0, 1.0, 4.0) + star(58.0, 44.0, 0.35, 3.0) + 0.02;
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.imshow(&data, nr, nc);
        // The image displays row 0 at the bottom, so overlay y = nr - row.
        let (bright, faint) = ((30.0, 60.0 - 28.0), (58.0, 60.0 - 44.0));
        // Aperture ring around the bright star (outline-only circle).
        ax.add_patch(
            Patch::circle([bright.0, bright.1], 9.0)
                .facecolor(None)
                .edgecolor(Some(Rgba::from_hex("#f5f5f5").unwrap()))
                .linewidth(1.6),
        );
        // Dashed ROI box around the faint neighbor.
        ax.add_patch(
            Patch::rectangle(faint.0 - 8.0, faint.1 - 8.0, 16.0, 16.0)
                .facecolor(None)
                .edgecolor(Some(Rgba::from_hex("#ff7f0e").unwrap()))
                .linewidth(1.4)
                .dashes(Some((0.0, vec![4.0, 3.0]))),
        );
        // A measurement arc sweeping from the ring toward the neighbor.
        ax.add_patch(
            Patch::arc([bright.0, bright.1], 22.0, -60.0, 30.0)
                .edgecolor(Some(Rgba::from_hex("#2ca02c").unwrap()))
                .linewidth(1.6),
        );
        ax.set_title("patches: aperture ring, ROI box, arc");
        fig.save_png("target/gallery_patches.png").unwrap();
    }

    // 44. annotate — resonance curve with the peak called out.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let x = linspace(0.0, 10.0, 400);
        let y: Vec<f64> = x
            .iter()
            .map(|&f| 1.0 / ((f * f - 25.0).powi(2) + 4.0 * f * f).sqrt() * 10.0)
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_xlim(0.0, 10.0);
        ax.set_ylim(0.0, 1.2);
        ax.annotate_with_box(
            "resonance at $\\omega_0 = 5$",
            (4.96, 1.0),
            (6.0, 1.05),
            TextBoxStyle::default(),
        );
        ax.annotate("half-power point", (5.7, 0.62), (6.9, 0.75));
        ax.text(0.5, 1.1, "driven oscillator response");
        ax.set_title("annotate: callouts with leader arrows");
        ax.set_xlabel("drive frequency $\\omega$");
        ax.set_ylabel("amplitude");
        fig.save_png("target/gallery_annotate.png").unwrap();
    }

    // Inverted axes — astronomical magnitude convention.
    {
        let mut fig = Figure::new(5.0, 3.5);
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&[1.0, 2.0, 3.0, 4.0], &[1.0, 4.0, 9.0, 16.0]);
        ax.invert_yaxis();
        ax.set_title("inverted y-axis");
        fig.save_png("target/gallery_inverted_axes.png").unwrap();
    }

    // Figure supertitle over a two-panel grid.
    {
        let mut fig = Figure::new(6.0, 3.0);
        fig.add_subplot(1, 2, 1)
            .plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
        fig.add_subplot(1, 2, 2)
            .plot(&[0.0, 1.0, 2.0], &[1.0, 0.0, 1.0]);
        fig.suptitle("A figure-level supertitle");
        fig.save_png("target/gallery_suptitle.png").unwrap();
    }

    // 45. tricontour / tricontourf — a wavefront-error map over the same
    // unstructured mesh: filled bands with isolines on top.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        // A tilted gaussian bump sampled at the jittered mesh vertices.
        let values: Vec<f64> = vx
            .iter()
            .zip(&vy)
            .map(|(&x, &y)| {
                (-6.0 * ((x - 0.62).powi(2) + (y - 0.4).powi(2))).exp() + 0.35 * (x + y)
            })
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.tricontourf(&vx, &vy, &triangles, &values);
        ax.tricontour(&vx, &vy, &triangles, &values);
        ax.set_title("tricontour(f): bands over a scattered mesh");
        fig.save_png("target/gallery_tricontour.png").unwrap();
    }

    // 46. pcolormesh gouraud — a chirp spectrogram, smoothly shaded.
    {
        let mut fig = gallery_figure(5.0, 3.8);
        let (nr, nc) = (48usize, 64usize);
        let mut z = vec![0.0; nr * nc];
        for r in 0..nr {
            for col in 0..nc {
                let t = col as f64 / (nc - 1) as f64; // time 0..1
                let f = r as f64 / (nr - 1) as f64; // frequency 0..1
                // A rising chirp ridge plus a faint constant tone.
                let chirp = (-((f - (0.15 + 0.7 * t)) / 0.06).powi(2)).exp();
                let tone = 0.35 * (-((f - 0.75) / 0.03).powi(2)).exp();
                z[r * nc + col] = chirp + tone;
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.pcolormesh_gouraud(&z, nr, nc);
        ax.set_title("pcolormesh (gouraud): chirp spectrogram");
        ax.set_xlabel("time");
        ax.set_ylabel("frequency");
        fig.save_png("target/gallery_gouraud.png").unwrap();
    }

    // 47. sky projection — a mollweide all-sky map: a band of sources along
    // an inclined great circle (a toy galactic plane) plus two clusters.
    {
        let mut noise = rng(0x5EEDED5EEDED);
        let (mut lon, mut lat) = (Vec::new(), Vec::new());
        // The band: points scattered around a great circle inclined 60°.
        for i in 0..240 {
            let t = i as f64 / 239.0 * TAU;
            let band_lat = (60f64.to_radians().sin() * t.sin()).asin();
            lon.push(t - PI + (noise() - 0.5) * 0.15);
            lat.push(band_lat + (noise() - 0.5) * 0.12);
        }
        let mut ax = SkyAxes::new(SkyProjection::Mollweide);
        ax.scatter(&lon, &lat);
        // Two compact clusters (toy Magellanic clouds).
        let (mut clon, mut clat) = (Vec::new(), Vec::new());
        for _ in 0..40 {
            clon.push(-1.4 + (noise() - 0.5) * 0.18);
            clat.push(-0.72 + (noise() - 0.5) * 0.10);
        }
        for _ in 0..25 {
            clon.push(-1.1 + (noise() - 0.5) * 0.12);
            clat.push(-0.60 + (noise() - 0.5) * 0.08);
        }
        ax.scatter(&clon, &clat);
        ax.set_title("mollweide projection: a toy Milky Way");
        ax.save_png(
            "target/gallery_sky.png",
            GALLERY_SKY_WIDTH_PX,
            GALLERY_SKY_HEIGHT_PX,
            GALLERY_SKY_DPI,
        )
        .unwrap();
    }

    // 48. twinx + secondary x — focus mechanism sweep: position (mm, left)
    // and measured tilt (µrad, right) over one time base, with a top axis in
    // seconds.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let t = linspace(0.0, 10.0, 200); // minutes
        let position: Vec<f64> = t.iter().map(|&m| 2.5 + 2.0 * (m * 0.6).sin()).collect();
        let tilt: Vec<f64> = t
            .iter()
            .map(|&m| 40.0 * (m * 0.6).cos() + 6.0 * (m * 2.1).sin())
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&t, &position);
        ax.set_xlabel("time (min)");
        ax.set_ylabel("position (mm)");
        ax.set_title("twinx: mm left, µrad right, seconds on top");
        ax.secondary_xaxis_linear(60.0, 0.0, Some("time (s)"));
        let twin = fig.twinx(0);
        let tw = &mut fig.axes_mut()[twin];
        tw.plot_with_color(&t, &tilt, c(1));
        tw.set_ylabel("tilt (µrad)");
        fig.save_png("target/gallery_twinx.png").unwrap();
    }

    // 49. 3D quiver + text — a toy spacecraft scene: body-axes triad and a
    // star-tracker boresight over a wireframe dish.
    {
        use rizzma::mplot3d::Axes3D;
        let n = 10usize;
        let gx: Vec<f64> = (0..n)
            .map(|i| -1.0 + 2.0 * i as f64 / (n - 1) as f64)
            .collect();
        let gy = gx.clone();
        let mut gz = Vec::with_capacity(n * n);
        for &yv in &gy {
            for &xv in &gx {
                gz.push(0.35 * (xv * xv + yv * yv)); // a shallow parabolic dish
            }
        }
        let mut ax = Axes3D::new();
        ax.plot_wireframe(&gx, &gy, &gz);
        // Body axes triad from the dish vertex.
        ax.quiver3d(
            &[0.0, 0.0, 0.0],
            &[0.0, 0.0, 0.0],
            &[0.0, 0.0, 0.0],
            &[1.2, 0.0, 0.0],
            &[0.0, 1.2, 0.0],
            &[0.0, 0.0, 1.2],
        );
        // Star-tracker boresight, canted off the +z axis.
        ax.quiver3d(&[0.0], &[0.0], &[0.6], &[0.7], &[-0.5], &[0.9]);
        ax.text3d(1.25, 0.0, 0.0, "+X");
        ax.text3d(0.0, 1.25, 0.0, "+Y");
        ax.text3d(0.0, 0.0, 1.3, "+Z");
        ax.text3d(0.75, -0.55, 1.55, "boresight");
        ax.set_title("quiver3d: body axes + boresight");
        ax.save_png(
            "target/gallery_quiver3d.png",
            GALLERY_3D_WIDTH_PX,
            GALLERY_3D_HEIGHT_PX,
            GALLERY_3D_DPI,
        )
        .unwrap();
    }

    // 50. colormap showcase — every builtin ramp as a horizontal colorbar,
    // grouped: the Kovesi CET maps (arXiv:1509.03700), the classics, and the
    // quarantined misleading: maps.
    {
        // Rows are either a section header (no bar) or a labeled colorbar.
        let rows: [(&str, Option<&str>); 31] = [
            ("the Kovesi CET maps (arXiv:1509.03700)", None),
            ("bgyw (default)", Some("bgyw")),
            ("fire", Some("fire")),
            ("cet_l01", Some("cet_l01")),
            ("cet_l05", Some("cet_l05")),
            ("cet_l10", Some("cet_l10")),
            ("cet_d01", Some("cet_d01")),
            ("cet_d04", Some("cet_d04")),
            ("cet_d07", Some("cet_d07")),
            ("cet_d11", Some("cet_d11")),
            ("cet_r2", Some("cet_r2")),
            ("cet_c1", Some("cet_c1")),
            ("cet_c2", Some("cet_c2")),
            ("cet_c3", Some("cet_c3")),
            ("cet_c5", Some("cet_c5")),
            ("cet_i1", Some("cet_i1")),
            ("the classics", None),
            ("viridis", Some("viridis")),
            ("plasma", Some("plasma")),
            ("inferno", Some("inferno")),
            ("magma", Some("magma")),
            ("cividis", Some("cividis")),
            ("coolwarm", Some("coolwarm")),
            ("RdBu", Some("RdBu")),
            ("gray", Some("gray")),
            ("quarantined (see the misleading module docs)", None),
            ("misleading:jet", Some("misleading:jet")),
            ("misleading:hot", Some("misleading:hot")),
            ("misleading:hsv", Some("misleading:hsv")),
            ("misleading:rainbow", Some("misleading:rainbow")),
            ("", None),
        ];
        let dy = 1.0 / rows.len() as f64;
        let row_y = |i: usize| 1.0 - (i + 1) as f64 * dy;
        let mut fig = gallery_figure(5.0, 8.2);
        let ramp: Vec<f64> = (0..256).map(f64::from).collect();
        let ax = fig.add_axes(0.0, 0.0, 1.0, 1.0);
        ax.set_xlim(0.0, 1.0);
        ax.set_ylim(0.0, 1.0);
        ax.set_axis_off();
        for (i, (label, bar)) in rows.iter().enumerate() {
            let y = row_y(i);
            // Indent bar labels under their section headers.
            let x = if bar.is_some() { 0.06 } else { 0.03 };
            ax.text(x, y + dy * 0.25, *label);
            if let Some(name) = bar {
                // Each swatch is a 1 x 256 ramp image drawn over its row.
                ax.imshow(&ramp, 1, 256).cmap(*name).set_extent([
                    0.30,
                    0.97,
                    y + dy * 0.12,
                    y + dy * 0.88,
                ]);
            }
        }
        fig.save_png("target/gallery_colormaps.png").unwrap();
    }

    // 51. aitoff — the same sky data as case 47, on the aitoff projection.
    {
        let mut noise = rng(0x5EEDED5EEDED);
        let (mut lon, mut lat) = (Vec::new(), Vec::new());
        for i in 0..240 {
            let t = i as f64 / 239.0 * TAU;
            let band_lat = (60f64.to_radians().sin() * t.sin()).asin();
            lon.push(t - PI + (noise() - 0.5) * 0.15);
            lat.push(band_lat + (noise() - 0.5) * 0.12);
        }
        let mut ax = SkyAxes::new(SkyProjection::Aitoff);
        ax.scatter(&lon, &lat);
        ax.set_title("aitoff projection: the same galactic band");
        ax.save_png(
            "target/gallery_sky_aitoff.png",
            GALLERY_SKY_WIDTH_PX,
            GALLERY_SKY_HEIGHT_PX,
            GALLERY_SKY_DPI,
        )
        .unwrap();
    }

    // 52. oscilloscope — three sparkline-height phosphor strips, x-linked.
    {
        let mut noise = rng(0x5C09E5C09E);
        let n = 300;
        let mut t = vec![0.0; n];
        let (mut bx, mut by, mut snr) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        let mut drift = 0.0;
        for i in 0..n {
            let tt = 10.0 * i as f64 / (n - 1) as f64;
            t[i] = tt;
            drift += (noise() - 0.5) * 0.05;
            bx[i] = (tt * 2.1).sin() * 0.4 + (noise() - 0.5) * 0.12;
            by[i] = (tt * 1.3).cos() * 0.3 + drift + (noise() - 0.5) * 0.12;
            snr[i] = 18.0 + (tt * 0.7).sin() * 3.0 + (noise() - 0.5) * 2.0;
        }
        let mut fig = gallery_figure(4.8, 1.7);
        fig.set_facecolor(Rgba::new(0.039, 0.051, 0.043, 1.0));
        for (b, h) in [(0.68, 0.32), (0.34, 0.32), (0.0, 0.32)] {
            fig.add_axes(0.0, b, 1.0, h).oscilloscope();
        }
        fig.axes_mut()[0].plot(&t, &bx);
        fig.axes_mut()[1].plot(&t, &by);
        fig.axes_mut()[2].plot(&t, &snr);
        fig.sharex(1, 0);
        fig.sharex(2, 0);
        fig.save_png("target/gallery_oscilloscope.png").unwrap();
    }

    // 53. plot_surface — the sombrero sinc(r), the scene the docs.rs live
    // matrix spins (its static fallback image).
    {
        use rizzma::mplot3d::Axes3D;
        let n = 40usize;
        let gx: Vec<f64> = (0..n)
            .map(|i| -8.0 + 16.0 * i as f64 / (n - 1) as f64)
            .collect();
        let gy = gx.clone();
        let mut gz = Vec::with_capacity(n * n);
        for &yv in &gy {
            for &xv in &gx {
                let r = xv.hypot(yv);
                gz.push(if r < 1e-9 { 1.0 } else { r.sin() / r });
            }
        }
        let mut ax = Axes3D::new();
        ax.plot_surface(&gx, &gy, &gz);
        ax.set_title("plot_surface: sombrero sinc(r)");
        ax.save_png(
            "target/gallery_surface3d.png",
            GALLERY_3D_WIDTH_PX,
            GALLERY_3D_HEIGHT_PX,
            GALLERY_3D_DPI,
        )
        .unwrap();
    }

    // 54. text / text_with_color — themed annotation ink on a dark figure:
    // plain text() inherits the rc text color, text_with_color() overrides it.
    {
        use rizzma::core::rcparams::RcParams;
        let mut fig = gallery_figure(5.0, 3.5).with_rcparams(RcParams::dark());
        let x = linspace(0.0, TAU, 400);
        let y: Vec<f64> = x.iter().map(|t| t.sin() * (-t / 4.0).exp()).collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_ylim(-0.65, 1.0);
        ax.text(0.25, 0.85, "themed ink follows rc");
        ax.text_with_color(1.5, 0.72, "peak", Rgba::from_hex("#f7768e").unwrap());
        ax.text_with_color(4.6, -0.5, "trough", Rgba::from_hex("#7dcfff").unwrap());
        ax.annotate("zero crossing", (PI, 0.0), (3.5, 0.55));
        ax.set_title("text: annotation ink inherits the theme");
        ax.set_xlabel("t (s)");
        ax.set_ylabel("response");
        fig.save_png("target/gallery_text_color.png").unwrap();
    }

    // 55. legend_auto with scatter rows — measurements keyed by a marker,
    // the fitted model by a line, in the order they were added.
    {
        let mut fig = gallery_figure(5.0, 3.5);
        let mut noise = rng(55);
        let x = linspace(0.5, 9.5, 28);
        let model = |t: f64| 4.0 * (-t / 3.5).exp() + 0.6;
        let y: Vec<f64> = x
            .iter()
            .map(|&t| model(t) + 0.35 * (noise() - 0.5))
            .collect();
        let ax = fig.add_subplot(1, 1, 1);
        ax.scatter(&x, &y)
            .set_sizes(vec![5.0])
            .set_label("measured");
        let fit_x = linspace(0.0, 10.0, 200);
        let fit_y: Vec<f64> = fit_x.iter().map(|&t| model(t)).collect();
        ax.plot(&fit_x, &fit_y).set_label("model");
        let square = rizzma::artist::MarkerStyle::from_char('s')
            .unwrap()
            .path()
            .clone();
        ax.scatter(&[2.2, 7.1], &[3.4, 2.1])
            .set_marker(square)
            .set_sizes(vec![6.0])
            .set_facecolors(vec![Rgba::WHITE])
            .set_edgecolors(vec![c(3)])
            .set_label("rejected");
        ax.legend_auto();
        ax.set_title("legend_auto: scatter rows keyed by their marker");
        ax.set_xlabel("time (h)");
        ax.set_ylabel("flux");
        fig.save_png("target/gallery_legend_auto.png").unwrap();
    }

    // 56. fig.text / supxlabel / supylabel — shared axis labels for a pair of
    // panels, a metadata footer, and a boxed stamp over everything.
    {
        use rizzma::{HorizontalAlignment, VerticalAlignment};
        let mut fig = gallery_figure(6.0, 3.2);
        let t = linspace(0.0, 4.0 * PI, 300);
        let left: Vec<f64> = t.iter().map(|t| t.sin() * (-t / 8.0).exp()).collect();
        let right: Vec<f64> = t
            .iter()
            .map(|t| (1.7 * t).cos() * (-t / 6.0).exp())
            .collect();
        fig.add_subplot(1, 2, 1).plot(&t, &left);
        fig.add_subplot(1, 2, 2).plot(&t, &right).set_color(c(1));
        fig.suptitle("fig.text: figure-level labels and notes");
        fig.supxlabel("time (s)");
        fig.supylabel("displacement (mm)");
        fig.text(0.995, 0.005, "run 42 · 2026-10-14 · rizzma")
            .set_ha(HorizontalAlignment::Right)
            .set_va(VerticalAlignment::Bottom)
            .set_size(7.0)
            .set_color(Rgba::from_hex("#565f89").unwrap());
        fig.text(0.5, 0.55, "PRELIMINARY")
            .set_ha(HorizontalAlignment::Center)
            .set_va(VerticalAlignment::Center)
            .set_size(22.0)
            .set_rotation(18.0)
            .set_color(c(3))
            .set_bbox(Some(TextBoxStyle {
                facecolor: Rgba::new(1.0, 1.0, 1.0, 0.8),
                edgecolor: Some(c(3)),
                ..TextBoxStyle::default()
            }));
        fig.save_png("target/gallery_figtext.png").unwrap();
    }

    // 57. norms — a star field through LogNorm with dead pixels marked bad
    // and saturation marked over; residuals through TwoSlopeNorm; signed
    // fluxes spanning decades through SymLogNorm.
    {
        use rizzma::core::color::{ColorMapping, LogNorm, SymLogNorm, TwoSlopeNorm};
        let mut fig = gallery_figure(9.0, 3.0);
        let mut noise = rng(57);
        let n = 48;
        let mut sky = vec![0.0; n * n];
        let stars = [
            (12.0, 30.0, 4000.0),
            (33.0, 14.0, 900.0),
            (36.0, 38.0, 60.0),
        ];
        for r in 0..n {
            for c in 0..n {
                let mut v = 2.0 + 1.5 * noise();
                for (sx, sy, amp) in stars {
                    let d2 = (c as f64 - sx).powi(2) + (r as f64 - sy).powi(2);
                    v += amp * (-d2 / 6.0).exp();
                }
                sky[r * n + c] = v;
            }
        }
        for dead in [5 * n + 40, 6 * n + 40, 7 * n + 40, 30 * n + 8] {
            sky[dead] = f64::NAN;
        }
        let ax = fig.add_subplot(1, 3, 1);
        ax.imshow(&sky, n, n)
            .cmap("magma")
            .norm(LogNorm::new(1.0, 1000.0))
            .bad(Rgba::from_hex("#00e5ff").unwrap())
            .over(Rgba::WHITE);
        ax.set_title("imshow · LogNorm, bad + over");

        let (rows, cols) = (12, 16);
        let residual: Vec<f64> = (0..rows * cols)
            .map(|i| {
                let (r, c) = ((i / cols) as f64, (i % cols) as f64);
                3.0 * ((c - 11.0).powi(2) + (r - 4.0).powi(2)).sqrt().cos() * (-c / 9.0).exp() - 0.4
            })
            .collect();
        let ax = fig.add_subplot(1, 3, 2);
        ax.pcolormesh_mapped(
            &residual,
            rows,
            cols,
            ColorMapping::new("coolwarm").norm(TwoSlopeNorm::new(-1.0, 0.0, 3.0)),
        );
        ax.set_title("pcolormesh · TwoSlopeNorm");

        let x = linspace(0.0, 10.0, 120);
        let y: Vec<f64> = x.iter().map(|t| (t * 0.9).sin() + 0.3 * noise()).collect();
        let flux: Vec<f64> = x
            .iter()
            .map(|t| (t - 5.0).signum() * 10f64.powf((t - 5.0).abs() * 0.7 - 1.0))
            .collect();
        let ax = fig.add_subplot(1, 3, 3);
        ax.scatter_mapped(
            &x,
            &y,
            &flux,
            ColorMapping::new("RdBu").norm(SymLogNorm::new(0.5, -1000.0, 1000.0)),
        )
        .set_sizes(vec![7.0]);
        ax.set_title("scatter_mapped · SymLogNorm");
        fig.save_png("target/gallery_norms.png").unwrap();
    }

    // 58. colorbars bound to their artists — each panel's bar follows its
    // norm: linear, decades for LogNorm, ticks through a TwoSlopeNorm, and
    // equal-length regions for a BoundaryNorm, on all four sides.
    {
        use rizzma::core::color::{BoundaryNorm, ColorMapping, LogNorm, TwoSlopeNorm};
        use rizzma::{ColorbarLocation, Mappable};
        let mut fig = gallery_figure(8.0, 6.0);
        let n = 32;
        let field = |f: &dyn Fn(f64, f64) -> f64| -> Vec<f64> {
            (0..n * n)
                .map(|i| f((i % n) as f64 / n as f64, (i / n) as f64 / n as f64))
                .collect()
        };

        let ax = fig.add_subplot(2, 2, 1);
        ax.imshow(&field(&|x, y| (6.0 * x).sin() * (4.0 * y).cos()), n, n)
            .cmap("viridis");
        ax.colorbar(Mappable::Image(0)).set_label("amplitude");
        ax.set_title("linear");

        let ax = fig.add_subplot(2, 2, 2);
        ax.imshow(
            &field(&|x, y| {
                10f64.powf(3.0 * (-((x - 0.3).powi(2) + (y - 0.6).powi(2)) * 9.0).exp())
            }),
            n,
            n,
        )
        .cmap("magma")
        .norm(LogNorm::new(1.0, 1000.0));
        ax.colorbar(Mappable::Image(0)).set_label("counts");
        ax.set_title("LogNorm");

        let ax = fig.add_subplot(2, 2, 3);
        let residual = field(&|x, y| 3.0 * (x - 0.35) * (1.0 + y) - 0.2);
        ax.pcolormesh_mapped(
            &residual,
            n,
            n,
            ColorMapping::new("coolwarm").norm(TwoSlopeNorm::new(-1.0, 0.0, 3.0)),
        );
        ax.colorbar(Mappable::Mesh(0))
            .set_location(ColorbarLocation::Bottom)
            .set_shrink(0.6)
            .set_label("residual");
        ax.set_title("TwoSlopeNorm, bottom");

        let ax = fig.add_subplot(2, 2, 4);
        let mut noise = rng(58);
        let x: Vec<f64> = (0..300).map(|_| noise()).collect();
        let y: Vec<f64> = (0..300).map(|_| noise()).collect();
        let depth: Vec<f64> = x.iter().zip(&y).map(|(a, b)| 1000.0 * a * b).collect();
        ax.scatter_mapped(
            &x,
            &y,
            &depth,
            ColorMapping::new("cividis").norm(BoundaryNorm::new(
                vec![0.0, 10.0, 50.0, 200.0, 500.0, 1000.0],
                256,
            )),
        )
        .set_sizes(vec![4.0]);
        ax.colorbar(Mappable::Collection(0))
            .set_location(ColorbarLocation::Left)
            .set_label("depth (m)");
        ax.set_title("BoundaryNorm, left");
        fig.save_png("target/gallery_colorbar_norms.png").unwrap();
    }

    // 59. astronomical normalization — one star field three ways: the full
    // min/max range (a black frame and a few dots), IRAF's zscale, and
    // zscale with an asinh stretch that keeps the bright cores.
    {
        use rizzma::Mappable;
        use rizzma::core::color::{ImageNormalize, Interval, Stretch};
        let mut fig = gallery_figure(10.0, 3.6);
        let mut noise = rng(59);
        let n = 96;
        let mut sky: Vec<f64> = (0..n * n).map(|_| 100.0 + 12.0 * (noise() - 0.5)).collect();
        for _ in 0..60 {
            let (cx, cy) = (noise() * n as f64, noise() * n as f64);
            let amp = 10f64.powf(1.0 + 3.5 * noise());
            for r in 0..n {
                for c in 0..n {
                    let d2 = (c as f64 - cx).powi(2) + (r as f64 - cy).powi(2);
                    sky[r * n + c] += amp * (-d2 / 3.0).exp();
                }
            }
        }
        let panels = [
            (
                "min/max, linear",
                ImageNormalize::new(Interval::MinMax, Stretch::linear()),
            ),
            (
                "zscale, linear",
                ImageNormalize::new(Interval::zscale(), Stretch::linear()),
            ),
            (
                "zscale, asinh(0.1)",
                ImageNormalize::new(Interval::zscale(), Stretch::Asinh(0.1)),
            ),
        ];
        for (i, (title, norm)) in panels.into_iter().enumerate() {
            let ax = fig.add_subplot(1, 3, i + 1);
            ax.imshow(&sky, n, n).cmap("gray").norm(norm);
            ax.set_title(title);
            if i == 2 {
                ax.colorbar(Mappable::Image(0)).set_label("counts");
            }
        }
        fig.save_png("target/gallery_astro_normalize.png").unwrap();
    }

    // 60. errorbar limits — a fading transient: detections with error bars,
    // then non-detections drawn as upper limits once it drops below the
    // survey depth.
    {
        use rizzma::figure::ErrorbarOptions;
        let mut fig = gallery_figure(5.0, 3.5);
        let mut noise = rng(60);
        let days: Vec<f64> = (0..14).map(|d| 2.0 * f64::from(d) + 1.0).collect();
        let depth = 0.6;
        let truth = |t: f64| 8.0 * (-t / 7.0).exp();
        let mut flux = Vec::new();
        let mut err = Vec::new();
        let mut upper = Vec::new();
        for &t in &days {
            let f = truth(t) + 0.25 * (noise() - 0.5);
            let detected = f > depth;
            flux.push(if detected { f } else { depth });
            err.push(if detected { 0.2 + 0.05 * f } else { 0.35 });
            upper.push(!detected);
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.errorbar_with(&days, &flux, &err, &ErrorbarOptions::new().uplims(&upper));
        ax.set_title("errorbar uplims: a fading transient");
        ax.set_xlabel("days since discovery");
        ax.set_ylabel("flux (mJy)");
        fig.save_png("target/gallery_errorbar_limits.png").unwrap();
    }

    // 61. colour-filled sky map — a Mollweide axes holding a pcolormesh of
    // galactic-plane emission on a 2° grid, with a colorbar, the graticule
    // and a scatter of sources drawn over it.
    {
        use rizzma::figure::{ColorbarLocation, Mappable};
        let mut fig = gallery_figure(8.0, 4.2);
        let lon: Vec<f64> = (-90..=90).map(|i| f64::from(2 * i).to_radians()).collect();
        let lat: Vec<f64> = (-45..=45).map(|i| f64::from(2 * i).to_radians()).collect();
        let mut c = Vec::with_capacity(90 * 180);
        for r in 0..90 {
            for col in 0..180 {
                let (lo, la) = (0.5 * (lon[col] + lon[col + 1]), 0.5 * (lat[r] + lat[r + 1]));
                // A plane tilted 60° to the equator, brightest toward the
                // center, over a faint dipole.
                let plane = (la - 60f64.to_radians().sin().asin() * lo.sin()).abs();
                c.push(3.0 * (-plane * plane / 0.04).exp() * (1.0 + lo.cos()) + 0.4 * la.sin());
            }
        }
        let mut noise = rng(61);
        let (mut slon, mut slat) = (Vec::new(), Vec::new());
        for _ in 0..60 {
            slon.push((noise() - 0.5) * TAU);
            slat.push((noise() - 0.5) * 2.4);
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.set_sky_projection(SkyProjection::Mollweide).grid(true);
        ax.pcolormesh_xy(&lon, &lat, &c, "inferno");
        ax.scatter(&slon, &slat);
        ax.colorbar(Mappable::Mesh(0))
            .set_location(ColorbarLocation::Bottom)
            .set_shrink(0.6)
            .set_label("emission");
        ax.set_title("pcolormesh on a mollweide axes");
        fig.save_png("target/gallery_sky_fill.png").unwrap();
    }

    // 62. zenithal fisheye — the night sky overhead: airglow brightening
    // toward the horizon and a band of starlight crossing near the zenith,
    // on a 2° azimuth/altitude grid, with altitude rings and N/E/S/W.
    {
        use rizzma::figure::Mappable;
        let mut fig = gallery_figure(5.5, 5.0);
        let az: Vec<f64> = (0..=180).map(|i| f64::from(2 * i).to_radians()).collect();
        let alt: Vec<f64> = (0..=45).map(|i| f64::from(2 * i).to_radians()).collect();
        let mut c = Vec::with_capacity(45 * 180);
        for r in 0..45 {
            for col in 0..180 {
                let (a, h) = (0.5 * (az[col] + az[col + 1]), 0.5 * (alt[r] + alt[r + 1]));
                let airglow = 21.6 - 1.2 * (-h / 0.35).exp();
                // A great circle through the zenith region, tilted 70°.
                let band = (h.sin() * 70f64.to_radians().cos()
                    - h.cos() * 70f64.to_radians().sin() * (a - 0.6).sin())
                .abs();
                c.push(airglow - 0.6 * (-band * band / 0.03).exp());
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.set_sky_projection(SkyProjection::Zenithal).grid(true);
        ax.pcolormesh_xy(&az, &alt, &c, "cividis");
        ax.colorbar(Mappable::Mesh(0))
            .set_shrink(0.8)
            .set_label("sky brightness (mag/arcsec²)");
        ax.set_title("zenithal fisheye: night-sky brightness");
        fig.save_png("target/gallery_sky_zenithal.png").unwrap();
    }

    // 63. polygon fill — survey depth over an equal-area pixelization:
    // rings of cells whose count follows cos(latitude), HEALPix-style, each
    // a free-standing polygon colored through `poly_collection`.
    {
        use rizzma::figure::{ColorbarLocation, Mappable};
        let mut fig = gallery_figure(8.0, 4.6);
        let mut polygons = Vec::new();
        let mut depth = Vec::new();
        let rings = 24;
        for ring in 0..rings {
            let (lat0, lat1) = (
                -FRAC_PI_2 + PI * f64::from(ring) / f64::from(rings),
                -FRAC_PI_2 + PI * f64::from(ring + 1) / f64::from(rings),
            );
            let cells = (48.0 * (0.5 * (lat0 + lat1)).cos()).round().max(4.0) as usize;
            for k in 0..cells {
                let lon0 = -PI + TAU * k as f64 / cells as f64;
                let lon1 = -PI + TAU * (k + 1) as f64 / cells as f64;
                polygons.push(vec![[lon0, lat0], [lon1, lat0], [lon1, lat1], [lon0, lat1]]);
                let (lo, la) = (0.5 * (lon0 + lon1), 0.5 * (lat0 + lat1));
                // Deeper toward the ecliptic poles, shallow near the plane.
                let ecliptic = (la.sin() * 0.917 - la.cos() * 0.398 * lo.sin()).abs();
                depth.push(20.0 + 2.5 * ecliptic + 0.3 * (3.0 * lo).cos());
            }
        }
        let ax = fig.add_subplot(1, 1, 1);
        ax.set_sky_projection(SkyProjection::Mollweide);
        ax.poly_collection(&polygons, &depth, "viridis")
            .set_edgecolor(Some(Rgba::WHITE))
            .set_linewidth(0.3);
        ax.colorbar(Mappable::Polygons(0))
            .set_location(ColorbarLocation::Bottom)
            .set_shrink(0.6)
            .set_label("5σ depth (mag)");
        ax.set_title("poly_collection: survey depth per sky cell");
        fig.save_png("target/gallery_sky_polygons.png").unwrap();
    }

    // 64–74. The migrating-from-plotters guide (`docs/migrating-from-plotters.md`):
    // each block between `// migrate:` markers is that guide's snippet,
    // verbatim (`tests/migrating_guide.rs` checks), so the image beside a
    // snippet is what the snippet draws.
    {
        // migrate: panels
        use rizzma::Figure;

        let t: Vec<f64> = (0..200).map(|i| f64::from(i) * 0.05).collect();
        let sin: Vec<f64> = t.iter().map(|v| v.sin()).collect();
        let cos: Vec<f64> = t.iter().map(|v| v.cos()).collect();

        let mut fig = Figure::new(8.0, 3.5);
        let ax = fig.add_subplot(1, 2, 1);
        ax.plot(&t, &sin).set_label("sin");
        ax.plot(&t, &cos).set_label("cos");
        ax.set_xlabel("t (s)").set_title("two series");
        ax.legend_auto();
        let ax = fig.add_subplot(1, 2, 2);
        ax.scatter(&sin, &cos);
        ax.set_aspect_equal().grid(true);
        ax.set_title("phase portrait");
        // end migrate
        fig.save_png("target/gallery_migrate_panels.png").unwrap();
    }
    {
        // migrate: images
        use rizzma::Figure;
        use rizzma::figure::Mappable;

        let (nrows, ncols) = (60, 90);
        let data: Vec<f64> = (0..nrows * ncols)
            .map(|k| {
                let (r, c) = (f64::from(k / ncols) / 9.0, f64::from(k % ncols) / 9.0);
                (r.sin() * c.cos() + 0.3 * (r + c).sin()).powi(2)
            })
            .collect();

        let mut fig = Figure::new(8.0, 5.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.imshow(&data, nrows as usize, ncols as usize)
            .cmap("inferno");
        ax.colorbar(Mappable::Image(0)).set_label("flux");
        ax.set_title("imshow: one call, no pixel loop");
        // end migrate
        fig.save_png("target/gallery_migrate_images.png").unwrap();
    }
    {
        // migrate: colormaps
        use rizzma::Figure;
        use rizzma::core::color::{ColorMapping, LogNorm, colormap};
        use rizzma::figure::Mappable;

        let n = 400;
        let x: Vec<f64> = (0..n)
            .map(|i| (f64::from(i) * 0.37).sin() * f64::from(i).sqrt())
            .collect();
        let y: Vec<f64> = (0..n)
            .map(|i| (f64::from(i) * 0.37).cos() * f64::from(i).sqrt())
            .collect();
        let flux: Vec<f64> = (0..n).map(|i| 10f64.powf(f64::from(i) / 100.0)).collect();

        let mut fig = Figure::new(8.0, 5.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.scatter_mapped(
            &x,
            &y,
            &flux,
            ColorMapping::new("viridis").norm(LogNorm::new(1.0, 1e4)),
        );
        ax.colorbar(Mappable::Collection(0)).set_label("flux (log)");
        ax.set_title("scatter_mapped with a LogNorm");
        // One color, for drawing code of your own.
        let mid = colormap("viridis").expect("a known colormap").sample(0.5);
        assert!(mid.g > mid.r);
        // end migrate
        fig.save_png("target/gallery_migrate_colormaps.png")
            .unwrap();
    }
    {
        // migrate: contours
        use rizzma::Figure;

        let (nrows, ncols) = (80, 80);
        let z: Vec<f64> = (0..nrows * ncols)
            .map(|k| {
                let (x, y) = (
                    f64::from(k % ncols) / 40.0 - 1.0,
                    f64::from(k / ncols) / 40.0 - 1.0,
                );
                (-(x * x + y * y) * 2.0).exp()
                    - 0.6 * (-((x - 0.5).powi(2) + (y + 0.4).powi(2)) * 6.0).exp()
            })
            .collect();

        let mut fig = Figure::new(8.0, 4.0);
        let ax = fig.add_subplot(1, 2, 1);
        ax.contourf(&z, nrows as usize, ncols as usize);
        ax.set_aspect_equal().set_title("contourf");
        let ax = fig.add_subplot(1, 2, 2);
        ax.contour(&z, nrows as usize, ncols as usize);
        ax.clabel();
        ax.set_aspect_equal().set_title("contour + clabel");
        // end migrate
        fig.save_png("target/gallery_migrate_contours.png").unwrap();
    }
    {
        // migrate: histograms
        use rizzma::Figure;

        // A deterministic, roughly normal sample: sums of twelve hashed uniforms.
        let uniform = |n: u32| (f64::from(n).sin() * 43_758.545_3).fract().abs();
        let sample: Vec<f64> = (0..5000)
            .map(|i| (0..12).map(|k| uniform(i * 12 + k)).sum::<f64>() - 6.0)
            .collect();

        let mut fig = Figure::new(8.0, 3.5);
        let ax = fig.add_subplot(1, 2, 1);
        let (counts, edges) = ax.hist(&sample, 40);
        assert_eq!((counts.len(), edges.len()), (40, 41));
        ax.set_title("hist: 40 bins");
        let ax = fig.add_subplot(1, 2, 2);
        ax.bar(&[1.0, 2.0, 3.0, 4.0], &[3.0, 7.0, 4.0, 6.0]);
        ax.set_title("bar");
        // end migrate
        fig.save_png("target/gallery_migrate_histograms.png")
            .unwrap();
    }
    {
        // migrate: aspect
        use rizzma::Figure;

        let theta: Vec<f64> = (0..=100)
            .map(|i| f64::from(i) * std::f64::consts::TAU / 100.0)
            .collect();
        let (x, y): (Vec<f64>, Vec<f64>) = theta.iter().map(|t| (3.0 * t.cos(), t.sin())).unzip();

        let mut fig = Figure::new(8.0, 4.0);
        let ax = fig.add_subplot(1, 2, 1);
        ax.plot(&x, &y);
        ax.set_title("default: stretched");
        let ax = fig.add_subplot(1, 2, 2);
        ax.plot(&x, &y);
        ax.set_aspect_equal().set_title("set_aspect_equal");
        // end migrate
        fig.save_png("target/gallery_migrate_aspect.png").unwrap();
    }
    {
        // migrate: bands
        use rizzma::Figure;

        let t: Vec<f64> = (0..300).map(|i| f64::from(i) / 30.0).collect();
        let model: Vec<f64> = t
            .iter()
            .map(|v| (-v / 4.0).exp() * (2.0 * v).cos())
            .collect();
        let lo: Vec<f64> = model
            .iter()
            .zip(&t)
            .map(|(m, v)| m - 0.1 - 0.02 * v)
            .collect();
        let hi: Vec<f64> = model
            .iter()
            .zip(&t)
            .map(|(m, v)| m + 0.1 + 0.02 * v)
            .collect();

        let mut fig = Figure::new(8.0, 4.0);
        let ax = fig.add_subplot(1, 1, 1);
        ax.fill_between(&t, &lo, &hi);
        ax.plot(&t, &model).set_label("model");
        ax.axvspan(3.0, 4.5);
        ax.axvline(6.0);
        ax.legend_auto();
        ax.set_title("fill_between, axvspan, axvline");
        // end migrate
        fig.save_png("target/gallery_migrate_bands.png").unwrap();
    }
    {
        // migrate: vectors
        use rizzma::Figure;

        let mut x = Vec::new();
        let mut y = Vec::new();
        for i in 0..12 {
            for j in 0..12 {
                x.push(f64::from(i) - 5.5);
                y.push(f64::from(j) - 5.5);
            }
        }
        let u: Vec<f64> = y.iter().map(|v| -v).collect();
        let v: Vec<f64> = x.clone();

        let mut fig = Figure::new(8.0, 3.5);
        let ax = fig.add_subplot(1, 2, 1);
        ax.quiver(&x, &y, &u, &v);
        ax.set_aspect_equal().set_title("quiver");
        let days: Vec<f64> = (0..60).map(f64::from).collect();
        let temp: Vec<f64> = days.iter().map(|d| 12.0 + 8.0 * (d / 9.0).sin()).collect();
        let rain: Vec<f64> = days.iter().map(|d| 40.0 + 30.0 * (d / 5.0).cos()).collect();
        let ax = fig.add_subplot(1, 2, 2);
        ax.plot(&days, &temp);
        ax.set_ylabel("temperature (°C)").set_title("twinx");
        let right = fig.twinx(1);
        let twin = &mut fig.axes_mut()[right];
        twin.plot(&days, &rain);
        twin.set_ylabel("humidity (%)");
        // end migrate
        fig.save_png("target/gallery_migrate_vectors.png").unwrap();
    }
    {
        // migrate: text
        use rizzma::Figure;

        let x: Vec<f64> = (1..=60).map(f64::from).collect();
        let y: Vec<f64> = x.iter().map(|v| 3.0 * v.powf(-1.5)).collect();

        let mut fig = Figure::new(8.0, 4.0);
        fig.suptitle("Power-law decay — Ωₘ = 0.31, λ = 656.3 nm");
        let ax = fig.add_subplot(1, 1, 1);
        ax.plot(&x, &y);
        ax.set_xscale_log(10.0).set_yscale_log(10.0);
        ax.set_xlabel(r"time $t$ (days)")
            .set_ylabel(r"$F_\nu \propto t^{-3/2}$");
        ax.set_title(r"mathtext: $\alpha = -\frac{3}{2}$");
        // end migrate
        fig.save_png("target/gallery_migrate_text.png").unwrap();
    }
    {
        // migrate: buffers
        use rizzma::Figure;
        use rizzma::figure::PixelFormat;

        let x: Vec<f64> = (0..100).map(|i| f64::from(i) / 10.0).collect();
        let y: Vec<f64> = x.iter().map(|v| v.sin() * (-v / 6.0).exp()).collect();
        let mut fig = Figure::new(8.0, 4.0);
        fig.add_subplot(1, 1, 1).plot(&x, &y);
        fig.axes_mut()[0].set_title("encode_png, to_svg, render_into");

        let png: Vec<u8> = fig.encode_png().expect("PNG encoding");
        let svg: String = fig.to_svg();
        let (w, h) = fig.raster_size();
        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        fig.render_into(&mut rgba, PixelFormat::Rgba8)
            .expect("a buffer of the right size");
        assert!(png.starts_with(b"\x89PNG") && svg.contains("<svg"));
        // end migrate
        fig.save_png("target/gallery_migrate_buffers.png").unwrap();
    }
    {
        // migrate: hipparcos
        use rizzma::Figure;
        use rizzma::core::color::{ListedColormap, Rgba};
        use rizzma::figure::Mappable;

        /// starfield's B−V color, plotters' `HSLColor(0.7 − 0.7·t, 0.65, 0.45)`,
        /// converted to RGB as plotters does.
        fn starfield_bv(t: f64) -> Rgba {
            let (h, s, l) = (0.7 - 0.7 * t, 0.65, 0.45);
            let q = if l < 0.5 {
                l * (1.0 + s)
            } else {
                l + s - l * s
            };
            let p = 2.0 * l - q;
            let channel = |t: f64| {
                let t = t.rem_euclid(1.0);
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

        /// G against Hp, colored by B−V, with the fitted relation at the mean B−V.
        fn plot_g_vs_hp(data: &[(f64, f64, f64)], coef: (f64, f64, f64), rms: f64) -> Figure {
            let hp: Vec<f64> = data.iter().map(|d| d.0).collect();
            let bv: Vec<f64> = data.iter().map(|d| d.1).collect();
            let g: Vec<f64> = data.iter().map(|d| d.2).collect();
            let mean_bv = bv.iter().sum::<f64>() / bv.len() as f64;
            let (lo, hi) = hp
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                    (lo.min(v), hi.max(v))
                });
            let fit_hp = [lo, hi];
            let fit_g = fit_hp.map(|h| coef.0 + coef.1 * h + coef.2 * mean_bv);

            let mut fig = Figure::new(8.0, 6.0);
            let ax = fig.add_subplot(1, 1, 1);
            ax.scatter_mapped(&hp, &g, &bv, ListedColormap::from_fn(starfield_bv))
                .set_sizes(vec![4.0]);
            ax.plot(&fit_hp, &fit_g)
                .set_linewidth(2.0)
                .set_label(format!(
                    "G = {:+.3} {:+.3}·Hp {:+.3}·(B−V) at ⟨B−V⟩ = {mean_bv:.2}",
                    coef.0, coef.1, coef.2
                ));
            ax.colorbar(Mappable::Collection(0)).set_label("B − V");
            ax.legend_auto();
            ax.set_xlabel("Hipparcos Hp magnitude")
                .set_ylabel("Gaia DR3 G magnitude");
            ax.set_title(format!(
                "Hipparcos × Gaia DR3: G vs Hp (N={}, RMS={rms:.3})",
                data.len()
            ));
            fig
        }

        // Synthetic stand-in for the cross-match: G ≈ Hp − 0.2 − 0.3·(B−V).
        let data: Vec<(f64, f64, f64)> = (0..1500)
            .map(|i| {
                let u = (f64::from(i) * 0.618_034).fract();
                let w = (f64::from(i) * 0.414_214).fract();
                let hp = 2.0 + 8.0 * u.sqrt();
                let bv = -0.2 + 1.8 * w;
                (
                    hp,
                    bv,
                    hp - 0.2 - 0.3 * bv + 0.05 * (f64::from(i) * 1.7).sin(),
                )
            })
            .collect();
        let fig = plot_g_vs_hp(&data, (-0.2, 1.0, -0.3), 0.035);
        // end migrate
        fig.save_png("target/gallery_migrate_hipparcos.png")
            .unwrap();
    }

    println!("wrote target/gallery_*.png");
}
