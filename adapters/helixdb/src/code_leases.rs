//! Durable exact read leases; retention is protection, never deletion authority.
use crate::{
    HelixStore,
    code_binding::{AttemptRow, changed, increment, tuple},
    records::*,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::*;
use serde_json::json;
use std::collections::BTreeSet;
/// Exact strict liveness, shared by the native retirement fence and its boundary proof.
pub(crate) fn live_lease_at(now: Timestamp) -> helix_db::dsl::prelude::SourcePredicate {
    use helix_db::dsl::prelude::SourcePredicate as P;
    P::or(vec![
        P::gt("expires_secs", now.timestamp()),
        P::and(vec![
            P::eq("expires_secs", now.timestamp()),
            P::gt("expires_nanos", i64::from(now.timestamp_subsec_nanos())),
        ]),
    ])
}
impl HelixStore {
    fn code_lease_claims(&self, lease: &CodeReadLease) -> CoreResult<()> {
        for g in &lease.eligibility.generations {
            let id = tuple(&[&lease.id, g]);
            self.code_put(CODE_LEASE_COVERAGE,&id,&lease.id,json!({"leaseId":lease.id,"generationId":g,"expires_ms":lease.expires_at.timestamp_millis(),"expires_secs":lease.expires_at.timestamp(),"expires_nanos":lease.expires_at.timestamp_subsec_nanos()}))?;
        }
        Ok(())
    }
}
#[async_trait]
impl CodeReadLeases for HelixStore {
    async fn acquire_read_lease(
        &self,
        l: CodeReadLease,
        now: Timestamp,
    ) -> CoreResult<CodeReadLease> {
        self.check_scope(&l.eligibility.scope).map_err(core)?;
        if l.id.is_empty() || l.fencing_token != 1 || l.expires_at <= now {
            return Err(changed());
        }
        self.code_write(|s|{if s.code_eligibility()?!=l.eligibility{return Err(changed());}if s.code_get::<String>(CODE_LEASE_TOMBSTONE,&l.id)?.is_some(){return Err(changed());}if let Some(old)=s.code_get::<CodeReadLease>(CODE_LEASE,&l.id)?{if old!=l{return Err(changed());}return Ok(old);}s.code_put(CODE_LEASE,&l.id,&l,json!({"expires_ms":l.expires_at.timestamp_millis(),"expires_secs":l.expires_at.timestamp(),"expires_nanos":l.expires_at.timestamp_subsec_nanos(),"fence_key":l.fencing_token.to_string()}))?;s.code_lease_claims(&l)?;Ok(l.clone())})
    }
    async fn renew_read_lease(
        &self,
        l: &CodeReadLease,
        now: Timestamp,
        expires: Timestamp,
    ) -> CoreResult<CodeReadLease> {
        self.check_scope(&l.eligibility.scope).map_err(core)?;
        if l.expires_at <= now || expires <= l.expires_at || expires <= now {
            return Err(changed());
        }
        self.code_write(|s|{if s.code_get::<CodeReadLease>(CODE_LEASE,&l.id)?.as_ref()!=Some(l){return Err(changed());}let mut r=l.clone();r.expires_at=expires;r.fencing_token=increment(r.fencing_token)?;s.code_put(CODE_LEASE,&l.id,&r,json!({"expires_ms":expires.timestamp_millis(),"expires_secs":expires.timestamp(),"expires_nanos":expires.timestamp_subsec_nanos(),"fence_key":r.fencing_token.to_string()}))?;s.code_lease_claims(&r)?;Ok(r)})
    }
    async fn release_read_lease(&self, l: &CodeReadLease) -> CoreResult<()> {
        self.check_scope(&l.eligibility.scope).map_err(core)?;
        self.code_write(|s| {
            if s.code_get::<CodeReadLease>(CODE_LEASE, &l.id)?.as_ref() != Some(l) {
                return Err(changed());
            }
            s.stage(CODE_LEASE, &l.id, None).map_err(core)?;
            for g in &l.eligibility.generations {
                s.stage(CODE_LEASE_COVERAGE, &tuple(&[&l.id, g]), None)
                    .map_err(core)?;
            }
            s.code_put(CODE_LEASE_TOMBSTONE, &l.id, &l.id, json!({}))?;
            Ok(())
        })
    }
    async fn validate_historical_read_lease(
        &self,
        l: &CodeReadLease,
        scope: &Scope,
        repo: &str,
        g: &str,
        now: Timestamp,
    ) -> CoreResult<bool> {
        if !self.visible_scope(scope)
            || l.eligibility.scope != *scope
            || l.expires_at <= now
            || !l.eligibility.repository_ids.iter().any(|r| r == repo)
            || !l.eligibility.generations.iter().any(|v| v == g)
        {
            return Ok(false);
        }
        Ok(self.code_get::<CodeReadLease>(CODE_LEASE, &l.id)?.as_ref() == Some(l))
    }
    async fn leased_generations(&self, scope: &Scope, now: Timestamp) -> CoreResult<Vec<String>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .code_all::<CodeReadLease>(CODE_LEASE)?
            .into_iter()
            .filter(|l| l.expires_at > now)
            .flat_map(|l| l.eligibility.generations)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
    async fn retained_generations(
        &self,
        scope: &Scope,
        now: Timestamp,
        grace: u64,
    ) -> CoreResult<Vec<String>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let grace = i64::try_from(grace).map_err(|_| changed())?;
        let cutoff = now
            .checked_sub_signed(chrono::Duration::milliseconds(grace))
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let mut retained = self
            .code_eligibility()?
            .generations
            .into_iter()
            .collect::<BTreeSet<_>>();
        let rows = self.code_all::<AttemptRow>(CODE_ATTEMPT)?;
        let bindings = self.code_all::<CodeBinding>(CODE_BINDING)?;
        for b in bindings {
            let previous = rows
                .iter()
                .filter(|r| {
                    r.repo == b.repo_id
                        && r.attempt.state == CodeAttemptState::Published
                        && Some(&r.generation.id) != b.active_generation.as_ref()
                })
                .map(|r| r.attempt.expected_epoch)
                .max();
            for r in rows.iter().filter(|r| r.repo == b.repo_id) {
                if matches!(
                    r.attempt.state,
                    CodeAttemptState::Created
                        | CodeAttemptState::Acquiring
                        | CodeAttemptState::Staging
                        | CodeAttemptState::Ready
                        | CodeAttemptState::CleanupPending
                ) || (r.attempt.state == CodeAttemptState::Published
                    && Some(&r.generation.id) != b.active_generation.as_ref()
                    && Some(r.attempt.expected_epoch) == previous)
                {
                    retained.insert(r.generation.id.clone());
                }
            }
        }
        for l in self
            .code_all::<CodeReadLease>(CODE_LEASE)?
            .into_iter()
            .filter(|l| l.expires_at > cutoff)
        {
            retained.extend(l.eligibility.generations);
        }
        Ok(retained.into_iter().collect())
    }
}

#[cfg(all(test, any(feature = "embedded", feature = "service")))]
mod exact_native_lease_controls {
    use super::*;
    use crate::{test_data_fixture::*, transport::Operation};
    use helix_db::dsl::prelude::*;
    fn exercise(mode: Mode) {
        let fixture = Fixture::new(mode);
        let store = &fixture.store;
        for (index, (instant, delta)) in [
            ("2100-01-01T00:00:00.123456700Z", -1),
            ("2100-01-01T00:00:00.123456700Z", 0),
            ("2100-01-01T00:00:00.123456700Z", 1),
            ("2100-01-01T00:00:00.999999999Z", 1),
            ("1970-01-01T00:00:00Z", -1),
            ("1969-12-31T23:59:59.999999999Z", 0),
            ("1969-12-31T23:59:59.999999999Z", 1),
        ]
        .into_iter()
        .enumerate()
        {
            let now: Timestamp = instant.parse().unwrap();
            let expires = now + chrono::Duration::nanoseconds(delta);
            let lease = format!("boundary-{index}");
            let id = tuple(&[&lease, "generation"]);
            let payload = json!({"id":id,"scope":scope(),"record":lease,"leaseId":lease,"generationId":"generation","expires_secs":expires.timestamp(),"expires_nanos":expires.timestamp_subsec_nanos(),"expires_ms":expires.timestamp_millis()});
            let request = store
                .inner
                .lease
                .as_ref()
                .unwrap()
                .guard()
                .var_as_if(
                    "created",
                    BatchCondition::VarMinSize("owner".into(), 1),
                    g().add_n(
                        CODE_LEASE_COVERAGE,
                        crate::records::properties(store, CODE_LEASE_COVERAGE, &id, &payload)
                            .unwrap(),
                    )
                    .value_map(Some(vec!["public_id"])),
                )
                .returning(["created"]);
            let write = Operation::new(QueryRequest::write(request)).unwrap();
            let result = store.inner.transport.execute(&write).unwrap();
            assert_eq!(result["created"].as_array().unwrap().len(), 1);
            let predicate = SourcePredicate::and(vec![
                SourcePredicate::eq("partition", fixture.partition.discriminator().unwrap()),
                SourcePredicate::eq(
                    "key",
                    fixture
                        .partition
                        .record_key(CODE_LEASE_COVERAGE, &id)
                        .unwrap(),
                ),
                live_lease_at(now),
            ]);
            let request = read_batch()
                .var_as(
                    "live",
                    g().n_with_label_where(CODE_LEASE_COVERAGE, predicate)
                        .value_map(Some(vec!["public_id"])),
                )
                .returning(["live"]);
            let result = store
                .inner
                .transport
                .execute(&Operation::new(QueryRequest::read(request)).unwrap())
                .unwrap();
            assert_eq!(
                result["live"].as_array().unwrap().len(),
                usize::from(expires > now),
                "strict native precision boundary {instant} delta={delta}"
            );
            println!(
                "TASK006 {mode:?}: native exact lease predicate now={now} expires={expires} delta_ns={delta} actual_live={}; fixed-time query witness, no wallclock retirement claim",
                expires > now
            );
        }
    }
    #[cfg(feature = "embedded")]
    #[test]
    fn embedded_native_lease_predicate_preserves_nanoseconds() {
        exercise(Mode::Embedded)
    }
    #[cfg(feature = "service")]
    #[test]
    fn service_native_lease_predicate_preserves_nanoseconds() {
        exercise(Mode::Service)
    }
}
