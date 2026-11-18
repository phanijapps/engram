#![cfg(any(feature = "embedded", feature = "service"))]
#[path = "support/data_fixture.rs"]
mod fixture;
use engram_domain::*;
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryStorage};
use engram_runtime::effect::{CleanupCertainty, EffectOutcome};
use fixture::*;
use futures::executor::block_on;
use serde_json::json;

#[cfg(feature = "embedded")]
#[test]
fn embedded_atomic_memory_written_event_and_scoped_response() {
    atomic_contract(Mode::Embedded);
}
#[cfg(feature = "service")]
#[test]
fn service_atomic_memory_written_event_and_scoped_response() {
    atomic_contract(Mode::Service);
}
fn atomic_contract(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let record = memory("atomic-memory");
        let event: MemoryEvent = make(
            "written",
            json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":"atomic-memory","payload":{"idempotencyKey":"user-key"},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:00Z"}),
        );
        let written = f
            .store
            .commit_write(Some("scope-bound-key"), record.clone(), event.clone())
            .await;
        let observation = written.observation;
        assert_eq!(observation.identity.scope, scope());
        assert_eq!(observation.identity.actor, "actor");
        assert_eq!(
            observation.identity.mode,
            match mode {
                #[cfg(feature = "embedded")]
                Mode::Embedded => "embedded",
                #[cfg(feature = "service")]
                Mode::Service => "service",
            }
        );
        assert_eq!(observation.outcome, EffectOutcome::Acknowledged);
        assert_eq!(observation.cleanup, CleanupCertainty::Confirmed);
        let commit = observation.commit_id.unwrap();
        assert_eq!(uuid::Uuid::parse_str(&commit).unwrap().get_version_num(), 4);
        let receipt = f.store.operation_receipt(&commit).unwrap().unwrap();
        assert_eq!(receipt["id"], commit);
        assert_eq!(receipt["mutation_count"], 3);
        assert_eq!(receipt["fingerprint"].as_str().unwrap().len(), 64);
        assert_eq!(written.result.unwrap().record, record);
        assert_eq!(
            f.store.get_memory(&record.id, &scope()).await.unwrap(),
            Some(record.clone())
        );
        assert_eq!(
            f.store
                .list_events_for_memory(&record.id, &scope())
                .await
                .unwrap(),
            vec![event.clone()]
        );
        let mut retry = record.clone();
        retry.id = "unused-new-id".into();
        let mut retry_event = event.clone();
        retry_event.id = "unused-new-event".into();
        retry_event.memory_id = Some(retry.id.clone());
        let duplicate = f
            .store
            .commit_write(Some("scope-bound-key"), retry, retry_event)
            .await
            .result
            .unwrap();
        assert_eq!(duplicate.record, record);
        assert_eq!(duplicate.deduplicated, Some(true));
        assert_eq!(f.store.enumerate(&scope()).await.unwrap().len(), 1);
        assert_eq!(
            f.store
                .list_events_for_memory(&record.id, &scope())
                .await
                .unwrap()
                .len(),
            1
        );
        for dimension in 0..5 {
            let mut wrong = scope();
            match dimension {
                0 => wrong.tenant.push('x'),
                1 => wrong.subject = Some("other".into()),
                2 => wrong.workspace = Some("other".into()),
                3 => wrong.session = Some("other".into()),
                _ => wrong.environment = None,
            }
            assert!(
                f.store
                    .idempotent_response(&wrong, "scope-bound-key")
                    .await
                    .is_err()
            );
        }
        f.reopen();
        let stored = f
            .store
            .idempotent_response(&scope(), "scope-bound-key")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.record, record);
        assert_eq!(stored.event, event);
        assert!(f.store.delete_memory_record(&record.id, &scope()).unwrap());
        assert!(
            f.store
                .get_memory(&record.id, &scope())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            f.store
                .list_events_for_memory(&record.id, &scope())
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            f.store.operation_receipt(&commit).unwrap().unwrap(),
            receipt
        );
    });
}

struct Authorizer;
impl engram_runtime::PolicyAuthorizer for Authorizer {
    fn can_write(&self, r: &Requester, _: &Scope, _: &Policy) -> engram_runtime::CoreResult<()> {
        authorize(r)
    }
    fn can_retrieve(&self, r: &Requester, _: &Scope, _: &Policy) -> engram_runtime::CoreResult<()> {
        authorize(r)
    }
    fn can_forget(&self, r: &Requester, _: &Scope, _: &Policy) -> engram_runtime::CoreResult<()> {
        authorize(r)
    }
}
fn authorize(r: &Requester) -> engram_runtime::CoreResult<()> {
    if r.actor.id.to_string() == "denied" {
        return Err(engram_runtime::CoreError::PolicyDenied {
            reason: "fixture denial".into(),
        });
    }
    Ok(())
}
fn requester() -> Requester {
    serde_json::from_value(json!({"actor":{"id":"actor","kind":"agent"}})).unwrap()
}
fn write_request() -> WriteMemoryRequest {
    serde_json::from_value(json!({"kind":"fact","content":{"text":"canonical governed orbit"},"scope":scope(),"requester":requester(),"provenance":{"source":"fixture","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},"idempotencyKey":"user-service-key"})).unwrap()
}
#[cfg(feature = "embedded")]
#[test]
fn embedded_governed_write_retrieve_forget_and_reopen() {
    governed_contract(Mode::Embedded);
}
#[cfg(feature = "service")]
#[test]
fn service_governed_write_retrieve_forget_and_reopen() {
    governed_contract(Mode::Service);
}
fn governed_contract(mode: Mode) {
    use engram_memory::MemoryService;
    block_on(async {
        let mut f = Fixture::new(mode);
        let service = f.store.memory_service(std::sync::Arc::new(Authorizer));
        let mut denied = write_request();
        denied.requester.actor.id = "denied".into();
        assert!(service.write_memory(denied).await.is_err());
        assert!(f.store.enumerate(&scope()).await.unwrap().is_empty());
        let written = service.write_memory(write_request()).await.unwrap();
        let retry = service.write_memory(write_request()).await.unwrap();
        assert_eq!(written.record.id, retry.record.id);
        assert_eq!(retry.deduplicated, Some(true));
        assert_eq!(
            service
                .list_events_for_memory(&written.record.id, &scope())
                .await
                .unwrap()
                .len(),
            1
        );
        let query:RetrievalRequest=serde_json::from_value(json!({"query":"orbit","scope":scope(),"requester":requester(),"modes":["keyword"],"budget":{"maxItems":1},"includeExplanations":true})).unwrap();
        let context = service.retrieve(query.clone()).await.unwrap();
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].target_id, written.record.id.to_string());
        let mut wrong = query.clone();
        wrong.scope.environment = Some("foreign".into());
        assert!(service.retrieve(wrong).await.unwrap().items.is_empty());
        drop(service);
        f.reopen();
        let service = f.store.memory_service(std::sync::Arc::new(Authorizer));
        assert_eq!(
            service.retrieve(query.clone()).await.unwrap().items.len(),
            1
        );
        let mut new = write_request();
        new.idempotency_key = Some("after-reopen".into());
        let reopened = service.write_memory(new).await.unwrap();
        assert_ne!(reopened.record.id, written.record.id);
        for mode in ["redact", "tombstone", "archive", "delete"] {
            let mut request = write_request();
            request.idempotency_key = Some(format!("forget-{mode}"));
            let target = service.write_memory(request).await.unwrap();
            let forget:ForgetRequest=serde_json::from_value(json!({"targetType":"memory","targetId":target.record.id,"mode":mode,"scope":scope(),"requester":requester(),"reason":"contract"})).unwrap();
            let forgotten = service.forget(forget).await.unwrap();
            assert!(forgotten.event.is_some());
            assert_eq!(
                service
                    .list_events_for_memory(&target.record.id, &scope())
                    .await
                    .unwrap()
                    .len(),
                2
            );
            let record = service
                .get_memory(&target.record.id, &scope())
                .await
                .unwrap();
            match mode {
                "delete" => assert!(record.is_none()),
                "redact" => {
                    let r = record.unwrap();
                    assert_eq!(r.status, MemoryStatus::Redacted);
                    assert!(r.content.text.is_empty());
                }
                "tombstone" => {
                    let r = record.unwrap();
                    assert_eq!(r.status, MemoryStatus::Forgotten);
                    assert!(r.content.text.is_empty());
                }
                _ => assert_eq!(record.unwrap().status, MemoryStatus::Archived),
            }
        }
        drop(service);
    });
}

#[cfg(feature = "embedded")]
#[test]
fn embedded_public_outer_step_is_staged_until_native_commit_or_abandon() {
    staged_public(Mode::Embedded);
}
#[cfg(feature = "service")]
#[test]
fn service_public_outer_step_is_staged_until_native_commit_or_abandon() {
    staged_public(Mode::Service);
}
fn staged_public(mode: Mode) {
    block_on(async {
        let f = Fixture::new(mode);
        let session = f.store.scope_session().unwrap();
        let bound = session.store();
        let step = bound.begin_step().unwrap();
        let mut first_step = None;
        for n in 0..2 {
            let record = memory(&format!("public-staged-{n}"));
            let event: MemoryEvent = make(
                &format!("public-staged-event-{n}"),
                json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":record.id,"payload":{},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:00Z"}),
            );
            let observed = bound
                .commit_write(
                    Some(&format!("public-staged-key-{n}")),
                    record.clone(),
                    event,
                )
                .await;
            assert!(observed.result.is_ok());
            assert_eq!(observed.observation.outcome, EffectOutcome::Staged);
            assert!(observed.observation.step.is_some());
            assert_eq!(observed.observation.commit_id, None);
            if n == 0 {
                first_step = observed.observation.step;
            } else {
                assert_eq!(observed.observation.step, first_step);
            }
            assert!(
                f.store
                    .get_memory(&record.id, &scope())
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        let commit = step.commit().unwrap();
        for n in 0..2 {
            let id: MemoryId = format!("public-staged-{n}").into();
            assert!(f.store.get_memory(&id, &scope()).await.unwrap().is_some());
            assert_eq!(
                f.store
                    .list_events_for_memory(&id, &scope())
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                f.store
                    .idempotent_response(&scope(), &format!("public-staged-key-{n}"))
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        let receipt = f.store.operation_receipt(&commit).unwrap().unwrap();
        assert_eq!(receipt["id"], commit);
        assert_eq!(receipt["mutation_count"], 6);
        let abandoned = bound.begin_step().unwrap();
        let record = memory("public-abandoned");
        let event: MemoryEvent = make(
            "public-abandoned-event",
            json!({"kind":"written","actor":{"id":"actor","kind":"agent"},"memoryId":record.id,"payload":{},"occurredAt":"2026-11-17T00:00:00Z","recordedAt":"2026-11-17T00:00:00Z"}),
        );
        let observed = bound
            .commit_write(Some("public-abandoned-key"), record.clone(), event)
            .await;
        assert_eq!(observed.observation.outcome, EffectOutcome::Staged);
        assert_eq!(observed.observation.commit_id, None);
        drop(abandoned);
        assert!(
            f.store
                .get_memory(&record.id, &scope())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            f.store
                .list_events_for_memory(&record.id, &scope())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            f.store
                .idempotent_response(&scope(), "public-abandoned-key")
                .await
                .unwrap()
                .is_none()
        );
        drop(bound);
        drop(session);
    });
}
