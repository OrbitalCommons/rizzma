# rizzma-wasm browser demo

A minimal page that builds a `WasmFigure.sample()`, renders it into a `<canvas>`
via the wasm canvas backend (`rizzma-wasm`) — tiny-skia rasterizes the figure to
straight RGBA, which is blitted onto the canvas as `ImageData` — and wires a
`mousemove` listener that calls `WasmFigure.data_at(px, py)` to show the data
coordinates under the cursor live (`x=…, y=…`, or `—` when off the axes).

## Build

Run from the **repo root**.

With [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) (recommended):

```sh
wasm-pack build --target web --out-dir www/pkg crates/rizzma-wasm
```

This emits `crates/rizzma-wasm/www/pkg/rizzma_wasm.js` and
`rizzma_wasm_bg.wasm`, which `index.html` imports.

Without `wasm-pack`, build with cargo + `wasm-bindgen` directly:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p rizzma-wasm --target wasm32-unknown-unknown --release
wasm-bindgen \
  --target web \
  --out-dir crates/rizzma-wasm/www/pkg \
  target/wasm32-unknown-unknown/release/rizzma_wasm.wasm
```

## Serve

A static file server is enough (ES modules and `.wasm` require HTTP, not
`file://`):

```sh
python3 -m http.server -d crates/rizzma-wasm/www 8000
```

Then open <http://localhost:8000/>.

## Notes

The generated `www/pkg/` directory is a build artifact and is git-ignored.
