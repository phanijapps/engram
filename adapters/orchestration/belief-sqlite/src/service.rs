//! SQLite-backed belief + contradiction repository and contradiction detector.
//!
//! Storage-only: this module persists belief and contradiction payloads as JSON
//! with scope indexing. Detection (`ContradictionDetector`) is advisory — it
//! surfaces tension between active beliefs on the same subject; resolution is a
//! deliberate action, never an automatic overwrite. Valid-time `as_of` queries
//! are implemented over `valid_from`/`valid_until`; record-time history is
//! rejected because this adapter stores current rows, not historical versions.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_core::{
    BeliefQuery, BeliefQueryOrder, BeliefReferenceQuery, BeliefRepository,
    belief_references_source, canonical_pair_key, canonicalize_pair, clear_stale_state, mark_stale,
    retract_belief, supersede_belief,
};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult, SqliteOpenOptions, SqlitePath};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};

/// SQLite-backed belief + contradiction repository.
///
/// Preserves beliefs and contradictions as contract JSON while indexing
/// identifiers and scope columns for repository reads.
#[derive(Clone)]
pub struct SqlBeliefStore {
    pub(crate) connection: Arc<Mutex<Connection>>,
}

impl SqlBeliefStore {
    /// Opens an in-memory belief store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql_error)?)
    }

    /// Opens a file-backed belief store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::from_connection(Connection::open(path).map_err(sql_error)?)
    }

    /// Opens a SQLite belief store with explicit configuration options.
    ///
    /// This constructor allows hosts like AgentZero's adapter to control WAL mode,
    /// busy timeout, foreign keys, migrations, and directory creation explicitly.
    ///
    /// # Arguments
    ///
    /// * `options` - SQLite configuration options including path, journal mode, and
    ///   pragma settings
    ///
    /// # Returns
    ///
    /// Returns a configured `SqlBeliefStore` instance with schema initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use engram_runtime::{SqliteOpenOptions, SqliteJournalMode, SqlitePath};
    /// use engram_store_belief_sqlite::SqlBeliefStore;
    ///
    /// let options = SqliteOpenOptions {
    ///     path: SqlitePath::File("/path/to/belief.db".into()),
    ///     create_parent_dirs: true,
    ///     journal_mode: SqliteJournalMode::Wal,
    ///     busy_timeout_ms: Some(5000),
    ///     foreign_keys: true,
    ///     run_migrations: true,
    /// };
    ///
    /// let store = SqlBeliefStore::open_with_options(options)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                            adapter: "engram-store-belief-sqlite".to_owned(),
                            message: format!("failed to create parent directory: {}", e),
                        })?;
                    }
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(|e| CoreError::Adapter {
            adapter: "engram-store-belief-sqlite".to_owned(),
            message: e.to_string(),
        })?;

        // Apply pragmas from options
        Self::apply_pragmas(&connection, &options)?;

        // Initialize schema
        initialize_schema(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        // Apply default pragmas for backward compatibility
        let options = SqliteOpenOptions::in_memory();
        Self::apply_pragmas(&connection, &options)?;

        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Applies SQLite PRAGMAs based on configuration options.
    ///
    /// This centralizes pragma application so both `open_with_options` and legacy
    /// constructors can use the same logic.
    fn apply_pragmas(connection: &Connection, options: &SqliteOpenOptions) -> CoreResult<()> {
        // Journal mode (returns result set, so we use query_row and ignore the result)
        let journal_pragma = format!(
            "PRAGMA journal_mode = {}",
            options.journal_mode.as_pragma_value()
        );
        connection
            .query_row(&journal_pragma, [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-belief-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Synchronous mode (NORMAL for WAL mode)
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-belief-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Busy timeout
        if let Some(timeout_ms) = options.busy_timeout_ms {
            let timeout_pragma = format!("PRAGMA busy_timeout = {}", timeout_ms);
            connection
                .query_row(&timeout_pragma, [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-belief-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Foreign keys
        if options.foreign_keys {
            connection
                .query_row("PRAGMA foreign_keys = ON", [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-belief-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Cache size (64MB default for better performance)
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-belief-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        Ok(())
    }

    pub(crate) fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-belief-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    fn visible_belief_or_not_found(&self, id: &BeliefId, scope: &Scope) -> CoreResult<Belief> {
        self.load_belief_by_id(id)?
            .filter(|belief| scope_allows(&belief.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })
    }

    /// Lists beliefs visible to `scope` (store-specific; not on the port). Used
    /// by the demo UI and as detector input.
    pub async fn list_beliefs(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        Ok(self
            .load_all_beliefs()?
            .into_iter()
            .filter(|belief| scope_allows(&belief.scope, scope))
            .collect())
    }

    /// Lists contradictions visible to `scope` (store-specific; not on the port).
    pub async fn list_contradictions(&self, scope: &Scope) -> CoreResult<Vec<Contradiction>> {
        Ok(self
            .load_all_contradictions()?
            .into_iter()
            .filter(|contradiction| scope_allows(&contradiction.scope, scope))
            .collect())
    }
}

#[async_trait]
impl BeliefRepository for SqlBeliefStore {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn upsert_belief(&self, mut belief: Belief) -> CoreResult<Belief> {
        if let Some(existing) = self.load_all_beliefs()?.into_iter().find(|existing| {
            existing.scope == belief.scope
                && existing.subject.key == belief.subject.key
                && existing.valid_from == belief.valid_from
        }) {
            belief.id = existing.id;
        }
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn get_belief(&self, query: BeliefQuery) -> CoreResult<Option<Belief>> {
        if query.requires_record_time_history() {
            return Err(CoreError::InvalidRequest {
                reason: "record-time belief history is not supported by engram-store-belief-sqlite"
                    .to_owned(),
            });
        }

        let now = Utc::now();
        let mut matches = self
            .load_all_beliefs()?
            .into_iter()
            .filter(|belief| scope_allows(&belief.scope, &query.scope))
            .filter(|belief| query.matches_after_scope(belief, now))
            .collect::<Vec<_>>();
        sort_beliefs_for_query(&mut matches, query.order);
        Ok(matches.into_iter().next())
    }

    async fn get_belief_by_id(&self, id: &BeliefId, scope: &Scope) -> CoreResult<Option<Belief>> {
        Ok(self
            .load_belief_by_id(id)?
            .filter(|belief| scope_allows(&belief.scope, scope)))
    }

    async fn mark_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope)?;
        let belief = mark_stale(belief, at);
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn clear_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope)?;
        let belief = clear_stale_state(belief, at);
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn supersede_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        replacement_id: BeliefId,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope)?;
        let belief = supersede_belief(belief, replacement_id, at);
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn retract_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope)?;
        let belief = retract_belief(belief, at);
        self.write_belief_row(&belief)?;
        Ok(belief)
    }

    async fn list_stale(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        Ok(self
            .load_all_beliefs()?
            .into_iter()
            .filter(|belief| scope_allows(&belief.scope, scope))
            .filter(|belief| belief.status == BeliefStatus::Stale || belief.stale == Some(true))
            .collect())
    }

    async fn beliefs_referencing_source(
        &self,
        query: BeliefReferenceQuery,
    ) -> CoreResult<Vec<Belief>> {
        let as_of = query.valid_at.unwrap_or_else(Utc::now);
        Ok(self
            .load_all_beliefs()?
            .into_iter()
            .filter(|belief| scope_allows(&belief.scope, &query.scope))
            .filter(|belief| {
                belief_references_source(belief, &query.source_type, &query.source_id, as_of)
            })
            .collect())
    }

    async fn put_contradiction(
        &self,
        mut contradiction: Contradiction,
    ) -> CoreResult<Contradiction> {
        if contradiction.targets.len() == 2 {
            let pair = canonicalize_pair(
                contradiction.targets[0].clone(),
                contradiction.targets[1].clone(),
            );
            contradiction.targets = vec![pair.left, pair.right];
            let pair_key = canonical_pair_key(&contradiction.targets[0], &contradiction.targets[1]);
            if let Some(existing) = self
                .load_all_contradictions()?
                .into_iter()
                .find(|existing| {
                    existing.scope == contradiction.scope
                        && existing.targets.len() == 2
                        && canonical_pair_key(&existing.targets[0], &existing.targets[1])
                            == pair_key
                })
            {
                return Ok(existing);
            }
        }
        self.write_contradiction_row(&contradiction)?;
        Ok(contradiction)
    }

    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        let connection = self.lock()?;
        let contradiction = connection
            .query_row(
                "SELECT record_json FROM contradictions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Contradiction>(&json).map_err(json_error))
            .transpose()?;
        Ok(contradiction.filter(|c| scope_allows(&c.scope, scope)))
    }

    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        let connection = self.lock()?;
        let existing = connection
            .query_row(
                "SELECT record_json FROM contradictions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Contradiction>(&json).map_err(json_error))
            .transpose()?;
        let mut contradiction = existing
            .filter(|c| scope_allows(&c.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "contradiction",
                target_id: id.to_string(),
            })?;

        contradiction.status = status_for_resolution(&resolution.kind);
        contradiction.updated_at = Some(resolution.resolved_at);
        contradiction.resolution = Some(resolution);

        let json = serde_json::to_string(&contradiction).map_err(json_error)?;
        connection
            .execute(
                "UPDATE contradictions SET record_json = ?1 WHERE id = ?2",
                params![json, id.to_string()],
            )
            .map_err(sql_error)?;
        Ok(contradiction)
    }
}

fn status_for_resolution(kind: &ContradictionResolutionKind) -> ContradictionStatus {
    match kind {
        ContradictionResolutionKind::ManualIgnore => ContradictionStatus::Ignored,
        ContradictionResolutionKind::NeedsMoreEvidence => ContradictionStatus::Open,
        ContradictionResolutionKind::TargetWon
        | ContradictionResolutionKind::Compatible
        | ContradictionResolutionKind::Merged
        | ContradictionResolutionKind::Retracted => ContradictionStatus::Resolved,
    }
}

fn sort_beliefs_for_query(beliefs: &mut [Belief], order: BeliefQueryOrder) {
    beliefs.sort_by(|left, right| match order {
        BeliefQueryOrder::LatestValidFirst => right
            .valid_from
            .cmp(&left.valid_from)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| right.id.to_string().cmp(&left.id.to_string())),
        BeliefQueryOrder::LatestRecordedFirst => right
            .updated_at
            .unwrap_or(right.created_at)
            .cmp(&left.updated_at.unwrap_or(left.created_at))
            .then_with(|| right.valid_from.cmp(&left.valid_from))
            .then_with(|| right.id.to_string().cmp(&left.id.to_string())),
    });
}
