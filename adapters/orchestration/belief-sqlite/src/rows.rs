//! JSON row persistence helpers for the SQLite belief adapter.
//!
//! These helpers own SQL read/write details only. Repository semantics,
//! valid-time filtering, lifecycle transitions, and contradiction detection stay
//! in sibling modules.

use engram_domain::*;
use engram_runtime::CoreResult;
use rusqlite::{OptionalExtension, params};

use crate::{
    schema::{json_error, sql_error},
    service::SqlBeliefStore,
};

impl SqlBeliefStore {
    pub(crate) fn write_belief_row(&self, belief: &Belief) -> CoreResult<()> {
        let json = serde_json::to_string(belief).map_err(json_error)?;
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
        Ok(())
    }

    pub(crate) fn write_contradiction_row(&self, contradiction: &Contradiction) -> CoreResult<()> {
        let json = serde_json::to_string(contradiction).map_err(json_error)?;
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
        Ok(())
    }

    pub(crate) fn load_all_beliefs(&self) -> CoreResult<Vec<Belief>> {
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
            beliefs.push(serde_json::from_str::<Belief>(&json).map_err(json_error)?);
        }
        Ok(beliefs)
    }

    pub(crate) fn load_belief_by_id(&self, id: &BeliefId) -> CoreResult<Option<Belief>> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT record_json FROM beliefs WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Belief>(&json).map_err(json_error))
            .transpose()
    }

    pub(crate) fn load_all_contradictions(&self) -> CoreResult<Vec<Contradiction>> {
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
            contradictions.push(serde_json::from_str::<Contradiction>(&json).map_err(json_error)?);
        }
        Ok(contradictions)
    }
}
