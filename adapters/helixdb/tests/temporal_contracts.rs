//! Real service and embedded temporal/remaining-port contracts.
#[path = "support/data_fixture.rs"]
mod fixture;
use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
use engram_knowledge::{
    GraphMaintenanceRepository, KnowledgeChangeLog, KnowledgeGraphRepository, KnowledgeRepository,
    ScanCheckpointStore,
};
use engram_procedures::ProcedureRepository;
use engram_proposals::ProposalRepository;
use engram_retrieval::OutcomeLedger;
use fixture::*;
use futures::executor::block_on;
use serde_json::json;
fn at(s: &str) -> Timestamp {
    chrono::DateTime::parse_from_rfc3339(s)
        .unwrap()
        .with_timezone(&chrono::Utc)
}
fn exercise(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let scope = f.partition.scope.clone();
        let mut foreign = scope.clone();
        foreign.subject = Some("foreign".into());
        let t0 = at("2026-11-17T00:00:00Z");
        let t1 = at("2026-11-17T01:00:00Z");
        let b: Belief = make(
            "belief",
            json!({"subject":{"key":"service"},"content":"up","status":"active","confidence":0.8,"validFrom":t0,"validUntil":t1}),
        );
        f.store.put_belief(b.clone()).await.unwrap();
        assert!(
            f.store
                .get_belief(BeliefQuery::live_subject(scope.clone(), "service", t0))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            f.store
                .get_belief(BeliefQuery::live_subject(scope.clone(), "service", t1))
                .await
                .unwrap()
                .is_none()
        );
        let mut history = BeliefQuery::live_subject(scope.clone(), "service", t0);
        history.recorded_at = Some(t0);
        assert!(
            matches!(
                f.store.get_belief(history).await,
                Err(engram_runtime::CoreError::InvalidRequest { .. })
            ),
            "record-time history cannot be faked from current rows"
        );
        assert!(
            f.store
                .get_belief_by_id(&b.id, &foreign)
                .await
                .unwrap()
                .is_none()
        );
        let mut replay = b.clone();
        replay.id = Id::from("new-opaque-id");
        assert_eq!(f.store.upsert_belief(replay).await.unwrap().id, b.id);
        f.store.mark_stale(&b.id, &scope, t0).await.unwrap();
        assert_eq!(f.store.list_stale(&scope).await.unwrap().len(), 1);
        f.store.clear_stale(&b.id, &scope, t0).await.unwrap();
        for id in ["aaa-ineligible-1", "aaa-ineligible-2", "aaa-ineligible-3"] {
            let mut ineligible = b.clone();
            ineligible.id = Id::from(id);
            ineligible.stale = Some(true);
            f.store.put_belief(ineligible).await.unwrap();
        }
        assert_eq!(
            f.store
                .list_beliefs_paged(&scope, None, 1)
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        let b2: Belief = make(
            "belief2",
            json!({"subject":{"key":"other"},"content":"down","status":"active","confidence":0.5}),
        );
        f.store.put_belief(b2.clone()).await.unwrap();
        f.store
            .supersede_belief(&b.id, &scope, b2.id.clone(), t1)
            .await
            .unwrap();
        assert_eq!(
            f.store
                .get_belief_by_id(&b.id, &scope)
                .await
                .unwrap()
                .unwrap()
                .superseded_by,
            Some(b2.id.clone())
        );
        f.store.retract_belief(&b2.id, &scope, t1).await.unwrap();
        let contradiction: Contradiction = make(
            "contradiction",
            json!({"kind":"logical","targets":[{"targetType":"belief","targetId":"belief"},{"targetType":"belief","targetId":"belief2"}],"severity":0.7,"status":"open","detectedAt":t0}),
        );
        f.store
            .put_contradiction(contradiction.clone())
            .await
            .unwrap();
        assert_eq!(f.store.list_contradictions(&scope).await.unwrap().len(), 1);
        let resolution: ContradictionResolution = serde_json::from_value(
            json!({"kind":"compatible","actor":{"id":"reviewer","kind":"user"},"resolvedAt":t1}),
        )
        .unwrap();
        f.store
            .resolve_contradiction(&contradiction.id, &scope, resolution)
            .await
            .unwrap();
        assert!(
            f.store
                .list_contradictions(&scope)
                .await
                .unwrap()
                .is_empty()
        );
        let root: HierarchyNode = make(
            "root",
            json!({"kind":"aggregate","layer":0,"status":"active"}),
        );
        let child: HierarchyNode = make(
            "child",
            json!({"kind":"base","layer":1,"parentId":"root","sourceTargetId":"seed","status":"active"}),
        );
        f.store.put_node(root.clone()).await.unwrap();
        f.store.put_node(child.clone()).await.unwrap();
        let rel: HierarchyRelation = make(
            "hierarchy-edge",
            json!({"sourceId":"child","targetId":"root","predicate":"part_of"}),
        );
        f.store.put_relation(rel).await.unwrap();
        assert_eq!(
            f.store
                .path_for(&["seed".into()], &scope, None)
                .await
                .unwrap()
                .nodes
                .len(),
            2
        );
        assert!(
            f.store
                .path_for(&["seed".into()], &foreign, None)
                .await
                .unwrap()
                .nodes
                .is_empty()
        );
        let mut cycle = root.clone();
        cycle.parent_id = Some(child.id.clone());
        assert!(f.store.put_node(cycle).await.is_err());
        let procedure: Procedure = make("procedure", json!({"name":"runbook","steps":["check"]}));
        f.store.upsert_procedure(procedure.clone()).await.unwrap();
        f.store
            .increment_success(&procedure.id, &scope)
            .await
            .unwrap();
        f.store
            .increment_failure(&procedure.id, &scope)
            .await
            .unwrap();
        let stats = f.store.procedure_stats(&scope).await.unwrap();
        assert_eq!(
            (stats.total, stats.total_success, stats.total_failure),
            (1, 1, 1)
        );
        assert!(
            f.store
                .get_procedure_by_name("runbook", &scope)
                .await
                .unwrap()
                .is_some()
        );
        let payload:ProposalMemoryWrite=serde_json::from_value(json!({"kind":"fact","content":{"text":"proposal never enters recall"},"policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}})).unwrap();
        let proposal: OperatorProposal = make(
            "proposal",
            json!({"payload":payload,"payloadFingerprint":engram_proposals::payload_fingerprint(&payload),"status":"pending","version":1,"submittedBy":{"id":"submitter","kind":"agent"}}),
        );
        f.store.insert_proposal(proposal.clone()).await.unwrap();
        assert_eq!(
            f.store.insert_proposal(proposal.clone()).await.unwrap(),
            proposal
        );
        let held = f.store.scope_session().unwrap();
        assert!(
            matches!(
                f.store.claim_approval(&proposal.id, &scope, 1).await,
                Err(engram_runtime::CoreError::Conflict { .. })
            ),
            "reservation rejection preserves typed no-effect claim conflict"
        );
        drop(held);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = vec![];
        for _ in 0..2 {
            let store = f.store.clone();
            let id = proposal.id.clone();
            let scope = scope.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                block_on(store.claim_approval(&id, &scope, 1))
            }));
        }
        barrier.wait();
        let claims = workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            claims.iter().filter(|v| v.is_ok()).count(),
            1,
            "exactly one concurrent claim commits"
        );
        assert_eq!(
            claims
                .iter()
                .filter(|v| matches!(v, Err(engram_runtime::CoreError::Conflict { .. })))
                .count(),
            1,
            "loser is typed conflict, not an adapter outage"
        );
        let claim = claims.into_iter().find_map(Result::ok).unwrap();
        assert!(matches!(
            f.store.claim_approval(&proposal.id, &scope, 1).await,
            Err(engram_runtime::CoreError::Conflict { .. })
        ));
        let promotion:ProposalPromotion=serde_json::from_value(json!({"memoryId":"memory","eventId":"event","idempotencyKey":proposal.promotion_idempotency_key(),"appliedAt":t1})).unwrap();
        f.store
            .record_promotion(&proposal.id, &scope, claim.version, promotion.clone())
            .await
            .unwrap();
        assert_eq!(
            f.store
                .record_promotion(&proposal.id, &scope, 0, promotion)
                .await
                .unwrap()
                .status,
            OperatorProposalStatus::Approved
        );
        assert_eq!(
            f.store
                .list_proposals(&scope, None, None, 10)
                .await
                .unwrap()
                .len(),
            1
        );
        let outcome:ReportOutcomeRequest=serde_json::from_value(json!({"queryId":"query","scope":scope,"recalledTargets":["belief"],"outcome":{"success":true,"verifier":{"id":"host","kind":"system"}},"occurredAt":t0})).unwrap();
        f.store.report_outcome(outcome).await.unwrap();
        assert_eq!(
            f.store
                .list_outcomes(&OutcomeWindow {
                    scope: scope.clone(),
                    since: t0,
                    until: t1
                })
                .await
                .unwrap()
                .len(),
            1
        );
        f.store
            .put_graph(make("graph", json!({"name":"graph"})))
            .await
            .unwrap();
        for id in ["left", "right", "dupe"] {
            f.store.put_entity(entity(id, &["source"])).await.unwrap();
        }
        f.store
            .put_relationship(relation("edge", &["source"]))
            .await
            .unwrap();
        {
            let session = f.store.scope_session().unwrap();
            let staged = session.store();
            let step = staged.begin_step().unwrap();
            staged
                .put_entity(entity("left", &["source", "source-extra"]))
                .await
                .unwrap();
            let id = step.commit().unwrap();
            let receipt = staged.operation_receipt(&id).unwrap().unwrap();
            assert_eq!(receipt["mutation_count"], 1, "one explicit entity effect");
            assert_eq!(
                receipt["auxiliary_mutation_count"], 2,
                "one durable change plus sequence-state effect"
            );
            assert_eq!(
                receipt["fingerprint"].as_str().unwrap().len(),
                64,
                "full physical-map fingerprint retained"
            );
        }
        let request = KnowledgeChangesRequest {
            scope: scope.clone(),
            source_key: None,
            after: None,
            limit: 2,
        };
        let changes = f.store.changes_since(&request).await.unwrap();
        assert_eq!(changes.items.len(), 2);
        assert!(changes.has_more);
        let mut next = request.clone();
        next.after = changes.next_cursor.clone();
        assert!(!f.store.changes_since(&next).await.unwrap().items.is_empty());
        let mut wrong = next.clone();
        wrong.source_key = Some("other-filter".into());
        assert!(f.store.changes_since(&wrong).await.unwrap().reset_required);
        let checkpoint:ScanCheckpoint=serde_json::from_value(json!({"scope":scope,"stableSourceKey":"repo@main","scanRunId":"run2","startedAt":t1,"status":"completed","indexedRevision":"a".repeat(40),"errors":0})).unwrap();
        f.store.put_checkpoint(checkpoint.clone()).await.unwrap();
        let mut stale = checkpoint.clone();
        stale.started_at = t0;
        stale.scan_run_id = "old".into();
        assert_eq!(f.store.put_checkpoint(stale).await.unwrap(), checkpoint);
        assert_eq!(
            f.store.latest_checkpoints(&scope).await.unwrap(),
            vec![checkpoint.clone()]
        );
        let actor = procedure.provenance.actor.clone();
        let plan = f
            .store
            .build_plan(MaintenancePlanRequest {
                graph_id: Some(Id::from("graph")),
                scope: scope.clone(),
                mutations: vec![MaintenanceMutation::Archive {
                    target: MaintenanceTarget::Entity(Id::from("left")),
                }],
                policy: MaintenancePolicy::default(),
                actor: actor.clone(),
            })
            .await
            .unwrap();
        assert_eq!(plan.previews.len(), 1);
        assert_eq!(
            f.store
                .apply_plan(&plan, ApplyMode::Preview)
                .await
                .unwrap()
                .applied,
            0
        );
        assert!(
            f.store
                .get_entity(&Id::from("left"), &scope)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            f.store
                .apply_plan(&plan, ApplyMode::Apply)
                .await
                .unwrap()
                .applied,
            1
        );
        assert_eq!(
            f.store
                .apply_plan(&plan, ApplyMode::Apply)
                .await
                .unwrap()
                .unchanged,
            1
        );
        let plan_for = |mutations| {
            MaintenancePlan::new(
                None,
                scope.clone(),
                mutations,
                MaintenancePolicy::default(),
                actor.clone(),
            )
        };
        let restore = plan_for(vec![MaintenanceMutation::Restore {
            target: MaintenanceTarget::Entity(Id::from("left")),
        }]);
        f.store
            .apply_plan(&restore, ApplyMode::Apply)
            .await
            .unwrap();
        let bad_delete = plan_for(vec![MaintenanceMutation::Delete {
            target: MaintenanceTarget::Entity(Id::from("right")),
        }]);
        assert!(
            f.store
                .apply_plan(&bad_delete, ApplyMode::Apply)
                .await
                .unwrap()
                .failed
                > 0
        );
        assert!(
            f.store
                .get_entity(&Id::from("right"), &scope)
                .await
                .unwrap()
                .is_some(),
            "verification rollback must preserve referenced entity"
        );
        let edit = plan_for(vec![
            MaintenanceMutation::AddAlias {
                entity: Id::from("left"),
                alias: "alias".into(),
            },
            MaintenanceMutation::RewriteRelationship {
                relationship: Id::from("edge"),
                new_predicate: Some("changed".into()),
                new_subject: None,
                new_object: None,
            },
            MaintenanceMutation::Merge {
                survivor: Id::from("left"),
                absorbed: vec![Id::from("dupe")],
            },
        ]);
        assert_eq!(
            f.store
                .apply_plan(&edit, ApplyMode::Apply)
                .await
                .unwrap()
                .applied,
            3
        );
        assert_eq!(
            f.store
                .apply_plan(&edit, ApplyMode::Apply)
                .await
                .unwrap()
                .unchanged,
            3
        );
        let remove_alias = plan_for(vec![MaintenanceMutation::RemoveAlias {
            entity: Id::from("left"),
            alias: "alias".into(),
        }]);
        f.store
            .apply_plan(&remove_alias, ApplyMode::Apply)
            .await
            .unwrap();
        assert!(
            f.store
                .detect_candidates(
                    &scope,
                    Some(&Id::from("graph")),
                    &MaintenancePolicy::default()
                )
                .await
                .unwrap()
                .is_empty(),
            "the current source-grounded connected graph has no repair candidates"
        );
        f.store
            .put_entity(entity("orphan", &["source"]))
            .await
            .unwrap();
        let candidates = f
            .store
            .detect_candidates(
                &scope,
                Some(&Id::from("graph")),
                &MaintenancePolicy::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            candidates.len(),
            1,
            "only the explicitly seeded orphan is a repair candidate"
        );
        assert_eq!(candidates[0].kind, CandidateKind::Orphan);
        assert_eq!(
            candidates[0].target,
            MaintenanceTarget::Entity(Id::from("orphan"))
        );
        assert_eq!(
            f.store
                .list_entities(&scope, &EntityFilter::default(), None, 20)
                .await
                .unwrap()
                .items
                .len(),
            3,
            "two connected active entities plus the seeded orphan"
        );
        assert_eq!(
            f.store
                .graph_health(&scope, None)
                .await
                .unwrap()
                .orphan_count,
            1,
            "the one explicit orphan is counted independently of the archived merged entity"
        );
        assert_eq!(
            f.store
                .list_relationships(&scope, &RelationshipFilter::default(), None, 20)
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            f.store
                .graph_health(&scope, None)
                .await
                .unwrap()
                .archived_entity_count,
            1
        );
        let delete_relationship = plan_for(vec![MaintenanceMutation::Delete {
            target: MaintenanceTarget::Relationship(Id::from("edge")),
        }]);
        assert_eq!(
            f.store
                .apply_plan(&delete_relationship, ApplyMode::Apply)
                .await
                .unwrap()
                .applied,
            1,
            "successful relationship deletion"
        );
        assert_eq!(
            f.store
                .apply_plan(&delete_relationship, ApplyMode::Apply)
                .await
                .unwrap()
                .unchanged,
            1,
            "repeated relationship deletion is inert"
        );
        assert_eq!(
            f.store
                .apply_plan(&bad_delete, ApplyMode::Apply)
                .await
                .unwrap()
                .applied,
            1,
            "entity deletion succeeds after its incident relationship is removed"
        );
        assert_eq!(
            f.store
                .apply_plan(&bad_delete, ApplyMode::Apply)
                .await
                .unwrap()
                .unchanged,
            1,
            "repeated entity deletion is inert"
        );
        assert!(
            f.store
                .get_entity(&Id::from("right"), &scope)
                .await
                .unwrap()
                .is_none()
        );
        f.reopen();
        assert_eq!(
            f.store
                .get_procedure(&procedure.id, &scope)
                .await
                .unwrap()
                .unwrap()
                .success_count,
            1
        );
        assert_eq!(
            f.store
                .latest_checkpoint(&scope, "repo@main")
                .await
                .unwrap(),
            Some(checkpoint)
        );
        assert!(
            f.store
                .delete_procedure(&procedure.id, &scope)
                .await
                .unwrap()
        );
        assert!(
            !f.store
                .delete_procedure(&procedure.id, &scope)
                .await
                .unwrap()
        );
        println!(
            "TASK005 temporal mode={mode:?}: valid-time/history/scope/proposalCAS/outcome/change/checkpoint/all-maintenance persistence verified"
        );
    });
}
#[cfg(feature = "embedded")]
#[test]
fn temporal_embedded() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn temporal_service() {
    exercise(Mode::Service)
}
