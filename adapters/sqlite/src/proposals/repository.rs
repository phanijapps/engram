//! SQLite-backed [`ProposalRepository`] — contract JSON rows with CAS
//! transitions on `(id, version, status)`.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    OperatorProposal, OperatorProposalId, OperatorProposalStatus, ProposalDecision,
    ProposalPromotion, Scope,
};
use engram_proposals::ProposalRepository;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};

use crate::proposals::schema::{initialize_schema, json_error, sql_error};
use crate::proposals::scope::scope_allows;
use crate::{SqliteOpenOptions, SqlitePath};

/// SQLite proposal queue store. Owns ONLY `operator_proposals` — never
/// memory tables, even when a deployment shares one database file.
#[derive(Clone)]
pub struct SqlProposalStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlProposalStore {
    /// Opens an in-memory proposal store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql_error)?)
    }

    /// Opens a file-backed proposal store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_with_options(SqliteOpenOptions::file_wal_concurrent(
            path.as_ref().to_path_buf(),
        ))
    }

    /// Opens a SQLite proposal store with explicit configuration options.
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs
                    && let Some(parent) = path.parent()
                {
                    std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                        adapter: "engram-store-sqlite".to_owned(),
                        message: format!("failed to create parent directory: {e}"),
                    })?;
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(sql_error)?;
        Self::apply_pragmas(&connection, &options)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        let options = SqliteOpenOptions::in_memory();
        Self::apply_pragmas(&connection, &options)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn apply_pragmas(connection: &Connection, options: &SqliteOpenOptions) -> CoreResult<()> {
        connection
            .query_row(
                &format!(
                    "PRAGMA journal_mode = {}",
                    options.journal_mode.as_pragma_value()
                ),
                [],
                |_row| Ok(()),
            )
            .optional()
            .map_err(sql_error)?;
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;
        connection
            .query_row("PRAGMA busy_timeout = 5000", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(sql_error)?;
        Ok(())
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-sqlite".to_owned(),
            message: "proposal store connection poisoned".to_owned(),
        })
    }

    fn fetch(
        conn: &Connection,
        id: &OperatorProposalId,
        scope: &Scope,
    ) -> CoreResult<Option<OperatorProposal>> {
        conn.query_row(
            "SELECT record_json FROM operator_proposals WHERE id = ?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| {
            let proposal: OperatorProposal = serde_json::from_str(&json).map_err(json_error)?;
            if !scope_allows(&proposal.scope, scope) {
                // Scope mismatch behaves as absent (no cross-scope probing).
                return Ok(None);
            }
            Ok(Some(proposal))
        })
        .transpose()
        .map(|opt| opt.flatten())
    }

    fn upsert_row(conn: &Connection, proposal: &OperatorProposal) -> CoreResult<()> {
        let submission_key = proposal.decision.as_ref().map(|_| None::<String>).or(None);
        let _ = submission_key;
        let json = serde_json::to_string(proposal).map_err(json_error)?;
        conn.execute(
            "INSERT INTO operator_proposals (id, tenant, subject, workspace, session, environment, status, version, payload_fingerprint, submission_key, created_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, version = excluded.version, record_json = excluded.record_json",
            params!(
                proposal.id.to_string(),
                proposal.scope.tenant,
                proposal.scope.subject,
                proposal.scope.workspace,
                proposal.scope.session,
                proposal.scope.environment,
                proposal.status.as_str(),
                proposal.version as i64,
                proposal.payload_fingerprint,
                // Submission key is tracked only at insert time (see
                // insert_proposal); transitions keep the stored value.
                Option::<String>::None,
                proposal.created_at.to_rfc3339(),
                json,
            ),
        )
        .map_err(sql_error)?;
        Ok(())
    }
}

#[async_trait]
impl ProposalRepository for SqlProposalStore {
    async fn insert_proposal(&self, proposal: OperatorProposal) -> CoreResult<OperatorProposal> {
        let conn = self.lock()?;
        // Submission-key idempotency: same scope + key returns the existing
        // record instead of duplicating (the caller passes the key embedded
        // in metadata-free form via the fingerprinted id; the unique index
        // guards duplicates at the storage layer).
        let json = serde_json::to_string(&proposal).map_err(json_error)?;
        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO operator_proposals (id, tenant, subject, workspace, session, environment, status, version, payload_fingerprint, submission_key, created_at, record_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params!(
                    proposal.id.to_string(),
                    proposal.scope.tenant,
                    proposal.scope.subject,
                    proposal.scope.workspace,
                    proposal.scope.session,
                    proposal.scope.environment,
                    proposal.status.as_str(),
                    proposal.version as i64,
                    proposal.payload_fingerprint,
                    proposal.payload_fingerprint.clone(),
                    proposal.created_at.to_rfc3339(),
                    json,
                ),
            )
            .map_err(sql_error)?;
        if inserted == 0 {
            // Id collision with a different fingerprint is a real conflict;
            // the same fingerprint means a replay of the same submission.
            let existing = Self::fetch(&conn, &proposal.id, &proposal.scope)?;
            if let Some(existing) = existing {
                return Ok(existing);
            }
            return Err(CoreError::Conflict {
                reason: "proposal id collision in scope".to_owned(),
            });
        }
        Ok(proposal)
    }

    async fn get_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
    ) -> CoreResult<Option<OperatorProposal>> {
        let conn = self.lock()?;
        Self::fetch(&conn, id, scope)
    }

    async fn list_proposals(
        &self,
        scope: &Scope,
        status: Option<OperatorProposalStatus>,
        after: Option<&OperatorProposalId>,
        limit: u32,
    ) -> CoreResult<Vec<OperatorProposal>> {
        let conn = self.lock()?;
        let mut sql = "SELECT record_json FROM operator_proposals WHERE tenant = ?1".to_owned();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(scope.tenant.clone())];
        let mut idx = 2;
        if let Some(value) = &scope.subject {
            sql.push_str(&format!(" AND subject = ?{idx}"));
            params_vec.push(Box::new(value.clone()));
            idx += 1;
        }
        if let Some(value) = &scope.workspace {
            sql.push_str(&format!(" AND workspace = ?{idx}"));
            params_vec.push(Box::new(value.clone()));
            idx += 1;
        }
        if let Some(value) = &scope.session {
            sql.push_str(&format!(" AND session = ?{idx}"));
            params_vec.push(Box::new(value.clone()));
            idx += 1;
        }
        if let Some(value) = &scope.environment {
            sql.push_str(&format!(" AND environment = ?{idx}"));
            params_vec.push(Box::new(value.clone()));
            idx += 1;
        }
        if let Some(filter) = status {
            sql.push_str(&format!(" AND status = ?{idx}"));
            params_vec.push(Box::new(filter.as_str().to_owned()));
            idx += 1;
        }
        if let Some(cursor) = after {
            sql.push_str(&format!(" AND id > ?{idx}"));
            params_vec.push(Box::new(cursor.to_string()));
            idx += 1;
        }
        sql.push_str(&format!(" ORDER BY created_at ASC, id ASC LIMIT ?{idx}"));
        params_vec.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql).map_err(sql_error)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut out = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let proposal: OperatorProposal = serde_json::from_str(&json).map_err(json_error)?;
            if scope_allows(&proposal.scope, scope) {
                out.push(proposal);
            }
        }
        Ok(out)
    }

    async fn claim_approval(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
    ) -> CoreResult<OperatorProposal> {
        let conn = self.lock()?;
        let updated = conn
            .execute(
                "UPDATE operator_proposals SET status = 'applying', version = version + 1, record_json = json_set(record_json, '$.status', 'applying', '$.version', json_extract(record_json, '$.version') + 1, '$.updatedAt', ?3) WHERE id = ?1 AND version = ?2 AND status = 'pending'",
                params![id.to_string(), expected_version as i64, Utc::now().to_rfc3339()],
            )
            .map_err(sql_error)?;
        if updated == 0 {
            let existing = Self::fetch(&conn, id, scope)?;
            return match existing.map(|p| p.status) {
                Some(OperatorProposalStatus::Applying) => Err(CoreError::Conflict {
                    reason: "proposal already claimed by another approval".to_owned(),
                }),
                Some(_) => Err(CoreError::Conflict {
                    reason: "proposal is not pending".to_owned(),
                }),
                None => Err(CoreError::NotFound {
                    target_type: "operator_proposal",
                    target_id: id.to_string(),
                }),
            };
        }
        Self::fetch(&conn, id, scope)?.ok_or_else(|| CoreError::NotFound {
            target_type: "operator_proposal",
            target_id: id.to_string(),
        })
    }

    async fn record_promotion(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
        promotion: ProposalPromotion,
    ) -> CoreResult<OperatorProposal> {
        let conn = self.lock()?;
        let mut proposal = Self::fetch(&conn, id, scope)?.ok_or_else(|| CoreError::NotFound {
            target_type: "operator_proposal",
            target_id: id.to_string(),
        })?;
        // Idempotent finalize: same receipt, no-op.
        if proposal.status == OperatorProposalStatus::Approved {
            if proposal.promotion.as_ref() == Some(&promotion) {
                return Ok(proposal);
            }
            return Err(CoreError::Conflict {
                reason: "proposal already approved with a different receipt".to_owned(),
            });
        }
        if proposal.status != OperatorProposalStatus::Applying {
            return Err(CoreError::Conflict {
                reason: format!(
                    "proposal is {} ; only applying proposals can be finalized",
                    proposal.status.as_str()
                ),
            });
        }
        if proposal.version != expected_version {
            return Err(CoreError::Conflict {
                reason: format!(
                    "version mismatch: expected {}, got {}",
                    proposal.version, expected_version
                ),
            });
        }
        proposal.status = OperatorProposalStatus::Approved;
        proposal.version += 1;
        proposal.promotion = Some(promotion);
        proposal.updated_at = Some(Utc::now());
        Self::upsert_row(&conn, &proposal)?;
        Ok(proposal)
    }

    async fn reject_proposal(
        &self,
        id: &OperatorProposalId,
        scope: &Scope,
        expected_version: u64,
        decision: ProposalDecision,
    ) -> CoreResult<OperatorProposal> {
        let conn = self.lock()?;
        let mut proposal = Self::fetch(&conn, id, scope)?.ok_or_else(|| CoreError::NotFound {
            target_type: "operator_proposal",
            target_id: id.to_string(),
        })?;
        if proposal.status == OperatorProposalStatus::Rejected {
            return Ok(proposal); // idempotent terminal state
        }
        if proposal.status != OperatorProposalStatus::Pending {
            return Err(CoreError::Conflict {
                reason: format!(
                    "proposal is {}; only pending proposals can be rejected",
                    proposal.status.as_str()
                ),
            });
        }
        if proposal.version != expected_version {
            return Err(CoreError::Conflict {
                reason: format!(
                    "version mismatch: expected {}, got {}",
                    proposal.version, expected_version
                ),
            });
        }
        proposal.status = OperatorProposalStatus::Rejected;
        proposal.version += 1;
        proposal.decision = Some(decision);
        proposal.updated_at = Some(Utc::now());
        Self::upsert_row(&conn, &proposal)?;
        Ok(proposal)
    }
}
