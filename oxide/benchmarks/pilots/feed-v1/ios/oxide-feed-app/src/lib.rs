//! Production-path Oxide treatment for the frozen feed-v1 device pilot.

#![deny(unsafe_op_in_unsafe_fn, rust_2018_idioms)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::large_enum_variant)]
#![warn(missing_docs)]

/// Frozen workload identity and deterministic row recipe.
pub mod contract;
/// Strict run-record schema and nonce-scoped observation transport.
pub mod observation;
