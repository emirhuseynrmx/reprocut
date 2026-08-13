#[cfg(test)]
mod gallery_contract {
    use super::*;

    #[test]
    fn redacted_gallery_submission_is_local_atomic_and_schema_bounded() {
        let sandbox = tempfile::tempdir().expect("sandbox");
        let artifact = sandbox.path().join("artifact");
        let output = sandbox.path().join("submission");
        fs::create_dir(&artifact).expect("artifact");
        let evidence = ReductionEvidence {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            source_root: "/private/source".to_owned(),
            source_snapshot_sha256: "c".repeat(64),
            output: "/private/output".to_owned(),
            command: vec!["python".to_owned(), "--token=private".to_owned()],
            ecosystem: "python".to_owned(),
            preparation: PreparationEvidence {
                mode: "offline".to_owned(),
                contract_sha256: Some("d".repeat(64)),
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
                attempts: 19,
                file_attempts: 19,
                structured_attempts: 0,
                inconclusive_attempts: 0,
                cache_hits: 5,
                baseline_runs: 3,
                final_verifications: 3,
                jobs: 1,
                state: Some("/private/state.sqlite3".to_owned()),
                resumed: false,
                accepted_file_sizes: vec![18, 9, 3],
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
                anchor: "private diagnostic".to_owned(),
                anchors: vec![ChannelAnchor {
                    channel: "stderr".to_owned(),
                    text: "private diagnostic".to_owned(),
                }],
                normalization_schema: 5,
                failure_patterns: Vec::new(),
                reject_patterns: Vec::new(),
                oracle_spec_sha256: "b".repeat(64),
            },
            kept_files: vec![
                RetentionEvidence {
                    path: "a".to_owned(),
                    observation: "retained".to_owned(),
                },
                RetentionEvidence {
                    path: "b".to_owned(),
                    observation: "retained".to_owned(),
                },
                RetentionEvidence {
                    path: "c".to_owned(),
                    observation: "retained".to_owned(),
                },
            ],
            retained_manifest: RetainedManifest::new(vec![
                RetainedEntry::regular_file("a", &vec![0; 510], 0).expect("entry"),
                RetainedEntry::regular_file("b", &[0], 0).expect("entry"),
                RetainedEntry::regular_file("c", &[0], 0).expect("entry"),
            ])
            .expect("retained manifest"),
            final_observations: (1..=3)
                .map(|ordinal| FinalObservationEvidence {
                    ordinal,
                    verdict: "preserved".to_owned(),
                    termination: "exit 1".to_owned(),
                    exit_code: Some(1),
                    signal: None,
                    timed_out: false,
                    streams_truncated: false,
                    containment: "direct_child".to_owned(),
                    stdout_sha256: "1".repeat(64),
                    stdout_bytes: 0,
                    stderr_sha256: "2".repeat(64),
                    stderr_bytes: 20,
                })
                .collect(),
            accepted_structured_edits: Vec::new(),
            attempts: Vec::new(),
            limitations: Vec::new(),
        };
        let project = artifact.join("project");
        fs::create_dir(&project).expect("project");
        fs::write(project.join("a"), vec![0; 510]).expect("a");
        fs::write(project.join("b"), [0]).expect("b");
        fs::write(project.join("c"), [0]).expect("c");
        fs::write(
            artifact.join("reduction.json"),
            serde_json::to_vec_pretty(&evidence).expect("JSON"),
        )
        .expect("evidence");
        let mut attempts = Vec::new();
        write_attempts_jsonl(&evidence.attempts, &mut attempts).expect("attempts");
        fs::write(artifact.join("attempts.jsonl"), attempts).expect("attempts");
        fs::write(
            artifact.join("report.html"),
            render_report(&ReportModel::from(&evidence)),
        )
        .expect("report");
        fs::write(artifact.join("issue.md"), render_issue(&evidence)).expect("issue");
        let scripts = render_reproduction_scripts(&evidence.command);
        fs::write(artifact.join("reproduce.sh"), scripts.shell).expect("shell");
        fs::write(artifact.join("reproduce.ps1"), scripts.powershell).expect("PowerShell");
        let manifest = build_artifact_manifest(&artifact).expect("artifact manifest");
        fs::write(
            artifact.join("artifact-manifest.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
        )
        .expect("manifest");

        prepare_gallery(GalleryPrepareArgs {
            from: artifact,
            output: output.clone(),
            title: "Decimal mismatch".to_owned(),
            license: "MIT".to_owned(),
            include_source: false,
        })
        .expect("prepare");

        let entry = fs::read_to_string(output.join("entry.json")).expect("entry");
        assert!(!entry.contains("private"));
        assert!(!entry.contains("command"));
        assert!(output.join("index.html").is_file());
        assert!(!output.join("source").exists());
    }
}
