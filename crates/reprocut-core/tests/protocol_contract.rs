use reprocut_core::{
    ProgressEventV1, ProtocolAction, ProtocolError, ReductionRequestV1, PROTOCOL_VERSION,
};

#[test]
fn request_defaults_are_explicit_and_version_is_strict() {
    let request: ReductionRequestV1 = serde_json::from_str(
        r#"{"protocol_version":1,"action":"minimize","root":"bug","output":"minimal"}"#,
    )
    .expect("request");

    request.validate().expect("supported request");
    assert_eq!(request.ecosystem, "auto");
    assert_eq!(request.preparation, "offline");
    assert_eq!(request.timeout_ms, 5_000);
    assert_eq!(request.oracle_mode, "automatic");

    let mut unsupported = request;
    unsupported.protocol_version = 9;
    assert_eq!(
        unsupported.validate(),
        Err(ProtocolError::UnsupportedVersion {
            found: 9,
            supported: PROTOCOL_VERSION,
        })
    );
}

#[test]
fn protocol_rejects_incomplete_or_cross_mode_integrity_fields() {
    for document in [
        r#"{"protocol_version":1,"action":"minimize","root":"bug","output":"minimal","oracle_mode":"regex"}"#,
        r#"{"protocol_version":1,"action":"minimize","root":"bug","output":"minimal","oracle_mode":"exit_zero","reject_patterns":["wrong"]}"#,
        r#"{"protocol_version":1,"action":"minimize","root":"bug","output":"minimal","preparation":"isolated_python"}"#,
    ] {
        let request: ReductionRequestV1 = serde_json::from_str(document).expect("request JSON");
        assert!(matches!(
            request.validate(),
            Err(ProtocolError::InvalidConfiguration(_))
        ));
    }

    let valid: ReductionRequestV1 = serde_json::from_str(
        r#"{"protocol_version":1,"action":"minimize","root":"bug","output":"minimal","oracle_mode":"regex","failure_patterns":["TypeError"],"preparation":"isolated_python","python_executable":"python","python_wheelhouse":"wheels"}"#,
    )
    .expect("valid request JSON");
    valid.validate().expect("valid integrity contract");
}

#[test]
fn events_are_one_tagged_additive_json_value() {
    let event = ProgressEventV1::Completed {
        protocol_version: PROTOCOL_VERSION,
        output: "minimal".into(),
        evidence: "minimal/reduction.json".into(),
        report: "minimal/report.html".into(),
        issue: "minimal/issue.md".into(),
    };
    let json = serde_json::to_string(&event).expect("event JSON");
    assert!(json.starts_with(r#"{"type":"completed","protocol_version":1"#));
    assert_eq!(
        serde_json::from_str::<ProgressEventV1>(&json).expect("round trip"),
        event
    );
}

#[test]
fn resume_restart_conflict_fails_before_work() {
    let request = ReductionRequestV1 {
        protocol_version: PROTOCOL_VERSION,
        action: ProtocolAction::Resume,
        root: "bug".into(),
        output: "minimal".into(),
        ecosystem: "auto".to_owned(),
        preparation: "offline".to_owned(),
        command: Vec::new(),
        timeout_ms: 5_000,
        max_output_bytes: 1_048_576,
        oracle_stream: "auto".to_owned(),
        oracle_mode: "automatic".to_owned(),
        failure_patterns: Vec::new(),
        reject_patterns: Vec::new(),
        python_executable: None,
        python_wheelhouse: None,
        python_extras: Vec::new(),
        prepare_spec: None,
        flaky_runs: None,
        flaky_required: None,
        jobs: 0,
        state: None,
        restart: true,
    };
    assert_eq!(
        request.validate(),
        Err(ProtocolError::ResumeRestartConflict)
    );
}

#[test]
fn event_stream_matches_the_reviewed_jsonl_golden() {
    let events = [
        ProgressEventV1::Started {
            protocol_version: PROTOCOL_VERSION,
            action: ProtocolAction::Minimize,
            root: "bug".into(),
        },
        ProgressEventV1::BaselineStable {
            protocol_version: PROTOCOL_VERSION,
            fingerprint_sha256: "abc123".to_owned(),
        },
        ProgressEventV1::Completed {
            protocol_version: PROTOCOL_VERSION,
            output: "minimal".into(),
            evidence: "minimal/reduction.json".into(),
            report: "minimal/report.html".into(),
            issue: "minimal/issue.md".into(),
        },
    ];
    let actual = events
        .iter()
        .map(|event| serde_json::to_string(event).expect("event"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        actual,
        include_str!("../../../tests/golden/protocol-events.jsonl").replace("\r\n", "\n")
    );
}
