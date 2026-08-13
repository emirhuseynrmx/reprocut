//! Paste-ready issue rendering and escaping contracts.

use reprocut_report::{
    render_issue, EvaluationPolicyEvidence, FailureEvidence, MaterialMeasurement, MeasurementSet,
    PreparationEvidence, ReductionEvidence, RetentionEvidence, SearchEvidence,
    EVIDENCE_SCHEMA_VERSION,
};

#[test]
fn issue_contains_the_same_fingerprint_command_tree_and_measurements() {
    let issue = render_issue(&fixture("ValueError: sentinel"));

    assert!(issue.starts_with("# Minimal reproduction: ValueError: sentinel"));
    assert!(issue.contains(&format!("`{}`", "a".repeat(64))));
    assert!(issue.contains("python bug.py \"two words\""));
    assert!(issue.contains("| Files | 18 | 3 | 15 |"));
    assert!(issue.contains("- `bug.py`"));
    assert!(issue.contains("`attempts.jsonl`"));
}

#[test]
fn hostile_markdown_and_embedded_fences_cannot_escape_their_sections() {
    let issue = render_issue(&fixture("<script>alert(1)</script>\n```\nowned"));

    assert!(!issue.contains("# Minimal reproduction: <script>"));
    assert!(issue.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(issue.contains("````text\n<script>alert(1)</script>\n```\nowned\n````"));
}

fn fixture(anchor: &str) -> ReductionEvidence {
    ReductionEvidence {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        source_root: "fixture".to_owned(),
        source_snapshot_sha256: "1".repeat(64),
        output: "minimal".to_owned(),
        command: vec![
            "python".to_owned(),
            "bug.py".to_owned(),
            "two words".to_owned(),
        ],
        ecosystem: "python".to_owned(),
        preparation: PreparationEvidence {
            mode: "offline".to_owned(),
            contract_sha256: Some("2".repeat(64)),
            limitations: Vec::new(),
        },
        measurements: MeasurementSet {
            original: MaterialMeasurement {
                files: 18,
                bytes: 4_096,
                lines: 300,
                syntax_nodes: None,
            },
            retained: MaterialMeasurement {
                files: 3,
                bytes: 512,
                lines: 40,
                syntax_nodes: None,
            },
            elapsed_ms: 4_200,
        },
        search: SearchEvidence {
            attempts: 41,
            file_attempts: 31,
            structured_attempts: 10,
            inconclusive_attempts: 2,
            cache_hits: 7,
            baseline_runs: 3,
            final_verifications: 3,
            jobs: 4,
            state: Some("state.sqlite3".to_owned()),
            resumed: false,
            accepted_file_sizes: vec![18, 9, 5, 3],
            evaluation_policy: EvaluationPolicyEvidence {
                mode: "strict".to_owned(),
                runs: 3,
                required: 3,
            },
        },
        failure: FailureEvidence {
            same_failure: true,
            fingerprint_sha256: "a".repeat(64),
            exit_code: Some(1),
            signal: None,
            termination: "exit 1".to_owned(),
            oracle_stream: "stderr".to_owned(),
            oracle_mode: "automatic".to_owned(),
            anchor: anchor.to_owned(),
            anchors: Vec::new(),
            normalization_schema: 4,
            failure_patterns: Vec::new(),
            reject_patterns: Vec::new(),
            oracle_spec_sha256: "b".repeat(64),
        },
        kept_files: vec![RetentionEvidence {
            path: "bug.py".to_owned(),
            observation: "Present in the final verified snapshot.".to_owned(),
        }],
        accepted_structured_edits: Vec::new(),
        attempts: Vec::new(),
        limitations: vec!["Timing is wall-clock, not a benchmark.".to_owned()],
    }
}
