# 10 — Portable figure: a self-describing artifact that renders itself into a canvas

Status: **P0 implemented** (`crate::portable`, feature `portable`, default-on);
P1–P3 scoped below. Design for
[#287](https://github.com/OrbitalCommons/rizzma/issues/287).
Companion docs: `06-wasm-interactive-plan.md` (charter), `09-show-browser.md` (whose
Phase 3 "scene transport" this subsumes). Size numbers below are measured on `main`
(1.6.2); method in the issue's size-breakdown comment.

## 1. Goal

One file you hand a `<canvas>` element id, which renders itself with pan/zoom and
animation intact, on a host that has never heard of your native process. It fills
the missing quadrant:

|  | **dead** | **live** |
|---|---|---|
| **tethered** | — | `show()` (design/09) |
| **portable** | PNG / SVG / PDF | **this document** |

The concrete driver is agent-portal — an agent produces a figure, a human reads the
transcript later, and the figure should still pan, zoom, and animate — but the same
artifact serves a static site, a notebook, an email attachment, or an archive.

## 2. Constraints inherited from the charter

Three properties from design/06 §1 shape everything here; none is negotiable.

1. **One rendering codepath.** The browser is a blit target, not a renderer:
   tiny-skia rasterizes in Rust, the RGBA buffer reaches `<canvas>` via
   `putImageData`. Therefore the artifact's renderer *is* the existing wasm build —
   we ship the pipeline, we do not reimplement it.
2. **The OO layer is unaware of the target.** The artifact deserializes into an
   ordinary `Figure` and renders through the identical pipeline. Nothing forks.
3. **Semantic model, not display list.** Pan/zoom means re-running layout and
   rasterization *from the data*; HiDPI means re-rasterizing at the device pixel
   ratio. A display list has neither the off-viewport data nor the resolution
   independence. So the artifact carries `Figure → Axes → Artists` + data +
   rcparams — data, not geometry. (Full argument in #287; taken as settled here.)

The mount contract already exists and is untouched: `WasmFigure::bind(canvas_id)` →
`WasmSession` (`wasm/mod.rs:812`, `:886`) with pointer/wheel wiring, rAF-coalesced
redraw, and HiDPI handling. This design adds a way to *construct* the `WasmFigure`
from bytes instead of imperative JS calls.

## 3. The artifact: one container, two profiles

A single chunked binary container, extension **`.riz`**, MIME
`application/vnd.rizzma.figure`. Layout borrows deliberately from glTF's GLB, which
solved the same problem (JSON scene + binary buffers + optional embedded payloads):

```
header:  magic "RZFG" | u32 container_version | u32 total_len
chunks:  u32 len | 4-byte tag | payload | pad to 8-byte alignment
```

| chunk | required | contents |
|---|---|---|
| `JSON` | yes | the spec (§4): figure model, rcparams, accessors, timeline, versions |
| `BIN ` | if any accessor | one blob of typed arrays, 8-byte aligned |
| `FONT` | no | subsetted font face(s) for this artifact (§8) |
| `WASM` | no | the renderer, inlined |

The **two profiles are one format**, distinguished only by chunk presence:

| profile | chunks | over the wire | for |
|---|---|---:|---|
| **linked** (default) | JSON + BIN | KB per figure; renderer ~370 KB gz fetched once by hash | pages with N figures; hosts with a vetted-renderer policy |
| **self-contained** | JSON + BIN + FONT + WASM | ~370 KB gz per artifact | email, offline, single static file, zero resolution |

Both carry `renderer: {version, sha256}` in the JSON; the self-contained profile
additionally carries the bytes that hash to it. A host may always ignore inlined
bytes and substitute its own copy of a compatible renderer — that is the security
story (§7), not an afterthought.

`Figure::save_html(path)` additionally wraps a self-contained artifact plus the
loader (§6) in one HTML file — the strongest "just open it" deliverable, and the
completion of design/09's Phase 3.

## 4. The spec: a wire model, not serde on the live types

### 4.1 Why a mirror model

Three options were on the table:

- **Derive `Serialize` on the live types.** Rejected: `Axis` holds
  `Box<dyn Scale>`, `Box<dyn Locator>`, `Box<dyn Formatter>` (`axis/axis.rs:53`),
  `FuncFormatter` holds a boxed closure (`axis/ticker.rs:2548`), and the live
  structs carry layout caches and `pub(crate)` invariants. Worse, it welds the
  schema to the crate's internals — every refactor becomes a format change.
- **A command log** (`SceneOp` replayed against `WasmFigure`, the design/09 Phase 3
  sketch). Rejected: it serializes *how the figure was built*, not *what it is* —
  ordering-sensitive, hard to validate as a whole, and it gives the timeline (§5)
  no stable objects to target.
- **A closed wire model** — a parallel set of plain serde structs/enums
  (`FigureSpec`, `AxesSpec`, `LineSpec`, …) in a new `crate::portable` module,
  with `Figure → FigureSpec` export and `FigureSpec → Figure` import. **Chosen.**
  The crate already keeps exactly this shadow for the hard case:
  `ScaleSpec { Linear, Log, Symlog, Logit, Asinh }` (`figure/axes.rs:38`) is the
  closed, `Copy`, trivially-serializable twin of `Box<dyn Scale>`. The wire model
  generalizes that pattern; import reconstructs the trait objects through the
  existing constructors, so the render path stays single.

`RcParams` and `Rgba` already round-trip through serde (`core/rcparams.rs:46`,
`core/color/mod.rs:201` — hex `#rrggbbaa`); they are used as-is.

### 4.2 What the five artists need

The artist layer is friendlier than it looks: five concrete structs, all flat data.

| live type | payload | wire notes |
|---|---|---|
| `Line2D` (`artist/line.rs:18`) | `xdata`, `ydata: Vec<f64>` + color/width/dashes/cap/join | accessors for x/y |
| `Collection` (`collection.rs:33`) | `offsets: Vec<[f64;2]>`, marker `Path`, sizes, per-point colors | offsets accessor; marker as path spec |
| `Patch` (`patch.rs:40`) | `Path` + face/edge styling | vertices accessor + `PathCode` array |
| `AxesImage` (`image.rs:26`) | `data: Vec<f64>` + nrows/ncols/extent/vmin/vmax/cmap name | data accessor (f32 opt-in) |
| `QuadMesh` (`quadmesh.rs:23`) | coordinates + facecolors | accessors |

`Path` itself is `{vertices: Vec<[f64;2]>, codes: Option<Vec<PathCode>>}` with a
closed five-variant code enum (`core/path.rs:24`) — directly serializable.

Around the artists, `AxesSpec` carries what `Axes` carries (`figure/axes.rs:280`):
position, xlim/ylim (`null` = auto), margins, sticky edges, `ScaleSpec` per axis,
title, axis labels, grid settings, `prop_cycle`, annotations, span lines/rects,
legend entries, `xlim_link` (twin/shared axes by index), colorbars at the figure
level. Wire artists also get `label: Option<String>` from day one — #286 (artists
carry no label) will move legends onto artists, and the schema should not need a
bump when it lands; until then export fills labels from legend entries by index.

### 4.3 The trait-object closures: closed enums, one loud hole

`LocatorSpec` and `FormatterSpec` join `ScaleSpec` as closed enums covering the
built-in ticker vocabulary — currently 14 locators (`MaxN`, `Auto`, `AutoMinor`,
`Multiple`, `Linear`, `Fixed`, `Log`, `Symlog`, `Asinh`, `Logit`, `Index`, `Null`,
plus the two date locators) and the formatters (`Scalar`, `Log`/`LogMathtext`,
`Symlog`/`SymlogMathtext`, `Asinh`/`AsinhMathtext`, `Logit`/`LogitMathtext`,
`Eng`, `Percent`, `Null`, `Fixed`, `Index`, `FormatStr`, `StrMethod`, `Date`,
`ConciseDate`) — each variant holding that type's parameters, all of which are
plain data.

The one thing that cannot cross the wire is **`FuncFormatter`** (a boxed Rust
closure). Export **fails loudly** with an error naming the axes and axis, rather
than silently substituting `ScalarFormatter` and shipping a figure whose tick
labels lie. This is the general policy (§9): a portable figure that renders
differently than the original is worse than no figure.

### 4.4 Data encoding: accessors into one binary chunk

JSON floats run ~2–3× binary and make exact round-trips fiddly; at 10⁵–10⁶ points
that decides usability. So bulk arrays live in the `BIN ` chunk, referenced by
accessor index:

```jsonc
"accessors": [
  { "dtype": "f64", "count": 500, "offset": 0 },
  { "dtype": "f32", "count": 250000, "offset": 4000 }
]
```

- dtypes: `f64` (default), `f32` (exporter opt-in quantization for images/large
  clouds), `u32`/`u16`/`u8` (codes, indices).
- every offset/count is bounds-checked against the chunk before any figure is
  constructed; malformed input is a `Result::Err` with a message, never a panic
  (under `panic = "abort"` a panic kills the whole wasm instance).
- paired arrays are validated for consistency at import (`x.count == y.count`),
  reusing the checks `Axes::set_line_data` already performs (`figure/axes.rs:645`).
- small hand-authored arrays may inline as plain JSON lists; the exporter always
  writes accessors.
- room is reserved for a per-accessor level-of-detail hint (min/max/stride);
  deferred until a real oversized-artifact case exists.

### 4.5 A sketch

```jsonc
{
  "schema": 1,
  "generator": { "rizzma": "1.7.0" },
  "renderer": { "version": "1.7.0", "sha256": "9f2c…" },
  "figure": {
    "width_in": 6.4, "height_in": 4.8, "dpi": 100.0,
    "facecolor": "#ffffffff",
    "rc": { /* RcParams, existing serde form */ },
    "axes": [{
      "position": [0.125, 0.11, 0.775, 0.77],
      "xlim": [0.0, 6.2832], "ylim": null,
      "xscale": { "linear": {} }, "yscale": { "log": { "base": 10.0 } },
      "xaxis": { "label": "time [s]", "locator": { "auto": {} },
                 "formatter": { "scalar": {} }, "grid": true },
      "yaxis": { "label": null, "locator": { "log": { "base": 10.0 } },
                 "formatter": { "log_mathtext": {} }, "grid": false },
      "title": "decay",
      "lines": [{ "x": { "acc": 0 }, "y": { "acc": 1 }, "color": "#1f77b4ff",
                  "linewidth": 1.5, "dashes": null, "label": "signal",
                  "zorder": 2.0 }],
      "patches": [], "collections": [], "images": [], "meshes": [],
      "legend": "auto"
    }]
  },
  "accessors": [ /* §4.4 */ ],
  "glyphs": [[32, 126], [956, 956]],
  "timeline": null
}
```

`xlim`/`ylim` as authored are the **initial view**: interaction mutates only
runtime state, and reset/⌂ Home returns to the artifact's values. Auto limits
(`null`) resolve at import from the same data through the same margin logic, so
they are equally deterministic.

## 5. Animation: a declarative timeline (schema 2)

A host-callback animation cannot be serialized; the demos today are JS
`setInterval` loops recomputing arrays and calling the three mutation verbs
`set_line_data` / `set_scatter_offsets` / `set_image_data`
(`www/index.html`, `docs-header.html`). For the animation to survive the trip, the
artifact needs a **timeline evaluable at any time `t`** — scrubbable, pausable,
re-renderable at any resolution. (A frame sequence is a video; video exists and is
smaller.)

The model is keyframed tracks over exactly the mutation vocabulary that already
exists — the targets are the same verbs the interactive sessions call:

```jsonc
"timeline": {
  "duration": 8.0, "loop": true,
  "tracks": [{
    "target": { "axes": 0, "artist": "line", "index": 0, "prop": "y" },
    "times":  { "acc": 4 },
    "values": { "acc": 5, "frames": 160 },   // frames × element_count
    "interp": "linear"                        // or "step"
  }]
}
```

- **Targets:** line `x`/`y`, collection `offsets`, image `data`, and axes
  `xlim`/`ylim` (the scrolling-window verb — an oscilloscope-style sweep is a
  2-keyframe linear `xlim` track, not 160 data frames).
- **Samplers:** `times` strictly increasing; `step` holds, `linear` lerps
  element-wise. Evaluation at `t` is a pure function — same `t`, same arrays,
  every host, which is what makes scrubbing and archival replay honest.
- **Orthogonality:** the timeline mutates *data*, interaction mutates *view*;
  both go through the same `Interactor`/redraw machinery and compose without
  special cases.

**Deliberately deferred: procedural tracks.** Closed-form operators ("rotate
offsets about an axis at angle θ(t)", "phase-shift `y`") would compress the
spinning-galaxy case enormously, but they are the first step onto the slope of an
expression DSL — a language design, a security surface, and a compatibility
promise all at once. Keyframes ship first; operators get added as *closed enum
variants* only when a real artifact is too heavy as keyframes, and never as a
general expression language.

The timeline lands as **schema 2** (§9) — deliberately, so the versioning
discipline is exercised while the stakes are low: a schema-1 loader rejects a
schema-2 artifact with a clear "needs rizzma ≥ X" error instead of silently
dropping the animation.

Runtime behavior copies the pattern proven in `docs-header.html:83`: one shared
clock, `IntersectionObserver` gating so off-screen figures don't burn CPU, and the
existing rAF-coalesced redraw underneath. The mount handle (§6) exposes
`play()/pause()/seek(t)`.

## 6. The runtime: mount API and loader

**Rust, native (`portable` feature):**

```rust
impl Figure {
    // P0, shipped — the linked profile is the only profile, so no config yet.
    pub fn to_portable(&self) -> Result<Vec<u8>, PortableError>;
    pub fn save_portable(&self, path: impl AsRef<Path>) -> Result<(), PortableError>;
    pub fn from_portable(bytes: &[u8]) -> Result<Figure, PortableError>;

    // P2, when there is a second profile to choose and a reason to tune.
    pub fn to_portable_with(&self, cfg: &PortableConfig) -> Result<Vec<u8>, PortableError>;
    pub fn save_html(&self, path: impl AsRef<Path>) -> …;  // self-contained + loader
}
pub struct PortableConfig { profile: Profile /* Linked | SelfContained */,
                            quantize_f32: bool, /* … */ }
```

Deferring `PortableConfig` to P2 keeps the P0 surface honest: a config struct
whose every field is a no-op is worse documentation than no config at all, and
adding the argument later as `to_portable_with` leaves the common call untouched.

`from_portable` on native is not decoration: it makes the whole format testable
without a browser, the same native-testable-core discipline `wasm/mod.rs` already
follows, and it is what the golden identity test (§10) drives.

**Rust, wasm:** one new constructor beside the existing imperative surface —

```rust
#[wasm_bindgen]
impl WasmFigure {
    pub fn from_portable(bytes: &[u8]) -> Result<WasmFigure, JsValue>;
}
```

— after which everything is the existing contract: `bind(canvas_id)` →
`WasmSession`, pan/zoom/hover/`data_at` for free.

**JS loader** — a small (~2 KB) hand-written `rizzma-mount.js` published beside
the runtime:

```js
import { mount } from "./rizzma-mount.js";
const fig = await mount(canvasEl, "decay.riz", {
  // optional; default resolves by hash against the published runtime registry
  resolveRenderer: async ({ version, sha256 }) => bytesOrUrl,
});
fig.play(); fig.pause(); fig.seek(2.5); fig.dispose();
```

The loader parses the container header, resolves the renderer (WASM chunk if
present *and* trusted, else `resolveRenderer`, else the default registry URL),
verifies **sha256 via SubtleCrypto before instantiation** in all cases, calls
`init()` once per distinct hash (N figures on a page share one module), constructs
`WasmFigure::from_portable`, binds, and starts the clock. `dispose()` frees the
session (listener teardown already exists via `WasmSession`'s `Drop`,
`wasm/mod.rs:895`).

## 7. Renderer identity, publishing, and the trust model

The renderer is byte-identical for every figure a given rizzma version produces,
so it is published once per release, addressed by content:

- `publish.yml` gains a step emitting `runtime/<sha256>/rizzma_bg.wasm` + glue +
  `rizzma-mount.js` to gh-pages (immutable path ⇒ cache-forever), plus the same
  files as GitHub release assets, plus `runtime/index.json` mapping
  `version → sha256`. Today's equivalent is `demo/pkg/` deployed by
  `gallery.yml:48` with no hash — that stays for the demos; the runtime registry
  is the hashed, versioned sibling.

The hash earns its place beyond dedup: **a host executes only renderers it already
trusts.** agent-portal serves uploaded SVG under a sandboxing CSP deliberately,
because agent-supplied media is attacker-influenced. A hash-addressed renderer
lets such a host accept portable figures without weakening that posture — verify
inlined bytes against an allowlist, or ignore them and use its own vetted copy of
that version. Complementing that:

- the JSON spec is **data only** — no field is ever interpreted as code, no URL in
  an artifact is ever fetched by the loader (renderer resolution is the host's
  call);
- the importer validates sizes/offsets/counts up front with `Result` errors and
  caps total decoded allocation, so a malformed artifact degrades to an error
  message, not a hung tab or an aborted instance.

## 8. Fonts

Text renders from embedded outlines on every target (charter non-goal: host
fonts). Today that means the full 756,072-byte `DejaVuSans.ttf` via
`include_bytes!` (`text/font.rs:13`) — **more than half the compressed payload**
of a size-tuned wasm build. Two moves, phased:

1. **Runtime build subsets to a fixed 381-glyph scientific set** (ASCII + Latin-1
   + Greek + math/arrows/super-subscripts): 756 KB → 63 KB measured, no
   per-artifact machinery. The native build keeps the full face.
2. **Self-contained artifacts carry their own subset in the `FONT` chunk**,
   loaded through the existing `FontSource::register_face` (`font.rs:53`) —
   possible only because the exporter holds the full figure before writing.

One subtlety the issue's "subset to exactly the strings drawn" framing misses:
**interaction regenerates text.** Pan and zoom re-run locators and formatters, so
the glyphs a live figure needs are not the glyphs currently on screen. The subset
must therefore be the **alphabet closure**: static strings (title, labels, legend)
∪ each `FormatterSpec`'s declared reachable alphabet (digits, sign, decimal,
`e`/`×10ⁿ` machinery; month names for date formatters; `%` for percent; …). Each
closed formatter variant implements `fn alphabet(&self) -> &str`; that this is
*enumerable at all* is a direct payoff of §4.3's closed enums.

The artifact declares its coverage (`"glyphs"` as codepoint ranges) so a runtime
can **fail loudly instead of drawing tofu** when asked for a glyph outside it —
the same policy as §4.3, applied to text.

## 9. Versioning and compatibility, written down

Three independent versions, three independent jobs:

| version | where | bumps when |
|---|---|---|
| container | binary header | byte-layout changes (expected: never) |
| **schema** (integer) | JSON `schema` | anything that changes how an artifact renders; 1 = static+interactive, 2 = +timeline |
| renderer | JSON `renderer.version` + sha256 | every crate release |

**The compatibility rule:** each rizzma build supports a contiguous schema range
`SCHEMA_MIN..=SCHEMA_CURRENT`. Newer artifact → hard error naming both versions
("artifact is schema 3; this runtime supports 1–2 — rendering it would drop
content"). The renderer hash is provenance and a dedup/trust key, *not* a
requirement — any renderer whose range covers the artifact's schema may render it.

**Unknown elements: fail loudly, don't skip.** Best-effort forward compatibility
is right for most formats and wrong for figures: silently dropping an artist a
newer schema introduced renders a *scientifically wrong plot* with no indication
anything is missing. Every wire struct is `deny_unknown_fields`; every enum is
closed; unknown variant → error. A lenient mode stays out until someone brings a
use case that survives that argument.

## 10. Determinism, as a stated guarantee

The single-codepath property already makes browser output pixel-identical to PNG
export; artifact + renderer hash makes it *checkable*. Promise it explicitly:

> Rendering (artifact, renderer bytes, t, view, scale) is a pure function of its
> arguments — identical pixels on every host, forever. Across *different*
> renderer versions, output is semantically equivalent but not pixel-guaranteed.

Enforced by tests, not intentions:

- **Golden identity (native):** for every gallery figure, `export → import →
  encode_png()` must be byte-identical to direct `encode_png()`. This one test
  transitively pins the entire wire model.
- **Round-trip:** `FigureSpec` → bytes → `FigureSpec` equality, including
  accessor bit-exactness.
- **Rejection:** truncated chunks, out-of-bounds accessors, unknown
  fields/variants, schema-from-the-future, mismatched array lengths,
  `FuncFormatter` export — each yields its specific error.
- **Browser (`tests/wasm_browser.rs`):** mount from bytes, ink appears; wheel
  zoom changes limits; timeline `seek` moves data; dispose detaches listeners —
  same harness the interactive surface already uses.

## 11. Size: the measured plan

From the issue's measurements (raw rustc artifact on `main`, 2,646,798 bytes,
96% of the CI cap): 20.8% is build metadata wasm-bindgen strips anyway, 29% is
the unsubsetted font, and there is no `[profile.release]` tuning at all today.

| step | effect (measured) |
|---|---|
| strip custom sections (any wasm-bindgen pipeline) | −549,956 |
| size-tuned profile (`lto="fat"`, `codegen-units=1`, `opt-level="z"`, `panic="abort"`) | code −28.7% |
| 381-glyph font subset | font −91.6% |
| **floor** | **1,101,308 raw ≈ 370 KB gzip**, pre-`wasm-opt` |

Actions, kept out of the artifact-format work so each lands independently:

- add a named **`[profile.wasm-release]`** (inherits release) so native render
  performance is untouched; wasm CI/build invocations move to `--profile
  wasm-release`. `panic = "abort"` is a semantic change, adopted knowingly —
  §4.4's validate-don't-panic rule is what makes it safe.
- `wasm-opt` stays on via wasm-pack's default pipeline (it is not currently
  invoked in the raw-cargo CI path at all — the budget gates pre-strip bytes).
- CI budget: keep `WASM_SIZE_MAX_BYTES` as the raw regression guard but have
  `cargo xtask wasm-size` also report the post-wasm-pack `pkg/rizzma_bg.wasm`
  number, so the gated figure and the shipped figure stop being conflated.

## 12. Phasing

| phase | delivers | schema |
|---|---|---|
| **P0** ✅ | `crate::portable` wire model + container + native `to/from_portable` + golden identity & rejection tests | 1 |
| **P1** | `WasmFigure::from_portable`, `rizzma-mount.js`, hashed runtime publishing; linked profile end-to-end (agent-portal, docs) | 1 |
| **P2** | self-contained profile (`WASM` chunk), `save_html`, size program (§11) | 1 |
| **P3** | timeline + shared clock/scrub UI; font subsetting with alphabet closure + `FONT` chunk | 2 |

P0 is pure native Rust — no browser, no JS — and already proves the hard 80%:
the wire model, strictness, and pixel-identity.

### What P0 shipped, and where it deviated

- `Figure::to_portable` / `save_portable` / `from_portable` behind the default-on
  `portable` feature, with `PortableError` re-exported at the crate root.
- The wire model lives in `portable::spec`, kept **private**. The public
  `Scale`/`Locator`/`Formatter` traits gained a `portable_spec()` hook returning
  an opaque `PortableScale`/`PortableLocator`/`PortableFormatter` newtype that
  only rizzma's built-ins can construct. A third-party implementation returns
  the default `None` and export fails loudly — the same rule as `FuncFormatter`,
  now enforced by the type system rather than by convention. This keeps the
  schema free to change through P1–P3 instead of freezing it as public API.
- **Golden identity runs over a purpose-built fixture set** (one figure per
  artist, scale, and decoration) rather than the gallery: `examples/gallery.rs`
  is a `main()` that writes PNGs, not a reusable figure library. If the gallery
  is ever refactored into named builders, point the test at it instead.
- The `f32` quantization option (§14.2) is not implemented; every accessor is
  `f64` (plus `u8` for path codes). Nothing in the format prevents adding it.

### One finding worth carrying forward

`serde_json` does not round-trip `f64` by default: `0.10999999999999999`
serializes and parses back one ULP off, so a re-exported artifact silently
differed from its original. Its `float_roundtrip` feature is therefore
**load-bearing** for the §10 determinism guarantee, and
`portable::tests::json_scalars_round_trip_bit_exactly` pins it so a future
dependency cleanup cannot quietly drop it. The lesson generalizes: JSON is safe
for *structure*, and every number that must be exact — not only the bulk arrays
§4.4 already moved — depends on the codec being correctly rounded.

## 13. Explicit non-goals

- **A Canvas2D/WebGL renderer.** Settled by the charter; the raster backend *is*
  the renderer on every target. (The feature-gate-tiny-skia idea was raised and
  withdrawn in #287.)
- **An expression DSL for animation** (§5). Keyframes plus, later, closed
  operator variants.
- **Streaming/live data.** The oscilloscope is tethered by definition; a
  *recording* of it is a timeline.
- **3D axes.** `WasmAxes3D` lives outside `Figure` today; it joins the schema
  when it joins the figure model, as a schema bump.
- **Editing artifacts in the browser.** The runtime renders and interacts; it is
  not an authoring surface.
- **Best-effort/lenient loading** (§9), until argued for.

## 14. Open questions

1. **#286 sequencing.** Artist labels reshape legend data. The wire model carries
   per-artist `label` from day one (§4.2), but landing #286 before schema 1
   freezes would remove the export-time legend→label mapping shim entirely.
2. **f32 quantization default** for `AxesImage` data — likely yes (display-bound
   anyway), but it breaks `data_at` bit-exactness for hover readouts.
3. **Rust font subsetter** for P3 — the measurement used Python `fontTools`;
   shipping needs a Rust path (e.g. the `subsetter` crate) or a build-time step.
4. **`show()` integration** — a toolbar "⬇ .riz" button beside PNG/SVG/PDF falls
   out nearly free once P0 lands, and is the natural dogfood.
5. **Registry fallback** — whether `resolveRenderer`'s default should try release
   assets when gh-pages is unreachable, or stay single-source.
