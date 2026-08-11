BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    contract_digest BLOB NOT NULL CHECK(length(contract_digest) = 32),
    source_digest BLOB NOT NULL CHECK(length(source_digest) = 32),
    command_digest BLOB NOT NULL CHECK(length(command_digest) = 32),
    normalization_schema INTEGER NOT NULL,
    adapter_version TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE attempts (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    candidate_digest BLOB NOT NULL CHECK(length(candidate_digest) = 32),
    verdict TEXT NOT NULL CHECK(verdict IN ('preserved', 'rejected', 'inconclusive')),
    observed_runs INTEGER NOT NULL CHECK(observed_runs >= 0),
    inconclusive_runs INTEGER NOT NULL CHECK(inconclusive_runs >= 0),
    evidence_json TEXT NOT NULL,
    completed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY(session_id, candidate_digest)
) STRICT;

CREATE TABLE cache_entries (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    candidate_digest BLOB NOT NULL CHECK(length(candidate_digest) = 32),
    verdict TEXT NOT NULL CHECK(verdict IN ('preserved', 'rejected')),
    evidence_json TEXT NOT NULL,
    PRIMARY KEY(session_id, candidate_digest)
) STRICT;

CREATE TABLE transitions (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    from_digest BLOB NOT NULL CHECK(length(from_digest) = 32),
    to_digest BLOB NOT NULL CHECK(length(to_digest) = 32),
    attempt_candidate_digest BLOB NOT NULL CHECK(length(attempt_candidate_digest) = 32),
    accepted_size INTEGER NOT NULL CHECK(accepted_size >= 0),
    committed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY(session_id, ordinal),
    FOREIGN KEY(session_id, attempt_candidate_digest)
        REFERENCES attempts(session_id, candidate_digest)
) STRICT;

CREATE INDEX attempts_by_verdict ON attempts(session_id, verdict);
CREATE UNIQUE INDEX transitions_by_output ON transitions(session_id, to_digest);
INSERT OR IGNORE INTO schema_migrations(version) VALUES (1);
PRAGMA user_version = 1;

COMMIT;
