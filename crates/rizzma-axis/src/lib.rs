//! Axis machinery for rizzma.
//!
//! `ticker` (locators + formatters), `scale` (linear/log/symlog/logit),
//! `units`/`category` conversion, `dates` (chrono-backed), and `Axis`/`Tick`/`Spine`
//! drawing with autoscaling.
//!
//! Build-order home: Phase 6 of `design/04-implementation-plan.md`.

pub mod axis;
pub mod scale;
pub mod ticker;
