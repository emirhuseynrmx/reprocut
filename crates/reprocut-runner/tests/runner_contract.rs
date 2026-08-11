use std::{env, ffi::OsString, thread, time::Duration};

use reprocut_runner::{CommandSpec, ProcessRunner};

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
