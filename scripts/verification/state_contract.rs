#[cfg(test)]
mod state_contract {
    use crate::reprocut_core::{CandidateVerdict, ContentDigest};
    use crate::reprocut_state::{
        AttemptRecord, SessionContract, StateError, StateStore, TransitionRecord,
    };

    #[test]
    fn terminal_cache_atomic_transition_and_exact_resume() {
        let temporary = tempfile::tempdir().expect("state directory");
        let database = temporary.path().join("state.sqlite3");
        let contract = session("one");
        let store = StateStore::create(&database, contract.clone()).expect("create state");
        let writer = store.writer();
        let first = attempt("first", CandidateVerdict::Preserved);
        let incomplete = attempt("incomplete", CandidateVerdict::Inconclusive);
        writer
            .record_attempt(incomplete.clone())
            .expect("record incomplete");
        writer
            .accept_transition(first.clone(), transition(0, "root", "first"))
            .expect("first transition");
        let before = writer.snapshot().expect("snapshot");

        assert!(writer
            .lookup_cache(incomplete.candidate())
            .expect("lookup")
            .is_none());
        assert_eq!(
            writer
                .lookup_cache(first.candidate())
                .expect("lookup")
                .expect("cache hit")
                .verdict(),
            CandidateVerdict::Preserved
        );

        let second = attempt("second", CandidateVerdict::Preserved);
        writer
            .accept_transition(second, transition(0, "first", "second"))
            .expect_err("duplicate transition ordinal must roll back");
        assert_eq!(writer.snapshot().expect("snapshot"), before);
        drop(store);

        drop(StateStore::resume(&database, contract).expect("exact resume"));
        assert!(matches!(
            StateStore::resume(&database, session("changed")),
            Err(StateError::IncompatibleSession { .. })
        ));
        assert!(matches!(
            StateStore::create(&database, session("fresh")),
            Err(StateError::ExistingSession)
        ));
        let restarted = StateStore::restart(&database, session("fresh")).expect("restart");
        assert_eq!(restarted.session_id(), 2);
    }

    fn session(seed: &str) -> SessionContract {
        SessionContract::new(
            ContentDigest::of(format!("source-{seed}").as_bytes()),
            ContentDigest::of(format!("command-{seed}").as_bytes()),
            1,
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

    fn transition(ordinal: u64, from: &str, to: &str) -> TransitionRecord {
        TransitionRecord::new(
            ordinal,
            ContentDigest::of(from.as_bytes()),
            ContentDigest::of(to.as_bytes()),
            ContentDigest::of(to.as_bytes()),
            1,
        )
    }
}
