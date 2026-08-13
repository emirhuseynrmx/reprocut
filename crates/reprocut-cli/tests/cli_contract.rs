//! End-to-end command-line and artifact contracts.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

use reprocut_report::build_artifact_manifest;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/python_failure")
        .canonicalize()
        .expect("fixture exists")
}

fn python() -> String {
    std::env::var("TEST_PYTHON").unwrap_or_else(|_| "python3".to_owned())
}

#[test]
fn help_leads_with_the_real_user_job() {
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shrink a failing project"))
        .stdout(predicate::str::contains("reprocut minimize"));
}

#[test]
fn reduce_help_exposes_failure_evidence_controls() {
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--oracle-stream"))
        .stdout(predicate::str::contains("--flaky"))
        .stdout(predicate::str::contains("--jobs"))
        .stdout(predicate::str::contains("--state"))
        .stdout(predicate::str::contains("--ecosystem"))
        .stdout(predicate::str::contains("--prepare"));
}

#[test]
fn oci_export_help_promises_a_real_archive_and_explicit_builder() {
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["export", "oci", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OCI image archive"))
        .stdout(predicate::str::contains("--from"))
        .stdout(predicate::str::contains("--builder"));
}

#[test]
fn emits_real_shell_completions_from_the_cli_schema() {
    for (shell, marker) in [
        ("bash", "_reprocut"),
        ("fish", "complete -c reprocut"),
        ("power-shell", "Register-ArgumentCompleter"),
        ("zsh", "#compdef reprocut"),
    ] {
        Command::cargo_bin("reprocut")
            .expect("binary is built")
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains(marker));
    }
}

#[test]
fn protocol_run_streams_versioned_jsonl_and_keeps_stdout_machine_only() {
    let sandbox = tempdir().expect("sandbox");
    let output = sandbox.path().join("minimal");
    let request_path = sandbox.path().join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "protocol_version": 1,
            "action": "minimize",
            "root": fixture_root(),
            "output": output,
            "ecosystem": "python",
            "preparation": "offline",
            "command": [python(), "bug.py"],
            "state": sandbox.path().join("state.sqlite3"),
            // Hosted Windows runners can briefly stall while Defender scans the
            // freshly linked executable and spawned Python interpreter. Keep the
            // product timeout explicit, but give this cross-platform contract a
            // budget that measures ReproCut rather than runner startup jitter.
            "timeout_ms": 10_000
        }))
        .expect("request JSON"),
    )
    .expect("request fixture");

    let result = Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["protocol", "run", "--request"])
        .arg(request_path)
        .output()
        .expect("protocol process");

    assert!(
        result.status.success(),
        "protocol failed with status {}\nstdout:\n{}\nstderr:\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "success protocol keeps stderr empty"
    );
    let events = String::from_utf8(result.stdout).expect("UTF-8 JSONL");
    let events = events
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("one event per line"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["type"], "started");
    assert_eq!(events[1]["type"], "baseline_stable");
    assert_eq!(events[2]["type"], "completed");
    assert_eq!(events[2]["protocol_version"], 1);
}

#[test]
fn protocol_validation_failure_is_one_machine_readable_terminal_event() {
    let sandbox = tempdir().expect("sandbox");
    let request_path = sandbox.path().join("request.json");
    fs::write(
        &request_path,
        r#"{"protocol_version":99,"action":"minimize","root":"bug","output":"minimal"}"#,
    )
    .expect("request fixture");

    let result = Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["protocol", "run", "--request"])
        .arg(request_path)
        .output()
        .expect("protocol process");

    assert!(!result.status.success());
    assert!(
        result.stderr.is_empty(),
        "protocol errors stay on JSONL stdout"
    );
    let events = String::from_utf8(result.stdout).expect("UTF-8 JSONL");
    let lines = events.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let event: Value = serde_json::from_str(lines[0]).expect("terminal event");
    assert_eq!(event["type"], "failed");
    assert_eq!(event["protocol_version"], 1);
    assert!(event["message"]
        .as_str()
        .expect("message")
        .contains("unsupported protocol version 99"));
}

#[test]
fn invalid_flaky_majority_is_rejected_before_execution() {
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args([
            "reduce",
            "--flaky",
            "--flaky-runs",
            "11",
            "--flaky-required",
            "6",
            "--",
            "never-executed",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supermajority"));
}

#[test]
fn unsupported_projects_require_a_command_and_explicit_none_adapter() {
    let project = tempdir().expect("empty project");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(project.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no supported ecosystem marker"));
}

#[test]
fn reduces_a_real_failure_and_publishes_a_complete_artifact() {
    let sandbox = tempdir().expect("sandbox created");
    let output = sandbox.path().join("minimal");
    let command_display = format!("{} bug.py", python());

    let result = Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&output)
        .arg("--state")
        .arg(sandbox.path().join("state.sqlite3"))
        .args([
            "--jobs",
            "4",
            "--timeout-ms",
            "3000",
            "--json",
            "--",
            &python(),
            "bug.py",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("stable baseline"))
        .get_output()
        .clone();

    let summary: Value = serde_json::from_slice(&result.stdout).expect("stdout is one JSON value");
    assert_eq!(summary["schema_version"], 4);
    assert_eq!(summary["measurements"]["original"]["files"], 3);
    assert_eq!(summary["measurements"]["retained"]["files"], 1);
    assert_eq!(summary["search"]["final_verifications"], 3);
    assert_eq!(summary["search"]["jobs"], 4);
    assert_eq!(summary["search"]["resumed"], false);
    assert_eq!(summary["kept_files"][0]["path"], "bug.py");
    assert_eq!(summary["failure"]["same_failure"], true);

    assert!(output.join("project/bug.py").is_file());
    assert!(!output.join("project/noise.txt").exists());
    assert!(output.join("report.html").is_file());
    assert!(output.join("reduction.json").is_file());
    assert!(output.join("attempts.jsonl").is_file());
    assert!(output.join("issue.md").is_file());
    assert!(output.join("reproduce.sh").is_file());
    assert!(output.join("reproduce.ps1").is_file());
    assert!(output.join("artifact-manifest.json").is_file());
    assert!(fs::read_to_string(output.join("report.html"))
        .expect("report is UTF-8")
        .contains(&command_display));
    let attempt_lines =
        fs::read_to_string(output.join("attempts.jsonl")).expect("attempt ledger is UTF-8");
    assert!(!attempt_lines.is_empty());
    assert!(attempt_lines
        .lines()
        .all(|line| serde_json::from_str::<Value>(line).is_ok()));

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .arg("verify")
        .arg(&output)
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"verified\":true"));
}

#[test]
fn verify_rejects_changed_members_extra_files_and_reauthored_derived_output() {
    let sandbox = tempdir().expect("sandbox created");
    let artifact = sandbox.path().join("minimal");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&artifact)
        .arg("--state")
        .arg(sandbox.path().join("state.sqlite3"))
        .args(["--", &python(), "bug.py"])
        .assert()
        .success();

    fs::write(artifact.join("project/bug.py"), b"changed\n").expect("tamper project");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .arg("verify")
        .arg(&artifact)
        .assert()
        .failure()
        .stderr(predicate::str::contains("artifact member changed"));

    fs::remove_dir_all(&artifact).expect("remove tampered artifact");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&artifact)
        .arg("--state")
        .arg(sandbox.path().join("second-state.sqlite3"))
        .args(["--", &python(), "bug.py"])
        .assert()
        .success();
    fs::write(artifact.join("extra.txt"), b"undeclared").expect("extra member");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .arg("verify")
        .arg(&artifact)
        .assert()
        .failure()
        .stderr(predicate::str::contains("member set"));

    fs::remove_file(artifact.join("extra.txt")).expect("remove extra");
    fs::write(artifact.join("report.html"), b"self-consistent forgery").expect("tamper report");
    refresh_manifest(&artifact);
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .arg("verify")
        .arg(&artifact)
        .assert()
        .failure()
        .stderr(predicate::str::contains("HTML report disagrees"));
}

fn refresh_manifest(artifact: &Path) {
    fs::remove_file(artifact.join("artifact-manifest.json")).expect("remove old envelope");
    let manifest = build_artifact_manifest(artifact).expect("rebuild manifest");
    fs::write(
        artifact.join("artifact-manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write envelope");
}

#[test]
fn resume_reuses_terminal_evidence_and_replays_the_same_chain() {
    let sandbox = tempdir().expect("sandbox created");
    let state = sandbox.path().join("state.sqlite3");
    let first_output = sandbox.path().join("first");
    let second_output = sandbox.path().join("second");

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&first_output)
        .arg("--state")
        .arg(&state)
        .args(["--jobs", "4", "--", &python(), "bug.py"])
        .assert()
        .success();

    let resumed = Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["resume", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&second_output)
        .arg("--state")
        .arg(&state)
        .args(["--jobs", "2", "--json", "--", &python(), "bug.py"])
        .assert()
        .success()
        .get_output()
        .clone();

    let summary: Value = serde_json::from_slice(&resumed.stdout).expect("resume JSON");
    assert_eq!(summary["search"]["resumed"], true);
    assert!(
        summary["search"]["cache_hits"]
            .as_u64()
            .expect("cache hits")
            > 0
    );
    assert_eq!(summary["kept_files"][0]["path"], "bug.py");
    assert_eq!(
        fs::read(first_output.join("project/bug.py")).expect("first project"),
        fs::read(second_output.join("project/bug.py")).expect("second project")
    );
}

#[test]
fn gallery_prepare_is_redacted_local_only_and_no_clobber() {
    let sandbox = tempdir().expect("sandbox created");
    let artifact = sandbox.path().join("minimal");
    let submission = sandbox.path().join("submission");

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&artifact)
        .arg("--state")
        .arg(sandbox.path().join("state.sqlite3"))
        .args(["--", &python(), "bug.py"])
        .assert()
        .success();

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["gallery", "prepare", "--from"])
        .arg(&artifact)
        .arg("--output")
        .arg(&submission)
        .args([
            "--title",
            "Decimal checkout type mismatch",
            "--license",
            "MIT",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("no upload performed"));

    let entry: Value =
        serde_json::from_slice(&fs::read(submission.join("entry.json")).expect("entry exists"))
            .expect("entry JSON");
    assert_eq!(entry["schema_version"], 1);
    assert_eq!(
        entry["parent_artifact_id"],
        serde_json::from_slice::<Value>(
            &fs::read(artifact.join("artifact-manifest.json")).expect("artifact manifest")
        )
        .expect("manifest JSON")["artifact_id"]
    );
    assert_eq!(entry["source_included"], false);
    assert_eq!(entry["featured"], false);
    assert_eq!(entry["original_files"], 3);
    assert_eq!(entry["retained_files"], 1);
    assert!(entry.get("command").is_none());
    assert!(entry.get("source_root").is_none());
    assert!(!submission.join("source").exists());
    assert!(submission.join("index.html").is_file());
    assert!(submission.join("LICENSE_DECLARATION.md").is_file());

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["gallery", "prepare", "--from"])
        .arg(&artifact)
        .arg("--output")
        .arg(&submission)
        .args(["--title", "Second", "--license", "MIT"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn tampered_artifacts_cannot_reach_gallery_or_oci_preparation() {
    let sandbox = tempdir().expect("sandbox created");
    let artifact = sandbox.path().join("minimal");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&artifact)
        .arg("--state")
        .arg(sandbox.path().join("state.sqlite3"))
        .args(["--", &python(), "bug.py"])
        .assert()
        .success();

    fs::write(artifact.join("project/bug.py"), b"changed\n").expect("tamper artifact");
    let submission = sandbox.path().join("submission");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["gallery", "prepare", "--from"])
        .arg(&artifact)
        .arg("--output")
        .arg(&submission)
        .args(["--title", "Tampered", "--license", "MIT"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("artifact member changed"));
    assert!(!submission.exists());

    let archive = sandbox.path().join("tampered.oci.tar");
    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["export", "oci", "--from"])
        .arg(&artifact)
        .arg("--output")
        .arg(&archive)
        .args(["--builder", "docker-buildx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("artifact member changed"));
    assert!(!archive.exists());
}

#[test]
fn refuses_to_overwrite_an_existing_output_directory() {
    let sandbox = tempdir().expect("sandbox created");
    let output = sandbox.path().join("already-here");
    fs::create_dir(&output).expect("output marker directory created");
    fs::write(output.join("marker.txt"), "owned by user").expect("marker written");

    Command::cargo_bin("reprocut")
        .expect("binary is built")
        .args(["reduce", "--root"])
        .arg(fixture_root())
        .arg("--output")
        .arg(&output)
        .args(["--", &python(), "bug.py"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    assert_eq!(
        fs::read_to_string(output.join("marker.txt")).expect("marker survives"),
        "owned by user"
    );
}
