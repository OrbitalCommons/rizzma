# rizzma wasm browser demo

A minimal page that builds a `WasmFigure.sample()`, renders it into a `<canvas>`
via the wasm canvas backend (`rizzma`'s `wasm` module) — tiny-skia rasterizes the
figure to straight RGBA, which is blitted onto the canvas as `ImageData` — and
wires a `mousemove` listener that calls `WasmFigure.data_at(px, py)` to show the
data coordinates under the cursor live (`x=…, y=…`, or `—` when off the axes).

## Build

Run from the **repo root**.

With [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) (recommended):

```sh
cargo install wasm-pack --version 0.15.0 --locked
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
`file://`):

```sh
python3 -m http.server -d crates/rizzma/www 8000
```

Then open <http://localhost:8000/>.

## Notes

The generated `www/pkg/` directory is a build artifact and is git-ignored.
