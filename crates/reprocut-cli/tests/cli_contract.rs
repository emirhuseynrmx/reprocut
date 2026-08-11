use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

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
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["original_files"], 3);
    assert_eq!(summary["retained_files"], 1);
    assert_eq!(summary["final_verifications"], 3);
    assert_eq!(summary["jobs"], 4);
    assert_eq!(summary["resumed"], false);
    assert_eq!(summary["kept_files"], serde_json::json!(["bug.py"]));

    assert!(output.join("project/bug.py").is_file());
    assert!(!output.join("project/noise.txt").exists());
    assert!(output.join("report.html").is_file());
    assert!(output.join("reduction.json").is_file());
    assert!(output.join("reproduce.sh").is_file());
    assert!(output.join("reproduce.ps1").is_file());
    assert!(fs::read_to_string(output.join("report.html"))
        .expect("report is UTF-8")
        .contains(&command_display));
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
    assert_eq!(summary["resumed"], true);
    assert!(summary["cache_hits"].as_u64().expect("cache hits") > 0);
    assert_eq!(summary["kept_files"], serde_json::json!(["bug.py"]));
    assert_eq!(
        fs::read(first_output.join("project/bug.py")).expect("first project"),
        fs::read(second_output.join("project/bug.py")).expect("second project")
    );
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
