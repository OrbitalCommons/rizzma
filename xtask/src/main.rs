//! Workspace automation entry point.
//!
//! Run via the cargo alias defined in `.cargo/config.toml`:
//!
//! ```text
//! cargo xtask image-diff <a.png> <b.png> [--tolerance <f64>] [--out <diff.png>]
//! ```
//!
//! The `image-diff` subcommand is the substrate for rizzma's matplotlib
//! golden-image tests: it compares a baseline render against a freshly
//! produced image and reports a root-mean-square per-channel difference.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use image::RgbaImage;

/// Default per-channel RMS tolerance (on the 0-255 scale) used when the
/// caller does not pass `--tolerance`.
const DEFAULT_TOLERANCE: f64 = 2.0;

/// Outcome of comparing two images.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffResult {
    /// Root-mean-square per-channel pixel difference on the 0-255 scale.
    pub rms: f64,
    /// Largest absolute single-channel difference encountered.
    pub max_abs: u8,
    /// Whether [`DiffResult::rms`] is within the requested tolerance.
    pub passed: bool,
}

/// Compare two RGBA images and optionally write an absolute-difference image.
///
/// Computes the root-mean-square per-channel difference (counting all four
/// RGBA channels of every pixel) on the 0-255 scale, plus the maximum absolute
/// single-channel difference. The comparison `passed` when the RMS value is
/// less than or equal to `tolerance`.
///
/// When `out` is `Some`, an image whose pixels are the absolute per-channel
/// difference of `a` and `b` is written to that path.
///
/// # Errors
///
/// Returns an error if the two images differ in dimensions, or if writing the
/// optional diff image fails.
pub fn compare(
    a: &RgbaImage,
    b: &RgbaImage,
    tolerance: f64,
    out: Option<&Path>,
) -> Result<DiffResult, String> {
    if a.dimensions() != b.dimensions() {
        return Err(format!(
            "image dimensions differ: {:?} vs {:?}",
            a.dimensions(),
            b.dimensions()
        ));
    }

    let mut sum_sq = 0.0_f64;
    let mut max_abs = 0_u8;
    let mut diff = out.map(|_| RgbaImage::new(a.width(), a.height()));

    for y in 0..a.height() {
        for x in 0..a.width() {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            let mut abs_px = [0_u8; 4];
            for c in 0..4 {
                let delta = i32::from(pa[c]) - i32::from(pb[c]);
                let abs = delta.unsigned_abs() as u8;
                abs_px[c] = abs;
                max_abs = max_abs.max(abs);
                sum_sq += f64::from(delta * delta);
            }
            if let Some(diff_img) = diff.as_mut() {
                diff_img.put_pixel(x, y, image::Rgba(abs_px));
            }
        }
    }

    let count = (a.width() as u64 * a.height() as u64 * 4) as f64;
    let rms = if count == 0.0 {
        0.0
    } else {
        (sum_sq / count).sqrt()
    };

    if let (Some(path), Some(diff_img)) = (out, diff.as_ref()) {
        diff_img
            .save(path)
            .map_err(|e| format!("failed to write diff image to {}: {e}", path.display()))?;
    }

    Ok(DiffResult {
        rms,
        max_abs,
        passed: rms <= tolerance,
    })
}

/// Print top-level usage to stderr.
fn print_usage() {
    eprintln!(
        "xtask - rizzma workspace automation\n\n\
         USAGE:\n    \
         cargo xtask <SUBCOMMAND>\n\n\
         SUBCOMMANDS:\n    \
         image-diff <a.png> <b.png> [--tolerance <f64>] [--out <diff.png>]\n        \
         Compare two PNG images by root-mean-square per-channel difference."
    );
}

/// Parsed arguments for the `image-diff` subcommand.
struct ImageDiffArgs {
    a: PathBuf,
    b: PathBuf,
    tolerance: f64,
    out: Option<PathBuf>,
}

/// Parse the `image-diff` arguments from the raw argument list (already
/// stripped of the program name and subcommand).
fn parse_image_diff(args: &[String]) -> Result<ImageDiffArgs, String> {
    let mut positional: Vec<PathBuf> = Vec::new();
    let mut tolerance = DEFAULT_TOLERANCE;
    let mut out: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tolerance" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--tolerance requires a value".to_string())?;
                tolerance = v
                    .parse::<f64>()
                    .map_err(|e| format!("invalid --tolerance value {v:?}: {e}"))?;
                i += 2;
            }
            "--out" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                out = Some(PathBuf::from(v));
                i += 2;
            }
            "-h" | "--help" => {
                return Err("help".to_string());
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {other}"));
            }
            other => {
                positional.push(PathBuf::from(other));
                i += 1;
            }
        }
    }

    if positional.len() != 2 {
        return Err(format!(
            "expected exactly two image paths, got {}",
            positional.len()
        ));
    }

    let mut it = positional.into_iter();
    Ok(ImageDiffArgs {
        a: it.next().expect("two positional args present"),
        b: it.next().expect("two positional args present"),
        tolerance,
        out,
    })
}

/// Run the `image-diff` subcommand, returning a process exit code.
fn run_image_diff(args: &[String]) -> ExitCode {
    let parsed = match parse_image_diff(args) {
        Ok(p) => p,
        Err(msg) => {
            if msg != "help" {
                eprintln!("error: {msg}\n");
            }
            eprintln!(
                "USAGE:\n    cargo xtask image-diff <a.png> <b.png> \
                 [--tolerance <f64>] [--out <diff.png>]"
            );
            return ExitCode::from(2);
        }
    };

    let a = match image::open(&parsed.a) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("error: failed to open {}: {e}", parsed.a.display());
            return ExitCode::FAILURE;
        }
    };
    let b = match image::open(&parsed.b) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("error: failed to open {}: {e}", parsed.b.display());
            return ExitCode::FAILURE;
        }
    };

    match compare(&a, &b, parsed.tolerance, parsed.out.as_deref()) {
        Ok(result) => {
            let verdict = if result.passed { "PASS" } else { "FAIL" };
            println!(
                "{verdict}: rms={:.4} max_abs={} tolerance={:.4}",
                result.rms, result.max_abs, parsed.tolerance
            );
            if let Some(out) = parsed.out.as_deref() {
                println!("wrote diff image to {}", out.display());
            }
            if result.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// CLI dispatch.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("image-diff") => run_image_diff(&args[1..]),
        Some("-h") | Some("--help") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown subcommand: {other}\n");
            print_usage();
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A solid image of a single RGBA color.
    fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(color))
    }

    #[test]
    fn identical_images_have_zero_rms_and_pass() {
        let a = solid(4, 3, [10, 20, 30, 255]);
        let b = solid(4, 3, [10, 20, 30, 255]);
        let result = compare(&a, &b, DEFAULT_TOLERANCE, None).expect("same dimensions");
        assert_eq!(result.rms, 0.0);
        assert_eq!(result.max_abs, 0);
        assert!(result.passed);
    }

    #[test]
    fn single_pixel_delta_has_expected_rms() {
        // 2x1 image, 4 channels => 8 channel samples total.
        let a = solid(2, 1, [0, 0, 0, 0]);
        let mut b = solid(2, 1, [0, 0, 0, 0]);
        // One channel of one pixel differs by 100.
        b.put_pixel(0, 0, Rgba([100, 0, 0, 0]));

        // sum_sq = 100^2 = 10000 over 8 channel samples.
        let expected = (10000.0_f64 / 8.0).sqrt();
        let result = compare(&a, &b, DEFAULT_TOLERANCE, None).expect("same dimensions");
        assert!((result.rms - expected).abs() < 1e-9);
        assert_eq!(result.max_abs, 100);
        assert!(!result.passed);
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = solid(2, 2, [0, 0, 0, 255]);
        let b = solid(3, 2, [0, 0, 0, 255]);
        let err = compare(&a, &b, DEFAULT_TOLERANCE, None).expect_err("dimensions differ");
        assert!(err.contains("dimensions differ"));
    }
}
