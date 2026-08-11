use std::{env, ffi::OsString, fs, process::Command, thread, time::Duration};

use reprocut_core::ContainmentMechanism;
use reprocut_runner::{containment_mechanism, CommandSpec, ProcessRunner};

#[test]
fn captures_real_child_output() {
    let spec = child_spec("child_emits", Duration::from_secs(5), 1_024);
    let observation = ProcessRunner::run(&spec).expect("child execution");

    assert_eq!(observation.exit_code(), Some(0));
    assert!(String::from_utf8_lossy(observation.stdout()).contains("child-stdout"));
    assert!(String::from_utf8_lossy(observation.stderr()).contains("child-stderr"));
    assert!(!observation.streams_truncated());
}

#[test]
fn output_is_bounded_while_the_pipe_is_fully_drained() {
    let spec = child_spec("child_floods", Duration::from_secs(5), 64);
    let observation = ProcessRunner::run(&spec).expect("child execution");

    assert_eq!(observation.stdout().len(), 64);
    assert!(observation.streams_truncated());
}

#[test]
fn timeout_kills_and_reaps_the_child() {
    let spec = child_spec("child_sleeps", Duration::from_millis(25), 1_024);
    let observation = ProcessRunner::run(&spec).expect("timed execution");

    assert!(observation.timed_out());
    assert_eq!(observation.containment(), containment_mechanism());
}

#[test]
fn timeout_prevents_a_descendant_from_writing_after_the_parent_dies() {
    let temporary = tempfile::tempdir().expect("temporary marker directory");
    let marker = temporary.path().join("descendant-survived");
    let spec = child_spec("child_spawns_descendant", Duration::from_millis(40), 1_024);
    env::set_var("REPROCUT_DESCENDANT_MARKER", &marker);
    let observation = ProcessRunner::run(&spec).expect("timed process group");
    env::remove_var("REPROCUT_DESCENDANT_MARKER");

    assert!(observation.timed_out());
    thread::sleep(Duration::from_millis(350));
    assert!(!marker.exists(), "a descendant escaped process containment");
}

#[test]
#[ignore = "spawned by captures_real_child_output"]
fn child_emits() {
    println!("child-stdout");
    eprintln!("child-stderr");
}

#[test]
#[ignore = "spawned by output_is_bounded_while_the_pipe_is_fully_drained"]
fn child_floods() {
    println!("{}", "x".repeat(4_096));
}

#[test]
#[ignore = "spawned by timeout_kills_and_reaps_the_child"]
fn child_sleeps() {
    thread::sleep(Duration::from_secs(5));
}

#[test]
#[ignore = "spawned by the descendant containment contract"]
fn child_spawns_descendant() {
    Command::new(env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "descendant_writes_marker",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .expect("spawn descendant");
    thread::sleep(Duration::from_secs(5));
}

#[test]
#[ignore = "spawned by child_spawns_descendant"]
fn descendant_writes_marker() {
    let marker = env::var_os("REPROCUT_DESCENDANT_MARKER").expect("marker path");
    thread::sleep(Duration::from_millis(200));
    fs::write(marker, b"descendant-survived").expect("write survival marker");
}

#[test]
fn platform_reports_a_real_group_containment_mechanism() {
    #[cfg(unix)]
    assert_eq!(
        containment_mechanism(),
        ContainmentMechanism::PosixProcessGroup
    );
    #[cfg(windows)]
    assert_eq!(
        containment_mechanism(),
        ContainmentMechanism::WindowsJobObject
    );
}

fn child_spec(test_name: &str, timeout: Duration, max_output_bytes: usize) -> CommandSpec {
    CommandSpec::new(
        env::current_exe().expect("test executable"),
        vec![
            OsString::from("--exact"),
            OsString::from(test_name),
            OsString::from("--ignored"),
            OsString::from("--nocapture"),
        ],
        env::current_dir().expect("current directory"),
        timeout,
        max_output_bytes,
    )
}
