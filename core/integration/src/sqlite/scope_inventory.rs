//! SQLite-side wiring of the engine-neutral [`ScopeInventory`] port.
//!
//! The adapter crate (`engram-store-sqlite`) cannot depend on the facade, so
//! the capability's reads live there as inherent methods on
//! [`SqlScopeInventory`] (over the same store handles the provider wires),
//! and this module supplies the port trait implementation. Merge, canonical
//! order, page validation, and continuation offsets come from the port module
//! itself — one shared truth for both engine recipes (RISK-002). The provider
//! bootstrap attaches the handle when the scope-inventory conformance check
//! passes.

use async_trait::async_trait;
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, MemoryRecord, Scope};
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlScopeInventory;

use crate::scope_inventory::{
    PartitionPage, PartitionRecords, ScopeInventory, merge_partition_counts, next_page_offset,
    validate_recent_limit,
};

#[async_trait]
impl ScopeInventory for SqlScopeInventory {
    async fn inventory(&self) -> CoreResult<Vec<crate::scope_inventory::PartitionSummary>> {
        let memories = self.count_memory_partitions()?;
        let entities = self.count_entity_partitions()?;
        let beliefs = self.count_belief_partitions()?;
        Ok(merge_partition_counts(
            &memories
                .into_iter()
                .map(|r| (r.scope, r.count))
                .collect::<Vec<_>>(),
            &entities
                .into_iter()
                .map(|r| (r.scope, r.count))
                .collect::<Vec<_>>(),
            &beliefs
                .into_iter()
                .map(|r| (r.scope, r.count))
                .collect::<Vec<_>>(),
        ))
    }

    async fn partition_memories(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<MemoryRecord>> {
        page.validate()?;
        let items =
            self.list_partition_memories(partition, page.limit as i64, page.offset as i64)?;
        let next_offset = next_page_offset(&items, &page);
        Ok(PartitionRecords { items, next_offset })
    }

    async fn partition_entities(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<KnowledgeEntity>> {
        page.validate()?;
        let items =
            self.list_partition_entities(partition, page.limit as i64, page.offset as i64)?;
        let next_offset = next_page_offset(&items, &page);
        Ok(PartitionRecords { items, next_offset })
    }

    async fn partition_relationships(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<KnowledgeRelationship>> {
        page.validate()?;
        let items =
            self.list_partition_relationships(partition, page.limit as i64, page.offset as i64)?;
        let next_offset = next_page_offset(&items, &page);
        Ok(PartitionRecords { items, next_offset })
    }

    async fn partition_recent_memories(
        &self,
        partition: &Scope,
        limit: usize,
    ) -> CoreResult<Vec<MemoryRecord>> {
        validate_recent_limit(limit)?;
        Ok(self.list_recent_partition_memories(partition, limit as i64)?)
    }
}
#[cfg(all(test, feature = "sqlite"))]
mod scope_inventory_trait_tests {
    //! Trait-level proofs over the real in-memory SQLite stores: the port's
    //! merge (zero-fill), canonical ordering, continuation offsets, and
    //! exact-partition reads through [`ScopeInventory`] itself.

    use super::*;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, Belief, BeliefId, BeliefStatus, BeliefSubject, Id,
        MemoryContent, MemoryKind, Policy, Provenance, Requester, Retention, Visibility,
        WriteMemoryRequest,
    };
    use engram_memory::MemoryService;
    use engram_store_sqlite::{SqlBeliefStore, SqlKnowledgeStore, SqlMemoryService};
    use futures::executor::block_on;
    use std::sync::Arc;

    fn scope(tenant: &str, workspace: Option<&str>) -> Scope {
        Scope {
            tenant: tenant.to_string(),
            subject: None,
            workspace: workspace.map(str::to_string),
            session: None,
            environment: None,
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("scope-inventory-trait-test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn write_req(text: &str, scope: Scope) -> WriteMemoryRequest {
        let now = chrono::Utc::now();
        WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: text.to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope,
            requester: Requester {
                actor: actor(),
                roles: Vec::new(),
                permissions: vec!["memory.write".to_owned()],
                on_behalf_of: None,
            },
            provenance: Provenance {
                source: "scope-inventory-trait-test".to_owned(),
                actor: actor(),
                observed_at: now,
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Private,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            links: Vec::new(),
            verdict: None,
            idempotency_key: None,
        }
    }

    fn belief(id: &str, scope: Scope) -> Belief {
        Belief {
            id: BeliefId::new(id).expect("belief id"),
            scope,
            subject: BeliefSubject {
                key: format!("belief-{id}"),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            content: format!("belief {id}"),
            status: BeliefStatus::Active,
            confidence: 0.9,
            sources: Vec::new(),
            valid_from: None,
            valid_until: None,
            superseded_by: None,
            stale: None,
            synthesizer: None,
            reasoning: None,
            embedding_refs: Vec::new(),
            policy: Policy {
                visibility: Visibility::Private,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "scope-inventory-trait-test".to_owned(),
                actor: actor(),
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    fn inventory() -> SqlScopeInventory {
        let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory store"));
        let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge store"));
        let belief_store = Arc::new(SqlBeliefStore::open_in_memory().expect("belief store"));
        block_on(async {
            memory
                .write_memory(write_req("m-z", scope("acme", Some("proj-z"))))
                .await
                .expect("write");
            memory
                .write_memory(write_req("m-unset", scope("acme", None)))
                .await
                .expect("write");
            memory
                .write_memory(write_req("m-b", scope("acme", Some("proj-a"))))
                .await
                .expect("write");
        });
        // No entity rows on purpose: the "(unset)" zero-fill is the point.
        let _ = &knowledge;
        block_on(async {
            use engram_belief::BeliefRepository;
            belief_store
                .put_belief(belief("bel-1", scope("acme", Some("proj-a"))))
                .await
                .expect("put belief");
        });
        SqlScopeInventory::new(memory, knowledge, belief_store)
    }

    #[test]
    fn scope_inventory_trait_merges_orders_and_zero_fills() {
        let inv = inventory();
        let summaries = block_on(async { inv.inventory().await }).expect("inventory");
        let order: Vec<Option<String>> = summaries
            .iter()
            .map(|s| s.scope.workspace.clone())
            .collect();
        assert_eq!(
            order,
            vec![None, Some("proj-a".to_owned()), Some("proj-z".to_owned())],
            "canonical order puts '(unset)' under its label among the workspaces"
        );
        let unset = summaries.first().expect("unset row");
        assert_eq!(unset.memory_count, 1);
        assert_eq!(unset.entity_count, 0, "zero-filled kind");
        assert_eq!(unset.belief_count, 0);
        let proj_a = &summaries[1];
        assert_eq!(proj_a.memory_count, 1);
        assert_eq!(proj_a.belief_count, 1);
        assert_eq!(proj_a.total_count(), 2);
    }

    #[test]
    fn scope_inventory_trait_pages_with_continuation() {
        let inv = inventory();
        let partition = scope("acme", Some("proj-a"));
        let page = block_on(async {
            inv.partition_memories(
                &partition,
                PartitionPage {
                    limit: 1,
                    offset: 0,
                },
            )
            .await
        })
        .expect("page");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.next_offset, Some(1), "full page continues");
        let next = block_on(async {
            inv.partition_memories(
                &partition,
                PartitionPage {
                    limit: 5,
                    offset: 1,
                },
            )
            .await
        })
        .expect("next");
        assert!(next.items.is_empty());
        assert_eq!(next.next_offset, None, "short page ends the walk");
        let bad = block_on(async {
            inv.partition_memories(
                &partition,
                PartitionPage {
                    limit: 0,
                    offset: 0,
                },
            )
            .await
        });
        assert!(bad.is_err(), "page validation surfaces through the port");
    }

    #[test]
    fn scope_inventory_trait_recent_is_bounded() {
        let inv = inventory();
        let partition = scope("acme", None);
        let recent =
            block_on(async { inv.partition_recent_memories(&partition, 5).await }).expect("recent");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content.text, "m-unset");
        let bad = block_on(async { inv.partition_recent_memories(&partition, 0).await });
        assert!(
            bad.is_err(),
            "recent limit validation surfaces through the port"
        );
    }
}
