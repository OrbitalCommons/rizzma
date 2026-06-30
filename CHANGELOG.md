# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). While the project is
pre-1.0, releases use `0.0.x` patch bumps; the version will move to `0.1.0` when
the umbrella `rizzma` crate re-exports the workspace's public API.

The version number lives in `crates/rizzma/Cargo.toml`. Bumping it on a push to
`main` triggers the publish workflow (`.github/workflows/publish.yml`), which
publishes the new version to crates.io.

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
