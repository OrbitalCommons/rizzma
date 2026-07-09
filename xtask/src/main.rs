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
//! `wasm-size` is the lightweight M4 guardrail for tracking the browser
//! artifact size without requiring `wasm-pack` or a browser.

use std::collections::{BTreeMap, BTreeSet};
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
         Compare two PNG images by root-mean-square per-channel difference.\n    \
         check-gallery-links [--gallery-dir <dir>] [--strict] [PATHS...]\n        \
         Verify every `gallery_*.png` referenced in the README/docs is actually\n        \
         produced by the gallery example (run it first). --strict also fails on\n        \
         generated-but-unreferenced images.\n    \
         wasm-size <artifact.wasm> [--max-bytes <N>]\n        \
         Report a wasm artifact size and optionally fail when it exceeds N bytes.\n    \
         serve-www [--port <u16>] [--dir <path>]\n        \
         Serve the interactive wasm demo site (default crates/rizzma/www) over HTTP."
    );
}

/// Every `gallery_<name>.png` token in `text`.
///
/// `<name>` is `[A-Za-z0-9_]+`. Matching is byte-exact on the ASCII `gallery_`
/// prefix and `.png` suffix, so it is safe to slice around UTF-8 in the source.
fn find_gallery_tokens(text: &str) -> Vec<String> {
    const PREFIX: &[u8] = b"gallery_";
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + PREFIX.len() <= bytes.len() {
        if &bytes[i..i + PREFIX.len()] != PREFIX {
            i += 1;
            continue;
        }
        let mut j = i + PREFIX.len();
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > i + PREFIX.len() && text[j..].starts_with(".png") {
            out.push(text[i..j + 4].to_string());
        }
        i = j.max(i + 1);
    }
    out
}

/// `gallery_*.png` files actually present in `dir`.
fn gather_generated(dir: &Path) -> std::io::Result<BTreeSet<String>> {
    let mut set = BTreeSet::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with("gallery_") && name.ends_with(".png") {
            set.insert(name);
        }
    }
    Ok(set)
}

/// Recursively collect `.rs`/`.md` files under `root` (skipping `target` and
/// dot-directories); a file `root` is taken as-is if it has a matching extension.
fn collect_doc_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if matches!(root.extension().and_then(|e| e.to_str()), Some("rs" | "md")) {
            out.push(root.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Skip build output, dot-dirs, and generators (examples/tests) — only
        // human-facing docs (READMEs + `src` doc comments) count as references.
        if matches!(name.as_ref(), "target" | "examples" | "tests") || name.starts_with('.') {
            continue;
        }
        collect_doc_files(&entry.path(), out);
    }
}

/// Run the `check-gallery-links` subcommand.
fn run_check_gallery_links(args: &[String]) -> ExitCode {
    let mut gallery_dir = PathBuf::from("target");
    let mut strict = false;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--gallery-dir" => {
                i += 1;
                match args.get(i) {
                    Some(v) => gallery_dir = PathBuf::from(v),
                    None => {
                        eprintln!("error: --gallery-dir requires a value");
                        return ExitCode::from(2);
                    }
                }
            }
            "--strict" => strict = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") => {
                eprintln!("error: unknown flag: {other}");
                return ExitCode::from(2);
            }
            other => roots.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if roots.is_empty() {
        roots = vec![PathBuf::from("README.md"), PathBuf::from("crates")];
    }

    let generated = match gather_generated(&gallery_dir) {
        Ok(set) => set,
        Err(e) => {
            eprintln!("error: reading {}: {e}", gallery_dir.display());
            return ExitCode::FAILURE;
        }
    };
    if generated.is_empty() {
        eprintln!(
            "error: no gallery_*.png in {} — run `cargo run -p rizzma --example gallery` first",
            gallery_dir.display()
        );
        return ExitCode::FAILURE;
    }

    let mut files = Vec::new();
    for root in &roots {
        collect_doc_files(root, &mut files);
    }
    let mut referenced: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for file in &files {
        if let Ok(text) = std::fs::read_to_string(file) {
            for token in find_gallery_tokens(&text) {
                referenced.entry(token).or_default().push(file.clone());
            }
        }
    }

    let dangling: Vec<(&String, &Vec<PathBuf>)> = referenced
        .iter()
        .filter(|(token, _)| !generated.contains(*token))
        .collect();
    let unreferenced: Vec<&String> = generated
        .iter()
        .filter(|name| !referenced.contains_key(*name))
        .collect();

    println!(
        "gallery link check: {} generated, {} referenced ({} files scanned)",
        generated.len(),
        referenced.len(),
        files.len()
    );
    for (token, where_) in &dangling {
        let locs: Vec<String> = where_.iter().map(|p| p.display().to_string()).collect();
        eprintln!(
            "  DANGLING: `{token}` referenced in {} but not produced by the gallery example",
            locs.join(", ")
        );
    }
    for name in &unreferenced {
        eprintln!("  UNREFERENCED: `{name}` is generated but not referenced in any README/doc");
    }

    if !dangling.is_empty() || (strict && !unreferenced.is_empty()) {
        ExitCode::FAILURE
    } else {
        println!(
            "OK: all {} referenced gallery image(s) are generated.",
            referenced.len()
        );
        ExitCode::SUCCESS
    }
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

/// Parsed arguments for the `wasm-size` subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WasmSizeArgs {
    path: PathBuf,
    max_bytes: Option<u64>,
}

/// Parse `wasm-size <artifact.wasm> [--max-bytes <N>]`.
fn parse_wasm_size(args: &[String]) -> Result<WasmSizeArgs, String> {
    let mut positional: Vec<PathBuf> = Vec::new();
    let mut max_bytes = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--max-bytes" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--max-bytes requires a value".to_string())?;
                max_bytes = Some(
                    v.parse::<u64>()
                        .map_err(|e| format!("invalid --max-bytes value {v:?}: {e}"))?,
                );
                i += 2;
            }
            "-h" | "--help" => return Err("help".to_string()),
            other if other.starts_with("--") => return Err(format!("unknown flag: {other}")),
            other => {
                positional.push(PathBuf::from(other));
                i += 1;
            }
        }
    }

    if positional.len() != 1 {
        return Err(format!(
            "expected exactly one wasm artifact path, got {}",
            positional.len()
        ));
    }

    Ok(WasmSizeArgs {
        path: positional.pop().expect("one positional arg present"),
        max_bytes,
    })
}

/// A short human-readable rendering of `bytes`.
fn format_bytes(bytes: u64) -> String {
    let kib = bytes as f64 / 1024.0;
    let mib = kib / 1024.0;
    format!("{bytes} bytes ({kib:.1} KiB, {mib:.2} MiB)")
}

/// Run the `wasm-size` subcommand.
fn run_wasm_size(args: &[String]) -> ExitCode {
    let parsed = match parse_wasm_size(args) {
        Ok(p) => p,
        Err(msg) => {
            if msg != "help" {
                eprintln!("error: {msg}\n");
            }
            eprintln!("USAGE:\n    cargo xtask wasm-size <artifact.wasm> [--max-bytes <N>]");
            return ExitCode::from(2);
        }
    };

    let metadata = match std::fs::metadata(&parsed.path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to stat {}: {e}", parsed.path.display());
            return ExitCode::FAILURE;
        }
    };
    let bytes = metadata.len();
    println!(
        "wasm-size: {} {}",
        parsed.path.display(),
        format_bytes(bytes)
    );

    if let Some(max) = parsed.max_bytes
        && bytes > max
    {
        eprintln!(
            "FAIL: wasm artifact is {} over the budget of {}",
            format_bytes(bytes - max),
            format_bytes(max)
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// The content type for a served file, by extension.
///
/// `application/wasm` matters: without it browsers refuse
/// `WebAssembly.instantiateStreaming`, and the ES-module demo fails to load.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") | Some("map") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Resolve a request path to a file under `root`, or `None` when the path
/// escapes the root or does not exist.
///
/// `/` maps to `index.html`; every path component must be a plain name (no
/// `..`, no absolute segments), so the server can only expose the demo dir.
fn resolve_request_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let path = request_path.split(['?', '#']).next().unwrap_or("");
    let relative = path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    let mut resolved = root.to_path_buf();
    for component in relative.split('/') {
        if component.is_empty() || component == ".." || component == "." || component.contains('\\')
        {
            return None;
        }
        resolved.push(component);
    }
    resolved.is_file().then_some(resolved)
}

/// Serve one HTTP connection: parse the request line, resolve the path, and
/// write the file (or a 404) back.
fn serve_connection(mut stream: std::net::TcpStream, root: &Path) {
    use std::io::{BufRead, BufReader, Read, Write};

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drain the request headers so the client sees a clean connection close.
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok() && line != "\r\n" && !line.is_empty() {
        line.clear();
    }

    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let response = if method != "GET" {
        (
            405,
            "text/plain; charset=utf-8",
            b"method not allowed".to_vec(),
        )
    } else {
        match resolve_request_path(root, path) {
            Some(file) => match std::fs::File::open(&file) {
                Ok(mut f) => {
                    let mut body = Vec::new();
                    match f.read_to_end(&mut body) {
                        Ok(_) => (200, content_type(&file), body),
                        Err(_) => (500, "text/plain; charset=utf-8", b"read error".to_vec()),
                    }
                }
                Err(_) => (404, "text/plain; charset=utf-8", b"not found".to_vec()),
            },
            None => (404, "text/plain; charset=utf-8", b"not found".to_vec()),
        }
    };

    let (status, mime, body) = response;
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
}

/// Run the `serve-www` subcommand: a tiny static file server for the wasm
/// demo site (`crates/rizzma/www` by default). Dependency-free by design —
/// browsers need real HTTP (not `file://`) for ES modules and `.wasm`.
fn run_serve_www(args: &[String]) -> ExitCode {
    let mut port: u16 = 8000;
    let mut dir = PathBuf::from("crates/rizzma/www");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(p) => port = p,
                    None => {
                        eprintln!("error: --port requires a number");
                        return ExitCode::from(2);
                    }
                }
            }
            "--dir" => {
                i += 1;
                match args.get(i) {
                    Some(v) => dir = PathBuf::from(v),
                    None => {
                        eprintln!("error: --dir requires a path");
                        return ExitCode::from(2);
                    }
                }
            }
            "-h" | "--help" => {
                eprintln!("USAGE:\n    cargo xtask serve-www [--port <u16>] [--dir <path>]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    if !dir.join("index.html").is_file() {
        eprintln!("error: {} has no index.html", dir.display());
        eprintln!("build the wasm bundle first:");
        eprintln!(
            "    wasm-pack build --target web --out-dir crates/rizzma/www/pkg crates/rizzma --features wasm"
        );
        return ExitCode::FAILURE;
    }

    let listener = match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: failed to bind port {port}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("serving {} at http://localhost:{port}/", dir.display());

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let root = dir.clone();
        std::thread::spawn(move || serve_connection(stream, &root));
    }
    ExitCode::SUCCESS
}

/// CLI dispatch.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("image-diff") => run_image_diff(&args[1..]),
        Some("check-gallery-links") => run_check_gallery_links(&args[1..]),
        Some("wasm-size") => run_wasm_size(&args[1..]),
        Some("serve-www") => run_serve_www(&args[1..]),
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

    #[test]
    fn finds_gallery_tokens_in_markdown_and_docs() {
        let text = "see ![p](https://x/gallery_plot.png) and `gallery_bar.png` here";
        let toks = find_gallery_tokens(text);
        assert_eq!(toks, vec!["gallery_plot.png", "gallery_bar.png"]);
    }

    #[test]
    fn ignores_prefix_without_png_suffix_and_survives_unicode() {
        // A `gallery_` with no `.png`, plus UTF-8 bytes around a real token.
        let text = "→ gallery_thing.svg café ![x](gallery_imshow.png) ✓";
        assert_eq!(find_gallery_tokens(text), vec!["gallery_imshow.png"]);
    }

    #[test]
    fn bare_prefix_yields_no_token() {
        assert!(find_gallery_tokens("gallery_.png and gallery_").is_empty());
    }

    #[test]
    fn parse_wasm_size_accepts_optional_budget() {
        let args = vec![
            "target/wasm32-unknown-unknown/release/rizzma.wasm".to_string(),
            "--max-bytes".to_string(),
            "2500000".to_string(),
        ];
        let parsed = parse_wasm_size(&args).expect("valid args");

        assert_eq!(
            parsed.path,
            PathBuf::from("target/wasm32-unknown-unknown/release/rizzma.wasm")
        );
        assert_eq!(parsed.max_bytes, Some(2_500_000));
    }

    #[test]
    fn parse_wasm_size_rejects_missing_path() {
        let err = parse_wasm_size(&[]).expect_err("path required");
        assert!(err.contains("expected exactly one wasm artifact path"));
    }

    #[test]
    fn format_bytes_reports_bytes_kib_and_mib() {
        assert_eq!(
            format_bytes(2_199_745),
            "2199745 bytes (2148.2 KiB, 2.10 MiB)"
        );
    }
}
