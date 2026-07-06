//! Ontology repository implementation for SQLite adapter.
//!
//! Handles ontologies, classes, properties, and axioms. This module focuses on
//! ontology operations while leaving knowledge CRUD, graph, and taxonomy
//! operations to their respective modules.

use async_trait::async_trait;
use chrono::Utc;
use engram_domain::*;
use engram_knowledge::OntologyRepository;
use engram_runtime::CoreResult;
use rusqlite::OptionalExtension;

use crate::{scope::scope_allows, schema::json_error, schema::sql_error, service::SqlKnowledgeStore, service::VALIDATE_RELATIONSHIP_LIMIT, service::validation_provenance};

#[async_trait]
impl OntologyRepository for SqlKnowledgeStore {
    async fn put_ontology(&self, ontology: Ontology) -> CoreResult<Ontology> {
        let json = serde_json::to_string(&ontology).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontologies
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
                    ontology.id.to_string(),
                    ontology.scope.tenant,
                    ontology.scope.subject,
                    ontology.scope.workspace,
                    ontology.scope.session,
                    ontology.scope.environment,
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(ontology)
    }

    async fn get_ontology(&self, id: &OntologyId, scope: &Scope) -> CoreResult<Option<Ontology>> {
        let connection = self.lock()?;
        let ontology = connection
            .query_row(
                "SELECT record_json FROM ontologies WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Ontology>(&json).map_err(json_error))
            .transpose()?;
        Ok(ontology.filter(|ontology| scope_allows(&ontology.scope, scope)))
    }

    // Classes, properties, and axioms carry no scope of their own — they inherit
    // visibility from their owning ontology (mirroring concepts ↔ concept
    // scheme). `put_*` does not re-verify the caller owns `ontology_id`; reads
    // (`get_ontology`, `validate_graph`) enforce scope on the parent ontology.
    async fn put_class(&self, class: OntologyClass) -> CoreResult<OntologyClass> {
        let json = serde_json::to_string(&class).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_classes (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![class.id.to_string(), class.ontology_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(class)
    }

    async fn put_property(&self, property: OntologyProperty) -> CoreResult<OntologyProperty> {
        let json = serde_json::to_string(&property).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_properties (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![
                    property.id.to_string(),
                    property.ontology_id.to_string(),
                    json
                ],
            )
            .map_err(sql_error)?;
        Ok(property)
    }

    async fn put_axiom(&self, axiom: OntologyAxiom) -> CoreResult<OntologyAxiom> {
        let json = serde_json::to_string(&axiom).map_err(crate::schema::json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO ontology_axioms (id, ontology_id, record_json)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    ontology_id = excluded.ontology_id,
                    record_json = excluded.record_json
                "#,
                rusqlite::params![axiom.id.to_string(), axiom.ontology_id.to_string(), json],
            )
            .map_err(sql_error)?;
        Ok(axiom)
    }

    /// Advisory validation: warns on relationships whose predicate is not declared
    /// as an ontology property (by label or URI). It never rejects writes — the
    /// port is advisory by contract. A missing or scope-hidden ontology, or a
    /// scope-hidden relationship, contributes no findings.
    async fn validate_graph(
        &self,
        graph_id: &KnowledgeGraphId,
        ontology_id: &OntologyId,
        scope: &Scope,
    ) -> CoreResult<Vec<OntologyValidationFinding>> {
        let connection = self.lock()?;
        let ontology = connection
            .query_row(
                "SELECT record_json FROM ontologies WHERE id = ?1",
                rusqlite::params![ontology_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|json| serde_json::from_str::<Ontology>(&json).map_err(json_error))
            .transpose()?;
        let Some(ontology) = ontology else {
            return Ok(Vec::new());
        };
        if !scope_allows(&ontology.scope, scope) {
            return Ok(Vec::new());
        }

        // Declared vocabulary = property labels + URIs (lowercased).
        let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut property_statement = connection
            .prepare("SELECT record_json FROM ontology_properties WHERE ontology_id = ?1")
            .map_err(sql_error)?;
        let property_rows = property_statement
            .query_map(rusqlite::params![ontology_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        for row in property_rows {
            let json = row.map_err(sql_error)?;
            let property = serde_json::from_str::<OntologyProperty>(&json).map_err(json_error)?;
            declared.insert(property.label.to_lowercase());
            declared.insert(property.uri.to_lowercase());
        }
        drop(property_statement);

        let now = Utc::now();
        // Bound the scan so an oversized graph cannot make advisory validation
        // unbounded: read LIMIT+1 rows, and if that many come back, report
        // truncation instead of scanning further. `drop` the statement before
        // reusing the connection (rusqlite borrow lifetime).
        let mut relationship_statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_relationships \
                 WHERE graph_id = ?1 ORDER BY id LIMIT ?2",
            )
            .map_err(sql_error)?;
        let relationship_rows: Vec<String> = relationship_statement
            .query_map(
                rusqlite::params![
                    graph_id.to_string(),
                    (VALIDATE_RELATIONSHIP_LIMIT + 1) as i64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?
            .collect::<Result<_, _>>()
            .map_err(sql_error)?;
        drop(relationship_statement);
        let truncated = relationship_rows.len() > VALIDATE_RELATIONSHIP_LIMIT;

        let mut findings = Vec::new();
        for json in relationship_rows
            .into_iter()
            .take(VALIDATE_RELATIONSHIP_LIMIT)
        {
            let relationship =
                serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error)?;
            if !scope_allows(&relationship.scope, scope) {
                continue;
            }
            if declared.contains(&relationship.predicate.to_lowercase()) {
                continue;
            }
            findings.push(OntologyValidationFinding {
                id: format!("finding-{ontology_id}-{}", relationship.id),
                ontology_id: ontology_id.clone(),
                severity: OntologyValidationSeverity::Warning,
                code: "undeclared_predicate".to_owned(),
                message: format!(
                    "relationship predicate `{}` is not declared by ontology `{ontology_id}`",
                    relationship.predicate
                ),
                target: Some(relationship.subject.clone()),
                axiom_id: None,
                provenance: validation_provenance(ontology_id, now),
                detected_at: now,
            });
        }
        if truncated {
            findings.push(OntologyValidationFinding {
                id: format!("finding-{ontology_id}-truncated"),
                ontology_id: ontology_id.clone(),
                severity: OntologyValidationSeverity::Info,
                code: "validation_truncated".to_owned(),
                message: format!(
                    "graph has more than {limit} relationships; validation truncated",
                    limit = VALIDATE_RELATIONSHIP_LIMIT
                ),
                target: None,
                axiom_id: None,
                provenance: validation_provenance(ontology_id, now),
                detected_at: now,
            });
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(findings)
    }
}
