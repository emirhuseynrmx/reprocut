//! Core value-model and serialization contracts.

use reprocut_core::{
    CandidateVerdict, ContainmentMechanism, DiagnosticChannel, ExecutionObservation,
    FailureFingerprint, TerminationReason,
};

#[test]
fn inconclusive_is_distinct_from_preserved_and_rejected() {
    assert_ne!(CandidateVerdict::Inconclusive, CandidateVerdict::Preserved);
    assert_ne!(CandidateVerdict::Inconclusive, CandidateVerdict::Rejected);
}

#[test]
fn observation_keeps_bounded_stream_metadata() {
    let observation = ExecutionObservation::new(
        Some(1),
        None,
        b"out".to_vec(),
        b"TypeError: currency".to_vec(),
        false,
        false,
    );

    assert_eq!(observation.exit_code(), Some(1));
    assert_eq!(observation.stderr(), b"TypeError: currency");
    assert_eq!(observation.termination(), TerminationReason::ExitCode(1));
    assert_eq!(observation.containment(), ContainmentMechanism::DirectChild);
}

#[test]
fn timeout_is_a_portable_termination_reason() {
    let observation = ExecutionObservation::new(None, None, Vec::new(), Vec::new(), true, false);

    assert_eq!(observation.termination(), TerminationReason::TimedOut);
}

#[test]
fn fingerprint_is_serializable_and_stable() {
    let fingerprint = FailureFingerprint::new(Some(1), None, "TypeError: currency".into());
    let encoded = serde_json::to_string(&fingerprint).expect("fingerprint must serialize");

    assert_eq!(
        encoded,
        r#"{"mode":"automatic","exit_code":1,"signal":null,"termination":{"kind":"exit_code","value":1},"anchor":"TypeError: currency","anchors":[{"channel":"stderr","text":"TypeError: currency"}],"failure_patterns":[],"reject_patterns":[],"normalization_schema\":5,"oracle_spec_digest":[115,17,16,101,50,192,37,39,18,188,201,177,17,215,174,253,45,167,172,81,126,247,54,9,57,181,156,184,67,25,18,253]}"#
    );
    assert_eq!(
        fingerprint.anchors()[0].channel(),
        DiagnosticChannel::Stderr
    );
    assert_eq!(fingerprint.normalization_schema(), 5);
}
