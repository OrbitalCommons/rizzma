# Twin & Secondary Axes Design Note

> Status: implemented (issue #220). The short design note that issue asked for,
> recording the mechanism and its tradeoffs.

## The problem

matplotlib's `twinx()` gives two y-scales over one shared x mapping; its
`secondary_xaxis()` labels the same pixel span in converted units. matplotlib
implements sharing with shared-transform *objects*; rizzma's axes are
index-addressed values owned by the `Figure`, so object sharing is unavailable
by design (see doc 01 §11.3).

## Mechanism: figure-resolved x-limit override

A twin is an ordinary `Axes` at the same figure-fraction position, configured
with a transparent background, no frame, a hidden x axis, and a **right-side**
y axis (the `Axis` type already renders all four `AxisSide`s). It stores only
`xlim_link: Some(source_index)`.

The link is resolved **by the figure at use time**, never copied: every path
that turns data into pixels — `Axes::draw`, `Figure::pixel_to_data`,
`Figure::data_to_pixel`, the `Interactor`'s transform — accepts an optional
x-limit override, and `Figure::xlim_override_for(idx)` supplies the source's
*effective* x-limits. Consequences:

- Later `set_xlim` or autoscale changes on the source track automatically
  (there is no stale copy to invalidate).
- The twin's own x data never influences its x mapping; its y side is entirely
  its own (autoscale, scale type, limits).
- A dangling or self-referential link degrades to "no override" rather than
  panicking.

## Draw order & hit-testing

The twin is pushed after its source, so it draws on top (transparent
background keeps the source visible) and `Figure::axes_at` resolves the
overlapping region to the **twin** (topmost-wins). Interaction therefore
drives the twin's stored limits; its y responds normally, while its x is
re-asserted from the source on the next resolve — i.e. pan/zoom in x
effectively acts only through the source. v1 accepts this; linking
interaction writes back to the source is a follow-up if it ever matters.

## Secondary x axis

`Axes::secondary_xaxis_linear(scale, offset, label)` covers the affine case
(unit conversions — the only case found in the downstream triage). It stores
the coefficients and draws a top-side `Axis` at draw time with
`data_lim = (scale * xlo + offset, scale * xhi + offset)`; the tick machinery
handles reversed ranges, so negative scales work. Arbitrary
forward/inverse *functions* (matplotlib's general form) would break the
`Axes`' derivability with boxed closures and are deferred until a non-affine
need appears.

## Known limits (v1)

- The twin's x scale is linear; `twinx` over a log-x source is untested and
  unsupported (the override carries raw data limits; the twin would need the
  source's scale too).
- `twiny` (shared y, twin x on top) is the mirrored construction and lands
  when something needs it.
- Legends are per-axes; a combined legend across a twin pair is the caller's
  manual `legend(entries)` today.
