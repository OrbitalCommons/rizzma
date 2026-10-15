# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/).

`rizzma` is a single crate. Bumping the version on a push to `main` triggers the
publish workflow (`.github/workflows/publish.yml`), which publishes it to crates.io.

## [Unreleased]

### Added
- **`Collection` `&mut` setters.** `set_marker`, `set_sizes`, `set_facecolors`,
  `set_edgecolors`, `set_linewidth`, `set_zorder` and `set_visible` restyle the
  collection `Axes::scatter` returns in place, without cloning its offsets. The
  `with_*` builders now delegate to them.
- **Scatter series in automatic legends.** `Collection::set_label` / `with_label` /
  `label`; `Axes::legend_auto` keys a labelled collection with one of its markers
  (its shape, first face and edge colors, and the middle of its size range, as
  matplotlib's scatter legend does). Unlabelled and `_`-prefixed collections are
  skipped and shared labels share a row, as for lines and patches.
- **Figure text.** `Figure::text(x, y, s)` places text in figure fractions and returns
  a styleable `Text` (size, color, `HorizontalAlignment` / `VerticalAlignment`,
  rotation, background box), drawn on top of every axes. It aligns by matplotlib
  3.11's layout box, landing within a pixel of matplotlib at 100 and 200 DPI.
  `Figure::supxlabel` / `supylabel` add shared axis labels that tight layout makes
  room for; the pyplot facade gains `figtext`, `supxlabel` and `supylabel`.
- **Raster export in caller pixel formats.** `Figure::to_rgba8` (straight alpha, the
  bytes PNG export stores) and `to_rgb8` return `(pixels, width, height)`;
  `render_into` fills a caller-owned buffer as `PixelFormat::Rgb8`, `Rgba8`,
  `Rgba8Premul`, `Bgrx8` or `Xrgb8888`, and `render_into_u32` a `u32` framebuffer
  (minifb, softbuffer). Opaque layouts composite over the face color; a buffer of the
  wrong length is a `BufferSizeError`, never a panic. `Figure::raster_size` and
  `Figure::facecolor` report what they need.
- **Norms and bad/over/under colors for images, scatter and pcolormesh.** New
  `TwoSlopeNorm`, `CenteredNorm` and `SymLogNorm`, and a `Norm` enum every norm
  converts into. `AxesImage::norm` / `with_norm` (superseding `vmin`/`vmax`) and
  `bad` / `over` / `under`; `scatter_mapped` now takes `impl Into<ColorMapping>`, so a
  colormap name still works and a `ColorMapping` adds `vmin`/`vmax`, a norm and
  extreme colors; `Axes::pcolormesh_mapped` does the same for meshes.
  `Normalize::normalize_unclipped` (a provided method) reports out-of-range data, and
  `map_color` colors one value as matplotlib's `Colormap.__call__` does. Every norm
  matches matplotlib 3.11's `imshow(norm=…)` to within one unit per channel.
- **Colorbars bound to an artist and its axes.** `Axes::colorbar(Mappable::Image(i) |
  Mesh(i) | Collection(i))` attaches a `Colorbar` that reads the artist's colormap and
  norm at every draw and takes its room from that axes alone (`fraction`, `pad`,
  `shrink`, `aspect`, `location` on any side, `label`), as matplotlib's
  `fig.colorbar(mappable, ax=ax)` does; other panels keep their size. Ticks follow the
  norm: decades for `LogNorm`, symlog ticks, ticks placed through a `TwoSlopeNorm`,
  equal regions for a `BoundaryNorm`. Geometry and tick labels match matplotlib 3.11.
  Meshes and mapped collections now remember their `ColorScale`;
  `Figure::axes_frame`, `colorbar_frame` and `colorbar_ticks` report the result.
  `Figure::colorbar(name, vmin, vmax)` stays as the free-standing convenience.
- **Astronomical image normalization.** astropy.visualization's intervals
  (`Interval::MinMax`, `Percentile`, `AsymmetricPercentile`, and IRAF's `ZScale` via
  `ZScaleInterval`) and stretches (`Stretch::Linear`, `Sqrt`, `Power`, `Log`, `Asinh`,
  `Sinh`, and histogram equalization via `Stretch::histeq`), combined in
  `ImageNormalize`, a norm any colormapped artist accepts:
  `imshow(..).norm(ImageNormalize::new(Interval::zscale(), Stretch::Asinh(0.1)))`. The
  interval takes its limits from the data the norm is applied to (`Norm::autoscaled`).
  ZScale limits match astropy 8 to 1e-15 relative, every stretch to 1e-12, and the
  render matches astropy + matplotlib to within one unit per channel.
- **Upper and lower limits on error bars.** `Axes::errorbar_with(x, y, yerr,
  &ErrorbarOptions)` takes matplotlib's `uplims` / `lolims` masks, drawing half the bar
  and a `caretdownbase` / `caretupbase` arrow at its open end (6 points, unfilled) with
  the cap on the value, plus horizontal errors (`xerr`) and their `xuplims` /
  `xlolims` carets. Carets land within a pixel of matplotlib's at 72, 100 and 200
  DPI. `errorbar(x, y, yerr)` is unchanged.
- **Long lines are simplified before drawing**, matplotlib's `PathSimplifier` ported
  line for line (it matches matplotlib's output vertex for vertex): runs of nearly
  collinear segments within `RcParams::path_simplify_threshold` (1/9 px) of each
  other merge, keeping each run's extremes. A noisy 1e6-point line renders in about
  0.6 s instead of 34 s (matplotlib: 0.43 s on the same machine). It applies to lines
  of 1000 or more vertices and can be turned off with `RcParams::path_simplify`; the
  `bench_noisy_line` example measures it.
- **Large uniform scatters render about 15× faster in PNG.** A scatter with one size,
  face and edge color hands its markers to `Renderer::draw_markers` together, and the
  tiny-skia backend rasterizes the marker once per sub-pixel phase (1/64 px) and
  stamps it, compositing exactly as tiny-skia would — matplotlib Agg's
  `draw_markers` approach, without its pixel snapping. 1e6 markers render in about
  0.32 s instead of 5.1 s (matplotlib 3.11: 1.9 s at the same size); per-point styles
  draw as before. The `bench_scatter` example measures it.
- **Sky projections on regular axes.** `Axes::set_sky_projection(SkyProjection::
  Mollweide | Aitoff)` turns an axes into an all-sky map, matplotlib's
  `add_subplot(projection="mollweide")`: data is longitude/latitude in radians and
  every artist projects at draw time, so `plot`, `scatter`, fills, meshes, colorbars
  and legends all work on it. Straight segments are subdivided to follow the
  projection, as matplotlib's geo axes interpolate them. The frame becomes the
  elliptical outline with degree labels on the equator and western edge, and
  `grid(true)` draws the graticule (meridians every 30°, parallels every 15°). Cell
  interiors of a Mollweide `pcolormesh` match matplotlib 3.11 to within one unit per
  channel.
- **A zenithal fisheye projection**, `SkyProjection::Zenithal`: the azimuthal
  equidistant view of the sky overhead, with data as azimuth and altitude. The zenith
  is at the center, the horizon on the unit circle, and north is up with east on the
  left. The outline is the horizon, with N/E/S/W outside it, and `grid(true)` draws
  altitude rings every 30° and azimuth spokes every 45°. Fills, meshes and colorbars
  work as on the all-sky projections, and the standalone `SkyAxes` draws it too.
- **Colour-mapped polygon fills.** `Axes::poly_collection(polygons, c, mapping)` fills
  each polygon with the color of its value through a colormap and norm, as
  matplotlib's `PolyCollection` with `set_array` does. The polygons need not share a
  grid, so it covers HEALPix pixels, survey footprints and Voronoi cells. It returns
  a `PolyCollection` (edge color, width, zorder), and `Mappable::Polygons` gives it a
  colorbar. On a sky projection each edge follows the projection: a Mollweide fill
  matches matplotlib 3.11's inside every cell to within one unit per channel.
- **A migrating-from-plotters guide**, `crates/rizzma/docs/migrating-from-plotters.md`
  (also `rizzma::guide::migrating_from_plotters` on docs.rs), linked from the README.
  It has a concept map from plotters, naming traps for people arriving from
  matplotlib, side-by-side ports of ten idioms, and a worked port of starfield's
  Hipparcos × Gaia plot. Every rizzma snippet runs as a doc test, and a test holds
  each one identical to the gallery case that draws the image beside it. A
  colormaps section covers the `bgyw` default (not viridis), the CET replacements
  for jet, hot and hsv, the `misleading:` prefix for exact reproduction, and the
  silent fallback for unknown names.
- `misleading:jet` joins viridis and coolwarm in the colormap lookup test against
  matplotlib, so the quarantined maps' lookup is checked too.
- **Colormaps of your own.** Every API that takes a colormap now takes a `CmapSpec`
  (a registry name, or a colormap value): `AxesImage::cmap`/`with_cmap`, and through
  `ColorMapping`, `scatter_mapped`, `pcolormesh_mapped`/`pcolormesh_xy` and
  `poly_collection`. Colorbars bound to those artists read the custom map. New
  constructors: `ListedColormap::from_colors` (matplotlib's `ListedColormap`,
  matching its render to within one unit per channel), `ListedColormap::from_fn`
  (a value-to-color function sampled at 256 points, so a hand-rolled ramp keeps its
  exact colors) and `LinearSegmentedColormap::from_list`. `register_colormap(name,
  cmap)` names one for string-only paths (pyplot, wasm), with an `_r` variant. The
  migrating-from-plotters guide's Hipparcos port now reproduces starfield's HSL ramp
  exactly instead of substituting coolwarm.
- **`try_colormap(name)`** returns the map, or an `UnknownColormap` error that
  names what to use instead. For a quarantined vendor map that is both the
  `misleading:` form and its perceptually uniform replacement (`jet`/`rainbow` →
  `misleading:jet` or `cet_r2`, `hot` → `cet_l03`, `hsv` → `cet_c1`); otherwise it is
  the registry names within two edits.
- **`Axes::pcolormesh_xy(x, y, c, mapping)`** draws a mesh over explicit cell edges
  (`ncols + 1` x edges and `nrows + 1` y edges, unevenly spaced if need be), such as
  a longitude/latitude grid on a sky projection.

### Changed
- **An unknown colormap name is reported** once per name per process: on stderr
  natively, `console.warn` in the browser, with the suggestions `try_colormap` gives.
  It still draws with the `bgyw` default, so renders are unchanged; the standalone
  `Figure::colorbar(name, ..)` still draws nothing for one.
- **The embedded DejaVu Sans is stored zlib-compressed** (379 KB instead of 756 KB in
  every binary and wasm bundle) and inflated once on first use, through
  `miniz_oxide`, which `png` already brought in. Every default `FontSource` (so
  every `Figure`) now shares that one copy instead of copying 756 KB. The
  `DEJAVU_SANS_TTF` static becomes `text::font::dejavu_sans_ttf()`.
- Portable figures are schema 7. Scatter legend samples, and the insertion order of
  an axes' lines, patches and collections when it is not lines first, then patches,
  then collections, are written only when present; schema 1–6 artifacts still load.
- Portable figures are schema 8: figure text and figure axis labels, written only
  when present; schema 1–7 artifacts still load.
- Portable figures are schema 9: an image's norm and bad/over/under colors, written
  only when set; schema 1–8 artifacts still load.
- Portable figures are schema 10: axes colorbars, and the color scale a mesh or
  mapped collection was drawn with. `pcolormesh` and `scatter_mapped` figures carry
  their scale, so they now export as schema 10; schema 1–9 artifacts still load.
- Portable figures are schema 11 when they use an `ImageNormalize`, written as its
  resolved limits and stretch; schema 1–10 artifacts still load.
- Portable figures are schema 12 when their rc turns line simplification off or
  changes its threshold (the settings are written only then); schema 1–11 artifacts
  still load.
- Portable figures are schema 13 when an axes has a sky projection (written only
  then); schema 1–12 artifacts still load.
- Portable figures are schema 14 when an axes has a polygon collection (written
  only then); schema 1–13 artifacts still load.
- Portable figures are schema 15 when an artist uses a colormap of the user's own,
  which is written as its colors (`"lut:"` and hex RGBA), so it renders where the map
  was never defined; schema 1–14 artifacts still load.
- `ColorScale::cmap` is a `CmapSpec` rather than a `String`, and
  `ColorMapping::cmap_name()` is now `ColorMapping::cmap()`, returning the `CmapSpec`.
  A `CmapSpec` converts from a name, so `ColorMapping::new("viridis")` and
  `.cmap("gray")` are unchanged.

### Output changes
- `legend_auto` lists rows in the order the artists were added, as matplotlib does,
  rather than every line before every patch: a labelled patch added before a
  labelled line now comes first. Legends built from lines alone, or from lines added
  before patches, render byte-identically.
- Every `.riz` artifact now declares `"schema":15`.
- **A sky map's graticule draws at matplotlib's grid zorder (1.5)**: above
  fills, meshes and scatters, below lines, where it drew beneath every artist.
- **Mesh cells fill without antialiasing**, as matplotlib's `pcolormesh` does
  (`antialiased=False`), so neighboring cells no longer leave a faint background-
  tinted seam along every shared edge. Affected: `pcolormesh`, `hist2d`, `contourf`
  and the colorbars beside them; edge pixels change, cell interiors are unchanged.
  Gouraud meshes are unchanged.
- **`pcolormesh` draws on non-linear axes.** Meshes on log, symlog, logit and asinh
  axes were skipped; their corners now map through the scales, as matplotlib's do.
- Lines of 1000 or more vertices are drawn simplified: PNG pixels move by at most one
  unit on all but a few hundredths of a percent of the image (the gallery's
  streamplot changes in one antialiased edge pixel), and SVG and PDF output carry the
  simplified vertices. Shorter lines, and every other artist, are byte-identical; set
  `path_simplify: false` for the exact previous drawing.
- Uniform scatters of 10 000 or more markers are stamped from sprites in PNG and
  other raster output: each marker sits within 1/128 px of its exact position, so
  99.2–99.9% of inked pixels match the previous per-marker drawing to within one unit
  (at most 35 units apart on antialiased edges), and they are as close to
  matplotlib's output as before. Smaller scatters, and all SVG and PDF output, are
  unchanged.
- NaN values in `scatter_mapped` and `pcolormesh` take the bad color (transparent
  unless set) rather than the colormap's bottom color, as in matplotlib; images
  already did. Renders without NaN in their color data are unchanged.
- **Colormap lookups pick matplotlib's entry**, `floor(t · 256)` (the last entry at
  `t = 1`), instead of `round(t · 255)`, so colormapped pixels now match matplotlib
  3.11's colors. A colormapped pixel moves by at most one lookup-table step: up to 3
  units per channel for viridis and magma, 5 for coolwarm and inferno. Affected:
  `imshow`, `matshow`, `pcolormesh` (and Gouraud), `hist2d`, `hexbin`, `scatter_mapped`,
  `contour` / `contourf`, `tripcolor` / `tricontour`, color-mapped `streamplot` and 3D
  surfaces. Colorbar strips are unchanged (they sample each entry at its own index),
  and every render without a colormap — lines, patches, text, legends — is
  byte-identical.

## [1.13.7] - 2026-10-08

### Changed
- **One set of live demos for the docs and the demo site.** The wasm demos now live in
  `www/rizzma-demos.js`. The crate docs import it from the published demo site, and the
  demo site mounts every entry, so it always carries all of the docs' demos. The demo
  site gains the beats, waves, rose, ripple-tank and Lorenz demos (12 in all), and its
  animations pause while scrolled out of view. `tests/live_demos.rs` fails if the docs
  name a demo the module does not define.

## [1.13.6] - 2026-10-07

### Added
- **Figure and free-text font sizes.** `Figure::set_suptitle_size` sizes the figure title
  (tight layout reserves room for it), and `Axes::set_text_size` sizes the text added after
  it through `text`, `text_with_color`, `text_with_box`, `annotate` and `annotate_with_box`,
  the role matplotlib's per-call `fontsize=` plays. Both were fixed at 14 and 10.

### Changed
- Portable figures are schema 6. The sizes are written only when they differ from the
  defaults, so other figures export exactly the fields they did; schema 1–5 artifacts
  still load.

## [1.13.5] - 2026-10-06

### Added
- **Minor ticks and minor grid lines.** `Axes::minorticks_on` / `minorticks_off`, and
  `Axis::set_minor_locator` for an explicit locator. `AutoMinorLocator` subdivides the
  axis' actual major ticks, including fixed ones from `set_xticks`. `Axes::minor_grid` /
  `minor_grid_with` and `Axis::set_minor_grid` / `set_minor_grid_style` draw grid lines
  at the minor ticks, and `Axis::set_minor_tick_params` sets their length and width.
- `Axis::tick_top` / `tick_bottom` move the x ticks and labels to the top edge; layout and
  the title make room for them.
- `Axes::set_frame_on` hides the frame box while keeping the axis spines, and
  `Axes::set_title_size` sets the title size.
- In-place `Axis` setters: `set_tick_length`, `set_tick_width`, `set_tick_label_size`,
  `set_tick_label_pad`.

### Fixed
- **Tick, spine and grid widths below 1 are drawn as given.** They were clamped up to
  1 pt, so a 1-pixel grid at 200 DPI came out almost 3 pixels wide. This also makes the
  default `grid.linewidth` of 0.8 draw at 0.8; a width of 0 now hides the line.
- **Spines, ticks, grid lines and the frame are snapped to whole pixels**, as matplotlib's
  `path.snap` does, so a 1-pixel line is one crisp pixel instead of a 2-pixel grey smear.
- `RcParams::axes_titlesize`, `axes_titlepad`, `xtick_labelsize`, `ytick_labelsize`,
  `xtick_major_size`, `ytick_major_size`, `xtick_major_pad` and `ytick_major_pad` are
  applied. They were defined but ignored. The defaults draw exactly as before.
- `AutoMinorLocator` anchors its subdivisions on the first major tick, as matplotlib
  does, so majors offset from zero (1, 6, 11, …) get aligned minor ticks.

### Changed
- Portable figures are schema 5. The new styling is written only when used, so other
  figures export exactly the fields they did; schema 1–4 artifacts still load.

## [1.13.4] - 2026-10-05

### Fixed
- **Inverting an axis before plotting no longer freezes its range.** `invert_xaxis` /
  `invert_yaxis` copied the current autoscaled limits into fixed limits, so inverting empty
  axes froze them at `(0, 1)`, and data plotted afterwards fell outside the view. Magnitude
  plots, which invert first, drew nothing visible. Autoscaled axes now keep autoscaling and
  apply the inversion afterwards, as matplotlib does; fixed limits are still swapped in place.

## [1.13.3] - 2026-10-01

### Changed
- Dropped the **Live demos** heading from the crate docs: the animated canvases
  open the page and speak for themselves. The explanatory note about what they
  are now sits *below* the grid rather than above it (and says "every cell above"
  to match). Documentation only.

## [1.13.2] - 2026-10-01

### Changed
- The crate-level docs now open with the **Live demos** grid, so the animated,
  interactive canvases are the first thing on the docs.rs page instead of sitting
  below the intro and the first code sample. Documentation only; no API or
  behaviour change.

## [1.13.1] - 2026-10-01

### Changed
- **MSRV lowered from 1.96 to 1.85**, the first release with edition 2024, so
  projects held to an older toolchain (e.g. by a supercomputer's module
  system) can depend on rizzma. The 20 `if let … &&` chains and 6
  `is_multiple_of` calls that needed newer Rust are rewritten with
  `Option::filter` / nested `if` and `%`; behaviour is unchanged. A new CI
  `msrv` job checks the library on 1.85 with all and with no default features.
  The pinned development toolchain stays at 1.96.

## [1.13.0] - 2026-09-14

### Added
- **`Line2D` in-place setters.** `set_color`, `set_linewidth`, `set_dashes`,
  `set_cap`, `set_join`, `set_visible`, and `set_zorder` take `&mut self`, so
  the `&mut Line2D` returned by `Axes::plot` can actually be styled
  (`ax.plot(&x, &y).set_color(c)`) instead of overwritten wholesale. The
  consuming `with_*` builders now delegate to them. `Axes::lines` /
  `Axes::lines_mut` expose the plotted lines for post-hoc restyling, e.g.
  toggling visibility without rebuilding the figure. (#305)
- **Explicit ticks: `Axes::set_xticks` / `set_xticklabels` and the `y`
  twins.** Pin major ticks to exact data positions and label them with
  arbitrary strings (category names, release ids, …) without implementing
  `Locator` / `Formatter` by hand — a `FixedLocator` / `FixedFormatter` pair,
  as in matplotlib. (#303)
- **Per-artist labels and `Axes::legend_auto`.** `Line2D` and `Patch` carry
  an optional legend label (`with_label` / `set_label` / `label`), and
  `legend_auto()` builds the legend from them — matplotlib's bare
  `ax.legend()`. Entries are keyed on the artist's own color, so restyling a
  series can no longer leave a confidently wrong legend; `_`-prefixed and
  empty labels are skipped and a label shared by several artists (a series
  split into runs) yields one row. Labels round-trip through portable
  figures, whose wire model already reserved the slot. The explicit
  `legend(entries)` remains for legends that are not one-to-one with
  artists. (#304, #286)
- **Explicit band colors: `Axes::stackplot_with_colors` and
  `fill_between_with_color`.** State the fill colors locally, mirroring
  `plot_with_color`: the property cycle is neither consulted nor advanced, so
  an overlaid line keeps its expected cycle color and a per-category palette
  (`end_turn` green, `error` red, …) no longer requires `set_prop_cycle` as a
  side effect on the axes. (#306)

### Fixed
- `Line2D` breaks its stroke at non-finite samples. A `NaN` (or `±inf`) in
  `x` or `y` now lifts the pen — the points on either side are not joined —
  matching matplotlib's idiom for a series with missing data. Previously the
  raw coordinates went straight to the renderer, joining across the gap or
  producing an undefined vertex depending on the backend. `data_extents`
  already ignored those samples; drawing is now consistent with it. The new
  `Line2D::path` exposes the gap-aware path. (#302)
- Position-indexed formatters (`FixedFormatter`) now pair label `i` with the
  locator's `i`-th position even when earlier positions fall outside the
  view; labels were previously assigned after out-of-range ticks were
  dropped, shifting every remaining label. Matches matplotlib's
  format-then-clip order.

## [1.12.0] - 2026-08-29

### Added
- **Controls: sliders other than time (schema 4)**. A figure can declare
  `portable::Control`s — a labelled range with a default and optional step —
  whose `Track`s are evaluated along the control's position instead of the
  clock: the same keyframe machinery, because nothing about a keyframe axis is
  inherently temporal. A control may also carry `portable::Grid`s, lattices of
  frames over `(time, position)` sampled bilinearly, which is what lets a
  slider reshape a figure **while it plays** — the wave keeps travelling as its
  wavelength changes under the user's finger. Displayed state is a pure
  function of `(t, control values)`, re-applied whole on every change, so
  event order can never show (`design/12-controls.md`).
- `Figure::add_control` / `set_control` / `controls` / `control_values`, the
  same surface on the wasm session (`controls()` returns the slider manifest
  as JSON, `setControl` rides the rAF-coalesced repaint), `Interactor::set_control`
  under the same user-view-wins policy as `seek`, and `controls` in the mount
  handle and `readMetadata`. Sliders themselves are host DOM: the `.riz.html`
  live tier emits its own.
- `Figure::is_animated`: true when the timeline has tracks **or** any control
  carries a grid — a figure whose only time-dependence is a grid still plays.
- `portable::ControlRef` and `Metadata::controls`: `inspect` reports the
  typed control manifest (declaration order, layout fields only — serde skips
  the track data rather than allocating it), with nonsense ranges refused at
  inspection, so a host validates once and persists typed rather than poking
  JSON.
- `Limits::max_controls` (64) and `max_control_label_bytes` (256): the
  control manifest becomes host-materialized state — a persisted entry, a DOM
  slider — so it answers to its own budgets at both `inspect` and import, not
  to whatever fits inside the JSON allowance.
- **Accepted figures are total**: `add_control`, export, and import all
  validate every driver target against the artists the figure actually has
  (existence and stride compatibility), so `seek`/`set_control` on an accepted
  figure cannot fail and a host never observes an applied-but-failed state.
  `add_control` now returns `Result<usize>`; a failed `set_control` rolls the
  control value back to the last successful application.
- **Schema cannot be under-declared**: a document carrying controls must
  declare schema ≥ 4, a timeline ≥ 3, meta ≥ 2 — enforced at inspection and
  import, because schema is the compatibility decision hosts select runtimes
  by. A forged "schema 3" artifact with controls previously rendered on new
  runtimes while failing on the runtime the declaration selected.
- Import validation for deserialized tracks, grids, and controls: a
  hand-crafted artifact carrying a stride that disagrees with its value
  count, an empty keyframe axis, or a lying lattice is now `Malformed` at
  `from_portable` instead of a panic in `sample()`. The hole predates schema
  4 — a forged schema-3 timeline panicked identically in 1.9 through 1.11.
- `examples/portable_demo.rs`, the canonical demo: a travelling wave that
  loops crisply (its final keyframe is byte-identical to its first, so the
  wrap interpolates into a repaint rather than a jump) with a wavelength
  slider, above a Gaussian pulse with a width slider.

### Changed
- **`mount()` autoplays animated figures by default**; pass `autoplay: false`
  to mount paused. An animated figure's author animated it on purpose, and the
  opt-in default produced figures that arrived dead until a second click. The
  host's design/11 §7 pause obligations are unchanged.
- `SCHEMA_VERSION` is 4. Schema-3 renderers refuse schema-4 artifacts loudly
  and show the poster, as designed.

## [1.11.0] - 2026-08-29

### Added
- **The reversible HTML wrapper (`.riz.html`)**: one file a browser opens and a
  host ingests. `Figure::save_html` writes a poster-tier page (the embedded
  poster and alt text, offline, ~1.5 KB plus base64's 33 % over the raw
  artifact); `Figure::save_html_live` embeds a caller-supplied runtime so the
  file pans, zooms, and animates when double-clicked, degrading to the poster
  where wasm cannot start. In both tiers the canonical `.riz` travels inside,
  **recoverable byte-for-byte** with `portable::unwrap_html` — the HTML file is
  the carrier of the artifact, not a second format that can drift from it.
  `portable::{wrap_html, wrap_html_live, is_raw_riz}` operate on raw bytes for
  tooling.

  The shape is deliberate (`design/10` §3.1): a true polyglot would force
  strict readers to sniff, and raw binary in markup would let attacker-chosen
  *plot values* spell `-->` or `<script` and break out — so the artifact is
  base64, whose alphabet cannot close a tag. Title and alt text are escaped at
  wrap time, the viewer never assigns artifact-derived strings via `innerHTML`,
  unwrapping requires the carrier element exactly once, and the core `.riz`
  validator remains byte-strict: one parser never accepts both forms.

## [1.10.0] - 2026-08-27

### Added
- **`WasmSession::seek(t)`** (and `WasmSession::animation()`): a host can now
  advance an animation on a canvas it has **already bound**, with the pointer
  and wheel listeners staying attached. Previously `seek` lived only on
  `WasmFigure`, which `bind` consumes, so a bound interactive canvas could not
  animate at all. Repaints ride the same `requestAnimationFrame` coalescing as
  interaction, so seeking faster than the display refreshes does not stack
  frames.
- **`Interactor::seek(t)`**, which owns the policy for who steers the view:
  data tracks always apply, but a timeline's `xlim`/`ylim` tracks **yield to a
  view the user has taken over** by panning or zooming — an authored camera
  fighting the user every frame would make interaction pointless. Double-click
  (Home) hands the view back and the camera tracks resume on the next seek.
  Seeking alone never suspends anything. `Figure::seek` stays policy-free.
- The `rizzma-mount.js` handle grew `play()`/`pause()`/`seek(t)`, `animated`,
  and `duration`, plus an `autoplay` mount option: a child-owned rAF clock per
  the mount protocol, with natural completion clamping to the end and
  resume-at-end restarting from zero. `dispose()` now also cancels the clock
  and zeroes the canvas backing store. `readMetadata` additionally reports
  `timeline: {duration, loop}`.

## [1.9.0] - 2026-08-26

### Added
- **Animated portable figures.** A figure can carry a `Timeline`: keyframed
  tracks over the same mutation verbs a live session already drives (line x/y,
  scatter offsets, image data, and view limits). `Figure::seek(t)` evaluates it,
  and export carries it, so an animation survives the trip to a host that never
  heard of the process that made it.

  The timeline is **evaluable at any `t`** rather than a frame sequence, which
  is what makes the difference worth having: seeking is exact rather than
  approximate, a paused figure is a full-quality render of that instant rather
  than a held frame, and the same `t` draws the same pixels on every host. A
  frame sequence would be a video, and video already exists and is smaller.

  A scrolling window — an oscilloscope sweep — is two keyframes on `Xlim`, not
  hundreds of frames of duplicated samples.
- `WasmFigure::seek(t)` and `WasmFigure::animation()` put the same control in
  the browser, and `inspect` reports `animated` and `duration` so a host can
  decide whether to show transport controls without parsing the timeline.
- `Axes::line_data`, `Axes::image_shape`, `Axes::collection_count`,
  `Axes::image_count`, and `AxesImage::shape` — accessors the animation path
  needs, and useful on their own for checking what a frame produced.

### Changed
- Portable figures are now **schema 3**. Schema 1 and 2 artifacts still load;
  schema 3 artifacts are not readable by earlier builds, which is what the
  version signals. A host renders schema 3 only once its vetted runtime
  manifest advertises `schema_max >= 3`; until then an animated artifact is
  shown as its poster, because an animated figure drawn by a runtime that
  cannot animate is a still figure presented as a live one.
- Seeking to exactly `duration` on a looping timeline shows the **end** of the
  animation rather than wrapping to the start. Wrapping begins strictly past
  the duration, which is where a playback clock crosses anyway.

## [1.8.1] - 2026-08-24

### Fixed
- The published runtime manifest pinned only the wasm renderer, leaving the
  wasm-bindgen glue and the mount loader as unverifiable executable inputs. That
  is a hole rather than an omission: the loader is what decides whether the
  renderer gets verified, so checking the renderer with an unverified loader
  checks nothing. `runtime.json` now carries `role`, `file`, `mime`, `size`, and
  `sha256` for each of the three, and `cargo xtask runtime-manifest` hashes every
  file itself and refuses to emit a manifest with a missing, duplicated, or
  unknown executable role, an empty asset, or an unsafe filename. The MIME
  follows from the role rather than the file extension, so renaming a file
  cannot change how a consumer executes its bytes. The manifest gained a
  `manifest` format field so a consumer can tell the shapes apart.

  v1.8.0's assets are left untouched — something may already have pinned them,
  and rewriting a published artifact to close a security gap is a worse
  precedent than leaving a version behind that under-specifies. Its release
  notes list the three digests as text; `runtime.json` in v1.8.1 and later is
  the pinnable form.

## [1.8.0] - 2026-08-24

### Added
- **`portable::inspect(bytes, &Limits)`**, which reads what an artifact *is* —
  exact pixel size, title, alt text, poster, provenance, and whether this build
  can render its schema — without building a figure or rendering one, and
  without allocating the binary payloads. That is the common case rather than
  an edge case: a transcript holding hundreds of figures paints a card for
  nearly all of them and renders one or two. Dead-code elimination keeps the
  cost honest for a consumer that only inspects: a wasm binary doing nothing
  else links none of the rasterizer or the embedded font and comes to ~22 KB.
- `portable::Limits`, the budgets a caller imposes on artifacts it did not
  produce (total bytes, chunk count, JSON and poster size, canvas pixels),
  enforced before allocating. Defaults are generous against a typical figure
  and low enough to still be a bound.
- `portable::container`, the `RZFG` chunk framing, made public so an
  independent implementation has something stable to target.
- **Posters.** Artifacts now embed a PNG of the figure as authored, so every
  path that cannot execute a renderer still has something to show: scripting
  disabled, an unsupported schema, an archive preview, or the card displayed
  while a runtime downloads. It is a byproduct of a figure the exporter already
  holds, and it is byte-identical to that figure's own `encode_png()`.
- `Figure::to_portable_with(&PortableConfig)` to set alt text or omit the
  poster, and `Figure::from_portable_limited(bytes, &Limits)` to enforce
  caller-supplied budgets on untrusted input.
- Each release now publishes its wasm renderer as immutable GitHub release
  assets (`rizzma_bg.wasm`, `rizzma.js`, and a `runtime.json` declaring the
  schema range it renders), so a host can vet a runtime once and fetch the
  exact one an archived figure was authored against.
- `cargo xtask runtime-manifest` prints that compatibility manifest, reading the
  schema range from the code so the two cannot drift.
- **`WasmFigure::from_portable(bytes, max_bytes)`** and `rizzma-mount.js`: a
  `.riz` can now be mounted into a `<canvas>` in the browser, with pan, zoom,
  and hover intact and no imperative plotting calls on the JS side. The loader
  **requires** the host to supply the renderer — there is no default that
  fetches a URL and no fallback to bytes an artifact brought with it — and
  verifies it against the digest the host recorded when it vetted those bytes.
  `WasmFigure::schemaRange()` lets a loader choose between mounting and showing
  the poster.

### Changed
- The wasm build now has **two size budgets**, because it produces two
  deliverables. The demo bundle that docs.rs pages and the gallery load builds
  figures imperatively and no longer carries the portable-figure importer or a
  JSON codec — about 590 KiB every reader would otherwise download unused, so
  it stays at its previous size. The runtime bundle that renders `.riz` pays
  that cost, is fetched once per host rather than per figure, and is not
  fetched at all by a host that only shows posters.
- Portable figures are now **schema 2**: the artifact carries a `meta` block
  (exact pixel size, title, alt text, poster reference) alongside the figure.
  Schema 1 artifacts written by 1.7.x still load — `meta` is optional for as
  long as the supported range starts at 1 — and schema 2 artifacts are *not*
  readable by 1.7.x, which is what the version signals.
- A renderer digest an artifact carries about itself is documented throughout as
  an identity and a lookup key, never an authorization: a hostile artifact can
  inline a hostile renderer and honestly report its hash. Hosts keep their own
  allowlist and ignore the inlined bytes for execution.

## [1.7.1] - 2026-08-23

### Fixed
- Subplot grids no longer reserve the space between panels twice. Tight layout
  insets each axes by the decorations it measured, but the envelope grid still
  carried matplotlib's default `wspace`/`hspace` — gaps that exist precisely to
  hold those same decorations — so an empty band sat between the insets. On a
  1200x700 two-by-two grid the gap between rows was 117 px where 53 px of
  decoration needed it, and 149 px between columns where 41 px did; each panel
  now gains roughly 11% in both dimensions. Figures built with `add_subplot`
  will lay out visibly tighter; `add_axes` rectangles are unaffected.

### Added
- **Portable figures (`.riz`)**: `Figure::to_portable`, `save_portable`, and
  `from_portable` serialize the semantic figure — axes, artists, data, and
  rcparams — into a chunked binary container and rebuild an identical figure
  from it. Unlike PNG/SVG/PDF export, which flattens a figure to geometry, the
  artifact carries the *data*, so a consumer can re-run layout and
  rasterization from it: the prerequisite for pan, zoom, and resolution
  independence away from the process that built the figure. The round trip is
  pixel-exact, and bulk arrays travel as typed binary accessors rather than
  JSON floats. Behind the default-on `portable` feature. See
  `design/10-portable-figure.md` and #287.
- `Axes::clabel` draws inline contour labels: one central segment per level is
  removed to make room and the level is drawn in the contour's own color, for
  both regular and triangular contours.
- `Figure::suptitle` sets a title centered above the whole subplot grid, with
  theme-aware rich text and automatic top-space reservation under tight layout.
- `Axes::legend_with_title` adds a heading above the legend entries, and
  `Axes::legend_at` (with the new public `LegendLocation`) places the box in any
  of the four corners; upper-right remains the default.
- `Axes::text_with_box` and `annotate_with_box` draw a background behind text,
  configured by the new public `TextBoxStyle` (fill, edge, padding, corner
  radius, line width), with plain or rounded corners.
- `Axes::invert_xaxis` and `invert_yaxis` reverse an axis while preserving its
  current explicit or autoscaled range; calling either again restores the
  original direction.
- `Axes::axhline_with` and `axvline_with` take an explicit color and line width;
  the plain `axhline`/`axvline` now inherit the axes theme's text color.

### Fixed
- Zero-extent paths are no longer handed to tiny-skia's stroker, which dropped
  them; single-point and coincident-point paths are skipped while genuine
  horizontal and vertical strokes still render.

## [1.6.2] - 2026-07-23

### Fixed
- `ScalarFormatter` tick labels now carry enough decimal places to distinguish
  the tick locations, matching matplotlib's draw-time `set_locs` behavior. An
  axis spanning a few millihertz around 500 previously rendered every label as
  `500.0` (the constructor's one-decimal default, since nothing on the axis
  draw path called `set_locs`); `format_ticks` now derives the precision from
  the tick vector itself. Integer-spaced ticks correspondingly drop the
  trailing `.0`, as matplotlib does. Single-value `format` calls keep the
  stored precision.

## [1.6.1] - 2026-07-16

### Fixed
- Equal-aspect axes with descending (inverted) limits — e.g. an image panel
  with `set_ylim(high, low)` — no longer collapse to a degenerate rect that
  dropped the image; the box equalizes on span magnitude and the limits keep
  their direction.

## [1.6.0] - 2026-07-16

### Added
- **Explicit annotation color: `Axes::text_with_color`.** Places text at a data
  point with a caller-chosen ink, matching matplotlib's `text(x, y, s, color=…)`.

### Changed
- `Axes::text` and `Axes::annotate` now draw in the figure's themed text color
  (`RcParams::text_color`) instead of unconditionally black, so annotations are
  legible on dark-styled figures. The default theme's text color is black, so
  unstyled figures render identically.

## [1.5.0] - 2026-07-13

### Added
- **Interactive local-browser viewer (`show()`).** `Figure::show()` /
  `pyplot::show()` (and `show::show_all` / `show_nonblocking`) open the system
  browser onto a single window holding every figure — drag to pan, wheel to zoom
  at the cursor, double-click or **⌂ Home** to reset, and export the active
  figure to PNG/SVG/PDF from the toolbar. Behind a dependency-free loopback
  HTTP server (matplotlib's WebAgg model) reusing the existing `Interactor` and
  render backends; gated behind the native-only `show` feature (on by default).
  See `design/09-show-browser.md`.

### Changed
- The axis trait objects `Scale`, `Locator`, and `Formatter` (and
  `FuncFormatter`'s closure) are now `Send + Sync`, so a `Figure` can be moved
  across threads (needed by the non-blocking viewer).

## [1.4.0] - 2026-07-13

### Added
- **Figure-level styling via `RcParams`.** `Figure::with_rcparams` /
  `set_rcparams` (and `pyplot::style`) seed every axes' resolved style —
  face/edge colors, frame width, title and axis ink, the property color
  cycle, grid on/off + color/width/alpha, tick direction, and legend
  colors — from a single typed config. `RcParams::dark()` ships as a preset;
  any custom `RcParams` works too. Re-exported at the crate root as
  `rizzma::RcParams` / `rizzma::TickDirection`.
- **Reachable per-axes styling.** `Axes::xaxis_mut`/`yaxis_mut` expose the
  axes' `Axis` for styling; `Axes::grid`/`grid_with`, `set_edgecolor`,
  `set_linewidth`, `set_title_color`, `set_axis_color`, and `set_prop_cycle`
  cover the common cases directly. `Axis` gains `set_color`, `set_grid`,
  `set_grid_style`, and `with_tick_direction`/`set_tick_direction`.

### Changed
- The axes frame stroke, title ink, grid ink, legend box colors, and the
  property color cycle are now driven by resolved style fields instead of
  hardcoded constants. The default `RcParams` reproduces the previous look
  byte-for-byte (guarded by a test).

## [1.3.2] - 2026-07-09

### Changed
- The galaxy demo now spins about an **out-of-plane axis**: 3D star
  positions (a thin, core-thickened disk) go through the full rotation
  matrix `Rz(precession) · Rx(inclination) · Rz(spin)` each tick and
  project orthographically — the disk tumbles in 3D on both the docs
  matrix and the demo site, and stays fully interactive.

## [1.3.1] - 2026-07-09

### Added
- Every live demo cell in the docs offers a **"⤢ full size" pop-up**: the
  same demo rebuilt at a viewport-fitted scale in an inline modal (close
  with Esc, the ✕ button, or a backdrop click; deep-linkable via a
  `#rizzma-live=<name>` URL hash).

### Fixed
- The signal + spectrogram demo plots against a **fixed trailing time
  window** (`t − now`), so the trace and the spectrogram image sit flush
  against the axes frame and the ever-growing time tick labels no longer
  make the frame edge bounce as they pop in and out of the layout's
  overhang reservation.

## [1.3.0] - 2026-07-09

### Added
- **A live demo matrix on the crate main page**: the docs.rs front page now
  hosts a responsive 3×3 grid of interactive, animated wasm demos —
  traveling beats, counter-propagating waves, a morphing rose, a random
  two-tone signal above a live spectrogram (sharex-linked), a slowly
  rotating spiral galaxy, a spinning 3D sombrero surface, a two-source
  ripple tank streamed through `imshow`, the Lorenz attractor traced live,
  and streaming oscilloscope strips. One shared ticker drives every cell
  and pauses whatever scrolls out of view; static gallery images remain
  the fallback.
- **Live scatter updates**: `Axes::set_collection_offsets` and
  `WasmSession::set_scatter_offsets` replace a scatter collection's offsets
  in place (markers, sizes, and colors kept), mirroring `set_line_data` —
  explicit limits survive, so an animated scatter keeps moving inside a
  zoomed view.
- **3D in the browser**: `WasmAxes3D` wraps `Axes3D` for full-frame,
  HiDPI-crisp canvas rendering from JS (`plot_surface`, `scatter3d`,
  `set_title`, `set_view`, `render`), and `Axes3D::set_view` sets the
  elevation/azimuth in place for animated views.
- **Images from JS, with live updates**: `WasmFigure::imshow` displays a
  colormapped row-major array with an explicit extent and `vmin`/`vmax`,
  and `AxesImage::set_data` / `Axes::set_image_data` /
  `WasmSession::set_image_data` replace an image's data (and extent) in
  place — explicit norms survive, so streaming spectrogram frames don't
  flicker.
- The demo site gains the spinning galaxy, the signal + spectrogram card
  (replacing the static chirp), and a spinning 3D surface card; the gallery
  gains a `surface3d` case.

## [1.2.1] - 2026-07-09

### Added
- **Live wasm demos in the crate docs**: docs.rs pages now mount real
  interactive canvases (wheel zoom, drag pan, double-click reset) where the
  docs previously showed static gallery images, via an injected
  `--html-in-header` loader that lazily imports the wasm bundle from the
  gh-pages `demo/` deployment. The static images remain as fallbacks
  everywhere the bundle cannot load. First two: an interactive line plot on
  the `wasm` module docs, and streaming x-linked strips on
  `Axes::oscilloscope`.
- The interactive demo site now deploys permanently to
  <https://orbitalcommons.github.io/rizzma/demo/> from the Gallery
  workflow, and the gallery gains an `oscilloscope` case.
- `cargo xtask serve-www` sends `Access-Control-Allow-Origin: *`, matching
  GitHub Pages, so locally built rustdoc can exercise the live-docs path.

## [1.2.0] - 2026-07-08

### Added
- **Ten capability upgrades** from the matplotlib-parity triage: the classic
  colormaps (`magma`, `inferno`, `plasma`, `cividis`, `RdBu`, `coolwarm`,
  all with `_r`), `Patch::circle`/`rectangle`/`arc` constructors,
  `Axes::text` and `Axes::annotate` with leader arrows, `tricontour` /
  `tricontourf` over triangulations, gouraud-shaded `pcolormesh`,
  `colorbar_at` placement with horizontal orientation, `SkyAxes` with
  aitoff and mollweide projections, `twinx` twins plus a secondary linear
  x axis, 3D quiver arrows and billboard text labels, and live line-data
  updates for wasm sessions (`set_line_data`).
- **Kovesi CET colormaps** (arXiv:1509.03700): 15 perceptually uniform maps
  embedded as compact 8-bit tables — linear (`cet_l01`, `cet_l03` "fire",
  `cet_l05`, `cet_l09`, `cet_l10`), diverging (`cet_d01`, `cet_d04`,
  `cet_d07`, `cet_d11`), the equalized rainbow `cet_r2`, cyclic
  `cet_c1`/`cet_c2`/`cet_c3`/`cet_c5`, and isoluminant `cet_i1`, each with
  `_r` variants. **The default colormap is now `bgyw` (CET-L09)** for every
  artist that previously hardcoded viridis. The classic vendor maps (`jet`,
  `hot`, `hsv`, `rainbow`) ship quarantined in `core::color::misleading` and
  only resolve with an explicit `misleading:` registry prefix; module docs
  explain the perceptual failure modes with the paper citation.
- **`sharex` linked axes** (`Figure::sharex`, `WasmFigure.sharex`): a
  follower mirrors its leader's x-limits at draw time, interactive pan/zoom
  on either axes keeps the group's x in lockstep (y stays per-axes), and
  double-click home restores the whole group. Stacked pairs hide the upper
  axes' x tick labels (matplotlib's `label_outer`).
- **Oscilloscope axes style** (`Axes::oscilloscope`): a chart built for
  arbitrary — including sparkline-strip — sizes. Everything draws inside the
  frame: near-black CRT face, fixed 10×4-division phosphor graticule,
  phosphor trace cycle (green/amber/cyan/magenta), dim bezel, and in-frame
  corner readouts (y-max, y-min, x-span) that track the live limits. Scope
  axes autoscale flush in x.
- **Interactive wasm demo site** (`crates/rizzma/www`): six live canvases —
  styled lines with `set_line_data` animation, scatter, log-log, `sharex`
  subplots, a Rust-side cursor trail, and streaming oscilloscope strips —
  served by the new dependency-free `cargo xtask serve-www`.
- **`WasmSession::track_cursor`**: record hovered data positions into a line
  artist as a rolling trail, entirely on the Rust side of the boundary.
- Titles for the self-contained axes: `SkyAxes`, `PolarAxes`, and `Axes3D`
  gain `set_title`, drawn centered and DPI-scaled like 2D axes titles.
- `Figure::set_facecolor` (and the `WasmFigure` export) for in-place canvas
  background changes; `Axes::set_x_tick_labels_visible` /
  `Axis::set_tick_labels_visible` for label-outer control.
- Sticky autoscale edges: bars, histograms, stems, stairs, stackplots, and
  grouped bars sit exactly on their zero baseline; images and meshes stay
  flush with their live extents; line charts are flush in x.
- Gallery: images render at ≥1600 px for HiDPI docs, every demo's title
  leads with the feature it demonstrates, and the colormap showcase is
  grouped into CET / classics / quarantined sections.

### Changed
- **Tight layout**: `add_subplot` frames are derived from measured
  decoration extents (matplotlib's tight layout) instead of fixed margins —
  including end-tick-label overhang, column-aligned frame widths for
  stacked subplots, and matplotlib-equivalent padding (full pad at figure
  edges, one shared pad between neighbors). Explicit `add_axes` rects stay
  literal.
- Text, tick, marker, and arrow geometry scales with renderer DPI, so
  high-DPI renders are true scale-ups rather than tiny text on big canvases.

### Fixed
- **Artists now clip to the axes frame** on every backend (raster mask with
  caching, SVG `clipPath`, PDF clip operator): zooming or setting limits
  tighter than the data no longer spills lines, patches, meshes, or images
  across the figure.
- Dropping a `WasmSession` (explicitly or via GC) detaches its DOM
  listeners, so events on the canvas no longer throw
  "closure invoked recursively or after being dropped".
- Degenerate axis ranges are judged relative to magnitude, log/logit limits
  survive extreme interaction, and the wasm browser CI pins wasm-pack
  0.15.0.

## [1.1.2] - 2026-07-07

### Changed
- The README and docs.rs crate page now open with the project tagline:
  "Scientific communication reflects on the scientist, and your figures should
  carry the same *rizzma* as your ideas."

## [1.1.1] - 2026-07-07

### Fixed
- Interaction hardening (from codex's post-release review): pan/zoom limit
  candidates are now rejected when non-finite and clamped to the axis scale's
  domain before being stored, so an extreme wheel or drag on a log/logit axis
  can no longer poison the view; `pointercancel`/`lostpointercapture` cancel an
  in-progress drag (touch/pen interruptions); a failed `requestAnimationFrame`
  no longer wedges the redraw flag; and the device pixel ratio is re-read every
  frame, so browser zoom or moving between monitors re-renders at the right
  density.
- Degenerate axis ranges are now judged relative to the values' magnitude
  (matplotlib's `nonsingular` semantics): a deep-zoomed log axis at tiny
  magnitudes — e.g. `(1e-30, 1e-22)`, eight healthy decades — is no longer
  misclassified as zero-width and blown up to `(-0.5, 0.5)`.

## [1.1.0] - 2026-07-05

### Added
- **Interactive wasm plots** (design doc 06, W1–W6). `WasmFigure` is now a real
  JS plotting surface — `new(w, h)`, `add_axes`/`add_subplot`, `plot`,
  `plot_styled({color, lw, ls})`, `scatter`, titles/labels/limits, log scales,
  and `legend` — and `WasmFigure.bind(canvas_id)` returns a `WasmSession` with
  wheel zoom anchored at the cursor, left-drag pan (log-scale-aware), double-click
  home reset, and an `on_hover` callback, repainting through a
  `requestAnimationFrame`-coalesced loop.
- HiDPI rendering: `Figure::render_scaled` / `wasm::figure_to_rgba_scaled`
  render at `devicePixelRatio` × DPI (bit-identical to a double-DPI render) and
  the canvas is presented at logical CSS size, so browser plots are crisp on
  retina displays.
- Host-agnostic interaction core: `figure::{Event, MouseButton}` (top-down
  logical pixels, no y-flip anywhere), `Figure::axes_at` hit-testing, and
  `figure::Interactor`/`Outcome` — pure Rust, fully covered by native tests.
- CI: `wasm browser tests` job runs the new `wasm-pack test --headless
  --chrome` suite (canvas pixel readback + synthetic pointer/wheel event
  end-to-end), and a native per-frame render budget test guards interactive
  latency (~14 ms release at 1600×1200 vs the 16 ms design target).

## [1.0.1] - 2026-07-01

### Changed
- Titles, axis labels, and tick labels now render with matplotlib-grade
  typography. Text layout and measurement apply the font's `kern` table (the
  same pairwise kerning FreeType uses), so pairs like `Ta`/`AV`/`Wo` tuck in
  correctly. The y-axis label is now rotated 90° and centered along the axis
  (previously drawn horizontally and clipped off the canvas), tick and axis
  labels are placed against measured text extents, and `axes.titlepad`/
  `axes.labelpad`/`xtick.major.pad`/`ytick.major.pad` are configurable via
  `RcParams`.

## [1.0.0] - 2026-07-01

**rizzma is now a single crate.** The former 14-crate workspace is collapsed into
one publishable `rizzma` crate: each `rizzma-*` sub-crate becomes a module
(`rizzma::core`, `rizzma::figure`, `rizzma::axis`, `rizzma::mathtext`,
`rizzma::skia`/`svg`/`pdf`, …), and the optional pieces are default-on Cargo
features you can opt out of. `cargo add rizzma` gives you the full 2D + 3D library.

### Changed
- Collapsed the workspace into a single `rizzma` crate; the `rizzma-*` sub-crates
  are now modules of `rizzma` (most-used types re-exported at the crate root).
- Optional, default-on features: `plot3d` (3D axes), `pyplot` (the facade), and
  `wasm` (browser backend). `default-features = false` builds core 2D + PNG/SVG/PDF.

### Added
- `contourf` filled contours; polar `scatter`/`fill`; 3D `bar3d` and
  `plot_wireframe`; mathtext `\operatorname`/named operators and math styles
  (`\mathbb`/`\mathcal`/`\mathfrak`, with font-coverage fallback).

## [0.1.0] - 2026-06-30

The umbrella `rizzma` crate now **re-exports the full workspace API** behind one
import surface (`use rizzma::Figure;`, plus namespaced modules `rizzma::figure`,
`rizzma::pyplot`, `rizzma::axis`, `rizzma::mathtext`, `rizzma::mplot3d`, and the
`skia`/`svg`/`pdf` backends). The whole workspace moves to a unified `0.1.0`.

### Added
- Single-crate public API via the `rizzma` umbrella (re-exports every `rizzma-*`
  crate; common types `Figure`/`Axes`/`PolarAxes`/`GridSpec` flattened to the root).
- Nonlinear axis scales integrated into `Axes`: log/`loglog`/`semilog*`, symlog,
  logit, and date axes — all with mathtext-superscript tick labels.
- `rizzma-3d`: `Axes3D` with `plot3d`/`scatter3d`/`plot_surface` and depth-sorted
  rendering; `PolarAxes` polar plots; `rizzma-pdf` PDF backend and `Figure::save_pdf`.
- pyplot `savefig` selects PNG/SVG/PDF from the file extension.
- Extensive mathtext: fractions, radicals/nth-root, over/underline, binomials,
  matrices, accents, spacing, and style commands.

### Note
- Publishing `0.1.0` to crates.io additionally requires a `CARGO_REGISTRY_TOKEN`
  secret and publishing the member crates in dependency order; the workflow skips
  (with a warning) until the token is configured.

## [0.0.2] - 2026-06-30

This entry tracks the workspace's progress. The published `rizzma` crate still
reserves the name — its public re-export of the functional crates is pending —
so the headline features below ship in the in-tree `rizzma-*` crates.

### Added
- 32 plot types on `Axes`, each with a rendered gallery example: line, scatter,
  bar/barh, hist, fill_between/fill_betweenx, errorbar, step, stem, stairs,
  stackplot, broken_barh, imshow, pcolormesh, contour, boxplot, violinplot,
  hexbin, grouped_bar, pie, eventplot, ecdf, matshow, spy, hist2d, quiver,
  streamplot, reference lines/spans, legend, and colorbar.
- Logarithmic axes: `set_xscale_log`/`set_yscale_log`, `semilogx`/`semilogy`/
  `loglog`, log tick locators and formatters, and mathtext-rendered tick and
  axis labels (real superscripts, e.g. `10^6`).
- Portable mathtext layout engine (`rizzma-mathtext`): fractions, radicals,
  accents, large operators, delimiters, and TeX spacing commands.
- `rizzma-pyplot` stateful facade mirroring `import matplotlib.pyplot as plt`,
  covering the full set of plot methods plus `legend`/`colorbar`.
- Backends: `tiny-skia` raster (PNG), a hand-emitted SVG vector backend, and a
  browser `<canvas>`/wasm backend, all behind one `Renderer` trait.
- Date tick locators/formatters (`AutoDateLocator`, `AutoDateMinorLocator`,
  `DateFormatter`, `ConciseDateFormatter`).
- CI: gallery rendered to the `gh-pages` branch, a wasm size budget check, a
  strict gallery-link checker, and a publish-on-version-change workflow.

## [0.0.1] - 2026

### Added
- Initial release reserving the `rizzma` crate name on crates.io.
