//! Errors from portable-figure export, import, and inspection.

/// Errors from portable-figure export, import, and inspection.
///
/// The read paths are strict by design: malformed containers, out-of-bounds
/// accessors, unknown fields or enum variants, budget overruns, and future
/// schema versions all fail loudly instead of best-effort rendering. A figure
/// that silently dropped content would be a scientifically wrong figure.
#[derive(Debug)]
pub enum PortableError {
    /// The figure holds state the wire format cannot represent (for example a
    /// tick formatter backed by a Rust closure). Exporting it would change how
    /// the figure renders, so export refuses instead.
    Unsupported(String),
    /// The artifact bytes are structurally invalid: bad magic, truncated or
    /// duplicate chunks, out-of-bounds accessors, inconsistent array lengths.
    Malformed(String),
    /// The artifact exceeds a caller-supplied budget (see [`Limits`]).
    ///
    /// A trusted renderer still parses attacker-influenced data, and memory
    /// safety removes a corruption class rather than parser denial-of-service,
    /// so budgets are enforced before allocating.
    ///
    /// [`Limits`]: super::Limits
    Budget(String),
    /// The artifact's schema version is outside this build's supported range.
    Schema {
        /// The version the artifact declares.
        found: u32,
        /// Oldest version this build supports.
        min: u32,
        /// Newest version this build supports.
        max: u32,
    },
    /// The JSON spec chunk failed to serialize or deserialize (including
    /// unknown fields and unknown enum variants, which are rejected).
    Json(String),
    /// Filesystem I/O failed while reading or writing an artifact.
    Io(std::io::Error),
}

impl std::fmt::Display for PortableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortableError::Unsupported(msg) => {
                write!(f, "figure cannot be exported portably: {msg}")
            }
            PortableError::Malformed(msg) => write!(f, "malformed portable figure: {msg}"),
            PortableError::Budget(msg) => write!(f, "portable figure exceeds a budget: {msg}"),
            PortableError::Schema { found, min, max } => write!(
                f,
                "artifact is schema {found}; this build supports {min}..={max} — \
                 rendering it would drop content"
            ),
            PortableError::Json(msg) => write!(f, "portable figure spec error: {msg}"),
            PortableError::Io(err) => write!(f, "portable figure i/o error: {err}"),
        }
    }
}

impl std::error::Error for PortableError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PortableError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PortableError {
    fn from(err: std::io::Error) -> Self {
        PortableError::Io(err)
    }
}

impl From<serde_json::Error> for PortableError {
    fn from(err: serde_json::Error) -> Self {
        PortableError::Json(err.to_string())
    }
}
