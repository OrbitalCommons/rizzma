# 11 — The figure mount protocol

Status: **proposed, for review before either side implements.** This is the
contract between a host page (the *parent*) and the sandboxed realm that draws a
portable figure (the *child*). It is deliberately written before code exists on
either side, because an origin or lifecycle assumption is far cheaper to fix now
than to unwind from two codebases later.

Companion: `design/10-portable-figure.md` — §12b fixes where validation lives and
what `Dispose` means, §12c the workload this has to survive, §9 renderer
selection. Those decisions are settled and are not reopened here.

## 1. Scope

Covers: the message types, the state machine, animation control, and the
resources `Dispose` must release.

Does **not** cover: how a host stores artifacts, authenticates users, or decides
which figures are live — all host concerns. Nor renderer *selection*, which
happens before any of this (§4).

## 2. Two properties everything else follows from

**Validation happens outside the realm.** The parent inspects the artifact,
enforces its budgets, resolves a vetted runtime whose declared schema range
covers the artifact, and decides interactive-versus-poster **before creating the
child**. A malformed, over-budget, or unsupported artifact therefore never
causes a realm to exist. The child still fails safely on bad bytes — that is
defense in depth, not where the safety comes from.

**The child is inert until spoken to.** It has no network, no storage, no
knowledge of the host, and nothing to do until a `Mount` arrives. Everything it
draws came through `postMessage` from a parent that had already verified it.

## 3. Transport

A dedicated `MessageChannel`. The parent creates it, keeps `port1`, and transfers
`port2` to the child in the bootstrap message. All subsequent traffic is on that
channel, so `event.origin` — which is `null` for an opaque origin and therefore
useless as an authenticator — never has to be the check.

Every message additionally carries a `nonce`, a 128-bit random value minted per
realm by the parent and delivered once in the bootstrap. **A message whose nonce
does not match is dropped silently** — no reply, no log to the peer, because
answering distinguishes a wrong guess from a wrong shape.

```ts
type Envelope<T> = { nonce: string; seq: number; body: T };
```

`seq` is monotonic per direction. Its only job is correlating a reply with its
request; a reply carries the `seq` it answers in `re`.

## 4. Before the child exists

Not part of the protocol, but the protocol is unsound without it, so it is
stated as a precondition:

1. Fetch `runtime.json`; verify its digest against the one the host's registry
   pins out of band.
2. Parse it; verify every asset in it (`renderer`, `glue`, `loader`) against its
   declared size and `sha256`.
3. Inspect the artifact (`rizzma::portable::inspect`) under host `Limits`; read
   `schema`, `schema_supported`, `meta`, and the poster.
4. If not renderable, or the host is at its active-renderer budget, **show the
   poster and stop**. No realm.
5. Otherwise create the realm and send the bootstrap.

The child never verifies the loader it is running. It cannot: it would be
verifying the verifier. That is why steps 1–2 are the parent's.

## 5. Messages

### Parent → child

```ts
type ToChild =
  | { t: "mount";   artifact: ArrayBuffer;      // transferred, not copied
                    renderer: { wasm: ArrayBuffer; glue: string };
                    view: { cssWidth: number; cssHeight: number; dpr: number };
                    limits: { maxBytes: number; maxCanvasPixels: number };
                    autoplay: boolean }
  | { t: "pause" }
  | { t: "resume" }
  | { t: "seek";    time: number }              // seconds, clamped to duration
  | { t: "resize";  cssWidth: number; cssHeight: number; dpr: number }
  | { t: "dispose" };
```

`renderer.glue` is a string of already-verified JavaScript rather than a URL,
because a URL the child fetches is a fetch the child initiates. How the child
turns it into a module — a blob URL it revokes on dispose, or a host-served
content-addressed route under the sandbox CSP — is the host's decision (§8).

`limits` is passed even though the parent already enforced it: the child applies
it again on its own parse, which is the defense-in-depth half of §2.

### Child → parent

```ts
type ToParent =
  | { t: "ready";    protocol: 1 }
  | { t: "mounted";  widthPx: number; heightPx: number;
                     animated: boolean; duration: number }   // seconds; 0 if static
  | { t: "error";    code: ErrorCode; message: string }
  | { t: "state";    playing: boolean; time: number }        // ack for pause/resume/seek
  | { t: "disposed" };

type ErrorCode =
  | "artifact"        // malformed, over-budget, or unsupported schema
  | "renderer"        // wasm failed to instantiate
  | "illegal"         // message not legal in the current state
  | "internal";
```

`error` carries a message for a human, never for a parser: hosts branch on
`code`.

## 6. States

```
        bootstrap                mount                  dispose
Booting ─────────► Ready ────────────────► Mounted ─────────────► Disposed
                     │                     ▲  │  ▲                    ▲
                     │  mount fails        │  │  │ resume             │
                     └────────────────► Failed  └─┴─ pause ─► Paused ─┘
                                          │                     │
                                          └──── dispose ────────┘
```

| in state | legal | effect |
|---|---|---|
| `Booting` | — | child emits `ready` when its runtime is instantiated |
| `Ready` | `mount`, `dispose` | `mount` → `Mounted` (emits `mounted`) or `Failed` (emits `error`) |
| `Mounted` | `pause`, `seek`, `resize`, `dispose` | `pause` → `Paused`; `seek`/`resize` stay, emit `state` |
| `Paused` | `resume`, `seek`, `resize`, `dispose` | `resume` → `Mounted`; others stay |
| `Failed` | `dispose` | nothing else; the realm is not reusable |
| `Disposed` | — | **every message is ignored, with no reply** |

Anything not listed is illegal: the child replies `error{code:"illegal"}` and
**does not change state**. A parent that receives `illegal` has a bug and should
dispose rather than retry.

`Disposed` is terminal, and this is the rule most likely to be "helpfully"
relaxed later: there is no `mount` that revives a realm. Reuse would demote
disposal to a strong pause and let anything realm-scoped — the loader's
digest-keyed module cache, most obviously — leak from one artifact to the next.
If pooling is ever worth its complexity it arrives as an explicit `reset`
carrying an argument that every prior resource is gone, never as a relaxation of
`dispose`.

**Exactly one `mount` per realm.** Combined with terminal disposal, that makes a
realm's whole life one artifact, which is what makes "no cross-artifact state"
checkable rather than aspirational.

## 7. Animation

A portable figure's animation is a **timeline evaluable at any `t`**, not a frame
sequence (`design/10` §5). That is what makes these controls meaningful: `seek`
is exact rather than approximate, and a paused figure at `t` is a full-quality
render of that instant rather than a held frame.

- `mounted` reports `animated` and `duration`, so a parent knows whether to show
  transport controls at all without parsing the artifact itself.
- **The child owns the clock while playing**, driving its own
  `requestAnimationFrame` loop, because a per-frame message round-trip would put
  the parent's event loop in the frame path. The parent owns *whether* it plays.
- `pause` stops the loop and frees nothing. `resume` restarts from the current
  `t`. `seek` sets `t` and repaints once, legal in both states.
- The parent must `pause` on: the figure leaving the viewport, the document
  becoming hidden, and the session view losing focus. The last one is the
  non-obvious one — a portal keeps every activated session mounted and merely
  CSS-hides the unfocused ones, so nothing else will tell the child to stop
  (`design/10` §12c).
- A static figure ignores `pause`/`resume`/`seek` beyond acking with `state`.

**Reduced resolution while interacting** is the renderer's job, not the host's:
at DPR 3 a 640×480 canvas is ~11 MB per RGBA frame, and `putImageData` — not the
runtime download — is the plausible bottleneck. The child caps effective DPR
during animation and pan/zoom and repaints once at full quality when motion
settles. That policy lives in one place rather than in every host.

## 8. One decision left to the host

Whether verified glue arrives as source imported via a blob URL, or is served
from a host-owned content-addressed route:

| | blob URL | host route |
|---|---|---|
| CSP | needs `blob:` in `script-src` | `'self'`-shaped, tighter |
| cleanup | must `URL.revokeObjectURL` on dispose | nothing to revoke |
| fetch | none — bytes already in the realm | child fetches, so the route must be same-origin to the child |

Both satisfy §2. The tradeoff is CSP shape against cleanup obligation, and the
host owns both, so the host picks. The child accepts either: `renderer.glue` is
a string, and how it is materialised is decided at the boundary.

## 9. What `dispose` must actually free

A host cannot enforce an active-renderer budget of two if disposal only stops
drawing. Before emitting `disposed`, the child must:

- cancel any pending `requestAnimationFrame` and every timer;
- remove every DOM listener it added (pointer, wheel, resize, visibility);
- drop the wasm-owned `Figure` and session so the module's allocations are
  reclaimed, not merely unreferenced;
- **set the canvas `width` and `height` to 0**, which is what actually releases
  the backing store — clearing pixels does not;
- revoke any blob URLs it created (§8).

The parent destroys the realm after `disposed`, or after a short timeout if it
never arrives. A child that cannot dispose cleanly must not be able to keep its
realm alive.

## 10. Open questions

1. **Resize semantics under `aspect_equal`.** A figure with equal-aspect axes has
   an intrinsic shape; a host box may not match it. Letterbox in the child, or
   report the natural size and let the parent size the box? Leaning the latter,
   since the parent already gets exact pixel dimensions from `inspect`.
2. **Backpressure on `seek`.** A scrub gesture can emit seeks faster than frames
   render. Coalesce in the child (drop all but the newest) or rate-limit in the
   parent? Child-side coalescing is one implementation instead of every host's.
3. **Whether `error{code:"artifact"}` should ever be reachable** given §4
   validates first. It should stay reachable as defense in depth, but a host
   seeing it in production has a validation bug worth alerting on, which argues
   for it being distinguishable from the others rather than folded into
   `internal`.
