# 08 — Styling (`RcParams` wire-up)

Status: **implemented** (rizzma 1.4.0).

## Goal

Support a wide variety of figure styling (themes, dark mode, custom grids,
color cycles, tick direction, legend colors) while keeping the built-in default
look byte-for-byte unchanged.

## Design

A single principle: **`Figure` owns an `RcParams`; each `Axes`/`Axis`/legend is
seeded from it at creation and reads its own resolved fields at draw time.** No
decoration ink is hardcoded in the render path.

`RcParams` (`core/rcparams.rs`) already existed as a typed, serde-able mirror of
matplotlib's `rcParams`, but nothing consumed it. The wire-up makes it the live
source of truth and adds the missing fields (`legend_facecolor`,
`legend_edgecolor`, `legend_labelcolor`, `text_color`).

### Flow

```
Figure { rc: RcParams }
  └─ add_axes / add_subplot / twinx
        └─ Axes::apply_rcparams(&rc)   // seeds face/edge/title/cycle/legend
              └─ Axis::{set_color, set_grid, set_grid_style, set_tick_direction}
```

`Figure::with_rcparams` / `set_rcparams` adopt a config (and re-seed existing
axes); `pyplot::style` is the stateful analog of `plt.style.use`. The default
`RcParams` reproduces the previous constants exactly, so the default render is
unchanged — verified by a cross-branch byte-compare of the full gallery and a
`default_rcparams_matches_implicit_default` test.

### Hardcode → field map

| Site (pre-1.4) | Now driven by |
|----------------|---------------|
| axes frame stroke `BLACK`/`0.8` | `Axes.edgecolor` / `linewidth` ← `axes_edgecolor`/`axes_linewidth` |
| axes title ink `BLACK` | `Axes.title_color` ← `text_color` |
| grid ink `rgb(0.85)` | `Axis.grid_color`/`grid_linewidth`/`grid_alpha` ← `grid_*` |
| tick direction (always out) | `Axis.tick_direction` ← `xtick/ytick_direction` |
| property cycle (global const) | `Axes.prop_cycle` ← `axes_prop_cycle` |
| legend box `WHITE`/`0.5`/`BLACK` | `Axes.legend_{facecolor,edgecolor,labelcolor}` ← `legend_*` |

### Public API added

- `Figure::with_rcparams` / `set_rcparams` / `rcparams`
- `Axes::grid`, `grid_with`, `set_edgecolor`, `set_linewidth`, `set_title_color`,
  `set_axis_color`, `set_prop_cycle`, `xaxis_mut`, `yaxis_mut`
- `Axis::set_color`, `set_grid`, `set_grid_style`, `with_tick_direction`,
  `set_tick_direction`, `color`, `tick_direction`
- `pyplot::style`
- crate root re-exports `RcParams`, `TickDirection`
- `RcParams::dark()` preset

## Not yet done (follow-ups)

- **Font families** (`font_family`): `FontSource` is DejaVu-only; alternate
  families need font embedding/loading.
- **Spine visibility / placement** (drop top/right spine for the seaborn look):
  entangled with the frame rectangle; needs frame-vs-spine separation.
- **Minor ticks / minor grid**, dashed grid line styles, per-cycle linestyle +
  marker cycling.
- More presets (seaborn-whitegrid, solarized, high-contrast, brand themes) —
  each is just an `RcParams` constructor and composes via `merge`.
