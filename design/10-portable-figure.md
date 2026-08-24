# 10 — Portable figure: a self-describing artifact that renders itself into a canvas

Status: **P0 shipped** in 1.7.0; **P1's four host prerequisites shipped** in
1.8.0 — the `rizzma-portable` inspection crate, the `PSTR` poster, schema 2's
`meta` block, and a published hash-addressed runtime. What remains of P1 is the
browser half: `WasmFigure::from_portable` and `rizzma-mount.js`. Revised against
an agent-portal host review on 2026-08-22 (§7, §4.6, §4.7, §9, §12b). Design for
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
| `PSTR` | recommended | a PNG poster of the figure as authored (§4.7) |
| `FONT` | no | subsetted font face(s) for this artifact (§8) |
| `WASM` | no | the renderer, inlined |

**Chunk order is guidance, not grammar.** Writers put `JSON` and `PSTR` *before*
the large `BIN` (and `WASM`) chunks, so a consumer can read the metadata and paint
a poster from a range request or a partial read instead of pulling megabytes.
Readers must tolerate any order — the framing is self-describing, and a parser
that depends on layout is a parser that breaks on the first writer that differs.
(The tag is `PSTR`, not `POST`: four bytes either way, and one of them reads like
an HTTP verb.)

The **two profiles are one format**, distinguished only by chunk presence:

| profile | chunks | over the wire | for |
|---|---|---:|---|
| **linked** (default) | JSON + BIN + PSTR | KB per figure; renderer ~370 KB gz fetched once by digest | pages with N figures; hosts with a vetted-renderer policy |
| **self-contained** | + FONT + WASM | ~370 KB gz per artifact | email, offline, single static file, zero resolution |

Both carry `renderer: {version, sha256}` in the JSON; the self-contained profile
additionally carries bytes that hash to it. A host should **always** prefer its
own vetted copy of a compatible renderer and ignore the inlined bytes — the
digest identifies, it does not authorize (§7).

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

### 4.6 Metadata a host can read without running anything

A host has decisions to make *before* it fetches 370 KB of renderer: how much
space to reserve, which runtime to pick, whether it can render this at all, and
what to show if it cannot. Making it instantiate wasm to answer those is
backwards, so the artifact keeps them all in the JSON chunk's first level,
readable by a dependency-free parse of the container framing:

```jsonc
{
  "schema": 1,
  "renderer": { "version": "1.7.0", "sha256": "9f2c…" },
  "meta": {
    "width_px": 640, "height_px": 480,   // exact, from width_in × dpi
    "alt": "Damped oscillation decaying over 20 seconds",
    "title": "damped oscillation",
    "poster": { "chunk": "PSTR", "mime": "image/png", "bytes": 18422 },
    "animated": false
  }
}
```

Exact pixel dimensions, not an aspect-ratio hint: the figure's size in inches and
its DPI are both authored, so a host can reserve the right box and never reflow.

**rizzma owns the inspector**, as a renderer-free Rust API — not a JS one. The
first draft assumed hosts would parse the container in TypeScript; agent-portal's
frontend is Rust/Yew/wasm, and more generally a host should not reimplement the
container parser just to allocate a card. So:

```rust
// Crate `rizzma-portable`: no tiny-skia, no font stack, no rendering.
pub fn inspect(bytes: &[u8], limits: &Limits) -> Result<PortableMetadata, PortableError>;
```

- compiles for `wasm32-unknown-unknown` and pulls in none of the render stack, so
  a host can link it without linking a rasterizer;
- returns schema, producer and renderer provenance, size/dpi/aspect, initial
  viewport, title and alt text, poster location/type/length, and a chunk
  directory (count, tags, declared lengths, declared total);
- validates against **host-supplied** `Limits` rather than rizzma's opinion of
  them, and **never allocates the `BIN` payloads** just to inspect.

The intended shape is: the host's backend validates once on receipt, extracts the
typed metadata and poster, and persists them; the frontend renders a card from
that stored metadata; the sandboxed runtime re-validates in full before drawing.
The container framing (§3) stays documented — a 12-byte header, then `u32 len` +
4-byte tag + payload padded to 8 — so independent implementations remain possible,
and a small JS inspector may ship alongside `rizzma-mount.js` for hosts that want
one. Neither is a privileged path; the Rust `inspect` is the source of truth.

**Budgets belong in the spec, not in each host's implementation.** A trusted
renderer still parses attacker-influenced data, and memory safety removes a
corruption class, not parser denial-of-service. So the format states limits a
conforming loader enforces before allocating: total artifact bytes, chunk count
and declared lengths (checked against the real buffer, no `tiny JSON, giant BIN`
surprise), JSON length and nesting depth, accessor counts and total decoded
points, canvas dimensions × device pixel ratio, and a time budget with
cancellation. A host cap of **10 MiB** per artifact is the suggested default —
generous against a typical 35 KB figure, and low enough to be a real bound.

Embedded fonts count as attacker-influenced parser input too, even though they
are never URLs: `FONT` chunks get their own count and byte budgets, and font
parsing belongs inside the same sandbox as rendering. Note that today this is
moot in the best way — P0 artifacts carry no font at all, because text renders
from the outlines already inside the vetted runtime. Per-artifact subsetting
(§8) is what would introduce the exposure, which is a reason to keep it to the
self-contained profile rather than making it the default.

### 4.7 The poster: what shows when the renderer will not run

Every path that cannot execute wasm still has something to show, so the
recommended profile carries a **PNG poster of the figure as authored**, plus
`alt` and `title` text. It is nearly free to *add* — encoding and storing a PNG
costs something, but rizzma already rasterizes, so the poster is a byproduct of a
figure the exporter is already holding — and it is what makes the format degrade
gracefully rather than vanish. What it covers:

- scripting disabled, or a host that declines to run wasm at all;
- a reduced-resource or mobile client;
- an artifact whose schema no runtime the host has vetted supports (§9);
- archive and search previews, and share/social surfaces;
- the card to show *while* the runtime is still downloading.

**What it does not cover: blob expiry.** An earlier draft of this section claimed
an expired artifact could degrade to "poster plus download", which is wrong and
quietly rebuilt the very lifetime overclaim §12b exists to correct: if the
canonical `.riz` expired, the `PSTR` inside it expired with it, and so did the
bytes any download would serve. Stated correctly:

- `PSTR` handles **runtime, schema, and scripting failure** — cases where the
  artifact is present but cannot be executed;
- a host **may** retain an *extracted* poster derivative longer than the
  interactive blob and then degrade to **poster-only** — not poster + download,
  because the original is gone;
- only **durable storage of the original** preserves poster *and* download *and*
  interactivity. Recovering it from archive is not the expired case; it is the
  not-actually-expired case.

The poster is optional — a producer may omit it, and a host must not reject an
artifact for lacking one (show a "load interactive figure" placeholder instead) —
but the exporter writes one by default, because a figure that cannot render and
has nothing to show is the one failure mode with no recovery.

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
import { mount, readMetadata } from "./rizzma-mount.js";

// The host says which renderer to run. This argument is required: there is no
// default that reaches for a URL, and none that falls back to artifact bytes.
const fig = await mount(canvasEl, bytes, {
  renderer: { bytes: vettedWasm, sha256: expectedDigest },
});
fig.play(); fig.pause(); fig.seek(2.5); fig.dispose();
```

The loader parses the container header, hashes the **host-supplied** renderer
bytes with SubtleCrypto and refuses to instantiate on a mismatch, calls `init()`
once per distinct digest **within its realm**, constructs
`WasmFigure::from_portable`, binds, and starts the clock. `dispose()` frees the
session (listener teardown already exists via `WasmSession`'s `Drop`,
`wasm/mod.rs:895`).

"Within its realm" is load-bearing, and an earlier draft got it wrong by claiming
N figures on a page share one module. Under the recommended isolation (§12b) each
figure lives in its own opaque-origin iframe, and **a module cache cannot cross
realms**: the HTTP bytes are cached, but every frame compiles and instantiates
separately. Keeping only one or two frames live (§12c) is what makes that
acceptable. If profiling ever shows compilation dominating, the answers are a
small pool of reusable sandbox frames or a dedicated renderer origin — never
relaxing isolation to share an in-page module cache.

`readMetadata(bytes)` returns the §4.6 metadata without instantiating anything,
so a host can size the layout, pick a renderer, and decide supported-vs-poster
before it fetches 370 KB.

## 7. Renderer identity, publishing, and the trust model

The renderer is byte-identical for every figure a given rizzma version produces,
so it is published once per release, addressed by content:

- `publish.yml` gains a step emitting `runtime/<sha256>/rizzma_bg.wasm` + glue +
  `rizzma-mount.js` to gh-pages (immutable path ⇒ cache-forever), plus the same
  files as GitHub release assets, plus `runtime/index.json` mapping
  `version → sha256`. Today's equivalent is `demo/pkg/` deployed by
  `gallery.yml:48` with no hash — that stays for the demos; the runtime registry
  is the hashed, versioned sibling.

### The hash is an identity, not an authority

This is worth stating flatly, because an earlier draft of §6 got it wrong and the
wrong version is the tempting one: **a digest an artifact carries about itself
proves nothing.** A malicious artifact can inline malicious wasm and honestly
report its hash. Verifying artifact bytes against the artifact's own claim is
circular and buys exactly nothing. (Correction owed to the agent-portal review,
2026-08-22.)

So the rule, in the order a host should apply it:

1. **Keep a host-owned allowlist** mapping schema (and optionally rizzma version)
   → the digest of a renderer *you* vetted, and serve your own immutable copy.
2. **Ignore the artifact's `WASM` chunk for execution.** Its `renderer.sha256` is
   a *lookup key* and a *mismatch alarm* — never an authorization.
3. **Treat the self-contained profile as a download/offline convenience**, not an
   execution source. For a multi-tenant host, "run our pinned copy, ignore the
   inlined bytes" is simpler than byte-comparing them and strictly no weaker.

That is what lets a host with a real security posture accept portable figures
without loosening it: agent-portal serves uploaded SVG under a sandboxing CSP
deliberately, because agent-supplied media is attacker-influenced. A figure whose
renderer the host chose is a different proposition from a document that brings
its own code. Complementing that:

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
content").

**Renderer selection is the host's, in this order** (tightened at the
agent-portal review's suggestion — the first clause is the part I had missing):

1. the **exact digest the artifact declares**, if that digest is in the host's
   vetted registry *and* its range covers the artifact's schema;
2. otherwise the host's **canonical vetted renderer for that schema**;
3. otherwise **poster + download** (§4.7).

Rule 1 exists for fidelity, not trust: rendering an archived figure through the
renderer it was authored against reproduces the pixels it was authored with.
Falling forward to a newer runtime (rule 2) is genuinely valuable for old
artifacts, but a renderer fix can move pixels, so **a host that fell forward
should say so in its selection metadata** rather than let it be an invisible
substitution. Retaining old vetted runtimes is cheap — ~370 KB compressed each —
so rule 1 should hit more often than not.

Note what is *not* in that list: bytes the artifact brought with it. The digest
is a lookup key into the host's registry (§7), never an authorization.

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
| **P1** | the four host prerequisites below, then `WasmFigure::from_portable` + `rizzma-mount.js`; linked profile end-to-end (agent-portal, docs) | 1 |
| **P2** | self-contained profile (`WASM` chunk), `save_html`, size program (§11) | 1 |
| **P3** | timeline + shared clock/scrub UI; font subsetting with alphabet closure + `FONT` chunk | 2 |

P0 is pure native Rust — no browser, no JS — and already proves the hard 80%:
the wire model, strictness, and pixel-identity.

**P1's four prerequisites**, as agreed with the agent-portal review — these are
what a host needs *before* the mount API is useful to it, and they are ordered so
each is independently checkable:

1. a **published hash-addressed runtime** and a stable mount API (§6, §7);
2. **`inspect()`**: renderer-free, wasm-safe metadata and validation (§4.6);
3. the **poster** plus intrinsic sizing and accessibility metadata (§4.7);
4. an explicit **runtime compatibility and selection policy** (§9).

Ship the **linked profile only** first; the self-contained file is offered as a
download, never as an execution source. And before calling P1 "integrated",
benchmark **pan latency at `devicePixelRatio` 2 *and* 3**, on a midrange phone,
against the retained-card counts in §12c — `putImageData` is the plausible
bottleneck, not the 370 KB download.

### What P1's prerequisites shipped (1.8.0), and where they deviated

1. **Runtime publishing** goes to **immutable GitHub release assets**, not
   gh-pages. The gallery deploy replaces that branch wholesale
   (`force_orphan: true`), so a runtime published there would be deleted by the
   next push to `main` — and rule 1 of §9 depends on old runtimes *staying*
   fetchable. Each release carries `rizzma_bg.wasm`, `rizzma.js`, and a
   `runtime.json` declaring the schema range it renders, generated by
   `cargo xtask runtime-manifest` so it reads the range from the code.
2. **`inspect()` is a separate crate**, `rizzma-portable`, not a feature of
   `rizzma`. A feature could not have delivered the promise: `tiny-skia` is a
   non-optional dependency of `rizzma`, so any in-crate inspector would still
   link a rasterizer. The crate depends on serde and a JSON codec, nothing else.
   It also owns the container framing and the shared metadata types, so the
   exporter and the inspector cannot drift.
3. **The poster** is a `PSTR` chunk holding exactly `Figure::encode_png()` — a
   test asserts byte equality, so the fallback is what the renderer would have
   drawn rather than an approximation. `PortableConfig` arrived earlier than
   §6 planned, because "should this artifact carry a poster" is a real knob
   rather than the placeholder that argued for deferring it.
4. **Compatibility** ships as the schema range in `runtime.json` plus
   `Metadata::schema_supported`. One honest gap: `renderer.sha256` is written
   as `null`, because a runtime cannot contain its own digest — embedding it
   would change the bytes being digested. **Version is therefore the lookup
   key** for §9's rule 1, and the digest, once a host has resolved one, is a
   consistency check rather than the primary index.

An artifact grew: a 2,000-point line went from 35,232 bytes to 59,848, of
which 24,554 is poster. That is the trade the review asked for — the common case is a host
that shows the poster and never renders.

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

## 12b. Where rizzma stops and a host begins

The agent-portal review (2026-08-22) settled this line, and writing it down keeps
both sides from building half of the other's job:

| rizzma owns | the host owns |
|---|---|
| the artifact bytes and their framing | the trust registry of vetted renderers |
| the poster and metadata (§4.6, §4.7) | renderer selection (§9) and isolation |
| `inspect()` validation against host limits | authorization and durable storage |
| schema/runtime compatibility declarations | extracted-poster cache, lifecycle, scheduling |
| publishing the pinned runtime artifacts | resource budgets and their enforcement |

Two consequences worth stating outright:

**No identity in the artifact.** It was tempting to mint a content-addressed
artifact id inside `.riz` so hosts get dedup and archive keying for free. That is
the wrong layer: a globally exposed content address is a cross-tenant correlation
and existence oracle, and it drags refcount-and-deletion complexity into a file
format. A host mints its own opaque id, computes its own digest over the bytes it
received, and keys authorization on `(id → owner → bytes)`. A self-digest in the
artifact would be circular anyway, for exactly the reason §7 gives about the
renderer hash. rizzma may offer `portable_sha256(bytes)` as a convenience; a
backend must still compute its own.

**Isolation is the host's call, and P0 does not deliver "interactive later" by
itself.** The reviewed posture for a multi-tenant host is a sandboxed iframe with
`allow-scripts` and deliberately *without* `allow-same-origin`, fed the artifact
bytes by `postMessage` from a parent that did the authenticated fetch, under a
tight CSP, with raster work on a worker/`OffscreenCanvas` where available (a jank
boundary, not a security one) and `wasm-unsafe-eval` scoped to that document if
the loader needs it. Just as important: agent-portal's live media is TTL-bounded
today (an hour by default) with durability only through archive write-through, so
"an agent makes a figure and a human reads it later, still interactive" is a
promise **durable artifact storage** has to keep — not something the format
delivers on its own, and not something the poster rescues: an embedded `PSTR`
expires with the blob that contains it (§4.7). A host that extracts the poster to
a separately retained derivative can degrade to poster-only; everything else about
"later" depends on retaining the original. That should be stated honestly
wherever this format is pitched.

> **When this caveat may be removed:** only once a host's durable-artifact path
> is merged *and* the deployed default — not when it is designed, and not when it
> is available behind a flag. Until then this paragraph describes current
> behavior, and deleting it early would make the doc describe an intention rather
> than a system.

## 12c. The workload this has to survive

Real numbers from the agent-portal review, because sizing the runtime against a
developer laptop is how this ships broken:

| dimension | reality |
|---|---|
| messages retained per session view | 100 (`MAX_MESSAGES_PER_SESSION`) — adversarially, all 100 are figures |
| sessions kept mounted | **every activated session stays mounted and is merely CSS-hidden**; heavy users run dozens, so ~2,000 retained figure cards is reachable |
| genuinely visible at once | 1 on a phone, 2–4 on desktop, 6–8 with pathologically small cards |
| low-end target | midrange 4 GB Android and mobile Safari, **DPR 3 is common** |
| frame cost | 640×480 at DPR 2 ≈ **4.9 MB per RGBA frame**; at DPR 3 ≈ **11 MB**. At 60 Hz that is hundreds of MB/s through `putImageData` alone |

The consequence is that **poster-first is the architecture, not a fallback**. Of
2,000 retained cards, ~1,998 must be metadata plus an `<img>` — no iframe, no
worker, no module. Concretely:

1. do not create the sandbox frame until near-viewport or explicit activation;
2. **dispose on unfocus** — an ancestor going `display:none` must tear the figure
   down. Do not rely on unmount or an `IntersectionObserver` alone: the session
   view stays mounted, so nothing else will tell you;
3. pause animation whenever not intersecting or the document is hidden;
4. cap **active renderers at 2 and active animations at 1** for v1, LRU as
   figures are activated;
5. cap backing store around **2 megapixels** and effective DPR at **2 during
   interaction**, with an optional full-quality redraw once pan/zoom settles;
6. prefer `OffscreenCanvas` in the worker so RGBA does not take a second
   main-thread hop; feature-detect, and drop resolution or frame rate on the
   fallback path.

Points 4 and 5 are the ones that touch rizzma rather than the host: a
"reduced-resolution while interacting, full quality when settled" mode is a
renderer capability, and it is the concrete deliverable behind §12's pan-latency
benchmark.

## 12d. The sandbox contract

Agreed with the agent-portal review before either side implements it, because an
origin or lifecycle assumption is far cheaper to fix now than to unwind later.
The message shapes and transition table are still to be written; what follows is
the part that is settled, and that the code should not quietly drift from.

**Validation happens outside the realm.** The parent reads the artifact with
`inspect` (§4.6), enforces its budgets, resolves a vetted runtime whose declared
schema range covers the artifact, and decides interactive-versus-poster —
*before* creating an opaque-origin child. An artifact that is malformed,
over-budget, or of an unsupported schema therefore never causes a realm to exist
at all. This is where the code lives, not merely what it does: it is easy for a
later reader to "helpfully" move validation inward, and that would mean hostile
bytes get a realm before anyone has looked at them.

**The child still fails safely on bad bytes anyway.** The parent's validation is
where the *decision* is made, not where the *safety* comes from. The importer
already behaves this way — bounds checked before allocation, `Result` rather
than panic — so the child's contract is a promise about what it will not do with
bad input, never an assumption that bad input cannot arrive.

**Sequence:** `Ready → Mount → Mounted | Error → Pause / Resume → Dispose →
Disposed`. The parent owns focus, visibility, and the active-renderer budget
(§12c), so `Pause` stops work without freeing state, and it is the parent that
decides when a figure stops being live.

**`Dispose` is terminal.** The realm is destroyed; there is no `Mount` that
revives it. An earlier draft of this contract made a post-`Dispose` mount a
legal no-op-except-mount, which quietly makes reuse the default and demotes
disposal to a strong pause. It also creates a cross-artifact state-leak path:
anything realm-scoped that outlives a single figure — the loader's
digest-keyed module cache is the obvious one — becomes shared between artifacts
the moment a realm is reused. If pooling is ever worth its complexity, it
arrives as an explicit `Reset → Ready` carrying an argument that every prior
resource is gone, never as a relaxation of `Dispose`.

`dispose()` must actually free: cancel `requestAnimationFrame` and timers, drop
listeners, free the wasm-owned figure and session, and zero the canvas width and
height so the backing store is released. A host cannot enforce an
active-renderer budget of two if disposal only stops drawing.

**Nothing an artifact names crosses the boundary.** The parent sends validated
bytes (or an unguessable same-origin capability), never an artifact-supplied
URL. The child receives no cookies, tokens, or backend URLs, initiates no fetch
or navigation, and both sides check a channel nonce alongside origin and source,
because opaque origins report `null` and cannot be distinguished by origin alone.

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
5. **Registry publication** — where the vetted runtime index lives and how a host
   bootstraps trust in it the first time. Since selection is host-owned (§9),
   this is a distribution question, not a security one, but it still needs an
   answer before P1 ships.
6. **Poster cost when it is never shown.** Every artifact pays PNG bytes even
   when a host renders live immediately. Cheap in absolute terms and worth it for
   graceful degradation, but if artifacts get large or numerous, an
   exporter-level "poster: off" is the escape hatch.

### Settled by the agent-portal review (2026-08-22)

- Poster lives **inside** the artifact as the canonical copy; a host may extract
  a sibling cache for fast `<img>` paint and should retain the original.
- A poster-less artifact is **not** rejected — it degrades to a "load interactive
  figure" placeholder.
- The inspector is **Rust**, not TypeScript (§4.6); a JS one may ship for other
  hosts but is not the source of truth.
- Renderer selection is **exact vetted digest first**, then canonical-for-schema,
  then poster (§9).
- Durable artifact identity and storage stay **host-side**; the format carries no
  artifact id and no self-digest (§12b).
- The poster covers **execution** failure, not **expiry** — an embedded `PSTR`
  dies with its blob (§4.7).
- A wasm module cache **cannot cross iframe realms**, so per-figure sandboxing
  means per-figure compilation; the fix is fewer live frames, never weaker
  isolation (§6, §12c).
- Chunk tag is **`PSTR`**, confirmed over `POST`.
