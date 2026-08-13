use reprocut_core::{CandidateVerdict, ContentDigest};
use reprocut_state::{AttemptRecord, SessionContract, StateError, StateStore, TransitionRecord};

#[test]
fn cache_reuses_only_complete_terminal_evidence() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    let store = StateStore::create(&database, &contract("one")).expect("create state");
    let writer = store.writer();
    let preserved = attempt("preserved", CandidateVerdict::Preserved);
    let incomplete = attempt("incomplete", CandidateVerdict::Inconclusive);

    writer
        .record_attempt(preserved.clone())
        .expect("record preserved");
    writer
        .record_attempt(incomplete.clone())
        .expect("record incomplete");

    assert_eq!(
        writer
            .lookup_cache(preserved.candidate())
            .expect("cache query")
            .expect("cache hit")
            .verdict(),
        CandidateVerdict::Preserved
    );
    assert!(writer
        .lookup_cache(incomplete.candidate())
        .expect("cache query")
        .is_none());
}

#[test]
fn transition_and_its_attempt_commit_atomically() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    let store = StateStore::create(&database, &contract("atomic")).expect("create state");
    let writer = store.writer();
    let first = attempt("first", CandidateVerdict::Preserved);
    writer
        .accept_transition(first.clone(), transition(0, "root", "first", "first"))
        .expect("first transition");
    let before = writer.snapshot().expect("snapshot");
    let second = attempt("second", CandidateVerdict::Preserved);

    writer
        .accept_transition(second, transition(0, "first", "second", "second"))
        .expect_err("duplicate ordinal fails after attempt insert inside the transaction");
    let after = writer.snapshot().expect("snapshot");

    assert_eq!(
        after, before,
        "failed transition must roll back its attempt"
    );
}

#[test]
fn material_output_identity_cannot_be_reused_by_a_later_transition() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    let store = StateStore::create(&database, &contract("unique-output")).expect("create state");
    let writer = store.writer();
    let first = attempt("first-output", CandidateVerdict::Preserved);
    writer
        .accept_transition(
            first.clone(),
            transition(0, "root", "material", "first-output"),
        )
        .expect("first material output");
    let before = writer.snapshot().expect("snapshot");

    let duplicate = attempt("duplicate-output", CandidateVerdict::Preserved);
    writer
        .accept_transition(
            duplicate,
            transition(1, "material", "material", "duplicate-output"),
        )
        .expect_err("a later transition cannot reuse a material output");

    assert_eq!(
        writer.snapshot().expect("snapshot"),
        before,
        "duplicate material output must roll back its attempt evidence"
    );
}

#[test]
fn resume_requires_the_exact_immutable_session_contract() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    let original = contract("same");
    drop(StateStore::create(&database, &original).expect("create state"));

    drop(StateStore::resume(&database, &original).expect("compatible resume"));
    let error = StateStore::resume(&database, &contract("changed"))
        .expect_err("changed command/source identity must refuse resume");
    assert!(matches!(error, StateError::IncompatibleSession { .. }));
}

#[test]
fn duplicate_attempt_messages_are_idempotent() {
    let temporary = tempfile::tempdir().expect("state directory");
    let store = StateStore::create(
        temporary.path().join("state.sqlite3"),
        &contract("duplicate"),
    )
    .expect("create state");
    let writer = store.writer();
    let record = attempt("same", CandidateVerdict::Rejected);
    writer
        .record_attempt(record.clone())
        .expect("first message");
    writer.record_attempt(record).expect("duplicate message");
    assert_eq!(writer.snapshot().expect("snapshot").attempts(), 1);
    assert_eq!(writer.snapshot().expect("snapshot").attempt_events(), 1);
}

#[test]
fn inconclusive_retries_append_evidence_and_can_become_terminal() {
    let temporary = tempfile::tempdir().expect("state directory");
    let store = StateStore::create(temporary.path().join("state.sqlite3"), &contract("retry"))
        .expect("create state");
    let writer = store.writer();
    let candidate = ContentDigest::of(b"retry-candidate");
    for (observed_runs, evidence_json) in [(1, "{\"run\":1}"), (2, "{\"run\":2}")] {
        writer
            .record_attempt(AttemptRecord::new(
                candidate,
                CandidateVerdict::Inconclusive,
                observed_runs,
                1,
                evidence_json.to_owned(),
            ))
            .expect("retry evidence");
    }
    writer
        .record_attempt(AttemptRecord::new(
            candidate,
            CandidateVerdict::Preserved,
            3,
            0,
            "{\"run\":3}".to_owned(),
        ))
        .expect("terminal evidence");

    let snapshot = writer.snapshot().expect("snapshot");
    assert_eq!(snapshot.attempts(), 1);
    assert_eq!(snapshot.attempt_events(), 3);
    assert_eq!(
        writer
            .lookup_cache(candidate)
            .expect("cache")
            .expect("terminal cache")
            .verdict(),
        CandidateVerdict::Preserved
    );
}

#[test]
fn schema_one_journals_migrate_but_old_session_contracts_are_not_reused() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    let connection = rusqlite::Connection::open(&database).expect("database");
    connection
        .execute_batch(include_str!("../migrations/0001.sql"))
        .expect("schema one");
    drop(connection);

    drop(StateStore::create(&database, &contract("migrated")).expect("migrated store"));

    let connection = rusqlite::Connection::open(&database).expect("database");
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .expect("version");
    assert_eq!(version, 3);
}

#[test]
fn restart_is_explicit_and_preserves_prior_sessions() {
    let temporary = tempfile::tempdir().expect("state directory");
    let database = temporary.path().join("state.sqlite3");
    drop(StateStore::create(&database, &contract("first")).expect("create state"));

    assert!(matches!(
        StateStore::create(&database, &contract("second")),
        Err(StateError::ExistingSession)
    ));
    let restarted = StateStore::restart(&database, &contract("second")).expect("explicit restart");
    assert_eq!(restarted.session_id(), 2);
}

fn contract(seed: &str) -> SessionContract {
    SessionContract::new_v2(
        ContentDigest::of(format!("source-{seed}").as_bytes()),
        ContentDigest::of(format!("command-{seed}").as_bytes()),
        ContentDigest::of(format!("oracle-{seed}").as_bytes()),
        ContentDigest::of(format!("preparation-{seed}").as_bytes()),
        ContentDigest::of(format!("policy-{seed}").as_bytes()),
        "files-v1".to_owned(),
        "0.1.0".to_owned(),
    )
}

fn attempt(seed: &str, verdict: CandidateVerdict) -> AttemptRecord {
    AttemptRecord::new(
        ContentDigest::of(seed.as_bytes()),
        verdict,
        3,
        u16::from(verdict == CandidateVerdict::Inconclusive),
        format!(r#"{{"seed":"{seed}"}}"#),
    )
}

fn transition(ordinal: u64, from: &str, to: &str, attempt: &str) -> TransitionRecord {
    TransitionRecord::new(
        ordinal,
        ContentDigest::of(from.as_bytes()),
        ContentDigest::of(to.as_bytes()),
        ContentDigest::of(attempt.as_bytes()),
        1,
    )
}
