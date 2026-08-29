# 12 — Controls: sliders other than time (schema 4)

Status: **implemented with this document.** Companion to `design/10-portable-figure.md`
(the artifact) and `design/11-figure-mount-protocol.md` (the mount contract, whose
animation section §7 this extends).

## 1. What a control is

Schema 3 made one observation: an animation that travels is not code, it is a
**pure function of `t`** over keyframed tracks. A control is the same observation
applied to an axis the *user* drives instead of a clock. A figure declares:

```
Control { label, min, max, default, step?, tracks: [Track], grids: [Grid] }
```

`tracks` are ordinary schema-3 `Track`s whose `times` are read as **positions of
this control** in `[min, max]` — same struct, same validation, same sampling,
because nothing about a keyframe axis is inherently temporal. `step` present
means the control snaps to multiples of `step` from `min` (a detent slider);
absent means continuous.

## 2. Grids: reshaping a figure while it plays

A 1D track answers "what does the figure look like at wavelength λ" — but the
interesting figures are *animated*, and there a slider must compose with the
clock: the wave keeps travelling while its wavelength changes under the user's
finger. That is a function of two variables, and pretending otherwise (slider
overwrites clock, clock overwrites slider on the next frame) produces flicker
that depends on event order — the exact non-determinism the timeline design
exists to forbid.

So a control may also carry `Grid`s:

```
Grid { target, times: [t…], positions: [p…], values, stride, interp }
```

`values` is a lattice, `times.len() × positions.len()` frames of `stride`
elements, sampled **bilinearly** (or by 2D step-hold) at `(t, p)`. A grid binds
one control and the clock on one target. Values outside the lattice clamp, as
schema 3 tracks clamp — extrapolation would invent data the author never drew.

One control per target per grid is deliberate: two sliders reshaping the same
artist is a 3D lattice whose size is the product of both samplings, and the
right answer to "my figure is a function of three variables" is to precompute
less or plot differently, not to grow the wire format combinatorially.

## 3. Evaluation: one pure function, re-applied whole

The figure's displayed state is a pure function of `(t, v₀ … vₙ)` — the clock
and every control's current value. On **any** change (a seek *or* a
`set_control`), the whole function is re-applied:

1. the timeline's tracks at `t` (wrapped/clamped as schema 3 defines),
2. each control in declaration order: its 1D tracks at `vᵢ`, then its grids at
   `(t, vᵢ)`.

Re-application is what makes overlap well-defined: if two drivers address the
same target, the later step wins **always**, not "whichever event fired last".
Event order cannot matter, because no event's effect survives except through
the state vector. This is the same move `Timeline::apply` made against replayed
mutation sequences, one level up.

Control values live on the figure (clamped to `[min, max]`, snapped to `step`,
non-finite input becomes the default — a nonsense slider should not produce a
nonsense figure). They are session state, like the user's pan: the artifact
carries `default`, not the live value, and export snapshots artist data as
drawn — an author exports at defaults because they built it at defaults.

## 4. The user's view still wins

Controls may drive `Xlim`/`Ylim` exactly as timelines may. The interactor wraps
`set_control` in the same snapshot-and-restore it wraps `seek` in: axes the user
has panned or zoomed keep the user's limits until double-click hands the view
back. One policy, one code path, both drivers.

## 5. The mount surface

`readMetadata` gains `controls`: `[{label, min, max, default, step}]` — the
manifest only, no track data, so a host can lay out sliders before spending a
renderer instantiation. The mount handle gains the same `controls` array and
`setControl(index, value)`, which rides the session's rAF-coalesced repaint —
dragging a slider faster than the display refreshes does not stack frames.

**Who draws the sliders: the host.** The renderer draws the figure and nothing
else; a slider is host DOM, styled like the host, laid out by the host, exactly
as §7 gave the host play/pause. The `.riz.html` wrapper is a host and emits its
own sliders; the portal is a host and emits its own.

## 6. Autoplay is the default

Experience with hosts showed the old default (mounted paused, `autoplay` opt-in)
produced figures that arrive dead until a second click. An animated figure's
author animated it on purpose. `mount()` now plays animated figures unless the
host passes `autoplay: false`; the host's §7 obligations (pause on viewport
exit, document hidden, session-view unfocus) are unchanged and are what make
the default safe to hold.

## 7. Schema and budgets

Artifacts with controls are schema 4; `deny_unknown_fields` plus the schema
range check means a schema-3 renderer refuses them loudly and shows the poster,
which is the degradation path working as designed. Grids ride the existing
byte budgets: a lattice is bulk `f64` data like any track, counted by the same
`max_total_bytes` the rest of the artifact answers to.
