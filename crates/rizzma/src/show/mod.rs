//! `show()` — an interactive local-browser viewer for figures.
//!
//! Mirrors matplotlib's `plt.show()`: hand one or more [`Figure`]s to
//! [`show`]/[`show_all`] (or call [`Figure::show`]) and rizzma opens the
//! default browser onto a single window holding every figure, each pan/zoom/
//! reset-interactive and exportable to PNG/SVG/PDF.
//!
//! # Architecture
//!
//! This is the *server round-trip* model (matplotlib's WebAgg): the native
//! process keeps each figure inside an `Interactor`, and the browser page is
//! a thin canvas that sends pointer events over HTTP and paints the PNG frame
//! the server renders back. It reuses the existing interaction engine
//! (`figure::interact`) and every render backend, so nothing about the
//! figures needs to be serialized.
//!
//! The server is a minimal, dependency-free HTTP/1.1 loop over
//! [`std::net::TcpListener`] bound to loopback on an ephemeral port, guarded by
//! a per-session token in the URL. [`show`] blocks until the window is closed
//! (the page pings `…/shutdown` on unload, and a toolbar **✕** does the same);
//! [`show_nonblocking`] runs the loop on a background thread and hands back a
//! [`ShowHandle`].
//!
//! ```no_run
//! use rizzma::Figure;
//!
//! let mut fig = Figure::new(6.0, 4.0);
//! let ax = fig.add_axes(0.12, 0.12, 0.8, 0.8);
//! ax.plot(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]);
//! fig.show(); // opens the browser and blocks until the window is closed
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::figure::Figure;
use crate::figure::event::{Event, MouseButton};
use crate::figure::interact::Interactor;

/// Configuration for a viewer session.
#[derive(Debug, Clone)]
pub struct ShowConfig {
    /// Window/tab title shown in the browser.
    pub title: String,
    /// Attempt to open the system browser automatically. When `false` (or when
    /// no opener is available), the URL is printed to stderr instead.
    pub open_browser: bool,
}

impl Default for ShowConfig {
    fn default() -> Self {
        Self {
            title: "rizzma".to_string(),
            open_browser: true,
        }
    }
}

/// A running non-blocking viewer, returned by [`show_nonblocking`].
///
/// The server keeps running until [`close`](ShowHandle::close) is called (or
/// the process exits). Dropping the handle detaches the server thread without
/// stopping it.
pub struct ShowHandle {
    url: String,
    port: u16,
    token: String,
    thread: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl ShowHandle {
    /// The `http://127.0.0.1:PORT/TOKEN/` URL the viewer is served at.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Ask the server to stop and wait for its thread to finish.
    pub fn close(mut self) {
        self.running.store(false, Ordering::SeqCst);
        // Unblock the accept loop with a throwaway request.
        let _ = fetch_local(self.port, &format!("/{}/shutdown", self.token));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Show a single figure, blocking until the window is closed.
///
/// Convenience for `show_all(vec![figure], ShowConfig::default())`.
pub fn show(figure: Figure) {
    show_all(vec![figure], ShowConfig::default());
}

/// Show several figures in one window, blocking until it is closed.
pub fn show_all(figures: Vec<Figure>, config: ShowConfig) {
    let mut server = Server::bind(figures, config).expect("bind loopback viewer");
    server.announce();
    server.run_blocking();
}

/// Show figures without blocking: the server runs on a background thread and a
/// [`ShowHandle`] is returned for later [`close`](ShowHandle::close).
#[must_use]
pub fn show_nonblocking(figures: Vec<Figure>, config: ShowConfig) -> ShowHandle {
    let server = Server::bind(figures, config).expect("bind loopback viewer");
    server.announce();
    server.spawn()
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The viewer's HTTP server: a figure per `Interactor`, a loopback listener,
/// and a session token.
struct Server {
    listener: TcpListener,
    interactors: Vec<Interactor>,
    sizes: Vec<(u32, u32)>,
    config: ShowConfig,
    token: String,
    port: u16,
    running: Arc<AtomicBool>,
}

impl Server {
    fn bind(figures: Vec<Figure>, config: ShowConfig) -> std::io::Result<Self> {
        assert!(!figures.is_empty(), "show: need at least one figure");
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let sizes = figures
            .iter()
            .map(|f| {
                let (w, h) = f.size_px();
                (w.round() as u32, h.round() as u32)
            })
            .collect();
        let interactors = figures.into_iter().map(Interactor::new).collect();
        Ok(Self {
            listener,
            interactors,
            sizes,
            config,
            token: session_token(),
            port,
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/{}/", self.port, self.token)
    }

    /// Open the browser (or print the URL).
    fn announce(&self) {
        let url = self.url();
        if self.config.open_browser && open_browser(&url) {
            eprintln!("rizzma: opened {url}");
        } else {
            eprintln!("rizzma: view your figures at {url}");
        }
    }

    /// Run the accept loop on the current thread until shutdown.
    fn run_blocking(&mut self) {
        loop {
            // `accept` borrows the listener only until it returns an owned
            // stream, leaving `self` free to borrow mutably in `serve`.
            match self.listener.accept() {
                Ok((s, _)) => self.serve(s),
                Err(_) => continue,
            }
            if !self.running.load(Ordering::SeqCst) {
                break;
            }
        }
    }

    /// Move the server onto a background thread.
    fn spawn(self) -> ShowHandle {
        let url = self.url();
        let port = self.port;
        let token = self.token.clone();
        let running = Arc::clone(&self.running);
        let mut server = self;
        let thread = std::thread::spawn(move || server.run_blocking());
        ShowHandle {
            url,
            port,
            token,
            thread: Some(thread),
            running,
        }
    }

    /// Handle one connection: parse the request line, route it, write a
    /// response, and close.
    fn serve(&mut self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        // Drain headers.
        let mut line = String::new();
        while reader.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            if line == "\r\n" || line == "\n" {
                break;
            }
            line.clear();
        }

        let target = request_line.split_whitespace().nth(1).unwrap_or("/");
        let response = self.route(target);
        let _ = write_response(&mut stream, response);
    }

    /// Route a request `target` (`/token/path?query`) to a [`Response`].
    fn route(&mut self, target: &str) -> Response {
        let (path, query) = match target.split_once('?') {
            Some((p, q)) => (p, q),
            None => (target, ""),
        };
        let trimmed = path.trim_start_matches('/');
        let (token, rest) = match trimmed.split_once('/') {
            Some((t, r)) => (t, r),
            None => (trimmed, ""),
        };
        if token != self.token {
            return Response::text(403, "forbidden");
        }

        if rest.is_empty() || rest == "index.html" {
            return Response::html(200, self.page());
        }
        if rest == "shutdown" {
            self.running.store(false, Ordering::SeqCst);
            return Response::text(200, "bye");
        }
        // fig/{i}(.png|.svg|.pdf) or fig/{i}/ev
        if let Some(fig_path) = rest.strip_prefix("fig/") {
            return self.route_figure(fig_path, query);
        }
        Response::text(404, "not found")
    }

    fn route_figure(&mut self, fig_path: &str, query: &str) -> Response {
        // Split index from the trailing verb/extension.
        let (idx_str, tail) = split_index(fig_path);
        let Some(idx) = idx_str
            .parse::<usize>()
            .ok()
            .filter(|&i| i < self.interactors.len())
        else {
            return Response::text(404, "no such figure");
        };

        match tail {
            "png" => match self.interactors[idx].figure().encode_png() {
                Ok(bytes) => Response::bytes(200, "image/png", bytes),
                Err(_) => Response::text(500, "png encode failed"),
            },
            "svg" => Response::bytes(
                200,
                "image/svg+xml",
                self.interactors[idx].figure().to_svg().into_bytes(),
            ),
            "pdf" => Response::bytes(
                200,
                "application/pdf",
                self.interactors[idx].figure().to_pdf(),
            ),
            "ev" => {
                if let Some(ev) = parse_event(query) {
                    self.interactors[idx].handle(ev);
                }
                match self.interactors[idx].figure().encode_png() {
                    Ok(bytes) => Response::bytes(200, "image/png", bytes),
                    Err(_) => Response::text(500, "png encode failed"),
                }
            }
            _ => Response::text(404, "not found"),
        }
    }

    /// The single-window HTML app shell listing every figure.
    fn page(&self) -> String {
        let cards: String = self
            .sizes
            .iter()
            .enumerate()
            .map(|(i, (w, h))| {
                format!(
                    "<figure class=\"card\"><canvas id=\"fig{i}\" width=\"{w}\" height=\"{h}\" \
                     style=\"width:{w}px;height:{h}px\"></canvas></figure>"
                )
            })
            .collect();
        let sizes_js: String = self
            .sizes
            .iter()
            .map(|(w, h)| format!("[{w},{h}]"))
            .collect::<Vec<_>>()
            .join(",");
        PAGE_TEMPLATE
            .replace("{{TITLE}}", &html_escape(&self.config.title))
            .replace("{{TOKEN}}", &self.token)
            .replace("{{COUNT}}", &self.sizes.len().to_string())
            .replace("{{SIZES}}", &sizes_js)
            .replace("{{CARDS}}", &cards)
    }
}

/// A minimal HTTP response.
struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn bytes(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }
    fn text(status: u16, body: &str) -> Self {
        Self::bytes(
            status,
            "text/plain; charset=utf-8",
            body.as_bytes().to_vec(),
        )
    }
    fn html(status: u16, body: String) -> Self {
        Self::bytes(status, "text/html; charset=utf-8", body.into_bytes())
    }
}

fn write_response(stream: &mut TcpStream, resp: Response) -> std::io::Result<()> {
    let reason = match resp.status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        resp.status,
        reason,
        resp.content_type,
        resp.body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&resp.body)?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split `"3.png"` / `"3/ev"` into (`"3"`, `"png"` | `"ev"`).
fn split_index(fig_path: &str) -> (&str, &str) {
    if let Some((idx, verb)) = fig_path.split_once('/') {
        (idx, verb)
    } else if let Some((idx, ext)) = fig_path.rsplit_once('.') {
        (idx, ext)
    } else {
        (fig_path, "")
    }
}

/// Parse a `type=…&x=…&y=…&dy=…&button=…` query into an [`Event`]. Coordinates
/// are top-down canvas pixels (matching rizzma's event convention).
fn parse_event(query: &str) -> Option<Event> {
    let mut kind = "";
    let (mut x, mut y, mut dy) = (0.0f64, 0.0f64, 0.0f64);
    let mut button = MouseButton::Left;
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        match k {
            "type" => kind = v,
            "x" => x = v.parse().unwrap_or(0.0),
            "y" => y = v.parse().unwrap_or(0.0),
            "dy" => dy = v.parse().unwrap_or(0.0),
            "button" if v == "right" => button = MouseButton::Right,
            _ => {}
        }
    }
    match kind {
        "down" => Some(Event::MouseDown { x, y, button }),
        "move" => Some(Event::MouseMove { x, y }),
        "up" => Some(Event::MouseUp { x, y, button }),
        "wheel" => Some(Event::Wheel { x, y, dy }),
        "home" => Some(Event::DoubleClick { x, y }),
        "leave" => Some(Event::Leave),
        _ => None,
    }
}

/// A per-session token derived from wall-clock nanos and the pid — enough to
/// keep other loopback clients from poking the viewer, not a security boundary.
fn session_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    format!(
        "{:x}",
        nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(pid)
    )
}

/// Escape the four characters that matter inside HTML text/attributes here.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fire a fire-and-forget GET at the loopback server (used by `close`).
fn fetch_local(port: u16, path: &str) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let _ = stream.flush();
    Ok(())
}

/// Best-effort system browser launch. Returns whether a launcher was spawned.
fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        // Headless sessions have no display; skip and let the URL print.
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return false;
        }
        return std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok();
    }
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open").arg(url).spawn().is_ok();
    }
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok();
    }
    #[allow(unreachable_code)]
    {
        let _ = url;
        false
    }
}

/// The single-window viewer page. Placeholders are filled by [`Server::page`].
const PAGE_TEMPLATE: &str = include_str!("page.html");

#[cfg(test)]
mod tests;
