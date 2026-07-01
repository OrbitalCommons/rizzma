# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/) (pre-1.0, so minor bumps may carry
breaking changes).

The whole workspace shares one version (`workspace.package.version`), and the
umbrella `rizzma` crate re-exports the `rizzma-*` crates. Bumping the version on a
push to `main` triggers the publish workflow (`.github/workflows/publish.yml`).

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
