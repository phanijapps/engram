//! SQLite construction options for adapter-local configuration.
//!
//! This module provides storage-neutral configuration for SQLite database
//! initialization. It lives in `engram-runtime` as adapter infrastructure,
//! not in the portable domain model.

use std::path::PathBuf;

/// Path configuration for SQLite database.
pub enum SqlitePath {
    /// File-based database at the given path.
    File(PathBuf),
    /// In-memory database (ephemeral, for tests).
    InMemory,
}

/// Journal mode for SQLite write-ahead logging.
pub enum SqliteJournalMode {
    /// WAL mode (Write-Ahead Logging) - better concurrency and crash recovery.
    Wal,
    /// Delete journal mode (rollback to truncate on commit).
    Delete,
    /// Truncate journal mode (rollback journal is truncated to zero length).
    Truncate,
    /// Persist journal mode (journal file is not deleted).
    Persist,
    /// Memory journal mode (in-memory database journal).
    Memory,
    /// No journal mode (rollback is disabled).
    Off,
}

impl SqliteJournalMode {
    /// Returns the PRAGMA string value for this journal mode.
    pub fn as_pragma_value(&self) -> &'static str {
        match self {
            SqliteJournalMode::Wal => "WAL",
            SqliteJournalMode::Delete => "DELETE",
            SqliteJournalMode::Truncate => "TRUNCATE",
            SqliteJournalMode::Persist => "PERSIST",
            SqliteJournalMode::Memory => "MEMORY",
            SqliteJournalMode::Off => "OFF",
        }
    }
}

/// Configuration options for opening SQLite databases.
///
/// This struct encapsulates common SQLite initialization parameters
/// that hosts like AgentZero's adapter need to control explicitly.
pub struct SqliteOpenOptions {
    /// Database path (file or in-memory).
    pub path: SqlitePath,

    /// Whether to create parent directories if they don't exist.
    pub create_parent_dirs: bool,

    /// Journal mode for write-ahead logging.
    pub journal_mode: SqliteJournalMode,

    /// Busy timeout in milliseconds. None uses SQLite default.
    pub busy_timeout_ms: Option<u64>,

    /// Whether to enforce foreign key constraints.
    pub foreign_keys: bool,

    /// Whether to run database migrations on open.
    pub run_migrations: bool,
}

impl SqliteOpenOptions {
    /// Create default options for a file-based database with WAL mode.
    ///
    /// This is the recommended configuration for production use.
    pub fn file_wal(path: PathBuf) -> Self {
        Self {
            path: SqlitePath::File(path),
            create_parent_dirs: true,
            journal_mode: SqliteJournalMode::Wal,
            busy_timeout_ms: Some(5000),
            foreign_keys: true,
            run_migrations: true,
        }
    }

    /// Create options for an in-memory database.
    ///
    /// In-memory databases use MEMORY journal mode by default.
    pub fn in_memory() -> Self {
        Self {
            path: SqlitePath::InMemory,
            create_parent_dirs: false,
            journal_mode: SqliteJournalMode::Memory,
            busy_timeout_ms: None,
            foreign_keys: false,
            run_migrations: false,
        }
    }
}
