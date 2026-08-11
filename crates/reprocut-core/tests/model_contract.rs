use reprocut_core::{CandidateVerdict, ExecutionObservation, FailureFingerprint};

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
}

#[test]
fn fingerprint_is_serializable_and_stable() {
    let fingerprint =
        FailureFingerprint::new(Some(1), None, "TypeError: currency".into());
    let encoded = serde_json::to_string(&fingerprint).expect("fingerprint must serialize");

    assert_eq!(
        encoded,
        r#"{"exit_code":1,"signal":null,"anchor":"TypeError: currency"}"#
    );
}
