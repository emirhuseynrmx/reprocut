//! Crash-safe, single-writer SQLite state for resumable reductions.

mod schema;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

use reprocut_core::{CandidateVerdict, ContentDigest};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use thiserror::Error;

use schema::{MIGRATION_0001, SCHEMA_VERSION};

const WRITER_QUEUE_CAPACITY: usize = 64;

/// Immutable fields that determine whether a previous session is reusable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionContract {
    source_digest: ContentDigest,
    command_digest: ContentDigest,
    normalization_schema: u16,
    adapter_version: String,
    engine_version: String,
}

impl SessionContract {
    /// Creates an exact resume contract.
    pub fn new(
        source_digest: ContentDigest,
        command_digest: ContentDigest,
        normalization_schema: u16,
        adapter_version: String,
        engine_version: String,
    ) -> Self {
        Self {
            source_digest,
            command_digest,
            normalization_schema,
            adapter_version,
            engine_version,
        }
    }

    /// Returns a stable identity covering every immutable field.
    pub fn digest(&self) -> ContentDigest {
        let mut bytes = Vec::with_capacity(
            80_usize
                .saturating_add(self.adapter_version.len())
                .saturating_add(self.engine_version.len()),
        );
        bytes.extend_from_slice(b"REPROCUT-SESSION\0");
        bytes.extend_from_slice(self.source_digest.as_bytes());
        bytes.extend_from_slice(self.command_digest.as_bytes());
        bytes.extend_from_slice(&self.normalization_schema.to_le_bytes());
        encode_text(&mut bytes, &self.adapter_version);
        encode_text(&mut bytes, &self.engine_version);
        ContentDigest::of(&bytes)
    }

    /// Returns the immutable source-tree identity.
    pub const fn source_digest(&self) -> ContentDigest {
        self.source_digest
    }

    /// Returns the normalized command identity.
    pub const fn command_digest(&self) -> ContentDigest {
        self.command_digest
    }

    /// Returns the failure-normalization schema version.
    pub const fn normalization_schema(&self) -> u16 {
        self.normalization_schema
    }

    /// Returns the selected ecosystem adapter version.
    pub fn adapter_version(&self) -> &str {
        &self.adapter_version
    }

    /// Returns the engine version that created the session.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }
}

/// One aggregate candidate evaluation persisted in the attempt ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptRecord {
    candidate: ContentDigest,
    verdict: CandidateVerdict,
    observed_runs: u16,
    inconclusive_runs: u16,
    evidence_json: String,
}

impl AttemptRecord {
    /// Creates immutable attempt evidence.
    pub fn new(
        candidate: ContentDigest,
        verdict: CandidateVerdict,
        observed_runs: u16,
        inconclusive_runs: u16,
        evidence_json: String,
    ) -> Self {
        Self {
            candidate,
            verdict,
            observed_runs,
            inconclusive_runs,
            evidence_json,
        }
    }

    /// Returns the content-addressed candidate.
    pub const fn candidate(&self) -> ContentDigest {
        self.candidate
    }

    /// Returns the aggregate verdict.
    pub const fn verdict(&self) -> CandidateVerdict {
        self.verdict
    }

    /// Returns how many executions contributed to the aggregate verdict.
    pub const fn observed_runs(&self) -> u16 {
        self.observed_runs
    }

    /// Returns how many executions could not be classified.
    pub const fn inconclusive_runs(&self) -> u16 {
        self.inconclusive_runs
    }

    /// Returns the detached evidence payload.
    pub fn evidence_json(&self) -> &str {
        &self.evidence_json
    }
}

/// One accepted state transition tied to a preserved attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionRecord {
    ordinal: u64,
    from: ContentDigest,
    to: ContentDigest,
    attempt_candidate: ContentDigest,
    accepted_size: u64,
}

impl TransitionRecord {
    /// Creates a causally linked accepted transition.
    pub const fn new(
        ordinal: u64,
        from: ContentDigest,
        to: ContentDigest,
        attempt_candidate: ContentDigest,
        accepted_size: u64,
    ) -> Self {
        Self {
            ordinal,
            from,
            to,
            attempt_candidate,
            accepted_size,
        }
    }

    /// Returns the total-order position of this accepted state change.
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// Returns the predecessor candidate identity.
    pub const fn from(&self) -> ContentDigest {
        self.from
    }

    /// Returns the accepted candidate identity.
    pub const fn to(&self) -> ContentDigest {
        self.to
    }

    /// Returns the attempt whose evidence authorized this transition.
    pub const fn attempt_candidate(&self) -> ContentDigest {
        self.attempt_candidate
    }

    /// Returns the resulting project size recorded at commit time.
    pub const fn accepted_size(&self) -> u64 {
        self.accepted_size
    }
}

/// A complete cached verdict safe to reuse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedVerdict {
    verdict: CandidateVerdict,
    observed_runs: u16,
    inconclusive_runs: u16,
    evidence_json: String,
}

impl CachedVerdict {
    /// Returns a preserved or rejected result; inconclusive results are never cached.
    pub const fn verdict(&self) -> CandidateVerdict {
        self.verdict
    }

    /// Returns the original aggregate execution count.
    pub const fn observed_runs(&self) -> u16 {
        self.observed_runs
    }

    /// Returns the original incomplete execution count.
    pub const fn inconclusive_runs(&self) -> u16 {
        self.inconclusive_runs
    }

    /// Returns the detached aggregate evidence JSON.
    pub fn evidence_json(&self) -> &str {
        &self.evidence_json
    }
}

/// Row counts used for shutdown, recovery, and regression contracts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateSnapshot {
    attempts: u64,
    transitions: u64,
    cache_entries: u64,
}

impl StateSnapshot {
    /// Returns persisted attempt count.
    pub const fn attempts(self) -> u64 {
        self.attempts
    }

    /// Returns committed transition count.
    pub const fn transitions(self) -> u64 {
        self.transitions
    }

    /// Returns reusable cache entry count.
    pub const fn cache_entries(self) -> u64 {
        self.cache_entries
    }
}

/// A bounded sender to the session's only SQLite writer connection.
#[derive(Clone, Debug)]
pub struct WriterHandle {
    sender: SyncSender<WriterCommand>,
}

impl WriterHandle {
    /// Persists one attempt and conditionally updates the reusable cache.
    pub fn record_attempt(&self, attempt: AttemptRecord) -> Result<(), StateError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.send(WriterCommand::RecordAttempt { attempt, reply })?;
        receive(receiver)
    }

    /// Atomically commits a preserved attempt and its causal transition.
    pub fn accept_transition(
        &self,
        attempt: AttemptRecord,
        transition: TransitionRecord,
    ) -> Result<(), StateError> {
        if attempt.verdict != CandidateVerdict::Preserved {
            return Err(StateError::InvalidRecord(
                "only preserved evidence can authorize a transition",
            ));
        }
        if attempt.candidate != transition.attempt_candidate {
            return Err(StateError::InvalidRecord(
                "transition does not reference its attempt candidate",
            ));
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        self.send(WriterCommand::AcceptTransition {
            attempt,
            transition,
            reply,
        })?;
        receive(receiver)
    }

    /// Looks up only complete preserved/rejected evidence.
    pub fn lookup_cache(
        &self,
        candidate: ContentDigest,
    ) -> Result<Option<CachedVerdict>, StateError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.send(WriterCommand::LookupCache { candidate, reply })?;
        receive(receiver)
    }

    /// Returns durable ledger counts after all earlier commands.
    pub fn snapshot(&self) -> Result<StateSnapshot, StateError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.send(WriterCommand::Snapshot { reply })?;
        receive(receiver)
    }

    /// Returns accepted transitions in ordinal order.
    pub fn transitions(&self) -> Result<Vec<TransitionRecord>, StateError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.send(WriterCommand::Transitions { reply })?;
        receive(receiver)
    }

    fn send(&self, command: WriterCommand) -> Result<(), StateError> {
        self.sender
            .send(command)
            .map_err(|_| StateError::WriterStopped)
    }
}

/// Owns one journal session and drains its writer during drop.
#[derive(Debug)]
pub struct StateStore {
    path: PathBuf,
    session_id: i64,
    writer: WriterHandle,
    join: Option<JoinHandle<()>>,
}

impl StateStore {
    /// Creates a new session after applying known migrations.
    pub fn create(path: impl AsRef<Path>, contract: SessionContract) -> Result<Self, StateError> {
        Self::create_inner(path.as_ref(), contract, false)
    }

    /// Starts a new session in an existing journal while preserving prior history.
    pub fn restart(path: impl AsRef<Path>, contract: SessionContract) -> Result<Self, StateError> {
        Self::create_inner(path.as_ref(), contract, true)
    }

    fn create_inner(
        path: &Path,
        contract: SessionContract,
        allow_existing: bool,
    ) -> Result<Self, StateError> {
        let path = prepare_path(path)?;
        let connection = open_connection(&path)?;
        migrate(&connection)?;
        let has_session = connection
            .query_row("SELECT EXISTS(SELECT 1 FROM sessions)", [], |row| {
                row.get::<_, bool>(0)
            })
            .map_err(database_error)?;
        if has_session && !allow_existing {
            return Err(StateError::ExistingSession);
        }
        connection
            .execute(
                "INSERT INTO sessions(
                    contract_digest, source_digest, command_digest,
                    normalization_schema, adapter_version, engine_version
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    contract.digest().as_bytes().as_slice(),
                    contract.source_digest.as_bytes().as_slice(),
                    contract.command_digest.as_bytes().as_slice(),
                    i64::from(contract.normalization_schema),
                    contract.adapter_version,
                    contract.engine_version,
                ],
            )
            .map_err(database_error)?;
        let session_id = connection.last_insert_rowid();
        drop(connection);
        Self::start(path, session_id)
    }

    /// Resumes the newest session only when every immutable field matches.
    pub fn resume(path: impl AsRef<Path>, contract: SessionContract) -> Result<Self, StateError> {
        let path = path.as_ref().to_path_buf();
        let connection = open_connection(&path)?;
        migrate(&connection)?;
        let latest = connection
            .query_row(
                "SELECT id, contract_digest FROM sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(database_error)?
            .ok_or(StateError::NoSession)?;
        if latest.1.as_slice() != contract.digest().as_bytes() {
            return Err(StateError::IncompatibleSession {
                session_id: latest.0,
            });
        }
        drop(connection);
        Self::start(path, latest.0)
    }

    fn start(path: PathBuf, session_id: i64) -> Result<Self, StateError> {
        let (sender, receiver) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let (ready, ready_receiver) = mpsc::sync_channel(1);
        let writer_path = path.clone();
        let join = thread::Builder::new()
            .name("reprocut-state-writer".to_owned())
            .spawn(move || writer_main(&writer_path, session_id, receiver, ready))
            .map_err(StateError::SpawnWriter)?;
        receive(ready_receiver)?;
        Ok(Self {
            path,
            session_id,
            writer: WriterHandle { sender },
            join: Some(join),
        })
    }

    /// Returns a cloneable bounded writer handle.
    pub fn writer(&self) -> WriterHandle {
        self.writer.clone()
    }

    /// Returns the SQLite path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the active session row identifier.
    pub const fn session_id(&self) -> i64 {
        self.session_id
    }
}

impl Drop for StateStore {
    fn drop(&mut self) {
        let (reply, receiver) = mpsc::sync_channel(1);
        let _ = self.writer.sender.send(WriterCommand::Shutdown { reply });
        let _ = receiver.recv();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// State creation, validation, migration, or writer failure.
#[derive(Debug, Error)]
pub enum StateError {
    /// An ordinary filesystem operation failed.
    #[error("prepare state path failed: {0}")]
    PreparePath(std::io::Error),
    /// SQLite rejected an operation.
    #[error("state database error: {0}")]
    Database(String),
    /// The database was created by an unsupported schema.
    #[error("unsupported state schema {found}; expected {expected}")]
    SchemaVersion {
        /// Schema found in `PRAGMA user_version`.
        found: i64,
        /// Only schema this binary can safely interpret.
        expected: i64,
    },
    /// No session exists to resume.
    #[error("state database has no session to resume")]
    NoSession,
    /// A normal start refused to hide resumable state.
    #[error("state database already contains a session; resume it or explicitly restart")]
    ExistingSession,
    /// At least one immutable contract field changed.
    #[error("session {session_id} is incompatible with the requested source or configuration")]
    IncompatibleSession {
        /// Refused previous session.
        session_id: i64,
    },
    /// A record violated fail-closed causal rules.
    #[error("invalid state record: {0}")]
    InvalidRecord(&'static str),
    /// The dedicated writer could not be started.
    #[error("spawn state writer failed: {0}")]
    SpawnWriter(std::io::Error),
    /// The writer exited before replying.
    #[error("state writer stopped unexpectedly")]
    WriterStopped,
}

enum WriterCommand {
    RecordAttempt {
        attempt: AttemptRecord,
        reply: SyncSender<Result<(), StateError>>,
    },
    AcceptTransition {
        attempt: AttemptRecord,
        transition: TransitionRecord,
        reply: SyncSender<Result<(), StateError>>,
    },
    LookupCache {
        candidate: ContentDigest,
        reply: SyncSender<Result<Option<CachedVerdict>, StateError>>,
    },
    Snapshot {
        reply: SyncSender<Result<StateSnapshot, StateError>>,
    },
    Transitions {
        reply: SyncSender<Result<Vec<TransitionRecord>, StateError>>,
    },
    Shutdown {
        reply: SyncSender<()>,
    },
}

fn writer_main(
    path: &Path,
    session_id: i64,
    receiver: Receiver<WriterCommand>,
    ready: SyncSender<Result<(), StateError>>,
) {
    let mut connection = match open_connection(path) {
        Ok(connection) => connection,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    while let Ok(command) = receiver.recv() {
        match command {
            WriterCommand::RecordAttempt { attempt, reply } => {
                let result = record_attempt(&mut connection, session_id, &attempt);
                let _ = reply.send(result);
            }
            WriterCommand::AcceptTransition {
                attempt,
                transition,
                reply,
            } => {
                let result = accept_transition(&mut connection, session_id, &attempt, &transition);
                let _ = reply.send(result);
            }
            WriterCommand::LookupCache { candidate, reply } => {
                let _ = reply.send(lookup_cache(&connection, session_id, candidate));
            }
            WriterCommand::Snapshot { reply } => {
                let _ = reply.send(snapshot(&connection, session_id));
            }
            WriterCommand::Transitions { reply } => {
                let _ = reply.send(read_transitions(&connection, session_id));
            }
            WriterCommand::Shutdown { reply } => {
                let _ = reply.send(());
                break;
            }
        }
    }
}

fn record_attempt(
    connection: &mut Connection,
    session_id: i64,
    attempt: &AttemptRecord,
) -> Result<(), StateError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    insert_attempt(&transaction, session_id, attempt)?;
    insert_cache(&transaction, session_id, attempt)?;
    transaction.commit().map_err(database_error)
}

fn accept_transition(
    connection: &mut Connection,
    session_id: i64,
    attempt: &AttemptRecord,
    transition: &TransitionRecord,
) -> Result<(), StateError> {
    let ordinal = i64::try_from(transition.ordinal)
        .map_err(|_| StateError::InvalidRecord("transition ordinal exceeds SQLite INTEGER"))?;
    let accepted_size = i64::try_from(transition.accepted_size)
        .map_err(|_| StateError::InvalidRecord("accepted size exceeds SQLite INTEGER"))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    insert_attempt(&transaction, session_id, attempt)?;
    let changed = transaction
        .execute(
            "INSERT INTO transitions(
                session_id, ordinal, from_digest, to_digest,
                attempt_candidate_digest, accepted_size
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id, ordinal) DO NOTHING",
            params![
                session_id,
                ordinal,
                transition.from.as_bytes().as_slice(),
                transition.to.as_bytes().as_slice(),
                transition.attempt_candidate.as_bytes().as_slice(),
                accepted_size,
            ],
        )
        .map_err(database_error)?;
    if changed == 0 {
        let existing = transaction
            .query_row(
                "SELECT from_digest, to_digest, attempt_candidate_digest, accepted_size
                 FROM transitions WHERE session_id = ?1 AND ordinal = ?2",
                params![session_id, ordinal],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(database_error)?;
        let expected = (
            transition.from.as_bytes().to_vec(),
            transition.to.as_bytes().to_vec(),
            transition.attempt_candidate.as_bytes().to_vec(),
            accepted_size,
        );
        if existing != expected {
            return Err(StateError::InvalidRecord(
                "transition ordinal was reused with different evidence",
            ));
        }
    }
    insert_cache(&transaction, session_id, attempt)?;
    transaction.commit().map_err(database_error)
}

fn insert_attempt(
    connection: &Connection,
    session_id: i64,
    attempt: &AttemptRecord,
) -> Result<(), StateError> {
    let changed = connection
        .execute(
            "INSERT INTO attempts(
                session_id, candidate_digest, verdict, observed_runs,
                inconclusive_runs, evidence_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id, candidate_digest) DO NOTHING",
            params![
                session_id,
                attempt.candidate.as_bytes().as_slice(),
                verdict_name(attempt.verdict),
                i64::from(attempt.observed_runs),
                i64::from(attempt.inconclusive_runs),
                attempt.evidence_json,
            ],
        )
        .map_err(database_error)?;
    if changed != 0 {
        return Ok(());
    }
    let existing = connection
        .query_row(
            "SELECT verdict, observed_runs, inconclusive_runs, evidence_json
             FROM attempts WHERE session_id = ?1 AND candidate_digest = ?2",
            params![session_id, attempt.candidate.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(database_error)?;
    let expected = (
        verdict_name(attempt.verdict).to_owned(),
        i64::from(attempt.observed_runs),
        i64::from(attempt.inconclusive_runs),
        attempt.evidence_json.clone(),
    );
    if existing == expected {
        Ok(())
    } else {
        Err(StateError::InvalidRecord(
            "candidate digest was reused with different evidence",
        ))
    }
}

fn insert_cache(
    connection: &Connection,
    session_id: i64,
    attempt: &AttemptRecord,
) -> Result<(), StateError> {
    if attempt.verdict == CandidateVerdict::Inconclusive {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO cache_entries(session_id, candidate_digest, verdict, evidence_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(session_id, candidate_digest) DO NOTHING",
            params![
                session_id,
                attempt.candidate.as_bytes().as_slice(),
                verdict_name(attempt.verdict),
                attempt.evidence_json,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn lookup_cache(
    connection: &Connection,
    session_id: i64,
    candidate: ContentDigest,
) -> Result<Option<CachedVerdict>, StateError> {
    connection
        .query_row(
            "SELECT cache_entries.verdict, attempts.observed_runs,
                    attempts.inconclusive_runs, cache_entries.evidence_json
             FROM cache_entries
             INNER JOIN attempts USING(session_id, candidate_digest)
             WHERE cache_entries.session_id = ?1 AND cache_entries.candidate_digest = ?2",
            params![session_id, candidate.as_bytes().as_slice()],
            |row| {
                let verdict = row.get::<_, String>(0)?;
                Ok((
                    verdict,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?
        .map(
            |(verdict, observed_runs, inconclusive_runs, evidence_json)| {
                Ok(CachedVerdict {
                    verdict: parse_verdict(&verdict)?,
                    observed_runs: u16::try_from(observed_runs).map_err(|_| {
                        StateError::Database("invalid observed run count".to_owned())
                    })?,
                    inconclusive_runs: u16::try_from(inconclusive_runs).map_err(|_| {
                        StateError::Database("invalid inconclusive run count".to_owned())
                    })?,
                    evidence_json,
                })
            },
        )
        .transpose()
}

fn snapshot(connection: &Connection, session_id: i64) -> Result<StateSnapshot, StateError> {
    Ok(StateSnapshot {
        attempts: count_rows(connection, "attempts", session_id)?,
        transitions: count_rows(connection, "transitions", session_id)?,
        cache_entries: count_rows(connection, "cache_entries", session_id)?,
    })
}

fn count_rows(
    connection: &Connection,
    table: &'static str,
    session_id: i64,
) -> Result<u64, StateError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1");
    let count = connection
        .query_row(&sql, [session_id], |row| row.get::<_, i64>(0))
        .map_err(database_error)?;
    u64::try_from(count).map_err(|_| StateError::Database("negative row count".to_owned()))
}

fn read_transitions(
    connection: &Connection,
    session_id: i64,
) -> Result<Vec<TransitionRecord>, StateError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal, from_digest, to_digest, attempt_candidate_digest, accepted_size
             FROM transitions WHERE session_id = ?1 ORDER BY ordinal",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(database_error)?;
    rows.map(|row| {
        let (ordinal, from, to, attempt_candidate, accepted_size) = row.map_err(database_error)?;
        Ok(TransitionRecord {
            ordinal: u64::try_from(ordinal)
                .map_err(|_| StateError::Database("negative transition ordinal".to_owned()))?,
            from: parse_digest(from)?,
            to: parse_digest(to)?,
            attempt_candidate: parse_digest(attempt_candidate)?,
            accepted_size: u64::try_from(accepted_size)
                .map_err(|_| StateError::Database("negative accepted size".to_owned()))?,
        })
    })
    .collect()
}

fn prepare_path(path: &Path) -> Result<PathBuf, StateError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(StateError::PreparePath)?;
    }
    Ok(path.to_path_buf())
}

fn open_connection(path: &Path) -> Result<Connection, StateError> {
    let connection = Connection::open(path).map_err(database_error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(database_error)?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(database_error)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(database_error)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(database_error)?;
    Ok(connection)
}

fn migrate(connection: &Connection) -> Result<(), StateError> {
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(database_error)?;
    match version {
        0 => connection
            .execute_batch(MIGRATION_0001)
            .map_err(database_error),
        SCHEMA_VERSION => Ok(()),
        found => Err(StateError::SchemaVersion {
            found,
            expected: SCHEMA_VERSION,
        }),
    }
}

fn verdict_name(verdict: CandidateVerdict) -> &'static str {
    match verdict {
        CandidateVerdict::Preserved => "preserved",
        CandidateVerdict::Rejected => "rejected",
        CandidateVerdict::Inconclusive => "inconclusive",
    }
}

fn parse_verdict(value: &str) -> Result<CandidateVerdict, StateError> {
    match value {
        "preserved" => Ok(CandidateVerdict::Preserved),
        "rejected" => Ok(CandidateVerdict::Rejected),
        "inconclusive" => Ok(CandidateVerdict::Inconclusive),
        _ => Err(StateError::Database("invalid persisted verdict".to_owned())),
    }
}

fn parse_digest(bytes: Vec<u8>) -> Result<ContentDigest, StateError> {
    let bytes = <[u8; 32]>::try_from(bytes)
        .map_err(|_| StateError::Database("invalid persisted digest length".to_owned()))?;
    Ok(ContentDigest::from_bytes(bytes))
}

fn encode_text(output: &mut Vec<u8>, value: &str) {
    let length = u64::try_from(value.len()).expect("usize fits into u64 on supported targets");
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn database_error(error: rusqlite::Error) -> StateError {
    StateError::Database(error.to_string())
}

fn receive<T>(receiver: Receiver<Result<T, StateError>>) -> Result<T, StateError> {
    receiver.recv().map_err(|_| StateError::WriterStopped)?
}
