//! Belief capability fixture.
//!
//! Exercises the valid-time vs record-time distinction: a valid-time query
//! returns the belief active at that time, while a record-time history query
//! is rejected with `CoreError::InvalidRequest` (temporal history unsupported).

use chrono::{TimeZone, Utc};
use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlBeliefStore;
use futures::executor::block_on;

use super::support::{policy, provenance, scope};

/// Runs the belief capability fixture.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if valid-time lookup is wrong, or if a
/// record-time query is accepted instead of rejected.
pub fn run_belief_fixture() -> CoreResult<()> {
    let store = SqlBeliefStore::open_in_memory()?;

    let mut old = belief("belief-old", "svc-a", "old", 0.7);
    old.valid_from = Some(ts(10));
    old.valid_until = Some(ts(20));
    old.created_at = ts(10);
    let mut current = belief("belief-current", "svc-a", "current", 0.9);
    current.valid_from = Some(ts(20));
    current.created_at = ts(20);

    block_on(store.put_belief(old)).map_err(err("put_belief"))?;
    block_on(store.put_belief(current)).map_err(err("put_belief"))?;

    // Valid-time query during the old window returns the old belief.
    let during_old = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(15),
    )))
    .map_err(err("get_belief(valid-time)"))?
    .ok_or_else(|| {
        err("get_belief(valid-time)")(CoreError::NotFound {
            target_type: "belief",
            target_id: "svc-a@15".to_string(),
        })
    })?;
    if during_old.id != Id::from("belief-old") {
        return Err(err("valid_time")(CoreError::Conflict {
            reason: format!("expected belief-old at t=15, got {}", during_old.id),
        }));
    }

    // Valid-time query after the switchover returns the current belief.
    let during_current = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(40),
    )))
    .map_err(err("get_belief(valid-time)"))?
    .ok_or_else(|| {
        err("get_belief(valid-time)")(CoreError::NotFound {
            target_type: "belief",
            target_id: "svc-a@40".to_string(),
        })
    })?;
    if during_current.id != Id::from("belief-current") {
        return Err(err("valid_time")(CoreError::Conflict {
            reason: format!("expected belief-current at t=40, got {}", during_current.id),
        }));
    }

    // Record-time history is unsupported and must be rejected.
    let mut record_time_query = BeliefQuery::live_subject(scope("tenant-a"), "svc-a", ts(40));
    record_time_query.recorded_at = Some(ts(25));
    let record_time_result = block_on(store.get_belief(record_time_query));
    if record_time_result.is_ok() {
        return Err(err("record_time")(CoreError::Conflict {
            reason: "record-time history query was accepted instead of rejected".to_string(),
        }));
    }
    Ok(())
}

fn ts(seconds: i64) -> Timestamp {
    Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
}

fn belief(id: &str, key: &str, content: &str, confidence: f32) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope("tenant-a"),
        subject: BeliefSubject {
            key: key.to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: content.to_owned(),
        status: BeliefStatus::Active,
        confidence,
        sources: Vec::new(),
        valid_from: None,
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.belief".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belief_fixture_passes() {
        if let Err(e) = run_belief_fixture() {
            panic!("belief fixture failed: {e}");
        }
    }
}
