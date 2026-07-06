//! Contract entity and relationship construction for OpenAPI operations.
//!
//! Builds KnowledgeEntity and KnowledgeRelationship instances from parsed
//! OpenAPI operations. Handles ID derivation, source references, and
//! cross-repo source ref union.

use chrono::{DateTime, Utc};
use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeRepository};
use serde_json::Value as JsonValue;

use crate::{extractor::repo_entity_id, hash::content_hash};

/// One REST operation extracted from an OpenAPI document.
#[derive(Debug, Clone)]
pub struct ParsedOperation {
    /// HTTP method in uppercase (e.g. `"GET"`).
    pub method: String,
    /// Raw path as it appears in the document (e.g. `"/orders/{id}"`).
    pub path: String,
    /// Operation summary from the spec, if present.
    pub summary: Option<String>,
    /// Media types from the request body content map.
    pub request_media_types: Vec<String>,
    /// Media types from the response content maps (de-duplicated).
    pub response_media_types: Vec<String>,
    /// Stable contract key (e.g. `"GET /orders/{}"`).
    pub normalized_key: String,
}

// ── Id derivation ─────────────────────────────────────────────────────────────

/// Derives a stable, document-independent `EntityId` for a contract node.
///
/// Keyed on the **full scope discriminator** + normalized key so two sources in
/// the same scope declaring the same operation resolve to the same entity id
/// (upsert converges to one node), while different scopes (tenants) never
/// collide.
pub fn contract_entity_id(scope: &Scope, normalized_key: &str) -> EntityId {
    Id::from(format!(
        "api-{}",
        content_hash(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{normalized_key}",
            scope.tenant,
            scope.subject.as_deref().unwrap_or(""),
            scope.workspace.as_deref().unwrap_or(""),
            scope.session.as_deref().unwrap_or(""),
            scope.environment.as_deref().unwrap_or(""),
        ))
        .trim_start_matches("sha256:")
    ))
}

/// Derives a stable `RelationshipId` for an `exposes` edge.
///
/// Keyed on `(stable_source_key, normalized_key)` — not on the ephemeral
/// `source_id` — so the edge id survives across commits of the same repo.
pub fn exposes_rel_id(stable_source_key: &str, normalized_key: &str) -> RelationshipId {
    Id::from(format!(
        "exposes-{}",
        content_hash(format!("{stable_source_key}\u{1f}{normalized_key}"))
            .trim_start_matches("sha256:")
    ))
}

// ── Entity + relationship construction ───────────────────────────────────────

/// Builds an `EntityKind::Api` entity for one REST operation.
///
/// The entity id is **document-independent** (keyed on scope + normalized key)
/// so upserts from different sources with the same operation converge to one
/// node. The `source_ref` uses `stable_source_key` (not the ephemeral source
/// id) so it is stable across commits.
pub fn build_api_entity(
    scope: &Scope,
    stable_source_key: &str,
    op: &ParsedOperation,
    provenance: &Provenance,
    now: DateTime<Utc>,
) -> KnowledgeEntity {
    let entity_id = contract_entity_id(scope, &op.normalized_key);

    let mut meta = Metadata::default();
    meta.insert("method".to_owned(), JsonValue::String(op.method.clone()));
    meta.insert("path".to_owned(), JsonValue::String(op.path.clone()));
    meta.insert(
        "normalizedKey".to_owned(),
        JsonValue::String(op.normalized_key.clone()),
    );
    if let Some(ref s) = op.summary {
        meta.insert("summary".to_owned(), JsonValue::String(s.clone()));
    }
    if !op.request_media_types.is_empty() {
        meta.insert(
            "requestMediaTypes".to_owned(),
            JsonValue::Array(
                op.request_media_types
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        );
    }
    if !op.response_media_types.is_empty() {
        meta.insert(
            "responseMediaTypes".to_owned(),
            JsonValue::Array(
                op.response_media_types
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        );
    }

    // source_ref: identifies the declaring repository by its stable key.
    // Using EvidenceTargetType::Source with target_id = stable_source_key so
    // the reference survives across commits (the stable key has no SHA).
    let source_ref = EvidenceRef {
        target_type: EvidenceTargetType::Source,
        target_id: Some(stable_source_key.to_owned()),
        uri: None,
        quote: None,
        location: None,
    };

    KnowledgeEntity {
        id: entity_id,
        graph_id: None, // not file-scoped; scope-global contract node
        kind: EntityKind::Api,
        name: op.normalized_key.clone(),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: vec![source_ref],
        concept_refs: Vec::new(),
        provenance: provenance.clone(),
        created_at: now,
        updated_at: None,
        metadata: Some(meta),
    }
}

/// Builds the `exposes` relationship from the Repository entity to the API entity.
///
/// The relationship id is derived from `(stable_source_key, normalized_key)`
/// so re-ingesting the same source (new commit) produces the same id and
/// upserts cleanly.
pub fn build_exposes_rel(
    scope: &Scope,
    stable_source_key: &str,
    op: &ParsedOperation,
    provenance: &Provenance,
    now: DateTime<Utc>,
) -> KnowledgeRelationship {
    let api_entity_id = contract_entity_id(scope, &op.normalized_key);
    let repo_id = repo_entity_id(scope, stable_source_key);

    KnowledgeRelationship {
        id: exposes_rel_id(stable_source_key, &op.normalized_key),
        graph_id: None, // not file-scoped
        subject: EntityRef {
            id: Some(repo_id),
            kind: Some("repository".to_owned()),
            name: Some(stable_source_key.to_owned()),
            aliases: Vec::new(),
        },
        predicate: "exposes".to_owned(),
        object: EntityRef {
            id: Some(api_entity_id),
            kind: Some("api".to_owned()),
            name: Some(op.normalized_key.clone()),
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(0.95),
        provenance: provenance.clone(),
        created_at: now,
        updated_at: None,
    }
}

// ── Source-ref union (T5 cross-repo merge) ────────────────────────────────────

/// Upserts an API entity with source-ref union (read-modify-write).
///
/// Reads any existing entity at the same id, unions the new source_ref into
/// the existing `source_refs` (idempotent: does not add a duplicate for the
/// same `stable_source_key`), then writes the merged entity back.
///
/// Safety guarantee rests on the **per-scope single-writer assumption** built
/// into this crate's ingest path: only one `scan_repository` call runs for a
/// given scope at a time. The SQLite adapter serializes individual store calls
/// through a `Mutex<Connection>`, but it does NOT hold that lock across the
/// read+write pair — the read and write are two separate lock acquisitions.
/// Concurrent ingests into the same scope from different processes could race
/// and lose a `source_ref`; that scenario is outside the supported operational
/// envelope for Phase A.
pub async fn upsert_api_entity_with_source_ref<R>(
    repo: &R,
    scope: &Scope,
    new_entity: KnowledgeEntity,
) -> CoreResult<()>
where
    R: KnowledgeRepository + ?Sized,
{
    let entity_id = new_entity.id.clone();
    // The new entity carries exactly one source_ref (this source's stable key).
    let new_ref = new_entity.source_refs.first().cloned();

    let merged = match repo.get_entity(&entity_id, scope).await? {
        Some(mut existing) => {
            // Union: only add if this source is not already in the list.
            if let Some(ref r) = new_ref {
                let already = existing
                    .source_refs
                    .iter()
                    .any(|e| e.target_id.as_deref() == r.target_id.as_deref());
                if !already {
                    existing.source_refs.push(r.clone());
                }
            }
            // Overwrite metadata with the latest parse (may have updated detail).
            existing.metadata = new_entity.metadata;
            existing
        }
        None => new_entity,
    };
    repo.put_entity(merged).await?;
    Ok(())
}

// ── Retraction (T8 per-source convergence) ───────────────────────────────────

/// Retracts one source's `exposes` edge and `source_ref` for a previously
/// declared contract key.
///
/// Steps:
/// 1. Delete the `exposes` relationship for `(stable_source_key, normalized_key)`.
/// 2. Read the API entity; remove the source_ref for `stable_source_key`.
/// 3. If `source_refs` is now empty, delete the entity (last source retracted).
///    Otherwise write back the trimmed entity.
///
/// Idempotent: if the relationship or entity is already absent, returns `Ok(())`.
pub async fn retract_contract_op<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
    normalized_key: &str,
) -> CoreResult<()>
where
    R: KnowledgeRepository + ?Sized,
{
    // Delete the exposes edge (ignore "not found" — idempotent).
    let rel_id = exposes_rel_id(stable_source_key, normalized_key);
    let _ = repo.delete_relationship(&rel_id, scope).await;

    // Remove this source's source_ref from the entity.
    let entity_id = contract_entity_id(scope, normalized_key);
    let Some(mut entity) = repo.get_entity(&entity_id, scope).await? else {
        return Ok(()); // already absent
    };
    entity
        .source_refs
        .retain(|r| r.target_id.as_deref() != Some(stable_source_key));

    if entity.source_refs.is_empty() {
        // Last source retracted — delete the orphaned contract node.
        let _ = repo.delete_entity(&entity_id, scope).await;
    } else {
        repo.put_entity(entity).await?;
    }
    Ok(())
}
