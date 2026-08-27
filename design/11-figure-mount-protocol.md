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

**The child does no figure work until told to.** It has no network, no storage,
and no knowledge of the host. It does import the loader during bootstrap (§3.1),
but touches no artifact and instantiates no renderer until `mount` arrives, and
everything it eventually draws came through `postMessage` from a parent that had
already verified it.

## 3. Bootstrap, then transport

### 3.1 The bootstrap is not part of the protocol

The child cannot speak the protocol until it *has* the protocol, so installing
it is a separate, one-shot step with its own shape. The realm starts as a tiny
**host-owned inline bootstrap** — not rizzma code — whose entire job is to
receive one message and then get out of the way.

```ts
// The single `window` message the bootstrap accepts. Not an Envelope.
type Boot = {
  kind: "rizzma-boot";
  nonce: string;          // 128-bit, hex
  loader: string;         // already parent-verified JS source (§4)
  port: MessagePort;      // transferred: the child's end of the channel
};
```

The bootstrap accepts it only if `event.source === window.parent`, the value is
a plain object with exactly these keys, `nonce` is 32 lowercase hex characters,
`port` is exactly one transferred `MessagePort`, and no boot has already been
accepted. It **detaches its `window` listener on both success and terminal
failure**, so a second boot cannot arrive either way.

`targetOrigin` must be `"*"`, because an opaque origin has no origin to target.
**`event.source === window.parent` is what authenticates the boot** — the nonce
cannot, since it arrives *inside* the message it would be authenticating. The
nonce's job starts afterwards: it binds every message on the channel.

The bootstrap materialises `loader` as a **child-local blob module** and imports
it. It is source, not a URL: a URL the child fetches is a fetch the child
initiates, and an opaque-origin child has no same-origin host route to fetch
from anyway. An earlier draft of this document offered a host-served route as an
alternative; that was wrong and is withdrawn.

`ready` then means **the protocol is installed** — not that a renderer exists.
The renderer arrives with `mount`.

### 3.1.1 The realm policy this assumes

§2 claims the child has no network and no storage. That is only true if the host
creates the realm that way, so it is a requirement rather than a description:

- `sandbox="allow-scripts"` and **nothing else** — no `allow-same-origin` (which
  would hand back an origin and with it storage and same-origin fetch), no
  `allow-forms`, `allow-popups`, `allow-top-navigation`, or `allow-downloads`.
- CSP `default-src 'none'`, `connect-src 'none'`, and `script-src` limited to
  the hash-authorized inline bootstrap plus `blob:` for the verified modules.
  `img-src`, `style-src`, `font-src`, and `media-src` stay `'none'`: a figure is
  drawn into a canvas from bytes, and needs to load nothing.
- No child-created workers unless a later revision needs them, in which case
  they are added deliberately rather than inherited.

`wasm-unsafe-eval` may be required in `script-src` for WebAssembly compilation
depending on browser and loader; scope it to this document, never host-wide.

### 3.2 The channel

Everything after the bootstrap is on the transferred `MessagePort`, so
`event.origin` — `null` for an opaque origin, and therefore useless as an
authenticator — never has to be the check.

```ts
type Envelope<T> = {
  nonce: string;      // must equal the boot nonce
  seq: number;        // safe positive integer, strictly increasing per direction
  re?: number;        // the `seq` this answers; absent on unsolicited events
  body: T;
};
```

- `seq` is **strictly increasing per direction**. `MessagePort` preserves
  ordering, so a `seq` that is not greater than the last seen is a duplicate or
  a replay: **drop it silently**.
- `re` is carried by `mounted`, `resized`, `disposed`, `superseded`, control
  acknowledgements (`state` answering `pause`/`resume`/`seek`), and every error
  caused by a request. It is **absent** on `ready`, on throttled progress
  `state`, and on natural-completion `state`. A parent correlates on `re`, never
  on arrival order.
- **Nonce mismatch: dropped silently.** No reply, no log to the peer — answering
  would distinguish a wrong guess from a wrong shape.
- A structurally malformed envelope that *does* carry the right nonce gets
  `error{code:"illegal"}` — but at most a small bounded number of them per
  realm, after which the child stops replying, so a malformed-message loop
  cannot be turned into an amplifier.

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
  | { t: "mount";   artifact: ArrayBuffer;          // transferred, not copied
                    renderer: { wasm: ArrayBuffer; glue: string };
                    view: View;
                    limits: Limits;                 // the FULL inspector budget
                    autoplay: boolean }
  | { t: "pause" }
  | { t: "resume" }
  | { t: "seek";    time: number }                  // seconds
  | { t: "resize";  view: View }
  | { t: "dispose" };

type View   = { cssWidth: number; cssHeight: number; dpr: number };
type Limits = { maxTotalBytes: number; maxChunks: number; maxJsonBytes: number;
                maxPosterBytes: number; maxCanvasPixels: number };
```

`limits` is the **whole** inspector budget, not a subset. Passing two of five
fields would let parent and child disagree about what is acceptable, which is the
one thing a defense-in-depth check must not do — the child would be enforcing a
different policy than the one the parent already applied.

**Numbers are validated before use, and the rule differs by kind** — an earlier
draft said everything is rejected, which contradicted `seek` being clamped:

- **Non-finite is always rejected.** `NaN` and infinities are
  `error{code:"illegal"}` wherever they appear.
- **Dimensions and budgets are rejected out of range.** `cssWidth`/`cssHeight`
  must be finite and non-negative, `dpr` strictly greater than zero,
  `maxChunks` and every byte and pixel limit a safe non-negative **integer**.
  Silently rendering at a size nobody asked for is worse than refusing.
- **`seek.time` is clamped** to `[0, duration]` once finite, because a time
  outside an animation has an obvious intended meaning and refusing a scrub that
  overshoots by a pixel would be hostile. This matches `Timeline::normalize`,
  which is what actually evaluates it.
- `dpr` and canvas dimensions are clamped against the pixel budget **before**
  multiplying, so an absurd `dpr` cannot overflow into a plausible-looking
  product.

`artifact` is **transferred**, which detaches the parent's buffer. The parent
must therefore keep its own durable copy (and its extracted poster) rather than
treating the transferred buffer as its retained artifact.

`autoplay` is the parent's decision at mount time, gated on visibility, focus,
and its active-renderer budget — not a property of the artifact.

### Child → parent

```ts
type ToParent =
  | { t: "ready";    protocol: 1 }                          // protocol installed
  | { t: "mounted";  widthPx: number; heightPx: number;
                     animated: boolean; duration: number;   // seconds; 0 if static
                     playing: boolean }
  | { t: "error";    code: ErrorCode; message: string }
  | { t: "state";    playing: boolean; time: number }
  | { t: "superseded"; by: number }                         // re = superseded seq
  | { t: "resized";  widthPx: number; heightPx: number; dpr: number }
  | { t: "disposed" };

type ErrorCode =
  | "artifact"   // malformed, over-budget, or unsupported schema
  | "renderer"   // glue import or wasm instantiation failed
  | "illegal"    // not legal in this state, or a malformed/invalid message
  | "internal";
```

`state` deliberately carries **no timestamp**. An earlier draft sent the child's
`performance.now()`, which is unusable: separate realms have separate time
origins, so the number means nothing in the parent's frame of reference. The
parent anchors each sample at **its own receipt time** instead — sound here
because the channel is in-process with no network in the path, and because a
4 Hz resync bounds how far an interpolated playhead can drift before it is
corrected. An epoch-aligned timestamp (`timeOrigin + now()`) with a negotiated
offset would be more precise and is not worth the machinery at this cadence.

**`error.message` is for a human reading a log, never for a parser.** It is
capped (256 chars) and must never echo artifact bytes, JS source, URLs, stack
traces, or anything from the host's environment. Hosts branch on `code`.

## 6. States

```
  boot        mount ──────────────┐
Booting ─► Ready ─► Mounting ─► Playing ⇄ Paused
                       │            │        │
                       │ fails      └────────┴──► Disposing ─► Disposed
                       └──► Failed ─────────────────────┘
```

| in state | legal | effect |
|---|---|---|
| `Booting` | — | emits `ready` once the protocol is installed |
| `Ready` | `mount`, `dispose` | `mount` → `Mounting` |
| `Mounting` | `pause`, `resume`, `resize`, `dispose` | `pause`/`resume` latch the desired state; `resize` is latest-wins; → `Playing`/`Paused` (emits `mounted`) or → `Failed` (emits `error`) |
| `Playing` | `pause`, `seek`, `resize`, `dispose` | `pause` → `Paused` |
| `Paused` | `resume`, `seek`, `resize`, `dispose` | `resume` → `Playing` (static figures stay `Paused`) |
| `Failed` | `dispose` | nothing else; the realm is not reusable |
| `Disposing` | — | emits `disposed`, then `Disposed` |
| `Disposed` | — | **every message ignored, no reply** |

`Mounting` exists because mounting is genuinely asynchronous — a blob import, a
wasm compile, an artifact parse, a first render — and a state machine that
pretends otherwise leaves a window in which the parent cannot tell what is legal.

**`pause` and `resume` are legal during `Mounting`**, and this is not a
convenience. A figure that was visible and focused when the parent decided to
mount can be scrolled away or unfocused while wasm compiles. If `pause` were
illegal there, the parent's only correct move — pause it — would draw
`illegal`, and `illegal` means dispose; the alternative is an animation that
starts playing in a hidden frame. So during `Mounting` they set a **desired-play
latch**, acked immediately, and the eventual `mounted` reports the state the
latch settled on. `resize` is latest-wins for the same reason. Losing the
active-renderer *slot* may still end in `dispose` — that is a real decision, not
a race — but ordinary visibility churn must not turn into protocol failure.

`dispose` is legal throughout and cancels what can be cancelled; anything else
during `Mounting` is `illegal`.

**Failure cleans up before it reports.** A mount that fails partway must release
whatever it already allocated — blob URLs, listeners, a partially initialised
module — *then* emit `error` and enter `Failed`. An internal failure from
`Playing` or `Paused` likewise transitions to `Failed`, after which the parent's
only move is `dispose`.

Anything not listed is illegal: the child replies `error{code:"illegal"}` and
**does not change state**. A parent that receives `illegal` has a version or
logic bug; it should dispose rather than retry.

`Disposed` is terminal, and this is the rule most likely to be "helpfully"
relaxed later: there is no `mount` that revives a realm. Reuse would demote
disposal to a strong pause and let anything realm-scoped — the loader's
digest-keyed module cache, most obviously — leak from one artifact to the next.
If pooling is ever worth its complexity it arrives as an explicit `reset`
carrying an argument that every prior resource is gone.

**Exactly one `mount` per realm.** With terminal disposal, a realm's whole life
is one artifact, which is what makes "no cross-artifact state" checkable rather
than aspirational.

## 7. Animation

A portable figure's animation is a **timeline evaluable at any `t`**, not a frame
sequence (`design/10` §5). That is what makes these controls meaningful: `seek`
is exact rather than approximate, and a paused figure at `t` is a full-quality
render of that instant rather than a held frame.

**The child owns the `requestAnimationFrame` clock while playing**; the parent
owns *whether* it plays. A per-frame message round trip would put the parent's
event loop in the frame path.

**The parent must `pause`** on viewport exit, document hidden, and session-view
unfocus. The last is the non-obvious one: a portal keeps every activated session
mounted and merely CSS-hides the unfocused ones, so nothing else will tell the
child to stop (`design/10` §12c).

### Observability without a message per frame

A parent driving a scrubber needs the playhead, and the child owns it. So:

- `state` is emitted on **every control transition** (with `re`), on **natural
  completion** (unsolicited), and as **throttled progress while playing** —
  at most **4 Hz**, unsolicited. Never per frame.
- Between updates the parent interpolates from **its own receipt time** for the
  last `state`, and **resyncs to the next `state` rather than to its own
  accumulated estimate**.

### Ending, looping, and static figures

- A non-looping timeline that reaches `duration` **stops the rAF loop, clamps
  `t = duration`, and emits `state{playing:false, time:duration}`**.
- `resume` at the end **restarts from 0**. The alternative — requiring an
  explicit `seek` first — makes the obvious gesture (press play again) do
  nothing, which reads as a bug.
- A looping timeline wraps and does not emit a completion `state`.
- A **static figure mounts `Paused`**, and acknowledges `pause`/`resume`/`seek`
  with `state` **without ever scheduling a frame**.

### Resolution while moving

At DPR 3 a 640×480 canvas is ~11 MB per RGBA frame, and `putImageData` — not the
runtime download — is the plausible bottleneck. The child caps effective DPR and
backing-store pixels while animating or interacting, then repaints once at full
quality when motion settles. That belongs in the renderer so it is one policy
rather than every host rediscovering it.

### Sizing

The parent sizes the outer box to the natural aspect it read from `inspect`. The
child **still letterboxes** inside whatever box it is given and never distorts
equal-aspect axes — belt and braces against rounding and responsive constraints,
since a figure that is silently 3% stretched is a wrong figure. `resized`
reports the dimensions actually applied.

### Scrub backpressure

Both ends, because they solve different halves.

The **parent coalesces** pointer movement to at most one `seek` per parent frame.
Those are never sent, so they need no acknowledgement.

The **child is latest-wins**: a `seek` arriving while a render is in flight
supersedes any earlier pending one. An earlier draft said the winning `state`
acknowledged the superseded requests too — it cannot, because an envelope
carries exactly one `re`. Each superseded request gets its own explicit reply:

```ts
| { t: "superseded"; by: number }   // re = the superseded seq
```

so every `seek` that was actually sent is answered exactly once, and the final
`state` carries `re` of the winning seq. Silence would leave a parent unable to
distinguish "replaced" from "lost".

## 8. What `dispose` must actually free

A host cannot enforce an active-renderer budget of two if disposal only stops
drawing. Before emitting `disposed`, the child must:

- cancel any pending `requestAnimationFrame` and every timer;
- remove every DOM listener it added (pointer, wheel, resize, visibility);
- call the **explicit free** on the wasm-owned `Figure` and session, and drop
  every JS reference to them;
- **set the canvas `width` and `height` to 0**, which is what actually releases
  the backing store — clearing pixels does not;
- revoke every blob URL it created (§3.1).

**What this does not promise.** An earlier draft said dropping the wasm-owned
objects meant "the module's allocations are reclaimed". JavaScript cannot
promise that: GC is nondeterministic, and emitting `disposed` says only that
the child released everything it holds and cancelled everything it scheduled.
The explicit `free` calls matter precisely because dropping a reference is not
a deallocation. **Realm destruction by the parent is the hard reclamation
boundary** — the only step that actually returns the memory.

So the parent destroys the realm after `disposed`, or after a short timeout if
it never arrives. A child that cannot dispose cleanly must not be able to keep
its realm alive by failing to answer.

## 9. Decisions recorded

Settled in review; listed because each was live and someone will wonder.

| question | answer |
|---|---|
| glue as blob source or host-served route? | **blob source only.** A host route means the child fetches, which contradicts no-network — and an opaque origin is not same-origin to it anyway |
| letterbox in child, or parent sizes the box? | **both.** Parent sizes to the natural aspect from `inspect`; child still letterboxes and never distorts equal-aspect axes, against rounding and responsive constraints |
| scrub backpressure in parent or child? | **both.** Parent coalesces to ≤1 `seek` per frame (never sent, so never acked); child is latest-wins, replying `superseded` to each replaced request |
| should `error{code:"artifact"}` be reachable? | **yes, and distinctly.** The parent validated first, so seeing it in production is an invariant violation worth alerting on — folding it into `internal` would hide that |
| `resume` at the end of a non-looping timeline? | **restarts from 0.** Requiring an explicit `seek` first makes pressing play do nothing, which reads as a bug |

## 10. A standing constraint on schema 3

The timeline lands as schema 3 (`design/10` §5). **A schema-3 artifact stays
poster-only on any host whose vetted runtime manifest advertises
`schema_max < 3`** — which is every host until one registers a runtime that
declares otherwise. That is the versioning rule doing its job rather than an
obstacle: an animated figure rendered by a runtime that cannot animate would be
a still, wrong figure presented as a live one.
