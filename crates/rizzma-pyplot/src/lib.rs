//! pyplot-style stateful facade for rizzma.
//!
//! A slim global layer over a `FigureRegistry` (`gcf`/`gca`/`figure`/`subplots`/`show`/
//! `savefig`) delegating to the object-oriented API. No REPL/displayhook machinery.
//!
//! Build-order home: Phase 8 of `design/04-implementation-plan.md`.
