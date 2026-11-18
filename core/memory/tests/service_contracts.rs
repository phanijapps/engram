//! Deterministic contract vectors for the canonical service, independent of a provider.
use async_trait::async_trait;
use engram_domain::*;
use engram_memory::*;
use engram_runtime::effect::*;
use serde_json::json;
use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    struct Ready;
    impl Wake for Ready {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Ready));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
fn now() -> Timestamp {
    "2026-11-01T12:00:00Z".parse().unwrap()
}
struct Dependencies(AtomicUsize);
impl Clock for Dependencies {
    fn now(&self) -> Timestamp {
        now()
    }
}
impl IdGenerator for Dependencies {
    fn new_id(&self, kind: &'static str) -> Id {
        Id::from(format!("{kind}-{}", self.0.fetch_add(1, Ordering::SeqCst)))
    }
}
impl PolicyAuthorizer for Dependencies {
    fn can_write(&self, requester: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        authorize(requester)
    }
    fn can_retrieve(&self, requester: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        authorize(requester)
    }
    fn can_forget(&self, requester: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        authorize(requester)
    }
}
fn authorize(requester: &Requester) -> CoreResult<()> {
    if requester.actor.id.as_str() == "denied" {
        Err(CoreError::PolicyDenied {
            reason: "test authorization denial".into(),
        })
    } else {
        Ok(())
    }
}
#[derive(Default)]
struct Data {
    records: Vec<MemoryRecord>,
    events: Vec<MemoryEvent>,
    responses: Vec<(Scope, String, WriteMemoryResponse)>,
}
#[derive(Default)]
struct Storage(Mutex<Data>);
#[async_trait]
impl MemoryRepository for Storage {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let mut d = self.0.lock().unwrap();
        d.records.retain(|r| r.id != record.id);
        d.records.push(record.clone());
        Ok(record)
    }
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .records
            .iter()
            .find(|r| r.id == *id && engram_memory::scope::scope_allows(&r.scope, scope))
            .cloned())
    }
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent> {
        self.0.lock().unwrap().events.push(event.clone());
        Ok(event)
    }
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        let mut r = self.get_memory(id, scope).await?.unwrap();
        r.status = status;
        self.put_memory(r).await
    }
}
#[async_trait]
impl MemoryEventRepository for Storage {
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .events
            .iter()
            .find(|e| e.id == *id && engram_memory::scope::scope_allows(&e.scope, scope))
            .cloned())
    }
    async fn list_events_for_memory(
        &self,
        id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Ok(self
            .list_events_for_scope(scope)
            .await?
            .into_iter()
            .filter(|e| e.memory_id.as_ref() == Some(id))
            .collect())
    }
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| engram_memory::scope::scope_allows(&e.scope, scope))
            .cloned()
            .collect())
    }
}
#[async_trait]
impl MemoryStorage for Storage {
    // Deliberately returns an overbroad candidate set: the canonical service must still filter it.
    async fn enumerate(&self, _: &Scope) -> CoreResult<Vec<MemoryRecord>> {
        Ok(self.0.lock().unwrap().records.clone())
    }
    async fn idempotent_response(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .responses
            .iter()
            .find(|(s, k, _)| s == scope && k == key)
            .map(|(_, _, r)| r.clone()))
    }
    async fn commit_write(
        &self,
        key: Option<&str>,
        record: MemoryRecord,
        event: MemoryEvent,
    ) -> ObservedResult<WriteMemoryResponse> {
        let identity = EffectIdentity {
            actor: event.actor.id.to_string(),
            call: event.id.to_string(),
            session: "fixture".into(),
            backend: "fixture".into(),
            mode: "test".into(),
            store: "fixture".into(),
            scope: record.scope.clone(),
            item: record.id.to_string(),
        };
        let mut d = self.0.lock().unwrap();
        let response = if let Some(existing) = key.and_then(|key| {
            d.responses
                .iter()
                .find(|(s, k, _)| s == &record.scope && k == key)
        }) {
            let mut r = existing.2.clone();
            r.deduplicated = Some(true);
            r
        } else {
            let r = WriteMemoryResponse {
                record: record.clone(),
                event: event.clone(),
                deduplicated: Some(false),
            };
            d.records.push(record.clone());
            d.events.push(event);
            if let Some(key) = key {
                d.responses.push((record.scope, key.into(), r.clone()));
            }
            r
        };
        ObservedResult {
            result: Ok(response),
            observation: EffectObservation {
                identity,
                step: Some("write".into()),
                commit_id: None,
                outcome: EffectOutcome::Acknowledged,
                cleanup: CleanupCertainty::Confirmed,
            },
        }
    }
    async fn remove(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool> {
        let mut d = self.0.lock().unwrap();
        let count = d.records.len();
        d.records.retain(|r| r.id != *id || r.scope != *scope);
        Ok(count != d.records.len())
    }
}
fn scope() -> Scope {
    serde_json::from_value(json!({"tenant":"tenant","subject":"subject","workspace":"workspace","session":"session","environment":"test"})).unwrap()
}
fn write(scope: Scope, text: &str) -> WriteMemoryRequest {
    serde_json::from_value(json!({
     "kind":"observation","content":{"text":text},"scope":scope,
     "requester":{"actor":{"id":"allowed","kind":"agent"}},
     "provenance":{"source":"fixture","actor":{"id":"allowed","kind":"agent"},"observedAt":now()},
     "policy":{"visibility":"private","retention":"durable","allowedUses":["retrieval"]}
    }))
    .unwrap()
}
fn retrieve(scope: Scope) -> RetrievalRequest {
    let w = write(scope.clone(), "anchor");
    RetrievalRequest {
        query: "anchor".into(),
        scope,
        requester: w.requester,
        modes: vec![],
        filters: None,
        cues: vec![],
        limit: Some(1),
        budget: None,
        include_explanations: Some(true),
    }
}
fn fixture() -> (GovernedMemoryService, Arc<Storage>) {
    let store = Arc::new(Storage::default());
    let dependencies = Arc::new(Dependencies(AtomicUsize::new(1)));
    (
        GovernedMemoryService::new(
            store.clone(),
            dependencies.clone(),
            dependencies.clone(),
            dependencies,
        ),
        store,
    )
}
fn forget(record: &MemoryRecord, mode: DeleteMode) -> ForgetRequest {
    ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: record.id.to_string(),
        scope: record.scope.clone(),
        requester: write(record.scope.clone(), "unused").requester,
        mode,
        reason: Some("contract".into()),
    }
}

#[test]
fn full_scope_and_policy_denials_precede_scoring_and_budget() {
    run(async {
        let (service, store) = fixture();
        let s = scope();
        let allowed = service
            .write_memory(write(s.clone(), "anchor secondary words"))
            .await
            .unwrap();
        for dimension in ["tenant", "subject", "workspace", "session", "environment"] {
            let mut sibling = serde_json::to_value(&s).unwrap();
            sibling[dimension] = json!("foreign");
            service
                .write_memory(write(serde_json::from_value(sibling).unwrap(), "anchor"))
                .await
                .unwrap();
        }
        let mut denied = write(s.clone(), "anchor");
        denied.requester.actor.id = Id::from("denied");
        assert!(matches!(
            service.write_memory(denied).await,
            Err(CoreError::PolicyDenied { .. })
        ));
        let output = service.retrieve(retrieve(s.clone())).await.unwrap();
        assert_eq!(output.items.len(), 1);
        assert_eq!(output.items[0].target_id, allowed.record.id.to_string());
        let mut request = retrieve(s.clone());
        request.requester.actor.id = Id::from("denied");
        let output = service.retrieve(request).await.unwrap();
        assert!(output.items.is_empty());
        assert_eq!(output.omitted.len(), 1);
        let mut request = forget(&allowed.record, DeleteMode::Delete);
        request.requester.actor.id = Id::from("denied");
        assert!(matches!(
            service.forget(request).await,
            Err(CoreError::PolicyDenied { .. })
        ));
        assert!(
            store
                .get_memory(&allowed.record.id, &s)
                .await
                .unwrap()
                .is_some()
        );
    });
}
#[test]
fn lifecycle_allowed_uses_and_time_filters_do_not_consume_budget() {
    run(async {
        let (service, store) = fixture();
        let s = scope();
        let allowed = service
            .write_memory(write(s.clone(), "anchor with other words"))
            .await
            .unwrap();
        for condition in [
            "expired",
            "denied",
            "redacted",
            "forgotten",
            "archived",
            "old",
        ] {
            let mut record = service
                .write_memory(write(s.clone(), "anchor"))
                .await
                .unwrap()
                .record;
            match condition {
                "expired" => record.policy.expires_at = Some(now() - chrono::Duration::seconds(1)),
                "denied" => record.policy.allowed_uses = vec![AllowedUse::Evaluation],
                "redacted" => record.status = MemoryStatus::Redacted,
                "forgotten" => record.status = MemoryStatus::Forgotten,
                "archived" => record.status = MemoryStatus::Archived,
                "old" => record.created_at = now() - chrono::Duration::days(1),
                _ => unreachable!(),
            }
            store.put_memory(record).await.unwrap();
        }
        let mut request = retrieve(s);
        request.filters = Some(
            serde_json::from_value(json!({"since":now()-chrono::Duration::hours(1)})).unwrap(),
        );
        let output = service.retrieve(request).await.unwrap();
        assert_eq!(output.items.len(), 1);
        assert_eq!(output.items[0].target_id, allowed.record.id.to_string());
        assert!(
            output
                .omitted
                .iter()
                .any(|o| o.reason == OmittedReason::Expired)
        );
        assert!(
            output
                .omitted
                .iter()
                .any(|o| o.reason == OmittedReason::PolicyDenied)
        );
        assert!(
            !output
                .omitted
                .iter()
                .any(|o| o.reason == OmittedReason::BudgetExceeded)
        );
    });
}
#[test]
fn write_retry_uses_one_record_event_and_stored_response_in_each_full_scope() {
    run(async {
        let (service, store) = fixture();
        let original = scope();
        for dimension in ["same", "subject", "session", "environment"] {
            let mut s = serde_json::to_value(&original).unwrap();
            if dimension != "same" {
                s[dimension] = json!("other");
            }
            let s: Scope = serde_json::from_value(s).unwrap();
            let mut request = write(s.clone(), "anchor");
            request.idempotency_key = Some("opaque\u{1f}key".into());
            let first = service.write_memory(request.clone()).await.unwrap();
            request.content.text = "changed retry".into();
            let second = service.write_memory(request).await.unwrap();
            assert_eq!(first.record, second.record);
            assert_eq!(first.event, second.event);
            assert_eq!(second.deduplicated, Some(true));
            assert_eq!(
                store
                    .list_events_for_memory(&first.record.id, &s)
                    .await
                    .unwrap()
                    .len(),
                1
            );
        }
        let d = store.0.lock().unwrap();
        assert_eq!(d.records.len(), 4);
        assert_eq!(d.events.len(), 4);
        assert_eq!(d.responses.len(), 4);
    });
}
#[test]
fn canonical_write_retrieve_forget_preserves_each_lifecycle_and_audit_history() {
    run(async {
        for mode in [
            DeleteMode::Delete,
            DeleteMode::Redact,
            DeleteMode::Tombstone,
            DeleteMode::Archive,
        ] {
            let (service, store) = fixture();
            let written = service
                .write_memory(write(scope(), "anchor Paris"))
                .await
                .unwrap();
            assert_eq!(
                service
                    .retrieve(retrieve(scope()))
                    .await
                    .unwrap()
                    .items
                    .len(),
                1
            );
            let result = service
                .forget(forget(&written.record, mode.clone()))
                .await
                .unwrap();
            assert!(result.event.is_some());
            assert!(
                service
                    .retrieve(retrieve(scope()))
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
            let events = store
                .list_events_for_memory(&written.record.id, &scope())
                .await
                .unwrap();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0], written.event);
            let record = store
                .get_memory(&written.record.id, &scope())
                .await
                .unwrap();
            match mode {
                DeleteMode::Delete => assert!(record.is_none()),
                DeleteMode::Redact => {
                    let r = record.unwrap();
                    assert_eq!(r.status, MemoryStatus::Redacted);
                    assert!(r.content.text.is_empty());
                    assert!(r.content.entities.is_empty());
                }
                DeleteMode::Tombstone => {
                    let r = record.unwrap();
                    assert_eq!(r.status, MemoryStatus::Forgotten);
                    assert!(r.content.text.is_empty());
                }
                DeleteMode::Archive => {
                    let r = record.unwrap();
                    assert_eq!(r.status, MemoryStatus::Archived);
                    assert_eq!(r.content.text, "anchor Paris");
                }
            }
        }
    });
}
