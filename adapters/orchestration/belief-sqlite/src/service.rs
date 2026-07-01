//! SQLite-backed belief + contradiction repository and contradiction detector.
//!
//! Storage-only: this module persists belief and contradiction payloads as JSON
//! with scope indexing. Detection (`ContradictionDetector`) is advisory — it
//! surfaces tension between active beliefs on the same subject; resolution is a
//! deliberate action, never an automatic overwrite. Bi-temporal `valid_from`/
//! `valid_until` are stored as part of the payload and surfaced for display only
//! (no `transaction_time`, no as-of queries).

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_core::{BeliefRepository, ContradictionDetector};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

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
    connection: Arc<Mutex<Connection>>,
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

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-belief-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    /// Lists beliefs visible to `scope` (store-specific; not on the port). Used
    /// by the demo UI and as detector input.
    pub async fn list_beliefs(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM beliefs ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut beliefs = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let belief = serde_json::from_str::<Belief>(&json).map_err(json_error)?;
            if scope_allows(&belief.scope, scope) {
                beliefs.push(belief);
            }
        }
        Ok(beliefs)
    }

    /// Lists contradictions visible to `scope` (store-specific; not on the port).
    pub async fn list_contradictions(&self, scope: &Scope) -> CoreResult<Vec<Contradiction>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM contradictions ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut contradictions = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let contradiction = serde_json::from_str::<Contradiction>(&json).map_err(json_error)?;
            if scope_allows(&contradiction.scope, scope) {
                contradictions.push(contradiction);
            }
        }
        Ok(contradictions)
    }
}

#[async_trait]
impl BeliefRepository for SqlBeliefStore {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        let json = serde_json::to_string(&belief).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO beliefs
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    belief.id.to_string(),
                    belief.scope.tenant,
                    belief.scope.subject,
                    belief.scope.workspace,
                    belief.scope.session,
                    belief.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(belief)
    }

    async fn put_contradiction(&self, contradiction: Contradiction) -> CoreResult<Contradiction> {
        let json = serde_json::to_string(&contradiction).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO contradictions
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    contradiction.id.to_string(),
                    contradiction.scope.tenant,
                    contradiction.scope.subject,
                    contradiction.scope.workspace,
                    contradiction.scope.session,
                    contradiction.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
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

#[async_trait]
impl ContradictionDetector for SqlBeliefStore {
    /// Advisory detection: groups ACTIVE beliefs by subject key and flags any
    /// group whose members do not all share the same content. Each such group
    /// yields one Logical contradiction (severity = the group's max confidence).
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>> {
        let mut groups: HashMap<String, Vec<&Belief>> = HashMap::new();
        for belief in beliefs {
            if belief.status == BeliefStatus::Active {
                groups
                    .entry(belief.subject.key.clone())
                    .or_default()
                    .push(belief);
            }
        }

        let now = Utc::now();
        let mut findings = Vec::new();
        for (key, group) in groups {
            if group.len() < 2 {
                continue;
            }
            let distinct: HashSet<&str> = group.iter().map(|b| b.content.as_str()).collect();
            if distinct.len() < 2 {
                continue;
            }
            let severity = group
                .iter()
                .map(|b| b.confidence)
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0);
            let targets = group
                .iter()
                .map(|b| ContradictionTarget {
                    target_type: ContradictionTargetType::Belief,
                    target_id: b.id.to_string(),
                    role: None,
                })
                .collect::<Vec<_>>();
            findings.push(Contradiction {
                id: contradiction_id_for(&key),
                scope: group[0].scope.clone(),
                kind: ContradictionKind::Logical,
                targets,
                severity,
                status: ContradictionStatus::Open,
                reasoning: Some(format!(
                    "{} active beliefs on `{key}` disagree",
                    group.len()
                )),
                detected_by: None,
                resolution: None,
                provenance: detector_provenance(&key, now),
                detected_at: now,
                updated_at: None,
            });
        }
        findings.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        Ok(findings)
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

/// Deterministic contradiction id from the subject key (one contradiction per
/// subject at a time; re-detection upserts the same row).
fn contradiction_id_for(key: &str) -> ContradictionId {
    let hash = Sha256::digest(key.as_bytes());
    ContradictionId::from(format!("contradiction-{}", hex(&hash[..8])))
}

/// Builds the advisory provenance stamped on detected contradictions.
fn detector_provenance(key: &str, now: chrono::DateTime<Utc>) -> Provenance {
    Provenance {
        source: format!("belief-detector:{key}"),
        actor: Actor {
            id: Id::from("engram-contradiction-detector"),
            kind: ActorKind::System,
            display_name: Some("Contradiction detector".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("contradiction_detection".to_owned()),
    }
}

/// Lowercase hex encoding (avoids pulling a `hex` crate for 8 bytes).
fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}
