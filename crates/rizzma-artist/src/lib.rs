//! Artist scene graph and drawable primitives for rizzma.
//!
//! An arena-owned (slotmap) artist tree drawn by zorder, plus `Line2D`, `MarkerStyle`,
//! the `Patch` hierarchy, `hatch`, and the batched `Collection` family
//! (`PathCollection`/`LineCollection`/`PolyCollection`/`QuadMesh`).
//!
//! Build-order home: Phase 5 of `design/04-implementation-plan.md`.
