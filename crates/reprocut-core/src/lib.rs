//! Correctness primitives for ReproCut.

mod model;
mod oracle;
mod reducer;

pub use model::{CandidateVerdict, ExecutionObservation, FailureFingerprint};
pub use oracle::{normalize_diagnostic, FailureOracle, OracleError};
pub use reducer::{reduce, ReductionResult, ReductionUnit};
