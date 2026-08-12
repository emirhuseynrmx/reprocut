BEGIN IMMEDIATE;

ALTER TABLE sessions
    ADD COLUMN contract_schema INTEGER NOT NULL DEFAULT 1
    CHECK(contract_schema IN (1, 2));

INSERT OR IGNORE INTO schema_migrations(version) VALUES (3);
PRAGMA user_version = 3;

COMMIT;
