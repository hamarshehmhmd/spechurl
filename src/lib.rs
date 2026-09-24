//! spechurl turns an OpenAPI 3.x document into a directory of [Hurl](https://hurl.dev)
//! contract tests and can check that a committed suite still matches the spec.
//!
//! The CLI in `src/main.rs` is a thin wrapper around [`generate_suite`],
//! [`write_suite`], and [`check_suite`].

#![forbid(unsafe_code)]

mod error;
mod render;
mod sample;
mod spec;
mod suite;

pub use error::{Error, Result};
pub use suite::{check_suite, generate_from_str, generate_suite, write_suite, Suite};

/// Crate version, matching `spechurl --version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
