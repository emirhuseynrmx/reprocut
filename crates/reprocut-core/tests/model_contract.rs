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
        r#"{"exit_code":1,"signal":null,"termination":{"kind":"exit_code","value":1},"anchor":"TypeError: currency","anchors":[{"channel":"stderr","text":"TypeError: currency"}],"normalization_schema":1}"#
    );
    assert_eq!(
        fingerprint.anchors()[0].channel(),
        DiagnosticChannel::Stderr
    );
    assert_eq!(fingerprint.normalization_schema(), 1);
}
