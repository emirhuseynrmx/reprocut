BEGIN IMMEDIATE;

CREATE TABLE attempt_events (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    candidate_digest BLOB NOT NULL CHECK(length(candidate_digest) = 32),
    verdict TEXT NOT NULL CHECK(verdict IN ('preserved', 'rejected', 'inconclusive')),
    observed_runs INTEGER NOT NULL CHECK(observed_runs >= 0),
    inconclusive_runs INTEGER NOT NULL CHECK(inconclusive_runs >= 0),
    evidence_json TEXT NOT NULL,
    completed_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

INSERT INTO attempt_events(
    session_id, candidate_digest, verdict, observed_runs,
    inconclusive_runs, evidence_json, completed_at
)
SELECT
    session_id, candidate_digest, verdict, observed_runs,
    inconclusive_runs, evidence_json, completed_at
FROM attempts;

CREATE INDEX attempt_events_by_candidate
    ON attempt_events(session_id, candidate_digest, id);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (2);
PRAGMA user_version = 2;

COMMIT;
