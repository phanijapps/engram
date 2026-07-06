//! Taxonomy repository implementation for SQLite adapter.
//!
//! Handles concept schemes, concepts, and concept relationships. This module
//! focuses on taxonomy operations while leaving knowledge CRUD and graph
//! operations to their respective modules.

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::TaxonomyRepository;
use engram_runtime::CoreResult;
use rusqlite::OptionalExtension;

use crate::{scope::scope_allows, schema::json_error, schema::sql_error, service::SqlKnowledgeStore};

#[async_trait]
impl TaxonomyRepository for SqlKnowledgeStore {
    async fn put_concept_scheme(&self, scheme: ConceptScheme) -> CoreResult<ConceptScheme> {
        let json = serde_json::to_string(&scheme).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concept_schemes
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
                rusqlite::params![
                    scheme.id.to_string(),
                    scheme.scope.tenant,
                    scheme.scope.subject,
                    scheme.scope.workspace,
                    scheme.scope.session,
                    scheme.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(scheme)
    }

    async fn get_concept_scheme(
        &self,
        id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Option<ConceptScheme>> {
        let connection = self.lock()?;
        let scheme = connection
            .query_row(
                "SELECT record_json FROM concept_schemes WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<ConceptScheme>(&json).map_err(json_error))
            .transpose()?;
        Ok(scheme.filter(|scheme| scope_allows(&scheme.scope, scope)))
    }

    async fn put_concept(&self, concept: Concept) -> CoreResult<Concept> {
        let json = serde_json::to_string(&concept).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concepts (id, scheme_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    scheme_id = excluded.scheme_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![concept.id.to_string(), concept.scheme_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(concept)
    }

    async fn put_concept_relation(&self, relation: ConceptRelation) -> CoreResult<ConceptRelation> {
        let json = serde_json::to_string(&relation).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO concept_relations (id, scheme_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    scheme_id = excluded.scheme_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![relation.id, relation.scheme_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(relation)
    }

    async fn list_concepts(
        &self,
        scheme_id: &ConceptSchemeId,
        scope: &Scope,
    ) -> CoreResult<Vec<Concept>> {
        let connection = self.lock()?;
        let scheme = connection
            .query_row(
                "SELECT record_json FROM concept_schemes WHERE id = ?1",
                rusqlite::params![scheme_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<ConceptScheme>(&json).map_err(json_error))
            .transpose()?;
        let Some(scheme) = scheme else {
            return Ok(Vec::new());
        };
        if !scope_allows(&scheme.scope, scope) {
            return Ok(Vec::new());
        }

        let mut statement = connection
            .prepare("SELECT record_json FROM concepts WHERE scheme_id = ?1")
            .map_err(sql_error)?;
        let rows = statement
            .query_map(rusqlite::params![scheme_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        let mut concepts = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            concepts.push(serde_json::from_str::<Concept>(&json).map_err(json_error)?);
        }
        concepts.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        Ok(concepts)
    }
}
