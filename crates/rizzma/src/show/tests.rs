//! Server-side tests for the browser viewer: they drive the real loopback HTTP
//! server with a raw `TcpStream` client (no browser required), exercising the
//! index page, per-figure render, event → re-render, export, and token guard.

use std::io::{Read, Write};
use std::net::TcpStream;

use super::{ShowConfig, ShowHandle, show_nonblocking};
use crate::Figure;

/// A demo figure with a line so zoom/pan visibly change the render.
fn demo() -> Figure {
    let mut fig = Figure::new(4.0, 3.0);
    let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);
    ax.plot(&[0.0, 5.0, 10.0], &[0.0, 8.0, 3.0]);
    ax.set_xlim(0.0, 10.0);
    ax.set_ylim(0.0, 10.0);
    fig
}

fn spawn(figs: Vec<Figure>) -> ShowHandle {
    show_nonblocking(
        figs,
        ShowConfig {
            title: "test".to_string(),
            open_browser: false,
        },
    )
}

/// Parse `http://127.0.0.1:PORT/TOKEN/` into `(port, token)`.
fn port_token(url: &str) -> (u16, String) {
    let rest = url.strip_prefix("http://127.0.0.1:").expect("loopback url");
    let (port, tail) = rest.split_once('/').expect("port/token");
    (
        port.parse().expect("port"),
        tail.trim_end_matches('/').to_string(),
    )
}

/// Minimal HTTP/1.1 GET: returns `(status, content_type, body)`.
fn get(port: u16, path: &str) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read");

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = raw[split + 4..].to_vec();

    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status code");
    let content_type = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Type: "))
        .unwrap_or("")
        .to_string();
    (status, content_type, body)
}

#[test]
fn serves_index_and_png() {
    let h = spawn(vec![demo()]);
    let (port, token) = port_token(h.url());

    let (status, ctype, body) = get(port, &format!("/{token}/"));
    assert_eq!(status, 200);
    assert!(ctype.starts_with("text/html"));
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("<canvas id=\"fig0\""),
        "index lists the figure"
    );

    let (status, ctype, body) = get(port, &format!("/{token}/fig/0.png"));
    assert_eq!(status, 200);
    assert_eq!(ctype, "image/png");
    assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");

    h.close();
}

#[test]
fn wheel_event_changes_the_frame() {
    let h = spawn(vec![demo()]);
    let (port, token) = port_token(h.url());

    let (_, _, before) = get(port, &format!("/{token}/fig/0.png"));
    // Zoom in at the center of the axes; the render must change.
    let (status, ctype, after) = get(
        port,
        &format!("/{token}/fig/0/ev?type=wheel&x=200&y=150&dy=-1"),
    );
    assert_eq!(status, 200);
    assert_eq!(ctype, "image/png");
    assert_ne!(
        before, after,
        "a zoom event must re-render a different frame"
    );

    h.close();
}

#[test]
fn exports_svg_and_pdf() {
    let h = spawn(vec![demo()]);
    let (port, token) = port_token(h.url());

    let (status, ctype, body) = get(port, &format!("/{token}/fig/0.svg"));
    assert_eq!(status, 200);
    assert_eq!(ctype, "image/svg+xml");
    assert!(String::from_utf8_lossy(&body).contains("<svg"));

    let (status, ctype, body) = get(port, &format!("/{token}/fig/0.pdf"));
    assert_eq!(status, 200);
    assert_eq!(ctype, "application/pdf");
    assert_eq!(&body[..5], b"%PDF-", "PDF magic");

    h.close();
}

#[test]
fn rejects_a_bad_token_and_missing_figure() {
    let h = spawn(vec![demo()]);
    let (port, token) = port_token(h.url());

    let (status, _, _) = get(port, "/not-the-token/");
    assert_eq!(status, 403, "wrong token is forbidden");

    let (status, _, _) = get(port, &format!("/{token}/fig/9.png"));
    assert_eq!(status, 404, "out-of-range figure is 404");

    h.close();
}

#[test]
fn shows_multiple_figures_in_one_window() {
    let h = spawn(vec![demo(), demo(), demo()]);
    let (port, token) = port_token(h.url());

    let (_, _, body) = get(port, &format!("/{token}/"));
    let html = String::from_utf8_lossy(&body);
    for i in 0..3 {
        assert!(
            html.contains(&format!("<canvas id=\"fig{i}\"")),
            "figure {i} present"
        );
    }
    let (status, _, _) = get(port, &format!("/{token}/fig/2.png"));
    assert_eq!(status, 200);

    h.close();
}
