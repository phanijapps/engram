//! Identity storage transactions reuse the canonical normalization and merge functions.
use crate::{HelixStore, error::Error, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{
    EntityIdentityRepository,
    identity::{compute_identity_key, compute_relationship_key, merge_entities},
};
use engram_runtime::CoreResult;
use std::collections::BTreeMap;
#[async_trait]
impl EntityIdentityRepository for HelixStore {
    async fn resolve_or_put_entity(
        &self,
        request: EntityWriteRequest,
    ) -> CoreResult<EntityWriteOutcome> {
        self.check_scope(&request.entity.scope).map_err(core)?;
        self.atomic(|s| {
            let key = compute_identity_key(&request.entity, &request.identity);
            let existing = match &key {
                Some(k) => s
                    .select_values(ENTITY, Some(("_identity_key", k)))?
                    .into_iter()
                    .next()
                    .map(decode::<KnowledgeEntity>)
                    .transpose()?,
                None => None,
            };
            let (entity, outcome) = match existing {
                None => (
                    request.entity.clone(),
                    EntityWriteOutcome::Created {
                        entity: request.entity.clone(),
                    },
                ),
                Some(prior) => {
                    let (merged, changed, conflicts) =
                        merge_entities(&prior, &request.entity, &request.merge_policy);
                    if changed.is_empty() {
                        return Ok(EntityWriteOutcome::Matched { entity: prior });
                    }
                    (
                        merged.clone(),
                        EntityWriteOutcome::Merged {
                            entity: merged,
                            changed_fields: changed,
                            conflicts,
                        },
                    )
                }
            };
            let mut payload = value(&entity)?;
            if let Some(key) = key {
                payload["_identity_key"] = key.into();
            }
            s.stage(ENTITY, &entity.id.to_string(), Some(payload))?;
            Ok(outcome)
        })
        .map_err(core)
    }
    async fn resolve_or_put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.check_scope(&relationship.scope).map_err(core)?;
        self.atomic(|s| {
            let key = compute_relationship_key(&relationship);
            let prior = s
                .select_values(RELATIONSHIP, Some(("_relationship_key", &key)))?
                .into_iter()
                .next()
                .map(decode::<KnowledgeRelationship>)
                .transpose()?;
            let mut record = if let Some(mut prior) = prior {
                for evidence in &relationship.evidence {
                    if !prior.evidence.contains(evidence) {
                        prior.evidence.push(evidence.clone());
                    }
                }
                if prior.confidence.is_none() {
                    prior.confidence = relationship.confidence;
                }
                prior.updated_at = Some(relationship.created_at);
                prior
            } else {
                relationship.clone()
            };
            s.write_relationship(&record)?;
            let mut payload = s
                .get::<serde_json::Value>(RELATIONSHIP, &record.id.to_string())?
                .ok_or(Error::Storage)?;
            payload["_relationship_key"] = key.into();
            s.stage(RELATIONSHIP, &record.id.to_string(), Some(payload))?;
            // Return the canonical domain record, never physical fields.
            record.scope = relationship.scope;
            Ok(record)
        })
        .map_err(core)
    }
    async fn discover_collisions(
        &self,
        scope: &Scope,
        mode: &EntityIdentityMode,
    ) -> CoreResult<Vec<CollisionGroup>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let mut groups: BTreeMap<String, Vec<EntityId>> = BTreeMap::new();
        for entity in self.all::<KnowledgeEntity>(ENTITY).map_err(core)? {
            if let Some(key) = compute_identity_key(&entity, mode) {
                groups.entry(key).or_default().push(entity.id);
            }
        }
        Ok(groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(identity_key, mut entity_ids)| {
                entity_ids.sort_by_key(ToString::to_string);
                CollisionGroup {
                    identity_key,
                    entity_ids,
                }
            })
            .collect())
    }
    async fn consolidate_entities(
        &self,
        request: EntityMergeRequest,
    ) -> CoreResult<EntityMergeResult> {
        self.check_scope(&request.scope).map_err(core)?;
        self.atomic(|s| {
            let mut canonical: KnowledgeEntity = s
                .get(ENTITY, &request.canonical_id.to_string())?
                .ok_or(Error::InvalidConfig("canonical entity absent"))?;
            let mut conflicts = vec![];
            let mut deleted = 0;
            let mut redirected = 0;
            let mut coalesced = 0;
            let mut seen = std::collections::HashSet::new();
            let duplicates = request
                .duplicate_ids
                .iter()
                .filter(|id| *id != &request.canonical_id && seen.insert((*id).clone()))
                .collect::<Vec<_>>();
            for id in &duplicates {
                if let Some(prior) = s.get::<KnowledgeEntity>(ENTITY, &id.to_string())? {
                    let (merged, _, found) = merge_entities(&canonical, &prior, &request.policy);
                    canonical = merged;
                    conflicts.extend(found);
                    s.stage(ENTITY, &id.to_string(), None)?;
                    deleted += 1;
                }
            }
            s.write_entity(&canonical)?;
            let mut grouped: BTreeMap<String, KnowledgeRelationship> = BTreeMap::new();
            for mut relation in s.all::<KnowledgeRelationship>(RELATIONSHIP)? {
                let mut changed = false;
                for endpoint in [&mut relation.subject, &mut relation.object] {
                    if endpoint.id.as_ref().is_some_and(|id| seen.contains(id)) {
                        endpoint.id = Some(request.canonical_id.clone());
                        changed = true;
                    }
                }
                if changed {
                    redirected += 1;
                }
                let key = compute_relationship_key(&relation);
                if let Some(kept) = grouped.get_mut(&key) {
                    for ev in &relation.evidence {
                        if !kept.evidence.contains(ev) {
                            kept.evidence.push(ev.clone());
                        }
                    }
                    s.stage(RELATIONSHIP, &relation.id.to_string(), None)?;
                    coalesced += 1;
                } else {
                    grouped.insert(key, relation);
                }
            }
            for (key, relation) in grouped {
                s.write_relationship(&relation)?;
                let mut v = s
                    .get::<serde_json::Value>(RELATIONSHIP, &relation.id.to_string())?
                    .ok_or(Error::Storage)?;
                v["_relationship_key"] = key.into();
                s.stage(RELATIONSHIP, &relation.id.to_string(), Some(v))?;
            }
            Ok(EntityMergeResult {
                canonical_entity: canonical,
                redirected_relationships: redirected,
                coalesced_relationships: coalesced,
                deleted_entities: deleted,
                conflicts,
                audit_id: format!("consolidate-{}", request.canonical_id),
            })
        })
        .map_err(core)
    }
}
