//! The renderer seam for rizzma.
//!
//! Defines the `Renderer` trait (`draw_path`/`draw_markers`/`draw_path_collection`/
//! `draw_image`/`draw_text`) plus the `GraphicsContext`/`Paint` state every backend
//! consumes. This is the one abstraction that must survive the port.
//!
//! Build-order home: Phase 3 of `design/04-implementation-plan.md`.
