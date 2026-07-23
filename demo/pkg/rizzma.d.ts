/* tslint:disable */
/* eslint-disable */

/**
 * An [`Axes3D`](crate::mplot3d::Axes3D) owned across the wasm boundary.
 *
 * 3D scenes render as full frames rather than through the interactive
 * session machinery: build the scene once, then call `render` (wasm only)
 * each time the view changes — `set_view` plus a JS interval is a spinning
 * plot. `width_px`/`height_px`/`dpi` are fixed at construction and match
 * [`Axes3D::render_png`](crate::mplot3d::Axes3D::render_png) semantics
 * (`dpi` scales titles and decorations).
 */
export class WasmAxes3D {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Create an empty `width_px` by `height_px` scene rendered at `dpi`.
     */
    constructor(width_px: number, height_px: number, dpi: number);
    /**
     * Add a flat-shaded colormapped surface over the `x` × `y` grid; `z` is
     * row-major with `x.len() * y.len()` heights. Degenerate input adds
     * nothing.
     */
    plot_surface(x: Float64Array, y: Float64Array, z: Float64Array): void;
    /**
     * Render the scene onto the canvas element with id `canvas_id`,
     * HiDPI-crisp: the backing store is `devicePixelRatio` × the logical
     * pixel size (decorations scale to match) and the canvas CSS size is
     * set to the logical size. Call again after `set_view` to animate.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if the canvas element cannot be found,
     * is not a canvas, has no 2D context, or `putImageData` fails.
     */
    render(canvas_id: string): void;
    /**
     * Add a cloud of 3D scatter markers (common prefix of the slices).
     */
    scatter3d(x: Float64Array, y: Float64Array, z: Float64Array): void;
    /**
     * Set a title drawn centered at the top of the canvas.
     */
    set_title(title: string): void;
    /**
     * Set the elevation and azimuth view angles in degrees.
     */
    set_view(elev: number, azim: number): void;
}

/**
 * A [`Figure`] owned across the wasm boundary, with an interactive
 * pixel-to-data readout for DOM hover.
 *
 * Construct one with [`WasmFigure::sample`], read its pixel size via
 * [`WasmFigure::size`], render it to a canvas with `WasmFigure::render`
 * (wasm only — `#[cfg(target_arch = "wasm32")]`, so it can't be an intra-doc
 * link on the host docs build), and translate cursor pixels to data
 * coordinates with [`WasmFigure::data_at`].
 */
export class WasmFigure {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Add axes at the figure-fraction rectangle `(left, bottom, width,
     * height)`, returning the new axes' index.
     */
    add_axes(l: number, b: number, w: number, h: number): number;
    /**
     * Add axes for 1-based cell `index` of an `nrows` x `ncols` grid,
     * returning the new axes' index.
     *
     * # Errors
     *
     * Returns an error if `index` is zero or exceeds `nrows * ncols`.
     */
    add_subplot(nrows: number, ncols: number, index: number): number;
    /**
     * Consume this figure and attach it to the canvas with id `canvas_id`
     * as an interactive session: HiDPI rendering plus wheel zoom
     * (anchored at the cursor), left-drag pan, double-click reset, and a
     * hover callback. Keep the returned [`WasmSession`] alive for as long
     * as the canvas should stay interactive.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if the canvas element cannot be found,
     * is not a canvas, has no 2D context, or rendering fails.
     */
    bind(canvas_id: string): WasmSession;
    /**
     * Map a **top-down canvas pixel** `(px, py)` to data coordinates in the
     * figure's first axes.
     *
     * Returns `Some([x, y])` when the pixel falls inside the axes rectangle,
     * else `None`. Across the wasm boundary this maps to a
     * `Float64Array | undefined`, so a hover readout can show `undefined`
     * (off-axes) versus a concrete `[x, y]`.
     */
    data_at(px: number, py: number): Float64Array | undefined;
    /**
     * Display row-major scalar `data` (`nrows` × `ncols`) as a colormapped
     * image on axes `axes` — `extent` is `[x0, x1, y0, y1]` in data space,
     * `cmap` a colormap name (empty string for the default), and
     * `vmin`/`vmax` the fixed normalization bounds (live updates through
     * `WasmSession::set_image_data` keep them, so streaming frames don't
     * flicker). Data row `0` sits at the top of the extent.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range, `extent` is not 4 numbers,
     * or `data.len()` is not `nrows * ncols`.
     */
    imshow(axes: number, data: Float64Array, nrows: number, ncols: number, extent: Float64Array, cmap: string, vmin: number, vmax: number): void;
    /**
     * Add a legend to axes `axes`: label `i` is paired with the color of the
     * `i`-th plotted line.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range or there are more labels
     * than lines.
     */
    legend(axes: number, labels: string[]): void;
    /**
     * The effective `[xlo, xhi, ylo, yhi]` limits of axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    limits(axes: number): Float64Array;
    /**
     * Create an empty `width_in` by `height_in` inch figure (default DPI).
     */
    constructor(width_in: number, height_in: number);
    /**
     * Switch axes `axes` to oscilloscope styling: CRT background, fixed
     * phosphor graticule, phosphor trace cycle, and in-frame corner
     * readouts — built to stay legible at any size, down to sparkline
     * strips. Call before plotting so traces pick up the phosphor cycle.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    oscilloscope(axes: number): void;
    /**
     * Plot `y` against `x` as a line on axes `axes`, using the color cycle.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    plot(axes: number, x: Float64Array, y: Float64Array): void;
    /**
     * Plot a styled line: `style` is a plain object with optional keys
     * `color` (matplotlib color spec string), `lw` (points), and `ls`
     * (`'-'`, `'--'`, `':'`, `'-.'` or long names). Unknown keys are errors.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range or `style` is invalid.
     */
    plot_styled(axes: number, x: Float64Array, y: Float64Array, style: any): void;
    /**
     * Render this figure onto the canvas element with id `canvas_id`,
     * HiDPI-crisp: the backing store is `devicePixelRatio` × the figure's
     * logical pixel size and the canvas CSS size is set to the logical
     * size.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if the canvas element cannot be found, is
     * not a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
     */
    render(canvas_id: string): void;
    /**
     * Build a [`WasmFigure`] wrapping the built-in [`sample_figure`].
     */
    static sample(): WasmFigure;
    /**
     * Scatter-plot `y` against `x` on axes `axes`, using the color cycle.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    scatter(axes: number, x: Float64Array, y: Float64Array): void;
    /**
     * Set the figure's canvas background color from a matplotlib-style
     * color spec (name, hex, `tab:*`, `C0`…) — e.g. a dark face behind
     * full-bleed oscilloscope strips.
     *
     * # Errors
     *
     * Returns an error if the color spec is not recognized.
     */
    set_facecolor(color: string): void;
    /**
     * Replace the data of line `line` on axes `axes` in place (live
     * updates), keeping its style. Autoscaled limits re-derive; explicit
     * limits are untouched.
     *
     * # Errors
     *
     * Returns an error if `axes` or `line` is out of range.
     */
    set_line_data(axes: number, line: number, x: Float64Array, y: Float64Array): void;
    /**
     * Set the title of axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_title(axes: number, title: string): void;
    /**
     * Set the x-axis label of axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_xlabel(axes: number, label: string): void;
    /**
     * Set explicit x limits on axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_xlim(axes: number, lo: number, hi: number): void;
    /**
     * Switch axes `axes` to a log-scaled x axis with `base`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_xscale_log(axes: number, base: number): void;
    /**
     * Set the y-axis label of axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_ylabel(axes: number, label: string): void;
    /**
     * Set explicit y limits on axes `axes`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_ylim(axes: number, lo: number, hi: number): void;
    /**
     * Switch axes `axes` to a log-scaled y axis with `base`.
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    set_yscale_log(axes: number, base: number): void;
    /**
     * Link `follower`'s x-limits to `leader`'s (matplotlib's `sharex`):
     * pan/zoom on either axes keeps the pair's x in lockstep while each y
     * stays independent.
     *
     * # Errors
     *
     * Returns an error if either index is out of range, the two are equal,
     * or `leader` itself already follows another axes.
     */
    sharex(follower: number, leader: number): void;
    /**
     * The figure's pixel size as a 2-element `[width, height]` array.
     *
     * Exposed across the wasm boundary as a `Float64Array`; callers size the
     * target canvas to `size[0]` by `size[1]`.
     */
    readonly size: Float64Array;
}

/**
 * An interactive figure bound to a canvas.
 *
 * Created by [`WasmFigure::bind`]. **Keep the session alive** (a
 * variable, an array, a field — any live JS reference) for as long as
 * the canvas should stay interactive. Dropping it — explicitly via
 * `session.free()` or implicitly when the GC finalizes an unreferenced
 * session — leaves the last frame on the canvas and detaches every DOM
 * listener it registered, so the canvas goes cleanly inert instead of
 * throwing "closure invoked after being dropped" on later events.
 */
export class WasmSession {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Map a **logical canvas pixel** to `[axes, x, y]` data coordinates,
     * or `undefined` when the pixel is over no axes.
     */
    data_at(px: number, py: number): Float64Array | undefined;
    /**
     * The effective `[xlo, xhi, ylo, yhi]` limits of axes `axes` (the
     * live values pan/zoom mutate).
     *
     * # Errors
     *
     * Returns an error if `axes` is out of range.
     */
    limits(axes: number): Float64Array;
    /**
     * Register a hover callback, called as `cb(axes, x, y)` while the
     * cursor is over axes data and `cb(null)` when it leaves the canvas.
     */
    on_hover(cb: Function): void;
    /**
     * Repaint the canvas now (outside the rAF coalescing).
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if `ImageData`/`putImageData` fails.
     */
    render(): void;
    /**
     * Replace the data and extent of image `image` on axes `axes` in
     * place (live updates — e.g. a scrolling spectrogram), keeping its
     * colormap and `vmin`/`vmax` normalization, and schedule a
     * rAF-coalesced repaint. `extent` is `[x0, x1, y0, y1]` in data
     * space.
     *
     * Autoscaled limits re-derive from the new extent; explicit limits
     * — including a view the user has panned/zoomed — are untouched.
     *
     * # Errors
     *
     * Returns an error if `axes` or `image` is out of range, `extent`
     * is not 4 numbers, or `data.len()` is not `nrows * ncols`.
     */
    set_image_data(axes: number, image: number, data: Float64Array, nrows: number, ncols: number, extent: Float64Array): void;
    /**
     * Replace the data of line `line` on axes `axes` in place (live
     * updates) and schedule a rAF-coalesced repaint: a burst of updates
     * between frames paints once.
     *
     * Autoscaled limits re-derive from the new data; explicit limits —
     * including a view the user has panned/zoomed — are untouched.
     *
     * # Errors
     *
     * Returns an error if `axes` or `line` is out of range.
     */
    set_line_data(axes: number, line: number, x: Float64Array, y: Float64Array): void;
    /**
     * Replace the offsets of scatter collection `collection` on axes
     * `axes` in place (live updates), keeping its markers and styling,
     * and schedule a rAF-coalesced repaint. Only the common prefix of
     * `x` and `y` is used.
     *
     * Autoscaled limits re-derive from the new offsets; explicit limits
     * — including a view the user has panned/zoomed — are untouched.
     *
     * # Errors
     *
     * Returns an error if `axes` or `collection` is out of range.
     */
    set_scatter_offsets(axes: number, collection: number, x: Float64Array, y: Float64Array): void;
    /**
     * Record the cursor's data-space position into line `line` of axes
     * `axes` as a rolling trail of up to `capacity` points — updated
     * entirely in Rust as pointer events arrive, with no JS in the loop.
     *
     * Each repaint is rAF-coalesced like any other update, and pan/zoom
     * keep working while the trail records (the trail pauses during a
     * drag, when the cursor is panning rather than hovering).
     *
     * # Errors
     *
     * Returns an error if `axes` or `line` is out of range, or
     * `capacity` is zero.
     */
    track_cursor(axes: number, line: number, capacity: number): void;
    /**
     * The figure's **logical** pixel size as `[width, height]` (CSS
     * pixels; multiply by `devicePixelRatio` for the backing size).
     */
    readonly size: Float64Array;
}

/**
 * Render the built-in [`sample_figure`] onto the canvas element with id
 * `canvas_id` (HiDPI-crisp, non-interactive).
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if the canvas element cannot be found, is not
 * a canvas, has no 2D context, or `ImageData`/`putImageData` fails.
 */
export function draw_sample_to_canvas(canvas_id: string): void;

/**
 * Blit a straight-RGBA8 buffer onto the canvas element with id
 * `canvas_id`, sizing the backing store to `width` by `height` device
 * pixels (no CSS sizing — presentation scale is the caller's concern).
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if the canvas cannot be found or resolved to a
 * 2D context, or if constructing/placing the `ImageData` fails.
 */
export function render_rgba_to_canvas(canvas_id: string, rgba: Uint8Array, width: number, height: number): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmsession_free: (a: number, b: number) => void;
    readonly draw_sample_to_canvas: (a: number, b: number) => [number, number];
    readonly render_rgba_to_canvas: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly wasmaxes3d_render: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_bind: (a: number, b: number, c: number) => [number, number, number];
    readonly wasmfigure_render: (a: number, b: number, c: number) => [number, number];
    readonly wasmsession_data_at: (a: number, b: number, c: number) => [number, number];
    readonly wasmsession_limits: (a: number, b: number) => [number, number, number, number];
    readonly wasmsession_on_hover: (a: number, b: any) => void;
    readonly wasmsession_render: (a: number) => [number, number];
    readonly wasmsession_set_image_data: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => [number, number];
    readonly wasmsession_set_line_data: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly wasmsession_set_scatter_offsets: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly wasmsession_size: (a: number) => [number, number];
    readonly wasmsession_track_cursor: (a: number, b: number, c: number, d: number) => [number, number];
    readonly __wbg_wasmaxes3d_free: (a: number, b: number) => void;
    readonly __wbg_wasmfigure_free: (a: number, b: number) => void;
    readonly wasmaxes3d_new: (a: number, b: number, c: number) => number;
    readonly wasmaxes3d_plot_surface: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly wasmaxes3d_scatter3d: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly wasmaxes3d_set_title: (a: number, b: number, c: number) => void;
    readonly wasmaxes3d_set_view: (a: number, b: number, c: number) => void;
    readonly wasmfigure_add_axes: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly wasmfigure_add_subplot: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly wasmfigure_data_at: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_imshow: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => [number, number];
    readonly wasmfigure_legend: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_limits: (a: number, b: number) => [number, number, number, number];
    readonly wasmfigure_new: (a: number, b: number) => number;
    readonly wasmfigure_oscilloscope: (a: number, b: number) => [number, number];
    readonly wasmfigure_plot: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly wasmfigure_plot_styled: (a: number, b: number, c: number, d: number, e: number, f: number, g: any) => [number, number];
    readonly wasmfigure_sample: () => number;
    readonly wasmfigure_scatter: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly wasmfigure_set_facecolor: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_set_line_data: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly wasmfigure_set_title: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_set_xlabel: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_set_xlim: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_set_xscale_log: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_set_ylabel: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_set_ylim: (a: number, b: number, c: number, d: number) => [number, number];
    readonly wasmfigure_set_yscale_log: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_sharex: (a: number, b: number, c: number) => [number, number];
    readonly wasmfigure_size: (a: number) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h2a2e651327021123: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h35142953de109de2: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
