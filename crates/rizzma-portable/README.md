# rizzma-portable

Container framing and **renderer-free inspection** for rizzma portable figures
(`.riz`).

A portable figure carries the semantic model of a plot — axes, artists, data,
and style — rather than a picture of one, so a consumer can re-run layout and
rasterization from the data and offer pan, zoom, and resolution independence
away from the process that produced it. [`rizzma`](https://crates.io/crates/rizzma)
produces and renders them.

This crate deliberately links none of that machinery: it depends only on serde
and a JSON codec, so a host can read what an artifact *is* — its exact pixel
size, title and alt text, poster, provenance, and whether this build can render
its schema — without linking a rasterizer or a font stack.

That matters because the common case for a host is the one where it never
draws. A transcript holding hundreds of figures paints a poster and a card for
nearly all of them and renders one or two.

```rust
use rizzma_portable::{inspect, Limits};

let info = inspect(&bytes, &Limits::default())?;
if let Some(meta) = &info.meta {
    reserve(meta.width_px, meta.height_px);   // never reflow
}
if !info.renderable() {
    show_poster(info.poster(&bytes));         // degrade, don't error
}
```

## Trust

An artifact's `renderer.sha256` is an **identity and a lookup key, never an
authorization**. A hostile artifact can inline a hostile renderer and honestly
report its digest, so checking artifact bytes against the artifact's own claim
is circular. Keep a host-owned allowlist mapping schema to renderers you vetted,
serve your own copy, and ignore any `WASM` chunk for execution.

Artifact bytes are treated as attacker-influenced throughout: every entry point
takes `Limits` you supply and enforces them before allocating.

## License

MIT OR Apache-2.0
