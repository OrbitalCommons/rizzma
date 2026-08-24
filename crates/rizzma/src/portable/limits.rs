//! Caller-supplied budgets enforced before allocating.
//!
//! A trusted renderer still parses attacker-influenced data, and memory safety
//! removes a corruption class rather than parser denial-of-service. These
//! budgets belong to the **host**, not to rizzma's opinion of what is
//! reasonable, so every entry point takes them explicitly.

/// Budgets a host imposes on an artifact it did not produce.
///
/// [`Limits::default`] is deliberately generous against a typical figure (a
/// 2,000-point line is roughly 35 KB) and low enough to still be a real bound.
/// Tighten any field for untrusted input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum size of the whole artifact, in bytes.
    pub max_total_bytes: usize,
    /// Maximum number of chunks in the container.
    pub max_chunks: usize,
    /// Maximum size of the JSON spec chunk, in bytes.
    pub max_json_bytes: usize,
    /// Maximum size of the poster chunk, in bytes.
    pub max_poster_bytes: usize,
    /// Maximum backing-store pixels (width × height × device pixel ratio²) a
    /// consumer should rasterize for this figure.
    pub max_canvas_pixels: usize,
}

impl Limits {
    /// The suggested host defaults: 10 MiB total, 16 chunks, 1 MiB of JSON,
    /// 4 MiB of poster, and 2 megapixels of backing store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_total_bytes: 10 * 1024 * 1024,
            max_chunks: 16,
            max_json_bytes: 1024 * 1024,
            max_poster_bytes: 4 * 1024 * 1024,
            max_canvas_pixels: 2_000_000,
        }
    }

    /// Set the total artifact budget, returning `self` for chaining.
    #[must_use]
    pub const fn with_max_total_bytes(mut self, bytes: usize) -> Self {
        self.max_total_bytes = bytes;
        self
    }

    /// Set the JSON chunk budget, returning `self` for chaining.
    #[must_use]
    pub const fn with_max_json_bytes(mut self, bytes: usize) -> Self {
        self.max_json_bytes = bytes;
        self
    }

    /// Set the backing-store pixel budget, returning `self` for chaining.
    #[must_use]
    pub const fn with_max_canvas_pixels(mut self, pixels: usize) -> Self {
        self.max_canvas_pixels = pixels;
        self
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::new()
    }
}
