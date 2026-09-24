use std::path::PathBuf;

/// Errors produced while loading a spec, generating a suite, or checking drift.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The spec file could not be read.
    #[error("could not read '{path}': {source}")]
    Read {
        /// Path that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A generated file could not be written.
    #[error("could not write '{path}': {source}")]
    Write {
        /// Path that could not be written.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The document is not valid YAML or JSON, or does not match the OpenAPI subset we read.
    #[error("could not parse '{path}': {message}")]
    Parse {
        /// Spec path (or name) that failed to parse.
        path: PathBuf,
        /// Parser or deserializer message.
        message: String,
    },
    /// The document parsed, but it is not a supported OpenAPI contract.
    #[error("{message}")]
    Spec {
        /// Human-readable explanation.
        message: String,
    },
    /// The committed Hurl suite does not match what `generate` would write.
    #[error("{message}")]
    Drift {
        /// Human-readable diff summary.
        message: String,
    },
}

impl Error {
    pub(crate) fn spec(message: impl Into<String>) -> Self {
        Self::Spec {
            message: message.into(),
        }
    }

    pub(crate) fn drift(message: impl Into<String>) -> Self {
        Self::Drift {
            message: message.into(),
        }
    }

    pub(crate) fn parse(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Parse {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Process exit code: `1` when the suite drifted, `2` for usage and spec errors.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Drift { .. } => 1,
            _ => 2,
        }
    }

    pub(crate) fn in_context(self, context: &str) -> Self {
        match self {
            Self::Spec { message } => Self::Spec {
                message: format!("{context}: {message}"),
            },
            other => other,
        }
    }
}

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;
