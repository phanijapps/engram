//! Durable belief records; temporal/lifecycle algorithms remain canonical.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_belief::{BeliefQuery, BeliefQueryOrder, BeliefReferenceQuery, BeliefRepository};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
impl HelixStore {
    fn change_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        change: impl FnOnce(Belief) -> Belief,
    ) -> CoreResult<Belief> {
        self.check_scope(scope).map_err(core)?;
        self.domain_atomic(|s| {
            let v = s
                .get::<Belief>(BELIEF, id.as_str())
                .map_err(core)?
                .ok_or_else(|| missing("belief", id))?;
            let v = change(v);
            s.put(BELIEF, &v).map_err(core)
        })
    }
}
#[async_trait]
impl BeliefRepository for HelixStore {
    async fn put_belief(&self, v: Belief) -> CoreResult<Belief> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(BELIEF, &v).map_err(core)
    }
    async fn upsert_belief(&self, mut v: Belief) -> CoreResult<Belief> {
        self.check_scope(&v.scope).map_err(core)?;
        self.domain_atomic(|s| {
            if let Some(old) = s
                .all::<Belief>(BELIEF)
                .map_err(core)?
                .into_iter()
                .find(|old| old.subject.key == v.subject.key && old.valid_from == v.valid_from)
            {
                v.id = old.id;
                v.created_at = old.created_at;
            }
            s.put(BELIEF, &v).map_err(core)
        })
    }
    async fn get_belief(&self, q: BeliefQuery) -> CoreResult<Option<Belief>> {
        if q.requires_record_time_history() {
            return Err(CoreError::InvalidRequest {
                reason: "record-time history is unsupported; valid-time lookup only".into(),
            });
        }
        if !self.visible_scope(&q.scope) {
            return Ok(None);
        }
        let mut rows = self.all::<Belief>(BELIEF).map_err(core)?;
        rows.retain(|v| q.matches_after_scope(v, chrono::Utc::now()));
        rows.sort_by(|a, b| {
            match q.order {
                BeliefQueryOrder::LatestValidFirst => b
                    .valid_from
                    .cmp(&a.valid_from)
                    .then_with(|| b.created_at.cmp(&a.created_at)),
                BeliefQueryOrder::LatestRecordedFirst => b
                    .created_at
                    .cmp(&a.created_at)
                    .then_with(|| b.valid_from.cmp(&a.valid_from)),
            }
            .then_with(|| a.id.cmp(&b.id))
        });
        Ok(rows.into_iter().next())
    }
    async fn get_belief_by_id(&self, id: &BeliefId, scope: &Scope) -> CoreResult<Option<Belief>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(BELIEF, id.as_str()).map_err(core)
    }
    async fn mark_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        self.change_belief(id, scope, |v| engram_belief::mark_stale(v, at))
    }
    async fn clear_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief> {
        self.change_belief(id, scope, |v| engram_belief::clear_stale_state(v, at))
    }
    async fn supersede_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        replacement_id: BeliefId,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        self.change_belief(id, scope, |v| {
            engram_belief::supersede_belief(v, replacement_id, at)
        })
    }
    async fn retract_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        at: Timestamp,
    ) -> CoreResult<Belief> {
        self.change_belief(id, scope, |v| engram_belief::retract_belief(v, at))
    }
    async fn list_stale(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<Belief>(BELIEF)
            .map_err(core)?
            .into_iter()
            .filter(|v| v.stale == Some(true) || v.status == BeliefStatus::Stale)
            .collect())
    }
    async fn list_beliefs(&self, scope: &Scope) -> CoreResult<Vec<Belief>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<Belief>(BELIEF)
            .map_err(core)?
            .into_iter()
            .filter(engram_belief::is_live_belief)
            .collect())
    }
    async fn list_beliefs_paged(
        &self,
        scope: &Scope,
        after: Option<&Cursor>,
        limit: usize,
    ) -> CoreResult<Page<Belief>> {
        if limit == 0 || limit > 500 {
            return Err(CoreError::InvalidRequest {
                reason: "invalid bounded belief page".into(),
            });
        }
        if !self.visible_scope(scope) {
            return Ok(Page::last(vec![]));
        }
        let mut rows = vec![];
        let mut scan_after = after.map(|c| c.as_str().to_owned());
        // Bound each native read; ineligible active/stale rows cannot consume the result limit.
        let window = (limit + 1).min(64);
        'pages: loop {
            let page = self
                .scan_window(
                    BELIEF,
                    Some(("status", "active")),
                    scan_after.as_deref(),
                    Some((0, window)),
                )
                .map_err(core)?;
            let count = page.len();
            for payload in page {
                let record: Belief = decode(payload).map_err(core)?;
                scan_after = Some(record.id.to_string());
                if engram_belief::is_live_belief(&record) {
                    rows.push(record);
                    if rows.len() > limit {
                        break 'pages;
                    }
                }
            }
            if count < window {
                break;
            }
        }
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last().map(|v| Cursor::new(v.id.to_string()))
        } else {
            None
        };
        Ok(Page::new(rows, next))
    }
    async fn list_contradictions(&self, scope: &Scope) -> CoreResult<Vec<Contradiction>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<Contradiction>(CONTRADICTION)
            .map_err(core)?
            .into_iter()
            .filter(|v| v.status == ContradictionStatus::Open)
            .collect())
    }
    async fn beliefs_referencing_source(&self, q: BeliefReferenceQuery) -> CoreResult<Vec<Belief>> {
        if !self.visible_scope(&q.scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<Belief>(BELIEF)
            .map_err(core)?
            .into_iter()
            .filter(|v| {
                engram_belief::belief_references_source(
                    v,
                    &q.source_type,
                    &q.source_id,
                    q.valid_at.unwrap_or_else(chrono::Utc::now),
                )
            })
            .collect())
    }
    async fn put_contradiction(&self, mut v: Contradiction) -> CoreResult<Contradiction> {
        self.check_scope(&v.scope).map_err(core)?;
        self.domain_atomic(|s| {
            for old in s.all::<Contradiction>(CONTRADICTION).map_err(core)? {
                if old.kind == v.kind
                    && old.targets.len() == 2
                    && v.targets.len() == 2
                    && engram_belief::canonical_pair_key(&old.targets[0], &old.targets[1])
                        == engram_belief::canonical_pair_key(&v.targets[0], &v.targets[1])
                {
                    v.id = old.id;
                    break;
                }
            }
            s.put(CONTRADICTION, &v).map_err(core)
        })
    }
    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(CONTRADICTION, id.as_str()).map_err(core)
    }
    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        self.check_scope(scope).map_err(core)?;
        self.domain_atomic(|s| {
            let mut v = s
                .get::<Contradiction>(CONTRADICTION, id.as_str())
                .map_err(core)?
                .ok_or_else(|| missing("contradiction", id))?;
            v.status = if resolution.kind == ContradictionResolutionKind::ManualIgnore {
                ContradictionStatus::Ignored
            } else {
                ContradictionStatus::Resolved
            };
            v.updated_at = Some(resolution.resolved_at);
            v.resolution = Some(resolution);
            s.put(CONTRADICTION, &v).map_err(core)
        })
    }
}
