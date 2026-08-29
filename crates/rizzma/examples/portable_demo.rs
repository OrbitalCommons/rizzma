//! The canonical portable-figure demo: an animated travelling wave whose
//! wavelength is a slider, above a Gaussian pulse whose width is a slider.
//!
//! Writes `demo.riz` plus the poster- and live-tier `.riz.html` wrappers (the
//! live tier only when `RIZZMA_RUNTIME_DIR` points at a directory holding
//! `rizzma_bg.wasm`, `rizzma.js`, and `rizzma-mount.js` — e.g. an unpacked
//! release).
//!
//! The wave loops **crisply**: its phase is `2π(x/λ − t/T)`, periodic in `t`
//! with period exactly `T` for every wavelength, and the keyframe at `t = T`
//! is byte-identical to the one at `t = 0`, so the loop point interpolates
//! into a repaint of the first frame rather than a jump.

use rizzma::Figure;
use rizzma::portable::{Control, Grid, Interp, Limits, Target, Timeline, Track};

const TAU: f64 = std::f64::consts::TAU;

fn wave(x: &[f64], t_frac: f64, wavelength: f64) -> Vec<f64> {
    x.iter()
        .map(|&v| (TAU * (v / wavelength - t_frac)).sin() * (-v / 6.0).exp())
        .collect()
}

fn pulse(x: &[f64], sigma: f64) -> Vec<f64> {
    x.iter()
        .map(|&v| (-((v - 6.0) * (v - 6.0)) / (2.0 * sigma * sigma)).exp())
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let x: Vec<f64> = (0..300).map(|i| i as f64 * 0.04).collect();
    let duration = 2.0;

    // Keyframes: 12 per period plus the wrap frame at t = T, identical to
    // t = 0 — the crisp loop is a property of the data, not the player.
    let times: Vec<f64> = (0..=12).map(|k| k as f64 * duration / 12.0).collect();
    let wavelengths = [0.6, 1.0, 1.5, 2.2, 3.0];
    let sigmas = [0.3, 0.8, 1.5, 2.5];

    let mut fig = Figure::new(6.4, 4.8);
    let ax = fig.add_subplot(2, 1, 1);
    ax.plot(&x, &wave(&x, 0.0, 1.5));
    ax.set_title("travelling wave");
    ax.set_ylim(-1.05, 1.05);

    let ax2 = fig.add_subplot(2, 1, 2);
    ax2.plot(&x, &pulse(&x, 0.8));
    ax2.set_title("gaussian pulse");
    ax2.set_xlabel("x");
    ax2.set_ylim(0.0, 1.05);

    // The clock alone drives nothing the wavelength doesn't also shape, so the
    // wave lives entirely in the control's grid; the timeline provides the
    // clock it is evaluated against.
    fig.set_timeline(Timeline::new(duration));

    let mut wavelength = Control::new("wavelength", 0.6, 3.0, 1.5)?;
    wavelength.push_grid(Grid::new(
        Target::LineY { axes: 0, index: 0 },
        times.clone(),
        wavelengths.to_vec(),
        times
            .iter()
            .map(|&t| {
                wavelengths
                    .iter()
                    .map(|&w| wave(&x, t / duration, w))
                    .collect()
            })
            .collect(),
        Interp::Linear,
    )?);
    fig.add_control(wavelength);

    let mut width = Control::new("pulse width", 0.3, 2.5, 0.8)?;
    width.push_track(Track::new(
        Target::LineY { axes: 1, index: 0 },
        sigmas.to_vec(),
        sigmas.iter().map(|&s| pulse(&x, s)).collect(),
        Interp::Linear,
    )?);
    fig.add_control(width);

    let riz = fig.to_portable()?;
    std::fs::write("demo.riz", &riz)?;
    let limits = Limits::default();
    std::fs::write("demo.riz.html", rizzma::portable::wrap_html(&riz, &limits)?)?;

    if let Ok(dir) = std::env::var("RIZZMA_RUNTIME_DIR") {
        let dir = std::path::Path::new(&dir);
        let wasm = std::fs::read(dir.join("rizzma_bg.wasm"))?;
        let glue = std::fs::read_to_string(dir.join("rizzma.js"))?;
        let loader = std::fs::read_to_string(dir.join("rizzma-mount.js"))?;
        let rt = rizzma::portable::HtmlRuntime {
            wasm: &wasm,
            glue: &glue,
            loader: &loader,
        };
        std::fs::write(
            "demo-live.riz.html",
            rizzma::portable::wrap_html_live(&riz, &limits, &rt)?,
        )?;
        println!("demo.riz + demo.riz.html + demo-live.riz.html");
    } else {
        println!("demo.riz + demo.riz.html (set RIZZMA_RUNTIME_DIR for the live tier)");
    }
    Ok(())
}
