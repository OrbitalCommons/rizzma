# AGENTS.md — rizzma

Guidance for AI agents (claude, codex, …) working in this repo. This is the repo-specific
and **multi-agent** contract; it pairs with each agent's global config.

## What this is

A Rust reimplementation of the good parts of matplotlib / pyplot, wasm-first. Design lives
in `design/`: `01` architecture, `02` plot types, `03` foundational components, `04`
implementation plan + **execution log (the ground-truth status)**. Read `04`'s execution
log before starting work.

## Multi-agent coordination

- **Talk to other agents over the chat / portal interface — NOT GitHub.** Do not use
  GitHub PR reviews, PR comments, or issues to communicate with another agent. Division of
  labor, cross-reviews, and design disagreements happen in chat. GitHub is for code + CI,
  not agent-to-agent messaging.
- **Agree on lane ownership before implementing.** Claim a crate/feature lane and confirm
  the split in chat before writing code, so two agents never edit the same files at once.
- **Cross-review skeptically — in chat.** Favor long-term maintainability over raw speed
  when they conflict. (Both agents currently push as the same GitHub account, so GitHub's
  approve / request-changes is unavailable regardless.)
- **Many small PRs.** Each PR small, single-purpose, independently reviewable, and green —
  so review and merge stay fast. Add a smaller step when you spot something suboptimal.
- **Shared surfaces need a heads-up in chat before any structural change:** the
  `rizzma-render` `Renderer` trait, and `rizzma-core` (geometry / color / rcparams).

## Workflow

- Branch `meawoppl/<topic>`; never commit to `main`. `main` is squash-only and protected by
  the required `fmt + clippy + test` status check.
- Definition of Done: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` all green; edition 2024; no `unsafe`; doc
  comments on new public items.
- A PR that adds or changes a **plotting method** also adds its `gallery.rs` case, a
  runnable `///` example, and the embedded gh-pages image — CI's `cargo xtask
  check-gallery-links --strict` and `cargo doc -D warnings` enforce this.
- Add dependencies with `cargo add`. No C dependencies in the default build; keep every
  crate `wasm32`-buildable or `cfg`-gated.

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p rizzma-figure --example gallery   # render the example gallery
cargo xtask check-gallery-links --strict       # doc ⇄ generated-image consistency
```
