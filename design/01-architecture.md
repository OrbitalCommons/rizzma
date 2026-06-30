# Matplotlib Architecture — A Reference for the Rizzma Rust/Wasm Reimplementation

> **Purpose of this document.** This is a deep architectural reading of matplotlib
> (the source cloned under `references/matplotlib/`) written specifically to inform
> **rizzma**, a clean-room reimplementation of "the good parts" of pyplot/matplotlib
> in Rust that also targets wasm/canvas. It is not just a description of matplotlib;
> for every subsystem it tries to separate **essential architecture** (load-bearing
> ideas you should keep), **incidental Python mechanics** (machinery that exists only
> because of CPython/REPL/numpy and should be discarded or replaced), and **awkward
> mappings** (places where matplotlib's design fights Rust ownership or wasm).
>
> All file/line references are into `references/matplotlib/lib/matplotlib/` (Python)
> and `references/matplotlib/src/` (C/C++) unless noted. Line numbers are from the
> cloned snapshot and may drift by a few lines across versions.
>
> A companion document (`03-...`) is expected to cover shared geometry/tooling
> (transforms, paths, bbox) in depth; here transforms are covered enough to explain
> the **data flow**, with a pointer where the shared-tooling doc takes over.

---

## Table of Contents

1. [The 30,000-foot view: three layers](#1-three-layers)
2. [The Artist hierarchy & the draw protocol](#2-artist-hierarchy)
3. [The rendering pipeline: Figure.draw → Renderer](#3-rendering-pipeline)
4. [The transform architecture & the coordinate chain](#4-transforms)
5. [The backend abstraction (canvas, manager, events, registry)](#5-backends)
6. [Figure / SubFigure / GridSpec / layout engines](#6-figure-layout)
7. [pyplot: the global state machine](#7-pyplot)
8. [Configuration: rcParams, matplotlibrc, styles](#8-config)
9. [The C/C++ extension boundary (src/)](#9-cpp)
10. [Cross-cutting Python mechanics that are incidental](#10-incidental)
11. [**Implications for a Rust + wasm reimplementation**](#11-rust)

---

<a name="1-three-layers"></a>
## 1. The 30,000-foot view: three layers

Matplotlib is deliberately stratified into three layers. Almost every design decision
in the codebase falls out of this separation. Understanding it is the single most
important thing for the rizzma port, because **rizzma collapses two of matplotlib's
native sub-layers (Python objects + C++ kernels) into one Rust layer**, while keeping
the three logical layers intact.

```
┌──────────────────────────────────────────────────────────────────────┐
│  LAYER 1 — Scripting / stateful API     (pyplot.py, pylab.py)          │
│  Global "current figure / current axes" registry; thin wrappers that   │
│  forward to the OO layer. MATLAB-flavored, REPL-oriented, side-effecty. │
└───────────────────────────────┬──────────────────────────────────────┘
                                 │ delegates to (gcf()/gca())
┌───────────────────────────────▼──────────────────────────────────────┐
│  LAYER 2 — Artist / Object-Oriented model                              │
│  Figure → Axes → Axis/Spine/Line2D/Patch/Text/Collection…              │
│  A retained-mode scene graph. Every drawable is an Artist. Holds the    │
│  containment tree, transforms, styling, data, and a draw() protocol.    │
│  artist.py, figure.py, axes/_base.py, axes/_axes.py, axis.py, …         │
└───────────────────────────────┬──────────────────────────────────────┘
                                 │ Artist.draw(renderer)
┌───────────────────────────────▼──────────────────────────────────────┐
│  LAYER 3 — Backend / Rendering                                         │
│  RendererBase + GraphicsContextBase: a tiny drawing-primitive API      │
│  (draw_path / draw_image / draw_text / draw_markers …).                 │
│  FigureCanvasBase + FigureManagerBase: surface + window/event plumbing. │
│  Concrete backends: Agg (raster), SVG/PDF/PS (vector), Qt/Tk/Gtk/wx     │
│  (GUI), WebAgg (browser). backend_bases.py, backends/                   │
└──────────────────────────────────────────────────────────────────────┘
```

**Why this matters for rizzma:**

| matplotlib layer | maps to rizzma as | keep? |
| --- | --- | --- |
| pyplot scripting | a stateful façade module/struct (`rizzma::pyplot`) holding a figure registry | keep, slimmed |
| Artist OO model | a Rust scene graph of trait objects / enums | **keep — this is the core** |
| RendererBase / GraphicsContext | a `Renderer` trait + `GraphicsContext` struct | **keep — this is the seam** |
| C++ kernels (Agg, FreeType, _path) | Rust crates (tiny-skia/lyon, ab_glyph/cosmic-text, geo) | replace |
| Canvas/Manager + GUI backends | trait + per-target impls; wasm/canvas is one impl | keep concept, drop most targets |

The key structural realization: in CPython, layer 2 is "Python objects" and the
hot numerical work is shoved down into layer-3-adjacent C++ (`src/`). **In Rust there
is no Python/C boundary** — layers 2 and the C++ kernels merge into ordinary Rust,
and the only true abstraction boundary that must survive is `Artist.draw(renderer)`
→ the `Renderer` trait.

---

<a name="2-artist-hierarchy"></a>
## 2. The Artist hierarchy & the draw protocol

File: `artist.py` (1938 lines). The `Artist` base class is at `artist.py:110`.

### 2.1 What an Artist is

> "Abstract base class for objects that render into a FigureCanvas. Typically, all
> visible elements in a figure are subclasses of Artist." (`artist.py:110`)

Everything you can see is an Artist: `Figure`, `Axes`, `Axis`, `Tick`, `Spine`,
`Line2D`, `Patch`/`Rectangle`/`Circle`, `Text`, `Collection` (the batched primitive),
`Image`, `Legend`, etc. Containers like `Figure` and `Axes` are *also* Artists, so the
whole scene is a uniform tree of Artists. This is a classic **retained-mode scene
graph**.

### 2.2 Core state (`Artist.__init__`, `artist.py:193`)

```
self._stale = True            # needs redraw to match internal state
self.stale_callback = None    # propagates staleness up the tree
self._axes = None             # back-pointer to owning Axes
self._parent_figure = None    # back-pointer to owning (Sub)Figure
self._transform = None        # the artist's transform (lazy IdentityTransform)
self._visible = True
self._animated = False        # excluded from normal draw loop (blitting)
self._alpha = None
self.clipbox / self._clippath / self._clipon   # clipping
self._label = ''              # legend label
self._zorder (class attr = 0) # paint order
self._sketch / self._path_effects             # styling pulled from rcParams
self._callbacks = CallbackRegistry(signals=["pchanged"])  # observer pattern
```

Note the **back-pointers** (`_axes`, `_parent_figure`): an Artist knows its parent.
The parent also owns the child (in lists like `Axes._children`, `Figure.artists`).
This is a **bidirectional parent/child relationship** — a direct red flag for Rust
ownership (see §11).

### 2.3 The containment tree

The tree is expressed by each container's `get_children()`:

- `FigureBase.get_children()` (`figure.py:220`) returns
  `[patch, *artists, *axes, *lines, *patches, *texts, *images, *legends, *subfigs]`.
- `_AxesBase.get_children()` (`axes/_base.py:4639`) returns
  `[*self._children, *self._axis_map.values()]` where `_children` accumulates lines,
  patches, collections, etc., and `_axis_map` holds the x/y (and more) `Axis` objects.
- `Axis.get_children()` (`axis.py:922`) returns `[label, offsetText, *majorTicks, *minorTicks]`.
- Each `Tick` owns two gridline `Line2D`s, two tick `Line2D`s, and two `Text` labels.

```
Figure
├── patch (background Rectangle)
├── Axes[0]
│   ├── patch (Axes background)
│   ├── XAxis ── label(Text), offsetText(Text), majorTicks[Tick…], minorTicks[Tick…]
│   │             each Tick ── tick1line, tick2line, gridline (Line2D), label1, label2 (Text)
│   ├── YAxis ── …
│   ├── Spines {left,right,top,bottom} (each a Patch)
│   ├── title / _left_title / _right_title (Text)
│   └── _children: Line2D, Patch, Collection, Image, Text, Legend, …
├── Axes[1] …
└── SubFigure …
```

**Essential vs incidental:** the *shape* of this tree is essential and rizzma should
mirror it. The fact that children are scattered across many typed lists
(`lines`, `patches`, `texts`, `images`…) rather than one heterogeneous list is
**incidental** — it exists for Python attribute-access ergonomics (`ax.lines`) and
back-compat. In Rust a single `Vec<Box<dyn Artist>>` (or an enum) per container plus
typed accessor methods is cleaner.

### 2.4 The draw protocol

The whole renderer-facing contract is one method:

```python
def draw(self, renderer):           # artist.py:1044 (base, near no-op)
    if not self.get_visible():
        return
    self.stale = False
```

Subclasses override it. A container's `draw` recurses into children sorted by
`zorder`. The base implementation just clears staleness; leaf artists translate
themselves into `renderer.draw_path(...)` / `draw_text(...)` calls.

**Three decorators wrap every `draw`** (`artist.py:23–99`), installed automatically
via `Artist.__init_subclass__` (`artist.py:119`):

- `allow_rasterization` (`artist.py:44`) — brackets `draw` with
  `renderer.start_rasterizing()/stop_rasterizing()` when `artist.get_rasterized()`,
  and `start_filter()/stop_filter()` for agg image filters. This is how a vector
  backend (PDF/SVG) can embed a rasterized sub-region.
- `_prevent_rasterization` (`artist.py:23`) — default wrapper that stops stray
  rasterization leaking across sibling artists.
- `_finalize_rasterization` (`artist.py:87`) — on the outermost artist (Figure),
  flush any pending rasterization.

For rizzma this rasterization bracketing is an **advanced/optional** concern (it only
matters for mixed raster-in-vector output). Keep the `draw(&self, renderer)` shape;
defer the rasterization decorators until you implement a vector backend that needs
embedded rasters.

### 2.5 z-order

`Artist.zorder` is a class attribute (default `0`, `artist.py:117`), overridable per
instance via `set_zorder` (`artist.py:1185`). Containers sort children by zorder at
draw time, e.g. `Axes.draw`: `artists = sorted(artists, key=attrgetter('zorder'))`
(`axes/_base.py:3334`). Default zorders are conventionally: patches 1, lines 2,
text 3, etc. **Essential**: rizzma needs a stable paint order; an `i32` zorder field
on each artist plus a stable sort is the whole story. (matplotlib's sort is stable, so
equal-zorder artists draw in insertion order — preserve that.)

### 2.6 The `stale` / `stale_callback` invalidation mechanism

`stale` is a property (`artist.py:316`). Setting `stale = True` propagates **up** the
tree through `stale_callback`:

```python
@stale.setter
def stale(self, val):
    self._stale = val
    if self._animated:          # animated artists opt out of the normal loop
        return
    if val and self.stale_callback is not None:
        self.stale_callback(self, val)     # artist.py:334
```

When an artist is added to an Axes, its `stale_callback` is set to
`_stale_axes_callback` (`artist.py:102`), which sets `self.axes.stale = val`, which in
turn (Axes being an Artist whose stale_callback points at the Figure) bubbles to the
Figure. The Figure's stale flag is what interactive backends watch to decide whether
to schedule a redraw (`draw_idle`). In pyplot's plain-REPL path, the Figure's
`stale_callback` is `_auto_draw_if_interactive` (`pyplot.py:1116`).

This is a **scene-graph dirty-flag** that flows *child → root*, the opposite direction
from the transform invalidation graph (§4, which also flows leaf → dependents but via
a different mechanism). Both exist because matplotlib does **lazy, on-demand redraw**:
nothing is recomputed until a draw is actually requested.

**For rizzma:** a single `dirty: bool` per figure plus per-axes flags is enough for a
first cut. The callback-chain implementation is an artifact of Python's lack of a
parent pointer you can cheaply walk; in Rust you can just walk parent pointers, or set
a figure-level dirty flag directly. The `_animated` opt-out is a blitting optimization
— defer it.

### 2.7 The `set`/`get` property system and `_docstring`/`kwdoc` machinery

Matplotlib exposes a uniform `get_x()`/`set_x()` accessor pair for every styleable
property, plus a bulk `Artist.set(**kwargs)` and `Artist.update(dict)`.

- `Artist.set(**kwargs)` (`artist.py:1317`) normalizes aliases (`lw`→`linewidth`)
  via `cbook.normalize_kwargs`, then calls `_internal_update` → `_update_props`,
  which dispatches each `k=v` to `self.set_k(v)`.
- `Artist.update_from(other)` (`artist.py:1230`) copies visual props between artists.
- `_cm_set` (`artist.py:1323`) is a context manager that temporarily sets props
  (used by e.g. `redraw_in_frame` to hide ticks).

The clever/heavy part is **auto-generated signatures and docstrings**:

- `Artist.__init_subclass__` (`artist.py:119`) injects a `set` method per subclass and
  calls `_update_set_signature_and_docstring` (`artist.py:153`), which builds an
  `inspect.Signature` listing every settable property as a keyword-only param, and a
  docstring assembled from `kwdoc(cls)`.
- `ArtistInspector` (`artist.py:1490`) introspects a class: it scans `dir()` for
  `set_*`/`get_*` methods (`get_setters` `artist.py:1594`), discovers aliases by
  parsing docstrings with regex (`get_aliases` `artist.py:1516`), and extracts
  "ACCEPTS:" lines (`get_valid_values` `artist.py:1544`).
- `kwdoc(artist)` (`artist.py:1915`) produces the formatted property table that gets
  spliced into docstrings.
- `_docstring.py` provides `Substitution`, the `interpd` registry, and
  `kwarg_doc`/`%(Cls:kwdoc)s` interpolation that stitches these tables into the docs
  of hundreds of functions at import time.

**This entire subsystem is incidental for rizzma.** It is a Python-documentation /
runtime-reflection convenience: dynamically building signatures and docstrings so that
`help(ax.plot)` and IDE autocomplete show every keyword. Rust has no runtime docstring
interpolation and no need for `ArtistInspector`-style reflection. The *semantic*
content (which properties exist, their aliases, their valid values) should instead be
encoded as:

- explicit typed setters/builder methods (`Line2D::set_linewidth(f32)`), and/or
- a `LineStyle`/`Line2DProps` struct with `Option<T>` fields and an `update_from`,
- alias resolution done once in a `kwargs`-parsing front end if you support a
  string-keyed `set(...)` for the pyplot layer.

Keep the *idea* of "a uniform settable-property surface"; drop the reflection.

### 2.8 The observer registry

Each artist owns a `CallbackRegistry` (`cbook.py:201`) for the `"pchanged"` signal;
`add_callback`/`pchanged` (`artist.py:404`, `:443`) let external code react to
property changes. The registry stores callbacks as **weak references**
(`weakref.WeakMethod`, `cbook.py:141`) keyed by signal, with proxy cleanup. This is
used sparingly (e.g. shared axes, colorbar tracking an image). **Incidental** for a
first rizzma cut; if you need it later, a `Vec<Weak<dyn Fn>>` or an event-id → boxed
closure map suffices.

---

<a name="3-rendering-pipeline"></a>
## 3. The rendering pipeline: Figure.draw → Renderer

### 3.1 The top-level draw

`Figure.draw(renderer)` (`figure.py:3264`):

```python
def draw(self, renderer):
    if not self.get_visible(): return
    with self._render_lock:
        artists = self._get_draw_artists(renderer)
        renderer.open_group('figure', gid=self.get_gid())
        if self.axes and self.get_layout_engine() is not None:
            self.get_layout_engine().execute(self)      # ← layout runs HERE, at draw
        self.patch.draw(renderer)
        mimage._draw_list_compositing_images(renderer, self, artists,
                                             self.suppressComposite)
        renderer.close_group('figure')
        self.stale = False
    DrawEvent("draw_event", self.canvas, renderer)._process()
```

Two load-bearing facts:

1. **Layout is a draw-time hook.** `layout_engine.execute(fig)` adjusts Axes positions
   right before painting (constrained/tight layout). See §6.
2. **Image compositing.** `_draw_list_compositing_images` sorts the artist list by
   zorder and draws them, with a special path that lets adjacent `Image` artists be
   alpha-composited together (controlled by `suppressComposite`). For most plots this
   is just "draw children in zorder."

### 3.2 The Axes draw

`_AxesBase.draw(renderer)` (`axes/_base.py:3296`) is where most visible content
emerges. The sequence:

1. `self._unstale_viewLim()` — force pending autoscale/limit recompute.
2. `apply_aspect(...)` — adjust the Axes box for `aspect='equal'` etc.
3. Collect `get_children()`, remove the `patch` (drawn specially as background), and
   optionally remove spines/axis if `axison`/`frameon` are off.
4. `_update_title_position(renderer)`.
5. Filter out animated artists (unless saving), then `sorted(... key=zorder)`.
6. Split off negative-zorder artists for rasterization if `_rasterization_zorder` set.
7. `mimage._draw_list_compositing_images(renderer, self, artists, …)` — recurse.

So **draw is a recursive tree walk, sorted by zorder at each container**, terminating
at leaf artists that emit primitive calls.

### 3.3 The leaf → Renderer protocol

The renderer API is intentionally tiny. From `RendererBase` (`backend_bases.py:134`):

| method | required? | meaning |
| --- | --- | --- |
| `draw_path(gc, path, transform, rgbFace=None)` | **required** | stroke/fill a `Path` under an affine transform |
| `draw_image(gc, x, y, im, transform=None)` | **required** | blit an RGBA buffer |
| `draw_gouraud_triangles(...)` | required for full support | smooth-shaded triangles |
| `draw_markers(gc, marker_path, marker_trans, path, trans, rgbFace)` | optional (falls back to N×draw_path) | stamp a marker at each vertex |
| `draw_path_collection(...)` | optional (falls back) | batched paths w/ per-item color/width/style |
| `draw_quad_mesh(...)` | optional | pcolormesh fast path |
| `draw_text(gc, x, y, s, prop, angle, ismath, mtext)` | optional (falls back to path-of-glyphs) | text |

The doc comment at `backend_bases.py:138` says it plainly: *"just implementing
`draw_path` alone would give a highly capable backend."* The other methods are
**performance specializations** — a backend can omit them and the base class provides
correct (slow) fallbacks built on `draw_path`. For example `draw_markers`'s fallback
(`backend_bases.py:202`) literally loops over the path's vertices calling `draw_path`
with a translated marker transform.

**This is the single most important seam for rizzma.** The entire OO layer talks to
the world through ~3 required + ~4 optional primitives. Define this as a Rust trait:

```rust
trait Renderer {
    fn draw_path(&mut self, gc: &GraphicsContext, path: &Path, tr: &Affine2D,
                 rgb_face: Option<Rgba>);
    fn draw_image(&mut self, gc: &GraphicsContext, x: f32, y: f32, im: &RgbaImage);
    fn draw_text(&mut self, gc: &GraphicsContext, x: f32, y: f32, text: &str,
                 font: &FontProps, angle: f32);          // provide a fallback
    // optional fast paths with default-trait fallbacks:
    fn draw_markers(&mut self, …) { /* default: loop draw_path */ }
    fn draw_path_collection(&mut self, …) { /* default: loop */ }
    // metrics & capabilities:
    fn flipy(&self) -> bool { true }
    fn canvas_width_height(&self) -> (u32, u32);
    fn points_to_pixels(&self, pts: f32) -> f32;
    fn text_extent(&self, s: &str, font: &FontProps) -> TextExtent;
}
```

### 3.4 GraphicsContext: the per-call state bag

`GraphicsContextBase` (`backend_bases.py:701`) is a plain state container the artist
fills in and hands to each primitive call: color (`_rgb`), alpha, linewidth, dash
pattern, cap/join style, clip rectangle, clip path, hatch, url, gid, snap, sketch
params, antialiasing flag. `new_gc()` mints one; `copy_properties` clones; `restore`
is a no-op for stateless backends. Crucially **the renderer reads it but never owns
or mutates it** — it's an immutable-by-convention parameter object. In Rust this is a
`#[derive(Clone)] struct GraphicsContext { … }` passed by `&`.

### 3.5 Path: the universal geometry primitive

`Path` (`path.py:24`) is two parallel arrays: `vertices` `(N,2) f64` and `codes`
`(N,) u8`, with codes `MOVETO=1, LINETO=2, CURVE3=3, CURVE4=4, CLOSEPOLY=79`
(`path.py:82`). If `codes is None`, it's an implicit MOVETO + LINETOs. Every shape —
a line, a rectangle, a circle (as cubic Béziers), a glyph outline — is a `Path`.
`iter_segments` (`path.py:366`) yields cleaned/transformed segments and is the
canonical consumer-facing API (users "should not access vertices/codes directly",
`path.py:64`).

**For rizzma:** this is essential and maps cleanly to a Rust `Path { verts: Vec<[f32;2]>,
codes: Vec<PathCode> }` or to `lyon`'s path types. Quadratic (`CURVE3`) and cubic
(`CURVE4`) Béziers plus close are the whole vocabulary. The renderer flattens curves;
in matplotlib that flattening + NaN-removal + clipping + simplification + snapping
happens in C++ (`path_converters.h`, §9). In Rust, `lyon`/`tiny-skia` provide curve
flattening; you'll re-implement the simplify/snap pipeline if you want matplotlib's
exact pixel output.

### 3.6 End-to-end data flow (one `ax.plot([1,2],[3,4])`)

```
ax.plot(x, y)                         # builds a Line2D, appends to ax._children, sets stale
   …later…
fig.canvas.draw()                     # backend canvas
  └─ Figure.draw(renderer)
       ├─ layout_engine.execute(fig)  # adjust Axes positions (optional)
       ├─ patch.draw(renderer)        # figure background
       └─ Axes.draw(renderer)
            ├─ Spines.draw, XAxis.draw, YAxis.draw  (ticks/labels/grid)
            └─ Line2D.draw(renderer)
                 ├─ build a Path from (x,y) data
                 ├─ trans = self.get_transform()  ==  ax.transData
                 │      = transScale + (transLimits + transAxes)   # data→display
                 ├─ gc = renderer.new_gc(); gc.set_foreground(color); gc.set_linewidth(lw)…
                 └─ renderer.draw_path(gc, path, trans)            # ← the seam
                          └─ (Agg) C++ rasterizes into RGBA buffer
                          └─ (SVG) emit <path d="…"/>
                          └─ (rizzma/wasm) tiny-skia fill/stroke OR Canvas2D ops
```

Note **the transform is passed into `draw_path`, not pre-applied** in matplotlib: the
renderer (or its C++ kernel) applies the affine to the vertex array. This lets Agg do
the matrix multiply over the whole array in C, and lets vector backends emit the
transform symbolically. rizzma should preserve this: pass an `Affine2D` to the
renderer and let the renderer/crate apply it (tiny-skia takes a transform; Canvas2D
has `setTransform`).

---

<a name="4-transforms"></a>
## 4. The transform architecture & the coordinate chain

File: `transforms.py` (3052 lines). *Shared-tooling doc 03 covers the geometry in full;
here we cover how transforms fit the data flow and the Rust-awkward parts.*

### 4.1 The TransformNode invalidation graph

Base class `TransformNode` (`transforms.py:82`). Transforms form a **lazy-evaluated,
cached dependency DAG**:

- Three invalidation states: valid / affine-only-invalid / fully-invalid
  (`transforms.py:95`).
- `invalidate()` (`transforms.py:154`) marks self and propagates **up to parents**
  via `_invalidate_internal` (`transforms.py:163`).
- Parents are tracked in `_parents`, a dict of `id(parent) → weakref(parent)`
  (`transforms.py:116`); a child registers itself with `set_children(*children)`
  (`transforms.py:178`), installing weakref callbacks so the entry is dropped when a
  parent is GC'd.

So when an Axes view limit changes, it invalidates a `Bbox`, which invalidates every
composite transform built on top of it (transData, the axis transforms, every
`TransformedPath` an artist cached), without eagerly recomputing anything. Recompute
happens lazily on the next `get_matrix()`/`transform()`.

This **weakref parent registry is the single most Rust-awkward construct in
matplotlib** (see §11.4): parents hold strong refs to children (children are owned),
but children hold *weak* refs back to parents for invalidation — an inverted,
cyclic-ish dependency graph with GC-driven cleanup.

### 4.2 The affine / non-affine split

Every `Transform` (`transforms.py:1328`) splits into an affine part and a non-affine
part:

```
transform(x) == transform_affine(transform_non_affine(x))
```

`is_affine` is a flag; affine transforms are 3×3 matrices (`Affine2D`,
`transforms.py:1941`) and are the *fast path* — the matrix is applied to whole vertex
arrays in C (`Affine2DBase.transform_affine` calls `_path.affine_transform`,
`transforms.py:1908`). Non-affine pieces (log scale, polar) are arbitrary Python and
slow. Composites (`CompositeGenericTransform` `:2391`, `CompositeAffine2D` `:2505`)
fold runs of affines into a single matrix; the clever bit is that they *cache the
folded matrix* and invalidate it through the DAG.

**Why this matters for the port:** the affine/non-affine split exists so the common
case (linear data→pixel) collapses to one matrix multiply. rizzma should keep:
`Affine2D` (a 6-float `[[a c e],[b d f]]`), composite-as-matrix-product, and a separate
non-affine `Scale`/`Projection` trait for log/polar. The split is essential; the lazy
weakref DAG is an optimization you can simplify (recompute eagerly, or cache with a
generation counter) — see §11.4.

### 4.3 The standard coordinate chain

Coordinates flow **data → axes → figure → display(pixels)**. The transforms are built
in `_AxesBase._set_lim_and_transforms` (`axes/_base.py:935`):

```python
self.transAxes = BboxTransformTo(self.bbox)                 # axes(0..1) → display
self.transScale = TransformWrapper(IdentityTransform())     # data scale (log etc.), non-affine
self.transLimits = BboxTransformFrom(                       # data → axes(0..1), via view limits
                       TransformedBbox(self._viewLim, self.transScale))
self.transData = self.transScale + (self.transLimits + self.transAxes)   # data → display
self._xaxis_transform = blended_transform_factory(self.transData, self.transAxes)
self._yaxis_transform = blended_transform_factory(self.transAxes, self.transData)
```

- `transData` (`axes/_base.py:964`): **data coords → display pixels**. This is the
  transform attached to data artists (lines, patches). The parenthesization groups the
  two affines (limits+axes) so they fold into one matrix, separate from the possibly
  non-affine `transScale`.
- `transAxes`: **axes fraction (0..1) → display**, anchored to the Axes `bbox`.
- `transFigure` (`figure.py:2644`): **figure fraction (0..1) → display**, a
  `BboxTransformTo(self.bbox)`.
- `bbox` itself is a `TransformedBbox` of the Axes position rectangle through
  `transAxes` (`axes/_base.py:884`), and the figure `bbox` is the figure-inches box
  through `dpi_scale_trans` (`figure.py:2642`) — this is where **DPI** enters: display
  units are pixels = inches × dpi.

```
data ──transScale──> scaled ──transLimits──> axes[0..1] ──transAxes──> display(px)
                                      │                        ▲
   axes[0..1] ─────────transAxes──────┘                        │
   figure[0..1] ──────transFigure────────────────────────────> display(px)
   inches ───────────dpi_scale_trans─────────────────────────> display(px)
```

The "blended" transforms (`_xaxis_transform` etc.) apply `transData` in x and
`transAxes` in y (so a gridline spans data-x but full axes-height-y).

**For rizzma:** model this chain explicitly. A first version can hardcode the
rectilinear chain as matrix products and add a `Scale` enum (`Linear`/`Log`/`Symlog`)
for the one non-affine slot. Polar and arbitrary projections (`projections/`) plug in
by overriding `_set_lim_and_transforms` — make that a trait method.

---

<a name="5-backends"></a>
## 5. The backend abstraction

Files: `backend_bases.py` (3737 lines), `backends/`, `backends/registry.py`.

### 5.1 The four roles a backend plays

The canonical minimal example is `backends/backend_template.py` (read it as the spec).
A backend module must provide:

| role | base class | responsibility |
| --- | --- | --- |
| **Renderer** | `RendererBase` (`backend_bases.py:134`) | the `draw_*` primitives (§3.3) |
| **GraphicsContext** | `GraphicsContextBase` (`:701`) | per-draw state bag (§3.4) |
| **Canvas** | `FigureCanvasBase` (`:1709`) | binds a Figure to a surface; `draw()`, `print_*()`, event source |
| **Manager** | `FigureManagerBase` (`:2696`) | window/lifecycle, toolbar; only meaningful for interactive backends |

Plus a module-level `show()` and `FigureCanvas`/`FigureManager` aliases. The
`_Backend` helper (`backend_bases.py:3625`) and `@_Backend.export`
(`backend_bases.py:3706`) wire `new_figure_manager`, `draw_if_interactive`, and `show`
into the module namespace.

### 5.2 Raster path: Agg

`backends/backend_agg.py`. `RendererAgg` (`backend_agg.py:59`) is a thin Python shell
over the C++ `_RendererAgg` (constructed at `backend_agg.py:71` with `(width, height,
dpi)`). `draw_path` etc. delegate straight to the C++ object; `buffer_rgba()` exposes
the raw RGBA pixel buffer; `FigureCanvasAgg.print_png` encodes it.

Two patterns worth stealing:

- **Renderer caching** keyed by `(w, h, dpi)` (`backend_agg.py:443`) — don't reallocate
  the pixel buffer when size is unchanged.
- A **draw lock** for thread safety (`backend_agg.py:436`).

Agg is the default for PNG/JPG/raw and underlies all the GUI `*agg` backends — they
render to the Agg buffer and blit it into a Qt/Tk/Gtk widget.

### 5.3 Vector path: SVG / PDF / PS

`backends/backend_svg.py`: `RendererSVG.__init__` takes a file-like `svgwriter`;
`draw_path` emits `<path d="...">` rather than rasterizing; `finalize()` flushes XML.
No pixel buffer. PDF/PS are analogous (their own object models). The structural
difference: **vector renderers are stateful emitters keyed to an output stream**;
raster renderers own a mutable framebuffer.

`backend_mixed.py` (`MixedModeRenderer`) composes both: it swaps a raster sub-renderer
in for `start_rasterizing()`, then on `stop_rasterizing()` blits that raster buffer
into the vector renderer as an image (`backend_mixed.py:71`). This is the machinery
behind §2.4's rasterization decorators.

### 5.4 The browser path: WebAgg — the template for wasm

`backends/backend_webagg_core.py` is *the* reference for rizzma's wasm target, because
the browser is already "just another backend" in matplotlib:

- `FigureCanvasWebAggCore` (`backend_webagg_core.py:155`) **inherits from
  `FigureCanvasAgg`** — it renders with Agg to an RGBA buffer, then ships pixels to the
  browser.
- **Diff/blit protocol** (`get_diff_image`, `backend_webagg_core.py:252`): view the
  RGBA buffer as `u32`, compare to the previous frame, send a PNG of only the changed
  pixels (full frame if transparency present or size changed). This minimizes bytes
  over the wire.
- **Event ingestion** (`handle_event`/`_handle_mouse`/`_handle_key`,
  `:285–349`): JSON events from JS are translated into matplotlib `MouseEvent`/
  `KeyEvent`, with the y-flip (`y = height - y`, `:309`) and button remap (JS 0/1/2 →
  mpl 1/2/3).
- **Manager broadcast** (`FigureManagerWebAgg.refresh_all`, `:504`): push the diff PNG
  to every connected websocket; `_send_event` pushes JSON (cursor, messages, toolbar).
- `backend_webagg.py` wraps this in a Tornado server with a `WebSocket` handler
  (`send_binary`/`send_json`).

The transport (websocket) is the only browser-specific part. **In wasm the transport
collapses**: the Rust renderer runs *inside* the page, so instead of encoding a PNG and
shipping it over a socket, you either (a) write the RGBA buffer directly into a
`<canvas>` via `ImageData`/`putImageData`, or (b) skip the raster buffer and emit
Canvas2D path ops directly. Either way the **canvas/manager/event interface is
unchanged** — that's the key insight: *wasm/canvas is isomorphic to WebAgg minus the
socket.*

### 5.5 Events & the event loop

`backend_bases.py`:

- `Event` base (`:1178`), with `LocationEvent` (`:1257`), `MouseEvent` (`:1323`),
  `KeyEvent` (`:1497`), `DrawEvent` (`:1206`), `ResizeEvent` (`:1233`),
  `CloseEvent` (`:1253`), `PickEvent` (`:1448`). `MouseButton` is an `IntEnum`
  (`:1315`).
- The canvas owns a `CallbackRegistry` (via `figure._canvas_callbacks`); user code
  connects with `mpl_connect(event_name, func)` (`:2324`) and disconnects with
  `mpl_disconnect` (`:2387`). `Event._process()` (`:1200`) dispatches to the registry.
- `FigureCanvasBase.events` (`:1730`) is the fixed list of event names.
- Backends provide a `Timer` (`TimerBase`, `:1032`) and `start_event_loop` (`:2437`)
  for blocking interactions.
- **Picking**: `Artist.pick` (`artist.py:538`) hit-tests `contains(mouseevent)` and
  recurses into children, firing `PickEvent`s.

**Interactive vs non-interactive**: a backend is interactive iff its manager overrides
`start_main_loop`/`pyplot_show` (`backend_bases.py:3666`). Non-interactive backends
(Agg, SVG, PDF) just render to files; their `show()` warns. The
`required_interactive_framework` class attr (`:1721`) names the GUI toolkit ("qt",
"tk", …) or `None`.

### 5.6 The registry

`backends/registry.py`: `BackendRegistry` (`registry.py:15`) maps backend names to
modules (`matplotlib.backends.backend_{name}`), classifies each as interactive vs
non-interactive / by GUI framework (`_BUILTIN_BACKEND_TO_GUI_FRAMEWORK`,
`registry.py:37`), supports `module://my.backend` custom backends, and does the dynamic
`load_backend_module` import (`:302`). A singleton `backend_registry`
(`registry.py:414`) is the entry point pyplot's `switch_backend` uses.

**For rizzma:** the registry is dynamic-import plumbing that only exists because Python
loads backends lazily by string name. In Rust, backends are types known at compile
time (or behind cargo features). Replace the registry with an enum or feature-gated
constructor; keep the *concept* of "selectable backend" but make it static dispatch.

---

<a name="6-figure-layout"></a>
## 6. Figure / SubFigure / GridSpec / layout engines

Files: `figure.py`, `gridspec.py`, `layout_engine.py`, `_constrained_layout.py`,
`_tight_layout.py`.

### 6.1 Figure and SubFigure

`FigureBase` (`figure.py:118`) is the common base; `Figure` (`figure.py:2431`) is the
root (owns dpi, canvas, layout engine, the `_AxesStack`), and `SubFigure`
(`figure.py:2223`) is a nestable sub-region that shares the parent's transform stack
(`figure.py:2293`) but has its own `bbox_relative`. Both are Artists. The Figure owns:

- `bbox_inches` (`figure.py:2637`) — the figure in inches.
- `dpi_scale_trans` — inches → pixels (the DPI multiply).
- `bbox`/`transFigure` (`:2642`/`:2644`).
- An `_AxesStack` (`figure.py:70`) tracking add-order and the "current" axes for
  `gca()`.

### 6.2 The Axes position model

An Axes has a **position rectangle in figure coordinates [0,1]** (`_position`, a
`Bbox`), and `bbox = TransformedBbox(_position, transAxes-source)` so its display box
follows the figure size/dpi. Two ways to get a position:

- **Manual:** `fig.add_axes([left, bottom, w, h])` (`figure.py:539`) sets `_position`
  directly.
- **Grid:** `fig.add_subplot(...)` (`figure.py:652`) builds a `SubplotSpec` and the
  Axes position is *computed* from it.

### 6.3 GridSpec → SubplotSpec → position

`gridspec.py`:

- `GridSpecBase.get_grid_positions(fig)` (`gridspec.py:145`) is the core geometry:
  given `nrows/ncols`, `width_ratios/height_ratios`, `wspace/hspace`, and the figure's
  subplot margins (`left/right/top/bottom`), it returns four arrays
  `fig_bottoms/tops/lefts/rights` of cell edges in figure [0,1] coords. Crucially,
  `wspace`/`hspace` are fractions of the **average cell size**, not the figure.
- `SubplotSpec` (`gridspec.py:531`) references a *range* of cells via flat indices
  `num1..num2` (inclusive). `get_position(fig)` (`gridspec.py:658`) unravels those to
  row/col spans and reads the min/max cell edges into a `Bbox`.
- `GridSpecFromSubplotSpec` (`gridspec.py:471`) nests a grid inside one cell of a
  parent grid.

```
GridSpec(nrows,ncols, ratios, spacing)
   └─ get_grid_positions(fig) → cell-edge arrays in figure[0,1]
        └─ SubplotSpec(num1,num2).get_position(fig) → Bbox  (figure[0,1] rect)
             └─ Axes._set_position(bbox)  → drives Axes.bbox → transAxes → transData
```

### 6.4 Layout engines

`layout_engine.py`: `LayoutEngine` (`layout_engine.py:32`) is an abstract base with one
real method, `execute(fig)` (`:103`), invoked from `Figure.draw`
(`figure.py:3274`) **at draw time**. Implementations:

- `PlaceHolderLayoutEngine` (`:111`) — no-op (layout off).
- `TightLayoutEngine` (`:137`) — measures decoration bboxes (tick labels, titles) with
  the renderer and computes `SubplotParams` (margins) so nothing overlaps
  (`_tight_layout.py`). Only adjusts margins/positions, not Axes sizes; compatible with
  `subplots_adjust`.
- `ConstrainedLayoutEngine` (`:216`) — builds a constraint system (`_layoutgrid.py`,
  `_constrained_layout.py`) over rows/cols and margins, solving for both Axes positions
  **and sizes** to fit decorations; incompatible with manual `subplots_adjust`.

`adjust_compatible`/`colorbar_gridspec` flags (`:48–54`) flag which manual operations
are allowed.

**For rizzma:** the GridSpec geometry (§6.3) is pure arithmetic and essential — port it
directly. The layout-engine *protocol* (a draw-time `fn execute(&self, fig: &mut
Figure)`) is essential. Of the two engines, **tight_layout is the pragmatic first
target** (measure label bboxes, shrink margins); constrained_layout is a full
constraint solver and can be a later milestone. Both depend on being able to measure
text extents — which is your font subsystem (§9.3).

---

<a name="7-pyplot"></a>
## 7. pyplot: the global state machine

Files: `pyplot.py` (4832 lines), `_pylab_helpers.py`.

### 7.1 The figure registry (Gcf)

`_pylab_helpers.Gcf` (`_pylab_helpers.py:9`) is a never-instantiated singleton holding
`figs: OrderedDict[num → FigureManager]` (`:31`). **The active figure is whichever is
last in insertion order**; `set_active` (`:120`) moves a manager to the end,
`get_active` (`:102`) returns the last. Managers are held by **strong** reference
(only `manager.num` is used as identity); `destroy`/`destroy_all` (`:45`/`:79`) remove
them and disconnect the per-figure "click to activate" callback
(`_set_new_active_manager`, `:107`). `atexit.register(Gcf.destroy_all)` (`:136`).

### 7.2 gcf / gca and auto-vivification

- `gcf()` (`pyplot.py:1146`): return `Gcf.get_active().canvas.figure`, or **create a
  new figure if none exists**.
- `gca()` (`pyplot.py:2929`): `gcf().gca()` — delegate to the current figure, which
  creates a default Axes if needed.

So the stateful API is a thin shell: **almost every pyplot function is `gca().method(...)`
or `gcf().method(...)`**.

### 7.3 How the wrappers are generated

There are two flavors:

- **Hand-written** thin wrappers (e.g. `plot`, `pyplot.py:4037` → `gca().plot(...)`).
- **Auto-generated** wrappers (everything below the `# Autogenerated by
  boilerplate.py` marker, `pyplot.py:2883`). The generator is `tools/boilerplate.py`,
  which introspects each `Axes`/`Figure` method's signature and emits a forwarding
  wrapper decorated with `@_copy_docstring_and_deprecators(Axes.<name>)`
  (`pyplot.py:194`) that copies the docstring and re-applies deprecation decorators.

**For rizzma:** the delegation pattern is essential and trivial in Rust (a
`pyplot::plot(...)` free function that grabs the current axes and calls
`Axes::plot`). The *code generation by signature introspection* is incidental Python
machinery — in Rust you'd either hand-write the façade or use a macro, but you do **not**
need runtime signature copying or docstring propagation.

### 7.4 figure() and switch_backend()

- `figure()` (`pyplot.py:900`): reuse-or-create; on create it calls the backend's
  `new_figure_manager(num, …)` (`:1093`), registers the manager as active, and runs
  `draw_if_interactive()`.
- `_get_backend_mod()` (`pyplot.py:377`) lazily resolves the backend on first use;
  `switch_backend()` (`pyplot.py:391`) handles the `_auto_backend_sentinel`
  auto-detection (probe for a running GUI framework, else fall back through a list
  ending at headless "agg"), dynamically imports the backend module via the registry,
  synthesizes missing `new_figure_manager`/`draw_if_interactive`, and rewrites the
  module-level function signatures.

### 7.5 Interactive mode & REPL integration

`ion`/`ioff`/`isinteractive` (`pyplot.py:724`/`680`/`673`) toggle a global
interactive flag and install/uninstall a REPL display hook
(`install_repl_displayhook`, `:300`) that hooks IPython's `post_execute` event (or the
plain Python displayhook) to auto-redraw stale figures. `pause(interval)`
(`pyplot.py:764`) draws, shows non-blocking, and runs the GUI event loop for
`interval`. `show()` (`:598`) delegates to the backend's `show`.

**This whole REPL/IPython story is Python-specific and largely irrelevant to rizzma.**
There is no `sys.displayhook`, no IPython, and in wasm no blocking main loop. The
equivalent in rizzma is: a stateful façade that renders on demand (`plt::show()` →
encode/emit), and in wasm an explicit "draw into this canvas" call driven by the page's
own RAF/event loop. Keep `gcf`/`gca`/figure-registry semantics; drop ion/ioff/displayhook.

---

<a name="8-config"></a>
## 8. Configuration: rcParams, matplotlibrc, styles

Files: `__init__.py` (RcParams), `rcsetup.py` (validators), `style/core.py`,
`_docstring.py`.

### 8.1 RcParams = a validated global dict

`RcParams` (`__init__.py:685`) subclasses `MutableMapping`+`dict`. Its `validate`
class attr is `rcsetup._validators` (a dict of ~450 `key → validator-fn`). The
load-bearing method is `__setitem__` (`__init__.py:770`): it looks up the validator for
the key, runs it (raising on failure), and stores the **validated** value. `_set`/`_get`/
`_update_raw` bypass validation for internal use.

The global singleton `rcParams` (`__init__.py:997`) is built by loading the packaged
`matplotlibrc`, applying hardcoded defaults, then the user's `matplotlibrc`
(`matplotlib_fname()` searches CWD, `$MATPLOTLIBRC`, config dir, …). `rc()`,
`rcdefaults()`, `rc_context()` (`__init__.py:1014`/`1092`/`1159`) are the control
functions; `rc_context` is a contextmanager that snapshots and restores.

### 8.2 Validators (rcsetup.py)

Examples: `validate_bool` (`rcsetup.py:179`), `validate_float`/`int`/`string` via
`_make_type_validator` (`:213`), `validate_color` (`:375`), `validate_fontsize`
(`:431`), `_validate_linestyle` (`:523`), `ValidateInStrings` (`:42`, an enum
validator), and `_listify_validator` (`:125`, a higher-order combinator producing
list validators). The `_validators` dict (`:999`) is the master table.

### 8.3 Styles

`style/core.py`: a style is just a dict of rcParam overrides loaded from a `.mplstyle`
file; `use()` filters out a `STYLE_BLACKLIST` (backend, interactive, etc.) and calls
`rcParams.update(...)`; `context()` is a contextmanager wrapper. The library is loaded
from packaged + user `stylelib/` dirs.

**For rizzma:** the essence is **a typed, validated config struct with a global default
and scoped overrides**. In Rust:

- Replace the stringly-typed `dict + validator table` with a `struct RcParams { … }`
  whose fields are already typed (so "validation" is mostly parsing at the
  config-file/string boundary, e.g. `serde` + custom deserializers for colors/linestyles).
- A global default (`RcParams::default()`), per-figure overrides, and a scoped
  `with_rc(overrides, || { … })` helper replace `rc_context`.
- Styles become named `RcParams` presets; `use_style` merges them.
- The **`_docstring.py` interpolation machinery is entirely discardable** — it only
  exists to splice property tables into Python docstrings at import time.

---

<a name="9-cpp"></a>
## 9. The C/C++ extension boundary (src/)

This section is critical because **rizzma's Rust replaces both matplotlib's Python
layer and its C++ layer**. Everything native exists for one reason: tight loops over
large arrays (vertices, pixels, glyphs) that are unacceptably slow in Python. In Rust
that motivation disappears — Rust *is* the fast loop — so each native module becomes
either ordinary Rust or a Rust crate dependency.

### 9.1 `_path` — vectorized path geometry

Files: `src/_path.h`, `src/path_converters.h`, `src/_path_wrapper.cpp`.

- Point-in-path / points-in-path (ray casting) — `point_in_path` (`_path.h:267`),
  `points_in_path` (`:241`); used for hit-testing/picking and clipping.
- `clip_path_to_rect` (Sutherland–Hodgman, `_path.h:617`).
- `affine_transform_2d` (`_path.h:675`) — apply a 2×3 matrix to a whole vertex array
  (the fast path that `Affine2DBase.transform_affine` calls).
- `convert_path_to_polygons` (`:921`).
- The **vertex pipeline** in `path_converters.h`: `PathNanRemover` (`:162`),
  `PathClipper` (Liang–Barsky, `:329`), `PathSnapper` (snap to pixel centers, `:552`),
  `PathSimplifier` (drop sub-pixel vertices, `:651`), `Sketch` (`:1045`). These chain
  NaN→clip→snap→simplify→curve-flatten on every drawn path.

*Why native:* millions of vertices per frame. *Rust replacement:* `lyon`/`tiny-skia`
for curve flattening and stroking; hand-port or use `geo`/`geo-clipped` for
clipping/point-in-poly; the simplify/snap stages are small and worth re-porting if you
want pixel-identical output to matplotlib (otherwise the rasterizer's own AA suffices).

### 9.2 `_backend_agg` — the Agg rasterizer wrapper

Files: `src/_backend_agg.{h,cpp}`, `src/_backend_agg_wrapper.cpp`,
`src/_backend_agg_basic_types.h`, `extern/agg24-svn` (the vendored Anti-Grain Geometry
library).

`RendererAgg` (`_backend_agg.h:108`) implements the real `draw_path` / `draw_markers` /
`draw_path_collection` / `draw_image` / `draw_text_image` / `draw_quad_mesh` /
`draw_gouraud_triangles` / `copy_from_bbox`/`restore_region` against Agg's
anti-aliased scanline rasterizer, stroke/dash/curve converters, and RGBA pixel format.
`GCAgg` (`_backend_agg_basic_types.h:77`) packs the Python graphics context into one
C++ struct. `BufferRegion` (`_backend_agg.h:54`) backs blitting.

*Why native:* this is the pixel-pushing inner loop. *Rust replacement:* **`tiny-skia`**
(CPU AA rasterizer with a near-identical conceptual model — paths, paints, strokes,
clips, an RGBA `Pixmap`) is the closest drop-in; `lyon` (tessellation) + a GPU/`wgpu`
backend is the alternative for the wasm/GPU path. `copy_from_bbox`/`restore_region`
map to copying sub-rects of the `Pixmap` (for blitting/animation).

### 9.3 `ft2font` — FreeType glyph loading & rasterization

Files: `src/ft2font.{h,cpp}`, `src/ft2font_wrapper.cpp`.

`FT2Font` (`ft2font.h:102`) loads font files, sets pixel size from pt+dpi, shapes text
(via Raqm/HarfBuzz for complex scripts), extracts glyph outlines as `Path`s
(`get_path`, `:148`), rasterizes glyphs to bitmaps (`draw_glyphs_to_bitmap`, `:141`),
and reports kerning/metrics. This feeds both raster (stamp glyph bitmaps) and vector
(emit glyph outlines) backends, and **text metrics drive layout** (§6.4).

*Why native:* FreeType is a C library; glyph rasterization is hot. *Rust replacement:*
`ab_glyph`/`fontdue` (rasterization + metrics, pure Rust), `rustybuzz` (shaping),
`fontdb`/`font-kit` (font discovery), or the integrated `cosmic-text` (discovery +
shaping + layout). For wasm you can also defer text to the Canvas2D `fillText` API and
skip glyph rasterization entirely — at the cost of pixel-exactness and SVG outline
export.

### 9.4 `_image_resample` — high-quality image resampling

Files: `src/_image_resample.h`, `src/_image_wrapper.cpp`. 15+ interpolation kernels
(nearest…Lanczos) over affine and mesh transforms, via Agg's image filters. *Why
native:* convolution over large images. *Rust replacement:* the `image`/`resize`/
`fast_image_resize` crates, or `tiny-skia`'s image shader for the common bilinear/
nearest cases.

### 9.5 The rest

- `_qhull_wrapper.cpp` (`:`) — Delaunay triangulation (qhull) for `tricontour`/
  `triplot`. Rust: `spade` or `delaunator`.
- `src/tri/` — triangulation mesh + marching-triangles contour generation. Rust:
  hand-port or a contour crate.
- `_tkagg.cpp`, `_macosx.m` — GUI blitting (Tk pixmap / Cocoa). Irrelevant to wasm.
- `_c_internal_utils.cpp` — trivial platform shims.

### 9.6 Native-boundary summary table

| C/C++ module | role | why native | rizzma Rust replacement |
| --- | --- | --- | --- |
| `_path` / `path_converters` | vertex pipeline, clip, point-in-path, affine | per-vertex hot loops | `lyon`/`tiny-skia` + small hand-ports |
| `_backend_agg` (+ Agg) | raster rendering | pixel inner loop | **`tiny-skia`** (CPU) or `lyon`+`wgpu` (GPU) |
| `ft2font` (FreeType/Raqm) | font load, shape, glyph raster, metrics | C lib + hot raster | `cosmic-text` / `ab_glyph`+`rustybuzz` (or Canvas2D text in wasm) |
| `_image_resample` | image interpolation | convolution | `fast_image_resize` / `image` |
| `_qhull` | Delaunay | O(n log n) geometry | `spade` / `delaunator` |
| `tri` | mesh + contour | spatial traversal | hand-port |
| `_tkagg`/`_macosx` | GUI blit | toolkit FFI | n/a (wasm uses canvas) |

---

<a name="10-incidental"></a>
## 10. Cross-cutting Python mechanics that are incidental

Collected so the port doesn't accidentally reimplement Python's quirks:

- **Reflection-driven `set`/`get`/`ArtistInspector`/`kwdoc`** (§2.7) — replace with
  typed setters / props structs.
- **`_docstring` interpolation** (§8) — drop entirely.
- **`pyplot` boilerplate generation** (§7.3) — hand-write or macro the façade.
- **ion/ioff/IPython displayhook/`pause`** (§7.5) — drop; wasm has its own loop.
- **Dynamic backend import + `BackendRegistry`** (§5.6) — static dispatch / cargo
  features.
- **Pickling (`__getstate__`/`__setstate__`)** of figures (`figure.py:3306`) —
  replace with `serde` if you need persistence, but it's optional.
- **`weakref`-based observer registries and the transform parent graph** (§2.8, §4.1) —
  re-architect for Rust ownership (§11.4).
- **numpy as the array substrate** — replace with `ndarray`/`Vec`/`glam`/`nalgebra`.
- **The `*.pyi` stub files** — Rust's type system makes these moot.

---

<a name="11-rust"></a>
## 11. Implications for a Rust + wasm reimplementation

This is the section the rest of the document exists to support.

### 11.1 Layer mapping (what to keep, what to discard)

```
matplotlib                          rizzma (Rust)
──────────                          ─────────────
pyplot (global, REPL)        →      rizzma::pyplot façade over a FigureRegistry  [KEEP, SLIM]
Artist OO scene graph        →      trait Artist + concrete structs/enums        [KEEP — core]
RendererBase/GraphicsContext →      trait Renderer + struct GraphicsContext      [KEEP — the seam]
transforms (lazy DAG)        →      Affine2D + Scale + composite (eager/cached)   [KEEP, SIMPLIFY]
GridSpec/layout engines      →      gridspec arithmetic + trait LayoutEngine      [KEEP geometry; tight first]
rcParams + styles            →      struct RcParams + presets                     [KEEP, TYPED]
backends (Agg/SVG/GUI/WebAgg)→      impl Renderer for {Skia, Svg, Canvas}         [KEEP CONCEPT; wasm = WebAgg−socket]
C++ kernels (Agg/FT/_path…)  →      crates (tiny-skia, cosmic-text, lyon, …)      [REPLACE]
reflection/docstring/pyi     →      —                                            [DISCARD]
```

### 11.2 Trait design for Artist / Renderer / Backend

**`Renderer` trait** — the load-bearing abstraction (§3.3). Keep it tiny:
`draw_path` (required), `draw_image`, `draw_text` with default fallbacks for
`draw_markers`/`draw_path_collection`, plus capability/metric methods
(`flipy`, `canvas_width_height`, `points_to_pixels`, `text_extent`). Pass `&Affine2D`
into draw calls; let the renderer apply it (Skia transform / Canvas2D `setTransform`).
This is what makes every output target (PNG via tiny-skia, SVG string, wasm canvas)
pluggable.

**`Artist` trait** — `fn draw(&self, r: &mut dyn Renderer)`, `fn children(&self) ->
&[ArtistId]` (or an iterator), `fn zorder(&self) -> i32`, `fn visible(&self) -> bool`,
`fn get_transform(&self) -> &Transform`, `fn window_extent(&self, r) -> Bbox`. Resist
making it huge — matplotlib's Artist is bloated with clip/url/gid/sketch/picker
concerns; start with draw/children/zorder/visible/transform and grow.

**Beware `dyn Artist` + downcasting.** matplotlib relies on `isinstance` checks
(`Line2D`, `Image`, `SubFigure`) in hot paths (e.g. image compositing, `add_line` vs
`add_patch`). In Rust prefer an **enum of concrete artist kinds** (`enum Artist {
Line(Line2D), Patch(Patch), Text(Text), Collection(Collection), Image(Image),
Axes(Box<Axes>), … }`) over `Box<dyn Artist>` where you need to match on kind. Enums
also dodge the trait-object-ownership headaches below and keep the scene graph
`Vec<Artist>` cache-friendly.

### 11.3 Ownership of the artist tree

matplotlib's tree is **bidirectional**: parents own children (in typed lists) and
children hold back-pointers (`_axes`, `_parent_figure`) plus per-artist
`stale_callback`s that walk *up* to the figure. This is the classic
"graph that fights `Box`/`Rc`" shape. Options for rizzma, in rough order of preference:

1. **Arena + indices (recommended).** Store all artists in a `Figure`-owned arena
   (`Vec<ArtistNode>` or `slotmap::SlotMap`), reference parents/children by typed `Id`s.
   Back-pointers become `parent: Option<ArtistId>`. Invalidation walks ids, no `Rc`,
   no `RefCell`, trivially `Send`. This matches the fact that the whole tree's lifetime
   = the figure's lifetime.
2. **Tree ownership without back-pointers.** `Figure` owns `Vec<Axes>`, `Axes` owns
   `Vec<Artist>`; drop the child→parent pointers and instead pass needed parent context
   (`transData`, dpi) *down* as arguments to `draw`. Staleness becomes a single
   `figure.dirty` flag set by `&mut self` setters. Simplest; loses cheap "which axes am
   I in" queries (rarely needed at draw time).
3. **`Rc<RefCell<…>>` everywhere** — closest to Python, but you inherit borrow-panics
   and lose `Send`/wasm-friendliness. **Avoid.**

The arena approach also cleanly solves the transform parent-graph problem (§11.4) and
the picking traversal (ids in, id out).

### 11.4 Transforms: drop the weakref DAG, keep the matrix math

The lazy weakref invalidation graph (§4.1) is the most Rust-hostile construct. You do
**not** need it. Two viable strategies:

- **Eager + cached-by-generation.** Each Axes computes `transData` as a concrete
  `Affine2D` (× optional `Scale`) whenever its view limits / position / dpi change,
  bumping a `u64` generation counter. Artists that cache a `TransformedPath` store the
  generation they cached at and recompute when it differs. No weakrefs, no callbacks.
- **Recompute on demand.** Transforms are cheap (a few 3×3 multiplies); just rebuild
  `transData` each draw from `(viewLim, position, dpi, scale)`. For interactive wasm
  this is fine; optimize only if profiling says so.

Keep: `Affine2D` (6 floats), composition as matrix product, the affine/non-affine split
(`Scale` enum for log/symlog), blended transforms (different transform per axis), and
the **coordinate chain** `data→axes→figure→display` built explicitly per Axes
(§4.3). Make `_set_lim_and_transforms` a trait method so polar/custom projections can
override it.

### 11.5 pyplot → Rust

Keep the **figure registry + current-figure/current-axes** semantics (`Gcf`, §7.1) as a
`thread_local!`/`OnceCell` `FigureRegistry`, and the **delegation pattern** (`plt::plot`
→ current axes). Since rizzma uses strong ownership (no GC), figures are dropped
explicitly (`plt::close(fig)`); that's actually *cleaner* than matplotlib's atexit
dance. Discard ion/ioff/displayhook/pause/IPython. In wasm, "current figure" maps to a
canvas the host page binds; `show()` becomes "render into the bound canvas."

### 11.6 Wasm / canvas is just another backend

Concretely, implement `impl Renderer for CanvasRenderer` where each method calls the
Web Canvas2D API via `web-sys`/`wasm-bindgen`:

- `draw_path` → build a `Path2D` from the matplotlib `Path` (moveTo/lineTo/
  quadraticCurveTo/bezierCurveTo/closePath), `setTransform(affine)`, then `fill`/`stroke`
  with the `GraphicsContext`'s paint, line width, dashes, clip.
- `draw_image` → `putImageData` / `drawImage`.
- `draw_text` → either `fillText` (fast, host-fonts, no SVG export) **or** the
  glyph-outline path (cosmic-text → `Path` → `draw_path`, for pixel-exactness and
  vector export).
- Events: wire DOM `mousemove`/`mousedown`/`keydown`/`resize` to your `MouseEvent`/
  `KeyEvent` with the **y-flip** and button remap exactly as WebAgg does
  (`backend_webagg_core.py:309`).

The **alternative wasm path** is to render with `tiny-skia` into an RGBA `Pixmap`
entirely in Rust, then `putImageData` the buffer into the canvas once per frame
(optionally with WebAgg's diff trick, §5.4). This gives **identical output across PNG
export and the browser** — a strong argument for making `tiny-skia` the primary
renderer and treating `<canvas>` as a dumb blit target, with a Canvas2D-native renderer
as an optional faster path.

Either way, the structural claim holds: **the OO layer is unaware of the target**; only
the `Renderer` impl differs. That is the property to protect above all others in the
port.

### 11.7 Suggested build order for rizzma

1. `Path`, `Affine2D`, `Bbox`, the coordinate-chain math (§3.5, §4).
2. `Renderer` trait + a `tiny-skia` impl producing PNG; `GraphicsContext`.
3. Minimal Artist set: `Figure`, `Axes` (arena-owned), `Line2D`, `Patch`, `Text`,
   with `draw` recursion sorted by zorder.
4. `Axis`/`Tick`/`Spine` + a tick `Locator`/`Formatter` (deferred to a later doc) +
   text metrics via `cosmic-text`.
5. `RcParams` (typed) + a default style.
6. GridSpec arithmetic + `add_subplot`; then tight-layout.
7. `pyplot` façade over a `FigureRegistry`.
8. Wasm: `impl Renderer` for canvas (blit `tiny-skia` `Pixmap`), DOM event bridge.
9. Optional: SVG renderer (validates the renderer abstraction holds for vector output),
   constrained layout, collections/images fast paths, picking.

---

### Appendix A — Key file/line index

| concept | location |
| --- | --- |
| Artist base / init / draw | `artist.py:110`, `:193`, `:1044` |
| stale / stale_callback | `artist.py:316`, `:102` |
| set/update/ArtistInspector/kwdoc | `artist.py:1317`, `:1280`, `:1490`, `:1915` |
| FigureBase/Figure/SubFigure | `figure.py:118`, `:2431`, `:2223` |
| Figure.draw | `figure.py:3264` |
| Figure transforms (bbox/transFigure/dpi) | `figure.py:2637`–`2645` |
| Axes transforms (transData chain) | `axes/_base.py:935`–`969` |
| Axes.draw | `axes/_base.py:3296` |
| RendererBase + draw_* | `backend_bases.py:134`, `:175`, `:438` |
| GraphicsContextBase | `backend_bases.py:701` |
| FigureCanvasBase / Manager / _Backend | `backend_bases.py:1709`, `:2696`, `:3625` |
| Event hierarchy | `backend_bases.py:1178`–`1497` |
| Agg renderer/canvas | `backends/backend_agg.py:59`, `:418` |
| WebAgg core (diff, events) | `backends/backend_webagg_core.py:155`, `:252`, `:285` |
| BackendRegistry | `backends/registry.py:15`, `:414` |
| Path | `path.py:24`, codes `:82` |
| TransformNode / Affine2D / composites | `transforms.py:82`, `:1941`, `:2391`/`:2505` |
| GridSpec.get_grid_positions / SubplotSpec.get_position | `gridspec.py:145`, `:658` |
| LayoutEngine / Tight / Constrained | `layout_engine.py:32`, `:137`, `:216` |
| Gcf registry | `_pylab_helpers.py:9`, `:31` |
| gcf/gca/figure/switch_backend | `pyplot.py:1146`, `:2929`, `:900`, `:391` |
| RcParams / validators / styles | `__init__.py:685`, `rcsetup.py:999`, `style/core.py` |
| C++ Agg renderer / FT2Font / _path | `src/_backend_agg.h:108`, `src/ft2font.h:102`, `src/_path.h` |
