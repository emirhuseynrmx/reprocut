//! Correctness primitives for ReproCut.

mod model;
mod oracle;
mod reducer;
mod winner;

pub use model::{
    CandidateVerdict, DiagnosticAnchor, DiagnosticChannel, ExecutionObservation, FailureFingerprint,
};
pub use oracle::{normalize_diagnostic, FailureOracle, OracleError};
pub use reducer::{reduce, ReductionResult, ReductionUnit};
pub use winner::LowestWinner;
