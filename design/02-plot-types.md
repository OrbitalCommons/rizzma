# 02 — Plot Types Catalogue (matplotlib → rizzma)

> A deep inventory of every plotting / visualization method matplotlib exposes,
> read directly from source. Source tree: `references/matplotlib/` (master, post-3.10
> — includes newly added `grouped_bar`, `pie_label`, `ecdf`, `stairs`).
> Primary file: `lib/matplotlib/axes/_axes.py`. Supporting: `axes/_base.py`,
> `collections.py`, `container.py`, `contour.py`, `tri/`, `quiver.py`,
> `streamplot.py`, `sankey.py`, `table.py`, `stackplot.py`,
> `mpl_toolkits/mplot3d/axes3d.py`.
>
> Purpose: prioritize which visualizations the Rust port implements, and document
> each one's anatomy (method → Artist/Container → primitive).

## Table of Contents

1. [How matplotlib structures plotting](#1-how-matplotlib-structures-plotting)
2. [Underlying primitive & container vocabulary](#2-underlying-primitive--container-vocabulary)
3. [Catalogue by category](#3-catalogue-by-category)
   - 3.1 [Basic: line & scatter](#31-basic-line--scatter)
   - 3.2 [Bars & categorical](#32-bars--categorical)
   - 3.3 [Statistical / distribution](#33-statistical--distribution)
   - 3.4 [Areas & fills](#34-areas--fills)
   - 3.5 [Spans & reference lines](#35-spans--reference-lines)
   - 3.6 [2D fields / images / grids](#36-2d-fields--images--grids)
   - 3.7 [Contours & unstructured (tri)](#37-contours--unstructured-tri)
   - 3.8 [Vector fields](#38-vector-fields)
   - 3.9 [Pie & polar](#39-pie--polar)
   - 3.10 [Spectral / signal](#310-spectral--signal)
   - 3.11 [Specialized & composite](#311-specialized--composite)
   - 3.12 [3D (mplot3d)](#312-3d-mplot3d)
   - 3.13 [Visualization-adjacent decorations](#313-visualization-adjacent-decorations)
4. [Compositions: which plots are built from others](#4-compositions-which-plots-are-built-from-others)
5. [Frequency / importance ranking & tiered priority](#5-frequency--importance-ranking--tiered-priority)
6. [Dependency analysis (shared primitives per tier)](#6-dependency-analysis-shared-primitives-per-tier)
7. [Total counts & Tier-1 recommendation](#7-total-counts--tier-1-recommendation)

---

## 1. How matplotlib structures plotting

Every plotting call is a **method on `Axes`** (`axes/_axes.py`, class `Axes` defined
~line 70; geometry/management lives in the base `_AxesBase` in `axes/_base.py`). A
plotting method does roughly:

1. **Normalize/validate inputs** (often via the `@_preprocess_data` decorator, which
   maps a `data=` mapping + string keys onto positional args, and via
   `cbook` / `np.asarray` coercion).
2. **Construct one or more low-level Artists** — `Line2D`, `Patch` subclasses, or a
   `Collection` subclass.
3. **Register them** with the Axes via `add_line` / `add_patch` /
   `add_collection` / `add_image` / `add_container` / `add_table`
   (all in `axes/_base.py`: `add_line` 2456, `add_patch` 2546, `add_collection`
   2383, `add_image` 2438, `add_table` 2623, `add_container` 2635).
4. **Update the data limits** so autoscaling works, then **return** either the Artist,
   a list of Artists, or a `Container` (a `tuple` subclass that bundles related
   artists — see `container.py`).

The pyplot module (`lib/matplotlib/pyplot.py`) auto-generates a thin free-function
wrapper for each public `Axes` method via `@_copy_docstring_and_deprecators`, so the
"plot type" surface area is essentially **the public methods of `Axes`** plus the
3D methods on `Axes3D`.

`@_docstring.interpd` / `@_docstring.dedent_interpd` decorate most methods to splice
in shared kwdoc tables (e.g. `Line2D` properties). Presence of `@_preprocess_data`
is a good signal that a method is a genuine "data plotting" entry point.

---

## 2. Underlying primitive & container vocabulary

The catalogue references these classes constantly. Knowing them up front makes the
tables compact.

### Primitive Artists
| Class | File:line | Primitive | Notes |
|---|---|---|---|
| `Line2D` | `lines.py` | polyline + markers | One artist draws *both* the connecting path and the per-vertex markers. Backbone of `plot`. |
| `Patch` (base) | `patches.py` | filled path | Subclasses: `Rectangle`, `Polygon`, `Wedge`, `Circle`, `Ellipse`, `FancyArrow`, `FancyArrowPatch`, `Arc`, `PathPatch`, `Annulus`. |
| `Text` / `Annotation` | `text.py` | glyph layout | `Annotation` adds an optional `FancyArrowPatch` connector. |
| `AxesImage` / `PcolorImage` / `NonUniformImage` | `image.py` | raster | `imshow` → `AxesImage`; `pcolorfast` may emit `AxesImage`/`PcolorImage`. |

### Collections (`collections.py`)
A `Collection` is a single Artist drawing **many** primitives efficiently (shared or
per-element styling, vectorized). Subclasses found at:

| Collection | line | Used by |
|---|---|---|
| `PathCollection` | 1127 | `scatter` |
| `PolyCollection` | 1300 | `fill`, `hexbin`, `stackplot`, `quiver`/`barbs` (subclass), `violin` bodies |
| `FillBetweenPolyCollection` | 1381 | `fill_between`, `fill_betweenx` |
| `RegularPolyCollection` | 1633 | regular n-gon scatter variants |
| `LineCollection` | 1709 | `hlines`, `vlines`, `eventplot` (via subclass), `streamplot`, contour lines |
| `EventCollection` | 1895 | `eventplot` |
| `CircleCollection` / `EllipseCollection` | 2078 / 2098 | sized circle/ellipse scatter |
| `PatchCollection` | 2206 | bulk patch drawing |
| `TriMesh` | 2266 | `tripcolor` (gouraud) |
| `QuadMesh` | 2483 | `pcolormesh`, `pcolorfast` (mesh path) |
| `PolyQuadMesh` | 2596 | `pcolor` |

### Containers (`container.py`) — `tuple` subclasses bundling artists
| Container | line | Returned by | Contents |
|---|---|---|---|
| `BarContainer` | 42 | `bar`, `barh`, `hist` | `.patches` (Rectangles), optional `.errorbar`, `.datavalues` |
| `ErrorbarContainer` | 119 | `errorbar` | `(data_line, caplines, barlinecols)` |
| `StemContainer` | 223 | `stem` | `(markerline, stemlines, baseline)` |
| `PieContainer` | 151 | `pie` (new API) | wedges + label artists |

### Contour / vector sets
| Class | File:line | Returned by |
|---|---|---|
| `ContourSet` / `QuadContourSet` | `contour.py` 582 / 1308 | `contour`, `contourf`, `tricontour(f)` |
| `Quiver` | `quiver.py` 499 (a `PolyCollection`) | `quiver` |
| `Barbs` | `quiver.py` 922 (a `PolyCollection`) | `barbs` |
| `QuiverKey` | `quiver.py` 277 (an `Artist`) | `quiverkey` |
| `StreamplotSet` | `streamplot.py` | `streamplot` (bundles a `LineCollection` + arrow `PatchCollection`) |

---

## 3. Catalogue by category

For each entry: **method (file:line)**, key signature params, visual result,
the Artist/Container produced, the underlying primitive, and notable
complexity/edge cases.

### 3.1 Basic: line & scatter

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `plot` (1549) | `*args` (x,y,fmt triples), `scalex/scaley`, `data` | connected lines and/or markers | `list[Line2D]` | polyline + marker | Heart of the library. Arg parsing in `_process_plot_var_args` (`_base.py`) handles `(y)`, `(x,y)`, `(x,y,fmt)`, repeated; fmt string parsing; color cycle; broadcasting columns to multiple lines. |
| `scatter` (5246) | `x, y, s, c, marker, cmap, norm, vmin, vmax, alpha, linewidths, edgecolors` | point cloud, per-point size/color | `PathCollection` (add_collection 5531) | marker path(s) | `c` is overloaded: color, color sequence, or scalar array → colormap. Size `s` in points². Single marker path scaled by sizes. Colorbar-mappable. |
| `step` (2139) | `x, y, *args, where{'pre','post','mid'}` | piecewise-constant line | `list[Line2D]` | polyline | Thin wrapper over `plot` with `drawstyle='steps-*'`; `where` maps to drawstyle. |
| `errorbar` (3979) | `x, y, yerr, xerr, fmt, ecolor, elinewidth, capsize, capthick, errorevery, (lo/up)lims` | data line + error bars + caps | `ErrorbarContainer` | Line2D + LineCollection + caplines | Composite. Asymmetric err (2×N), lims arrows, `errorevery` slicing, masked data, log-axis clipping of negative lower bars. |
| `loglog` (1804) | `*args` + `(sub/nonpos)(x/y)` | line on log–log axes | `list[Line2D]` | polyline | Sets both scales to `log` then calls `plot`. |
| `semilogx` (1858) | same, x only | line, log x | `list[Line2D]` | polyline | Sets x scale log, calls `plot`. |
| `semilogy` (1905) | same, y only | line, log y | `list[Line2D]` | polyline | Sets y scale log, calls `plot`. |

> `plot_date` — **removed** (deprecated 3.5, gone in this tree). Use `plot` with
> date arrays + `ConciseDateFormatter`. Do **not** port it.

### 3.2 Bars & categorical

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `bar` (2308) | `x, height, width=0.8, bottom, align{'center','edge'}, color, yerr/xerr, log` | vertical bars | `BarContainer` (built 2646, returned 2656) | `Rectangle` patches | Broadcasting of x/height/width/bottom; categorical x via unit conversion; integrated errorbars (calls `errorbar`); `align`; stacking via `bottom`. |
| `barh` (2660) | `y, width, height=0.8, left, align` | horizontal bars | `BarContainer` | `Rectangle` | Transpose of `bar` (delegates with swapped axes). |
| `broken_barh` (2974) | `xranges=[(start,width)...], yrange=(ymin,height)` | row of horizontal bars (Gantt-like) | `PolyCollection` (add_collection) | rectangles as polys | Used for Gantt/timeline. Single collection, not a container. |
| `stem` (3377) | `*args (locs, heads)`, `linefmt, markerfmt, basefmt, bottom, orientation` | stems from baseline to markers | `StemContainer` | LineCollection(stems) + Line2D(markers) + Line2D(baseline) | Composite. Orientation h/v; fmt parsing for three styles. |
| `eventplot` (1303) | `positions, orientation, lineoffsets, linelengths, linewidths, colors, linestyles` | rug/raster of event ticks | `list[EventCollection]` | line segments | One `EventCollection` per row; broadcasting of offsets/lengths; used for spike rasters. |
| `bar_label` (2799) | `container, labels, fmt, label_type{'edge','center'}, padding` | text labels on bars | `list[Annotation]` (Text) | text | Not a plot per se — annotates a `BarContainer`. Auto-positions per bar, handles +/- heights, error bars. |
| `grouped_bar` (3061) | `heights, positions, group_spacing, bar_spacing, tick_labels, labels, orientation` | grouped/clustered bars | list of `BarContainer` | Rectangles | **New** convenience API. Accepts dict / 2D / list-of-arrays; lays out groups; one container per series. |

### 3.3 Statistical / distribution

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `hist` (7260) | `x, bins, range, density, weights, cumulative, bottom, histtype{'bar','barstacked','step','stepfilled'}, align, orientation, rwidth, log, color, stacked` | binned frequency bars | `BarContainer` or `list[Polygon]` (step types) + arrays `(n, bins, patches)` | Rectangles or step Polygon | Most-parameterized method. Uses `np.histogram`; multi-dataset stacking; `histtype` switches between bars and filled/unfilled step paths; density vs count; log; cumulative. |
| `hist2d` (7859) | `x, y, bins, range, density, weights, cmin, cmax` | 2D heatmap of counts | returns `(h, xedges, yedges, QuadMesh)` | `QuadMesh` via `pcolormesh` | Wraps `np.histogram2d` + `pcolormesh`. cmin/cmax mask empty bins. |
| `hexbin` (5538) | `x, y, C, gridsize, bins, xscale/yscale, mincnt, marginals, reduce_C_function` | hexagonal binning heatmap | `PolyCollection` (built 5526) | hexagon polys | Hex tiling math; optional reduction of `C` per bin; log-binned counts; optional marginal histograms (extra PolyCollections). Colorbar-mappable. |
| `boxplot` (4370) | `x, notch, sym, vert, whis, positions, widths, patch_artist, showmeans, showfliers, bootstrap, …` | box-and-whisker | dict of artists (delegates to `bxp`) | Line2D + Patch(box) | Computes stats via `cbook.boxplot_stats`, then calls `bxp`. whis as IQR mult or percentiles; bootstrap CIs for notches. |
| `bxp` (4707) | `bxpstats (precomputed), positions, widths, vert, patch_artist, showcaps, capwidths, …` | box-and-whisker from stats | dict: `boxes, medians, whiskers, caps, fliers, means` | Line2D/Patch | The rendering core. Takes precomputed stats so you can supply your own. |
| `violinplot` (8930) | `dataset, positions, vert, widths, showmeans/extrema/medians, quantiles, bw_method, side` | KDE density "violins" | dict of artists (delegates to `violin`) | PolyCollection(body) + LineCollection(bars) | Computes Gaussian KDE via `mlab.GaussianKDE`, then calls `violin`. |
| `violin` (9064) | `vpstats (precomputed: coords, vals, mean, median, min, max, quantiles), positions, widths, side` | violins from stats | dict: `bodies(PolyCollection), cmeans, cmins, cmaxes, cbars, cmedians, cquantiles` | PolyCollection + LineCollection | Rendering core. `side` allows half-violins. |
| `ecdf` (7968) | `x, weights, complementary, orientation, compress` | empirical CDF step line | `Line2D` | step polyline | Sorts data, cumulative weights; `complementary` = survival function; emits a `steps-post` line. |
| `stairs` (7756) | `values, edges, orientation, baseline, fill` | step/staircase (post-hist) | `StepPatch` (a `Patch`) | step path | Modern replacement for plotting histogram outlines; N values, N+1 edges; optional fill to baseline. |

### 3.4 Areas & fills

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `fill` (6018) | `*args (x,y[,color] triples), data` | filled polygon(s) | `list[Polygon]` | filled path | Like `plot` but closes and fills each (x,y) pair. |
| `fill_between` (6180) | `x, y1, y2=0, where, interpolate, step` | band between two y-curves | `FillBetweenPolyCollection` (6172 add_collection) | poly band | `where` masking splits into disjoint polys; `interpolate` adds crossing vertices; `step` aligns with step plots. |
| `fill_betweenx` (6194) | `y, x1, x2=0, where, step, interpolate` | band between two x-curves | `FillBetweenPolyCollection` | poly band | Transpose of `fill_between`. |
| `stackplot` (`stackplot.py:17`) | `x, *ys, labels, colors, baseline{'zero','sym','wiggle','weighted_wiggle'}` | stacked area / streamgraph | `list[PolyCollection]` | poly bands | Free function attached to Axes. Cumulative stacking; baseline algorithms incl. ThemeRiver/streamgraph (`wiggle`). |

### 3.5 Spans & reference lines

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `axhline` (749) | `y, xmin, xmax` (axes-frac) | horizontal full-width line | `Line2D` | line | Uses blended transform (data y, axes x). Not scaled by data limits. |
| `axvline` (832) | `x, ymin, ymax` | vertical full-height line | `Line2D` | line | Blended transform. |
| `axline` (923) | `xy1, xy2 | slope` | infinite line through point(s) | `_AxLine(Line2D)` | line | Recomputes endpoints on every draw to stay infinite; slope not allowed on log axes. |
| `axhspan` (997) | `ymin, ymax, xmin, xmax` | horizontal shaded band | `Rectangle` (Polygon) | patch | Blended transform; full axes width by default. |
| `axvspan` (1052) | `xmin, xmax, ymin, ymax` | vertical shaded band | `Rectangle` | patch | Blended transform. |
| `hlines` (1117) | `y, xmin, xmax, colors, linestyles` | many horizontal segments | `LineCollection` | segments | Vectorized; broadcasting of y vs xmin/xmax. |
| `vlines` (1209) | `x, ymin, ymax, colors, linestyles` | many vertical segments | `LineCollection` | segments | Vectorized. |

### 3.6 2D fields / images / grids

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `imshow` (6212) | `X, cmap, norm, aspect, interpolation, alpha, vmin, vmax, origin, extent, interpolation_stage` | raster image of array | `AxesImage` | raster | RGB(A) or scalar→colormap; resample/interpolation in image space; `extent` & `origin` orient pixels; aspect handling. |
| `pcolor` (6573) | `*args (C or X,Y,C), shading, edgecolors` | colored quad mesh (irregular) | `PolyQuadMesh` (collection) | quad polys | Handles masked cells (drops them); flat shading; per-cell edges; slower than pcolormesh but supports masks/edges. |
| `pcolormesh` (6768) | `*args, shading{'flat','gouraud','nearest','auto'}, cmap, norm` | fast colored mesh | `QuadMesh` | quad mesh | Optimized; gouraud interpolation; rectilinear fast path. Returned by `hist2d`. |
| `pcolorfast` (7000) | `*args` | fastest mesh (regular grids) | `AxesImage` / `PcolorImage` / `QuadMesh` (depends on grid regularity) | raster or mesh | Chooses cheapest representation; experimental; rectilinear → image. |
| `matshow` (8877) | `Z, **kwargs` | matrix as image, origin top-left | `AxesImage` (via `imshow`) | raster | Sets origin='upper', integer ticks, square aspect; for visualizing matrices. |
| `spy` (8739) | `Z, precision, marker, markersize, aspect, origin` | sparsity pattern of matrix | `AxesImage` (image mode) **or** `Line2D` (marker mode) | raster or markers | Two modes: dense image of nonzeros, or marker scatter of nonzero coords. Handles `scipy.sparse`. |

### 3.7 Contours & unstructured (tri)

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `contour` (7202) | `*args (Z or X,Y,Z), levels, colors, cmap, norm, linewidths, linestyles` | iso-lines | `QuadContourSet` (`contour.py:1308`) | LineCollection(s) | Heavy machinery: level selection, `_contour` C-extension marching squares, label support via `ContourLabeler`. |
| `contourf` (7220) | same + filled bands | filled iso-regions | `QuadContourSet` | filled polys | Filled variant; handles `extend` for out-of-range bands. |
| `clabel` (7236) | `CS, levels, fmt, inline, manual` | inline level labels | `list[Text]` | text | Computes label rotation/placement along contour lines; breaks line under label. |
| `tricontour` (`tri/_tricontour.py`) | `*args (Triangulation or x,y[,triangles]), Z, levels` | contour on unstructured mesh | `TriContourSet(ContourSet)` | LineCollection | Needs a `Triangulation`; Delaunay if triangles omitted. |
| `tricontourf` (`tri/_tricontour.py`) | same | filled tri contours | `TriContourSet` | filled polys | As above, filled. |
| `tripcolor` (`tri/_tripcolor.py`) | `*args, C, shading{'flat','gouraud'}` | pseudocolor on triangles | `TriMesh` (gouraud) / `PolyCollection` (flat) | tri mesh/polys | Per-vertex or per-face coloring. |
| `triplot` (`tri/_triplot.py`) | `*args (triangulation), fmt` | the triangulation wireframe | `list[Line2D]` | lines | Draws edges + optional nodes; debugging/mesh viz. |

### 3.8 Vector fields

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `quiver` (5997) | `*args (X,Y,U,V[,C]), scale, units, angles, pivot, width, headwidth, …` | arrow field | `Quiver` (`quiver.py:499`, a `PolyCollection`) | arrow polys | Complex scaling/units model (`'width','height','dots','xy'`); angles `'uv'` vs `'xy'`; autoscale of arrow length. Colorbar-mappable when `C` given. |
| `quiverkey` (5981) | `Q, X, Y, U, label, coordinates, labelpos` | legend/key arrow + label | `QuiverKey` (`quiver.py:277`) | arrow + text | Reference arrow to interpret quiver scale. |
| `barbs` (6008) | `*args (X,Y,U,V[,C]), length, barb_increments, pivot, …` | meteorological wind barbs | `Barbs` (`quiver.py:922`, a `PolyCollection`) | barb polys | Encodes magnitude as flags/barbs/half-barbs; rounding rules. |
| `streamplot` (`streamplot.py`) | `x, y, u, v, density, linewidth, color, cmap, arrowsize, integration_direction, broken_streamlines` | flow streamlines with arrows | `StreamplotSet` (bundles `LineCollection` + `PatchCollection` of FancyArrowPatch) | lines + arrow patches | RK integrator over a grid; density-controlled seeding mask; variable color/width along lines; arrowheads as patches. |

### 3.9 Pie & polar

| Method (line) | Key params | Produces | Artist/Container | Primitive | Complexity / edge cases |
|---|---|---|---|---|---|
| `pie` (3540) | `x, explode, labels, colors, autopct, pctdistance, shadow, startangle, radius, counterclock, wedgeprops, …` | pie chart | `PieContainer` (new) / tuple `(wedges, texts, autotexts)` | `Wedge` patches + Text | Normalizes fractions; angle layout; label + autopct text placement; explode offset; forces equal aspect. |
| `pie_label` (3843) | `container, labels, distance, …` | labels for pie wedges | label artists | text | **New** API to label a `PieContainer` separately. |
| Polar plots | `projection='polar'` (`projections/polar.py`) | — | n/a | — | Not a distinct method: a `PolarAxes` where `plot`, `scatter`, `bar`, `fill` reinterpret (theta, r). Bars become `Wedge`/annular patches. Implementation cost is the *projection/transform*, not new methods. |

### 3.10 Spectral / signal

All in `_axes.py`; mostly thin wrappers that compute a spectrum (via `mlab`) then
call `plot` / `imshow` / `pcolormesh`.

| Method (line) | Produces | Builds on | Notes |
|---|---|---|---|
| `acorr` (1951) | autocorrelation stem/line | `xcorr` | Calls `xcorr(x,x)`. |
| `xcorr` (2026) | cross-correlation | `plot` + `axhline`, or `vlines` | `usevlines` toggles `Line2D` vs `LineCollection`; returns lags, correlation, line, b. |
| `psd` (8071) | power spectral density (line) | `plot` | Welch via `mlab.psd`. |
| `csd` (8183) | cross spectral density | `plot` | `mlab.csd`. |
| `magnitude_spectrum` (8286) | |FFT| magnitude | `plot` | scaling dB/linear. |
| `angle_spectrum` (8373) | unwrapped phase? no — angle | `plot` | wrapped phase. |
| `phase_spectrum` (8443) | unwrapped phase | `plot` | unwrapped. |
| `cohere` (8513) | magnitude-squared coherence | `plot` | `mlab.cohere`. |
| `specgram` (8578) | spectrogram heatmap | `imshow` | STFT → `AxesImage`; returns spectrum, freqs, t, image. |

These are **long-tail**; their value is the DSP math (in `mlab`), not new rendering.

### 3.11 Specialized & composite

| Method / function (loc) | Produces | Artist/Container | Notes |
|---|---|---|---|
| `text` (654) | text at data coords | `Text` | Core annotation primitive. |
| `annotate` (733) | text + optional arrow | `Annotation` (Text + FancyArrowPatch) | `xycoords`/`textcoords`, `arrowprops` (connection + arrow styles). |
| `arrow` (5941) | single arrow | `FancyArrow` (Patch) | Simple data-space arrow; usually annotate is preferred. |
| `table` (`table.py:658`) | data grid / table | `Table` (Artist of `Cell`s) | Layout of rowLabels/colLabels/cellText; used under plots. |
| `Sankey` (`sankey.py:30`, methods `add` 351, `finish` 778) | flow/Sankey diagram | `PatchCollection`/`Polygon`s + Text | **Class, not an Axes method.** Builds flow paths via `Path` arithmetic (`_arc`, `_add_input/_output`). Niche. |
| `indicate_inset` / `indicate_inset_zoom` (433/515) | inset rectangle + connectors | Rectangle + ConnectionPatch | Zoom-indicator decoration. |
| `inset_axes` (360) | child axes | `Axes` | Layout, not a plot type. |
| `secondary_xaxis/yaxis` (556/610) | linked twin axis | `SecondaryAxis` | Decoration. |

### 3.12 3D (mpl_toolkits/mplot3d/axes3d.py — class `Axes3D`)

| Method (line) | Produces | Artist/Collection | Notes |
|---|---|---|---|
| `plot` (2200) | 3D polyline | `Line3D` | `zdir` projects 2D→3D. |
| `scatter` (3147) | 3D point cloud | `Path3DCollection` | `depthshade` fades by depth. |
| `plot_surface` (2376) | parametric surface | `Poly3DCollection` | Quad faces from X,Y,Z grids; shading/normals; rcount/ccount downsampling. |
| `plot_wireframe` (2575) | surface wireframe | `Line3DCollection` | rstride/cstride. |
| `plot_trisurf` (2700) | triangulated surface | `Poly3DCollection` | Delaunay or supplied triangles. |
| `contour` (2869) / `contourf` (2982) | 3D contours | `Line3DCollection` / `Poly3DCollection` | Projected onto a plane via `zdir`/`offset`. |
| `tricontour` (2914) / `tricontourf` (3021) | unstructured 3D contours | as above | |
| `bar` (3248) | 3D bars (flat) | `Patch3DCollection` | Z-projected 2D bars. |
| `bar3d` (3308) | true 3D boxes | `Poly3DCollection` | 6 faces per bar; shading. |
| `quiver` (3483) | 3D arrows | `Line3DCollection` | Arrow segments. |
| `voxels` (3612) | filled volumetric cells | `Poly3DCollection` | Boolean 3D mask → cube faces; face culling between adjacent filled voxels. |
| `errorbar` (3828) | 3D error bars | Line3D collections | Composite. |
| `stem` (4177) | 3D stems | Line3D + markers | Composite. |
| `fill_between` (2246) | 3D ribbon | `Poly3DCollection` | Between two 3D curves. |
| `text` (2158) | 3D text | `Text3D` | |

3D depends on a **z-sorting painter's-algorithm projection layer** (`proj3d`,
`art3d`) — a substantial subsystem; treat the whole of mplot3d as one large tier.

### 3.13 Visualization-adjacent decorations

| Method (loc) | Role |
|---|---|
| `legend` (235) | Auto-collects labeled handles → `Legend` artist (handler map per artist type). |
| `colorbar` (`Figure.colorbar` / `figure.py`, plus `colorbar.py`) | Maps a `ScalarMappable` (scatter/imshow/contourf/hexbin/quiver) to a gradient bar. Essential companion to all colormapped plots. |
| `grid` (`_base.py:3454`) | Toggles axis gridlines (Line2D via the `Axis`). |
| `set_title` (139) / axis labels / ticks | `Text` + `Axis` machinery. |
| `twinx`/`twiny` (`_base.py:4841/4895`) | Shared-axis overlays. |

---

## 4. Compositions: which plots are built from others

Several "plot types" are **not new primitives** — they are recipes over a small core.
Porting the core unlocks them nearly for free:

| Plot | = composition of |
|---|---|
| `errorbar` | `plot` (Line2D) + `vlines/hlines`-style segments (LineCollection) + cap markers |
| `stem` | `vlines` (LineCollection) + markers (Line2D) + baseline (Line2D) |
| `boxplot` | `bxp` rendering = Line2D (whiskers/caps/medians) + Rectangle/Patch (box) + flier markers |
| `violinplot` | `violin` = `fill_between`-like PolyCollection (KDE body) + LineCollection (bars) |
| `step` | `plot` with `drawstyle='steps-*'` |
| `loglog`/`semilogx`/`semilogy` | `plot` + set scale to log |
| `bar`/`barh`/`grouped_bar`/`broken_barh` | tiled `Rectangle` patches (+ optional errorbar) |
| `hist` | binning + `bar` (bar types) or step `Polygon` (step types) |
| `hist2d` | `np.histogram2d` + `pcolormesh` (`QuadMesh`) |
| `stackplot` | cumulative sums + `fill_between` (PolyCollection) |
| `matshow`/`spy` (dense) | `imshow` (`AxesImage`) + axis config |
| `tricontour(f)` | `contour(f)` core over a triangulation instead of a grid |
| `acorr` | `xcorr(x,x)` |
| all spectral methods | DSP math (`mlab`) + `plot`/`imshow`/`pcolormesh` |
| `fill_between(x)` | `fill` semantics specialized to two curves |

**Implication for rizzma:** ~15 of the ~70 methods collapse onto a handful of cores
(Line2D, Rectangle/Patch, PolyCollection band, LineCollection, AxesImage, QuadMesh,
PathCollection, ContourSet). Build those well and the long tail follows cheaply.

---

## 5. Frequency / importance ranking & tiered priority

Ranking blends real-world usage frequency (tutorials, docs gallery prevalence, Stack
Overflow) with implementation leverage (how many other plots it unlocks).

### Tier 1 — "the good parts" (ship first; covers ~80% of real plots)
`plot`, `scatter`, `bar`/`barh`, `hist`, `errorbar`, `fill_between`,
`step`, `axhline`/`axvline`/`axhspan`/`axvspan`/`hlines`/`vlines`, `imshow`,
`legend`, `text`/`annotate`, `colorbar`, `grid`/`set_title`/axis labels.

> Rationale: lines, scatter, bars, histograms, simple fills, reference lines, and a
> raster image cover the overwhelming majority of everyday plotting. Each rides on a
> small primitive set already needed by the foundational components doc.

### Tier 2 — common, higher complexity
`stem`, `boxplot`/`bxp`, `violinplot`/`violin`, `pcolormesh`/`pcolor`, `hist2d`,
`hexbin`, `contour`/`contourf` (+`clabel`), `stackplot`, `fill`/`fill_betweenx`,
`pie`, `eventplot`, `bar_label`, `loglog`/`semilogx`/`semilogy`, `matshow`,
`stairs`, `ecdf`, `broken_barh`, `axline`, `grouped_bar`.

### Tier 3 — vector fields, unstructured, polar
`quiver`/`quiverkey`, `barbs`, `streamplot`, `tricontour(f)`, `tripcolor`,
`triplot`, polar projection, `spy`, `pcolorfast`, `arrow`, `table`.

### Tier 4 — long tail / niche
Spectral family (`psd`, `csd`, `cohere`, `specgram`, `magnitude/angle/phase_spectrum`,
`xcorr`, `acorr`), `Sankey`, `indicate_inset(_zoom)`, `secondary_xaxis/yaxis`.

### Tier 5 — 3D (whole mplot3d subsystem)
`plot3D`, `scatter3D`, `plot_surface`, `plot_wireframe`, `plot_trisurf`,
`contour3D`/`contourf3D`, `bar3d`, `voxels`, `quiver3D`, 3D `errorbar`/`stem`/
`fill_between`/`text`. Gated behind a z-sorted projection layer (`proj3d`, `art3d`).

---

## 6. Dependency analysis (shared primitives per tier)

This connects directly to the foundational-components doc. Each tier's "you must
first have" list:

**Tier 1 needs:**
- `Line2D` (polyline + marker drawing) — `lines.py`
- Marker registry / marker paths — `markers.py`
- `Rectangle`/`Patch` fill+stroke — `patches.py`
- `PathCollection` (scatter) — `collections.py`
- `LineCollection` (hlines/vlines, errorbar bars) — `collections.py`
- `FillBetweenPolyCollection` / `PolyCollection` (fill_between) — `collections.py`
- `AxesImage` + resampling (imshow) — `image.py`
- **Color mapping**: `Colormap`, `Normalize`, `ScalarMappable`, color cycle
- **Transforms**: data↔axes↔display, blended transforms (for axh/axv lines/spans)
- **Axis/ticking**: locators, formatters, autoscaling, `BarContainer`/containers infra
- **Colorbar** (for scatter/imshow color scales)
- Text layout + font (legend, titles, labels, annotate)
- `np.histogram` equivalent (hist)

**Tier 2 adds:**
- `BarContainer`/`StemContainer` patterns (already partly in Tier 1)
- `QuadMesh` + gouraud (pcolormesh/hist2d) — `collections.py:2483`
- `PolyQuadMesh` with masking (pcolor)
- `ContourSet`/`QuadContourSet` + **marching squares** + contour labeling — `contour.py`
- Box/violin **statistics** (`cbook.boxplot_stats`, Gaussian KDE in `mlab`)
- `Wedge` patch + equal-aspect handling (pie)
- `EventCollection` (eventplot)
- Hex tiling + per-bin reduction (hexbin)
- Log scale (`LogScale`, `LogLocator`, `LogFormatter`)

**Tier 3 adds:**
- `Quiver`/`Barbs` (PolyCollection subclasses) + arrow geometry & scaling — `quiver.py`
- `StreamplotSet`: ODE integrator + seeding mask + `FancyArrowPatch` — `streamplot.py`
- `Triangulation` (Delaunay) + `TriMesh`/`TriContourSet` + tri interpolation — `tri/`
- **Polar projection** (`PolarAxes`, `PolarTransform`) — `projections/polar.py`
- `Table`/`Cell` layout

**Tier 4 adds:**
- DSP layer (`mlab`: psd/csd/cohere/specgram/window/detrend) — pure math
- `Path` boolean/arc arithmetic for Sankey

**Tier 5 adds:**
- `proj3d` projection + `art3d` (`*3DCollection`, depth sorting / painter's algorithm)
- 3D-aware autoscaling, shading/normals for surfaces, voxel face culling

Each higher tier strictly extends the lower tiers' primitives — no tier reaches down
past what Tier 1's core already establishes, except the new collection/contour/3D
machinery introduced at its own level.

---

## 7. Total counts & Tier-1 recommendation

- **Distinct public *plotting/visualization* methods on `Axes` (2D):** ~62 — counting
  `plot, scatter, step, errorbar, loglog, semilogx, semilogy, bar, barh, broken_barh,
  grouped_bar, stem, eventplot, bar_label, hist, hist2d, hexbin, boxplot, bxp,
  violinplot, violin, ecdf, stairs, fill, fill_between, fill_betweenx, stackplot,
  axhline, axvline, axline, axhspan, axvspan, hlines, vlines, imshow, pcolor,
  pcolormesh, pcolorfast, matshow, spy, contour, contourf, clabel, tricontour,
  tricontourf, tripcolor, triplot, quiver, quiverkey, barbs, streamplot, pie,
  pie_label, arrow, text, annotate, table, acorr, xcorr, psd, csd, cohere,
  magnitude_spectrum, angle_spectrum, phase_spectrum, specgram`. (`plot_date` is
  removed.)
- **3D methods on `Axes3D`:** ~18 (`plot, scatter, plot_surface, plot_wireframe,
  plot_trisurf, contour, contourf, tricontour, tricontourf, bar, bar3d, quiver,
  voxels, errorbar, stem, fill_between, text` + projections).
- **Grand total of distinct plotting methods catalogued: ~80** (62 2D + 18 3D),
  resting on ~10 core primitives/collections.

**Recommended Tier-1 list to implement first:**
`plot`, `scatter`, `bar`/`barh`, `hist`, `errorbar`, `fill_between`, `step`,
the reference-line/span family (`axhline`/`axvline`/`axhspan`/`axvspan`/`hlines`/`vlines`),
`imshow`, plus the decorations `legend`, `text`/`annotate`, `colorbar`, and
axis/grid/title machinery — because these cover ~80% of real-world plots and share a
single primitive core (Line2D + markers, Rectangle/Patch, PathCollection,
LineCollection, PolyCollection band, AxesImage, color mapping, transforms, axis/ticks).
