//! Correctness primitives for ReproCut.

mod model;
mod oracle;

pub use model::{CandidateVerdict, ExecutionObservation, FailureFingerprint};
pub use oracle::{normalize_diagnostic, FailureOracle, OracleError};
