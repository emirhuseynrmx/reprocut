#[cfg(test)]
mod session_integrity_contract {
    use crate::reprocut_core::ContentDigest;
    use crate::reprocut_state::{SessionContract, StateError, StateStore};

    fn digest(seed: &str) -> ContentDigest {
        ContentDigest::of(seed.as_bytes())
    }

    fn contract(field: &str, value: &str) -> SessionContract {
        SessionContract::new_v2(
            digest(if field == "source" { value } else { "source" }),
            digest(if field == "command" { value } else { "command" }),
            digest(if field == "oracle" { value } else { "oracle" }),
            digest(if field == "preparation" {
                value
            } else {
                "preparation"
            }),
            digest(if field == "policy" { value } else { "policy" }),
            if field == "adapter" {
                value
            } else {
                "adapter-v2"
            }
            .to_owned(),
            if field == "engine" { value } else { "0.1.0" }.to_owned(),
        )
    }

    #[test]
    fn every_integrity_dimension_changes_the_session_identity() {
        let baseline = contract("", "").digest();
        for field in [
            "source",
            "command",
            "oracle",
            "preparation",
            "policy",
            "adapter",
            "engine",
        ] {
            assert_ne!(contract(field, "changed").digest(), baseline, "{field}");
        }
        assert_eq!(contract("", "").contract_schema(), 2);
    }

    #[test]
    fn schema_one_session_is_refused_with_an_actionable_error() {
        let temporary = tempfile::tempdir().expect("state");
        let path = temporary.path().join("state.sqlite3");
        let connection = rusqlite::Connection::open(&path).expect("database");
        connection
            .execute_batch(include_str!(
                "../../crates/reprocut-state/migrations/0001.sql"
            ))
            .expect("schema one");
        let legacy = SessionContract::new(
            digest("source"),
            digest("command"),
            1,
            "files-v1".to_owned(),
            "0.1.0".to_owned(),
        );
        connection
            .execute(
                "INSERT INTO sessions(contract_digest,source_digest,command_digest,normalization_schema,adapter_version,engine_version) VALUES (?1,?2,?3,1,'files-v1','0.1.0')",
                rusqlite::params![legacy.digest().as_bytes().as_slice(), digest("source").as_bytes().as_slice(), digest("command").as_bytes().as_slice()],
            )
            .expect("legacy session");
        drop(connection);

        assert!(matches!(
            StateStore::resume(&path, contract("", "")),
            Err(StateError::LegacyContractSchema { found: 1 })
        ));
    }
}
