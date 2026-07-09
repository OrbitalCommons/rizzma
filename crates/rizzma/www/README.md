# rizzma wasm browser demo site

An interactive demo site: four figures built from JS data via `WasmFigure`,
rendered into `<canvas>` elements by the wasm canvas backend (tiny-skia
rasterizes to straight RGBA, blitted as `ImageData`, HiDPI-crisp), each bound
to a `WasmSession` for wheel-zoom at the cursor, drag pan, double-click reset,
and hover readouts. The demos cover styled lines with a legend and live
`set_line_data` updates, scatter, log-log axes, and a subplot grid.

## Build

Run from the **repo root**.

With [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) (recommended):

```sh
wasm-pack build --target web --out-dir crates/rizzma/www/pkg crates/rizzma --features wasm
```

This emits `crates/rizzma/www/pkg/rizzma.js` and `rizzma_bg.wasm`, which
`index.html` imports.

Without `wasm-pack`, build with cargo + `wasm-bindgen` directly:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p rizzma --features wasm --target wasm32-unknown-unknown --release
wasm-bindgen \
  --target web \
  --out-dir crates/rizzma/www/pkg \
  target/wasm32-unknown-unknown/release/rizzma.wasm
```

## Serve

A static file server is enough (ES modules and `.wasm` require HTTP, not
`file://`). The workspace ships one — dependency-free, correct
`application/wasm` MIME type included:

```sh
cargo xtask serve-www            # http://localhost:8000/
cargo xtask serve-www --port 8777
```

Any other static server (e.g. `python3 -m http.server -d crates/rizzma/www`)
works too.

## Notes

The generated `www/pkg/` directory is a build artifact and is git-ignored.
