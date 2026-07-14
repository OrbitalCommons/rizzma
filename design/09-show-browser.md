# 09 — `show()`: interactive local-browser viewer

Status: **Phase 0–1 implemented** (rizzma 1.5.0); Phases 2–3 scoped below.

## Goal

A matplotlib-style `.show()` that opens the local browser onto a single window
holding every figure — each pan/zoom/reset-interactive with the same tools, and
exportable — ideally as interactive as bokeh.

## What shipped (`crate::show`, feature `show`, native-only)

The **server round-trip** model (matplotlib's WebAgg). The native process keeps
each figure inside an [`Interactor`], and the browser page is a thin canvas that
sends pointer events over HTTP and paints the PNG frame the server renders back.
This reuses the existing interaction engine (`figure/interact.rs`) and every
render/export backend, so **nothing about the figures is serialized**.

- **API:** `Figure::show(self)` (blocking), `show::show_all(figs, ShowConfig)`,
  `show::show_nonblocking(...) -> ShowHandle`, and `pyplot::show()` (consumes the
  current figure). `ShowConfig { title, open_browser }`.
- **Server:** a dependency-free HTTP/1.1 loop over `std::net::TcpListener`, bound
  to `127.0.0.1` on an ephemeral port, guarded by a per-session token in the URL.
  Routes: `/{token}/` (page), `fig/{i}.png` (render), `fig/{i}/ev?type=…` (event
  → re-render), `fig/{i}.svg` / `.pdf` (export), `shutdown`.
- **Page:** one responsive window, a card per figure, a canvas each. Drag to pan,
  wheel to zoom at the cursor, double-click or **⌂ Home** to reset, **PNG/SVG/PDF**
  toolbar export of the active figure, **✕** to close. Pointer moves are
  rAF-coalesced; `beforeunload` pings `shutdown` so `show()` returns.
- **Blocking semantics:** `show()` blocks until the window closes;
  `show_nonblocking` runs on a background thread behind a `ShowHandle`.
- **Enabling factor:** the axis trait objects (`Scale`/`Locator`/`Formatter`) are
  now `Send + Sync`, so a `Figure` can move to the server thread.

Tests drive the real loopback server with a raw `TcpStream` client (no browser):
index, render, event→re-render, export, token/404 guards, multi-figure window.

## Why server round-trip first (not client-side wasm)

rizzma already interacts client-side via `WasmSession`, so a wasm viewer is the
bokeh-standalone endgame. But it needs a *scene transport* (native `Figure` →
browser), which is the one genuinely new subsystem. The server model needs none
of that — it reuses `Interactor` + backends directly — so it delivers a complete,
tested `.show()` first. The wasm path is Phase 3.

## Roadmap

**Phase 2 — matplotlib/bokeh-grade tools (server model, additive):**
- Box-zoom (rubber-band select → set limits), nav stack (back/forward/home).
- Live **data-coordinate readout** (`Interactor` already emits `Outcome::Hover`;
  return it in a response header and show it in the toolbar).
- Linked pan/zoom across subplots (native `sharex`/`sharey` already exist).
- HiDPI frames via `Figure::render_scaled(devicePixelRatio)`.
- Save-all; legend-toggle; crosshair cursor.

**Phase 3 — standalone client-side wasm (bokeh-offline):**
- A serializable **scene transport** (recommended: a command-log `enum SceneOp`
  replayed against `WasmFigure`, sidestepping trait-object serialization).
- `Figure::to_html()` / `save_html()` → a single self-contained file that renders,
  interacts, *and* exports entirely in-browser (all backends compile to wasm).
- Unify: retire the bespoke docs-demo harness onto this path; `_repr_html_`-style
  inline output for notebooks.

## Cross-cutting

- **Feature-gate** `show` (native only; `#[cfg(not(target_arch = "wasm32"))]`) so
  the core and wasm builds stay lean.
- **Security:** loopback bind + ephemeral port + per-session URL token. Not a
  security boundary — a guard against other localhost clients. `agent-portal
  forward` is the opt-in path for remote viewing.
- **Headless:** Linux with no `DISPLAY`/`WAYLAND_DISPLAY` prints the URL instead
  of launching a browser.
