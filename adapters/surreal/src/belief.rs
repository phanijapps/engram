//! Surreal belief cell — `BeliefRepository` over embedded SurrealKV.
//!
//! Mirrors `engram-store-sqlite::belief`: persists beliefs + contradictions
//! (DTO under a `data` field, scope-indexed) and delegates lifecycle / temporal
//! / query / contradiction logic to the SHARED `engram_belief` helpers — identical
//! behavior across backends. Valid-time `as_of` queries filter on `valid_from`/
//! `valid_until`; record-time history is rejected (current rows, not versions).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use engram_belief::{
    BeliefQuery, BeliefQueryOrder, BeliefReferenceQuery, BeliefRepository, belief_references_source,
    canonical_pair_key, canonicalize_pair, clear_stale_state, mark_stale, retract_belief,
    supersede_belief,
};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

use crate::util::{DataWrapper, scope_allows, surreal_err};
use crate::SurrealConnection;

const BELIEF_TABLE: &str = "belief";
const CONTRADICTION_TABLE: &str = "contradiction";

/// `BeliefRepository` backed by embedded SurrealKV.
pub struct SurrealBeliefStore {
    conn: Arc<SurrealConnection>,
}

impl SurrealBeliefStore {
    pub fn new(conn: Arc<SurrealConnection>) -> Self {
        Self { conn }
    }

    async fn write_belief(&self, belief: &Belief) -> CoreResult<()> {
        let db = self.conn.db().await?;
        let key = belief.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{BELIEF_TABLE}', $key) SET data = $belief"
        ))
        .bind(("key", key))
        .bind(("belief", belief.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(())
    }

    async fn load_all_beliefs(&self) -> CoreResult<Vec<Belief>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM {BELIEF_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<Belief>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().map(|w| w.data).collect())
    }

    async fn load_belief_by_id(&self, id: &BeliefId) -> CoreResult<Option<Belief>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!(
                "SELECT data FROM type::thing('{BELIEF_TABLE}', $key)"
            ))
            .bind(("key", id.to_string()))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<Belief>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().next().map(|w| w.data))
    }

    async fn visible_belief_or_not_found(
        &self,
        id: &BeliefId,
        scope: &Scope,
    ) -> CoreResult<Belief> {
        self.load_belief_by_id(id)
            .await?
            .filter(|b| scope_allows(&b.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "belief",
                target_id: id.to_string(),
            })
    }

    async fn write_contradiction(&self, contradiction: &Contradiction) -> CoreResult<()> {
        let db = self.conn.db().await?;
        let key = contradiction.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{CONTRADICTION_TABLE}', $key) SET data = $contradiction"
        ))
        .bind(("key", key))
        .bind(("contradiction", contradiction.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(())
    }

    async fn load_all_contradictions(&self) -> CoreResult<Vec<Contradiction>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!("SELECT data FROM {CONTRADICTION_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<Contradiction>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().map(|w| w.data).collect())
    }

    async fn load_contradiction_by_id(
        &self,
        id: &ContradictionId,
    ) -> CoreResult<Option<Contradiction>> {
        let db = self.conn.db().await?;
        let mut res = db
            .query(&format!(
                "SELECT data FROM type::thing('{CONTRADICTION_TABLE}', $key)"
            ))
            .bind(("key", id.to_string()))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<DataWrapper<Contradiction>> = res.take(0).map_err(surreal_err)?;
        Ok(rows.into_iter().next().map(|w| w.data))
    }
}

#[async_trait]
impl BeliefRepository for SurrealBeliefStore {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn upsert_belief(&self, mut belief: Belief) -> CoreResult<Belief> {
        if let Some(existing) = self.load_all_beliefs().await?.into_iter().find(|existing| {
            existing.scope == belief.scope
                && existing.subject.key == belief.subject.key
                && existing.valid_from == belief.valid_from
        }) {
            belief.id = existing.id;
        }
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn get_belief(&self, query: BeliefQuery) -> CoreResult<Option<Belief>> {
        if query.requires_record_time_history() {
            return Err(CoreError::InvalidRequest {
                reason: "record-time belief history is not supported by engram-store-surreal"
                    .to_owned(),
            });
        }
        let now = Utc::now();
        let mut matches = self
            .load_all_beliefs()
            .await?
            .into_iter()
            .filter(|belief| scope_allows(&belief.scope, &query.scope))
            .filter(|belief| query.matches_after_scope(belief, now))
            .collect::<Vec<_>>();
        sort_beliefs_for_query(&mut matches, query.order);
        Ok(matches.into_iter().next())
    }

    async fn get_belief_by_id(
        &self,
        id: &BeliefId,
        scope: &Scope,
    ) -> CoreResult<Option<Belief>> {
        Ok(self
            .load_belief_by_id(id)
            .await?
            .filter(|belief| scope_allows(&belief.scope, scope)))
    }

    async fn mark_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope).await?;
        let belief = mark_stale(belief, at);
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn clear_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope).await?;
        let belief = clear_stale_state(belief, at);
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn supersede_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        replacement_id: BeliefId,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope).await?;
        let belief = supersede_belief(belief, replacement_id, at);
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn retract_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        let belief = self.visible_belief_or_not_found(id, scope).await?;
        let belief = retract_belief(belief, at);
        self.write_belief(&belief).await?;
        Ok(belief)
    }

    async fn list_stale(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        Ok(self
            .load_all_beliefs()
            .await?
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
            .load_all_beliefs()
            .await?
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
                .load_all_contradictions()
                .await?
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
        self.write_contradiction(&contradiction).await?;
        Ok(contradiction)
    }

    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        Ok(self
            .load_contradiction_by_id(id)
            .await?
            .filter(|c| scope_allows(&c.scope, scope)))
    }

    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        let mut contradiction = self
            .load_contradiction_by_id(id)
            .await?
            .filter(|c| scope_allows(&c.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "contradiction",
                target_id: id.to_string(),
            })?;
        contradiction.status = status_for_resolution(&resolution.kind);
        contradiction.updated_at = Some(resolution.resolved_at);
        contradiction.resolution = Some(resolution);
        self.write_contradiction(&contradiction).await?;
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
