//! Scope matching and SQL scope binding helpers.
//!
//! SQL rows keep scope columns for filtering, while JSON payloads preserve the
//! full accepted contract record. The visibility rule mirrors the in-memory
//! adapter: tenant must match, and optional request fields narrow visibility.

use engram_domain::Scope;

/// Returns true when a stored SQL row is visible to a request scope.
///
/// Tenant must match exactly. Optional request scope fields narrow visibility
/// without requiring every scope dimension to be supplied.
pub(crate) fn scope_allows(record_scope: &Scope, request_scope: &Scope) -> bool {
    record_scope.tenant == request_scope.tenant
        && optional_scope_matches(&record_scope.subject, &request_scope.subject)
        && optional_scope_matches(&record_scope.workspace, &request_scope.workspace)
        && optional_scope_matches(&record_scope.session, &request_scope.session)
        && optional_scope_matches(&record_scope.environment, &request_scope.environment)
}

fn optional_scope_matches(record_value: &Option<String>, request_value: &Option<String>) -> bool {
    request_value
        .as_ref()
        .is_none_or(|value| record_value.as_ref() == Some(value))
}

// ============================================================================
// Scope inventory (job 012-admin-console-v1, STEP-002)
//
// The scope-inventory port trait lives in `engram-integration`
// (`ScopeInventory`); this crate cannot depend on the facade, so the
// capability is implemented here as inherent methods over the three governed
// stores, and the trait impl + provider wiring live in
// `core/integration/src/sqlite/scope_inventory.rs`. The unified partition
// semantics are fixed by the port: NULL dimensions are their own distinct
// value (SQLite `GROUP BY` groups NULLs together — exactly the required
// shape), and partition reads match every dimension with NULL-equality
// (`subject IS ?n`), never the request-narrowing wildcards above.
// ============================================================================

use std::sync::Arc;

use engram_domain::{KnowledgeEntity, KnowledgeRelationship, MemoryRecord};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

use crate::belief::SqlBeliefStore;
use crate::knowledge::SqlKnowledgeStore;

use super::engine::SqlMemoryService;
use super::schema::sql_error;

/// One grouped row of the scope inventory: a distinct partition tuple plus
/// the number of governed rows of one kind stored under exactly that tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeCountRow {
    pub scope: Scope,
    pub count: u64,
}

/// SQLite scope-inventory reader over the three governed stores.
///
/// Holds the same store handles the provider wires (memory service, knowledge
/// store, belief store); every read is a short read-only query against one
/// connection, so WAL writers are never blocked.
#[derive(Clone)]
pub struct SqlScopeInventory {
    memory: Arc<SqlMemoryService>,
    knowledge: Arc<SqlKnowledgeStore>,
    belief: Arc<SqlBeliefStore>,
}

/// Binds the exact-tuple predicate parameters plus the page bounds as the
/// `&[&dyn ToSql]` slice rusqlite takes. A macro (not a function) so the
/// bindings borrow the caller's locals — no owned-parameter round trip.
macro_rules! partition_params {
    ($partition:expr, $limit:expr, $offset:expr) => {
        [
            &$partition.tenant as &dyn rusqlite::ToSql,
            &$partition.subject,
            &$partition.workspace,
            &$partition.session,
            &$partition.environment,
            &$limit,
            &$offset,
        ]
    };
}
/// The exact-tuple partition predicate: every scope dimension matched with
/// NULL-equality (`IS`), the unified partition semantics — never a wildcard.
const EXACT_PARTITION_WHERE: &str =
    "tenant = ?1 AND subject IS ?2 AND workspace IS ?3 AND session IS ?4 AND environment IS ?5";

impl SqlScopeInventory {
    /// Builds the inventory over the provider's wired store handles.
    pub fn new(
        memory: Arc<SqlMemoryService>,
        knowledge: Arc<SqlKnowledgeStore>,
        belief: Arc<SqlBeliefStore>,
    ) -> Self {
        Self {
            memory,
            knowledge,
            belief,
        }
    }

    /// Distinct memory partitions with row counts (NULL dimensions are their
    /// own group; every memories row counts — the memories table has no
    /// archive column).
    pub fn count_memory_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        let connection = self.memory.store.lock_connection()?;
        count_scope_groups(&connection, "memories")
    }

    /// Distinct knowledge-entity partitions with row counts. Counts reflect
    /// rows present in the governed table (soft-archived rows included): the
    /// pgvector mirror of this table has no archive column, so rows-present is
    /// the one shape both engines can aggregate identically (RISK-002).
    pub fn count_entity_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        let connection = self.knowledge.lock_conn()?;
        count_scope_groups(&connection, "knowledge_entities")
    }

    /// Distinct belief partitions with counts of stored belief rows.
    pub fn count_belief_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        let connection = self.belief.lock()?;
        count_scope_groups(&connection, "beliefs")
    }

    /// Memories of exactly one partition, ascending by record id, bounded by
    /// `limit`/`offset`.
    pub fn list_partition_memories(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<MemoryRecord>> {
        let connection = self.memory.store.lock_connection()?;
        let mut stmt = connection
            .prepare(&format!(
                "SELECT record_json FROM memories WHERE {EXACT_PARTITION_WHERE} \
                 ORDER BY id ASC LIMIT ?6 OFFSET ?7"
            ))
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(partition_params!(partition, limit, offset), |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        decode_rows(rows)
    }

    /// Knowledge entities of exactly one partition (rows present, matching
    /// the count semantics), ascending by record id, bounded by
    /// `limit`/`offset`.
    pub fn list_partition_entities(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.knowledge.lock_conn()?;
        let mut stmt = connection
            .prepare(&format!(
                "SELECT record_json FROM knowledge_entities WHERE {EXACT_PARTITION_WHERE} \
                 ORDER BY id ASC LIMIT ?6 OFFSET ?7"
            ))
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(partition_params!(partition, limit, offset), |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        decode_rows(rows)
    }

    /// Knowledge relationships of exactly one partition (rows present,
    /// matching the count semantics), ascending by record id, bounded by
    /// `limit`/`offset`.
    pub fn list_partition_relationships(
        &self,
        partition: &Scope,
        limit: i64,
        offset: i64,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.knowledge.lock_conn()?;
        let mut stmt = connection
            .prepare(&format!(
                "SELECT record_json FROM knowledge_relationships WHERE {EXACT_PARTITION_WHERE} \
                 ORDER BY id ASC LIMIT ?6 OFFSET ?7"
            ))
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(partition_params!(partition, limit, offset), |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?;
        decode_rows(rows)
    }

    /// The most recently written memories of exactly one partition, newest
    /// first, bounded by `limit`. Recency follows write order (descending
    /// `rowid`): the memories table carries no timestamp column, and `rowid`
    /// is assigned at insert.
    pub fn list_recent_partition_memories(
        &self,
        partition: &Scope,
        limit: i64,
    ) -> CoreResult<Vec<MemoryRecord>> {
        let connection = self.memory.store.lock_connection()?;
        let mut stmt = connection
            .prepare(&format!(
                "SELECT record_json FROM memories WHERE {EXACT_PARTITION_WHERE} \
                 ORDER BY rowid DESC LIMIT ?6"
            ))
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(
                [
                    &partition.tenant as &dyn rusqlite::ToSql,
                    &partition.subject,
                    &partition.workspace,
                    &partition.session,
                    &partition.environment,
                    &limit,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        decode_rows(rows)
    }
}

/// Groups one governed table by the five scope columns and counts each
/// distinct tuple. SQLite `GROUP BY` treats NULLs as their own single group —
/// the NULL-as-own-value semantics the port requires. Counts reflect rows
/// present in the table (the pgvector mirror has no archive column; see the
/// method docs for the parity rationale).
fn count_scope_groups(connection: &Connection, table: &str) -> CoreResult<Vec<ScopeCountRow>> {
    // Table names come from this crate's own call sites only — never caller
    // input. The match keeps that invariant visible at compile time.
    let table = match table {
        "memories" | "knowledge_entities" | "knowledge_relationships" | "beliefs" => table,
        other => {
            return Err(CoreError::Adapter {
                adapter: "engram-store-sql".to_owned(),
                message: format!("scope inventory queried an unexpected table: {other}"),
            });
        }
    };
    let mut stmt = connection
        .prepare(&format!(
            "SELECT tenant, subject, workspace, session, environment, COUNT(*) \
             FROM {table} \
             GROUP BY tenant, subject, workspace, session, environment"
        ))
        .map_err(sql_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ScopeCountRow {
                scope: Scope {
                    tenant: row.get(0)?,
                    subject: row.get(1)?,
                    workspace: row.get(2)?,
                    session: row.get(3)?,
                    environment: row.get(4)?,
                },
                count: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(sql_error)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql_error)?);
    }
    Ok(out)
}

/// Deserializes `record_json` rows into contract records.
fn decode_rows<T: serde::de::DeserializeOwned>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
) -> CoreResult<Vec<T>> {
    let mut out = Vec::new();
    for row in rows {
        let json = row.map_err(sql_error)?;
        out.push(serde_json::from_str(&json).map_err(|e| CoreError::Adapter {
            adapter: "engram-store-sql".to_owned(),
            message: format!("scope inventory record decode failed: {e}"),
        })?);
    }
    Ok(out)
}

#[cfg(test)]
mod scope_inventory_tests {
    //! STEP-002 focused proofs: the sqlite scope-inventory aggregation and
    //! partition-scoped listings against REAL in-memory stores, exercised
    //! through the stores' own write APIs (never hand-inserted rows).

    use super::*;
    use crate::belief::SqlBeliefStore;
    use crate::knowledge::SqlKnowledgeStore;
    use engram_belief::BeliefRepository;
    use engram_domain::{
        Actor, ActorKind, AllowedUse, Belief, BeliefId, BeliefStatus, BeliefSubject, EntityKind,
        EntityRef, Id, KnowledgeEntity, MemoryContent, MemoryKind, Policy, Provenance, Requester,
        Retention, Sensitivity, Visibility, WriteMemoryRequest,
    };
    use engram_knowledge::KnowledgeRepository;
    use engram_memory::MemoryService;
    use futures::executor::block_on;

    fn scope(
        tenant: &str,
        subject: Option<&str>,
        workspace: Option<&str>,
        session: Option<&str>,
        environment: Option<&str>,
    ) -> Scope {
        Scope {
            tenant: tenant.to_string(),
            subject: subject.map(str::to_string),
            workspace: workspace.map(str::to_string),
            session: session.map(str::to_string),
            environment: environment.map(str::to_string),
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("scope-inventory-test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn requester() -> Requester {
        Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
            on_behalf_of: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "scope-inventory-test".to_owned(),
            actor: actor(),
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Private,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        }
    }

    fn write_req(text: &str, scope: Scope) -> WriteMemoryRequest {
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
            requester: requester(),
            provenance: provenance(),
            policy: policy(),
            links: Vec::new(),
            verdict: None,
            idempotency_key: None,
        }
    }

    fn entity(id: &str, name: &str, scope: Scope) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::new(id).expect("entity id"),
            graph_id: None,
            kind: EntityKind::Project,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope,
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }

    fn relationship(
        id: &str,
        subject_id: &str,
        object_id: &str,
        scope: Scope,
    ) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::new(id).expect("relationship id"),
            graph_id: None,
            subject: EntityRef {
                id: Some(Id::new(subject_id).expect("subject id")),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            predicate: "relates-to".to_owned(),
            object: EntityRef {
                id: Some(Id::new(object_id).expect("object id")),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            scope,
            evidence: Vec::new(),
            confidence: None,
            code_resolution: None,
            provenance: provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
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
            content: format!("belief content {id}"),
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
            policy: policy(),
            provenance: provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    fn inventory() -> SqlScopeInventory {
        SqlScopeInventory::new(
            std::sync::Arc::new(SqlMemoryService::open_in_memory().expect("memory store")),
            std::sync::Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge store")),
            std::sync::Arc::new(SqlBeliefStore::open_in_memory().expect("belief store")),
        )
    }

    fn write_memory(inv: &SqlScopeInventory, text: &str, scope: Scope) {
        block_on(async {
            inv.memory
                .write_memory(write_req(text, scope))
                .await
                .expect("write memory")
        });
    }

    #[test]
    fn scope_inventory_groups_null_as_its_own_value() {
        let inv = inventory();
        let unset = scope("acme", None, None, None, None);
        let defaulted = scope("acme", None, Some("default"), None, None);
        write_memory(&inv, "unset workspace memory", unset.clone());
        write_memory(&inv, "default workspace memory", defaulted.clone());

        let rows = inv.count_memory_partitions().expect("memory partitions");
        assert_eq!(
            rows.len(),
            2,
            "NULL workspace and 'default' are distinct partitions"
        );
        let unset_row = rows
            .iter()
            .find(|r| r.scope.workspace.is_none())
            .expect("unset row");
        let default_row = rows
            .iter()
            .find(|r| r.scope.workspace.as_deref() == Some("default"))
            .expect("default row");
        assert_eq!(unset_row.count, 1);
        assert_eq!(default_row.count, 1);
    }

    #[test]
    fn scope_inventory_counts_three_kinds_across_stores() {
        let inv = inventory();
        let a = scope("acme", Some("agent"), Some("proj-a"), None, None);
        let b = scope("acme", Some("agent"), Some("proj-b"), None, None);
        // Partition A: 2 memories, 3 entities, 1 relationship, 2 beliefs.
        write_memory(&inv, "a1", a.clone());
        write_memory(&inv, "a2", a.clone());
        for i in 1..=3 {
            block_on(async {
                inv.knowledge
                    .put_entity(entity(
                        &format!("ent-a-{i}"),
                        &format!("Entity {i}"),
                        a.clone(),
                    ))
                    .await
                    .expect("put entity")
            });
        }
        block_on(async {
            inv.knowledge
                .put_relationship(relationship("rel-a-1", "ent-a-1", "ent-a-2", a.clone()))
                .await
                .expect("put relationship")
        });
        for i in 1..=2 {
            block_on(async {
                inv.belief
                    .put_belief(belief(&format!("bel-a-{i}"), a.clone()))
                    .await
                    .expect("put belief")
            });
        }
        // Partition B: 1 memory only.
        write_memory(&inv, "b1", b.clone());

        let memories = inv.count_memory_partitions().expect("memories");
        let entities = inv.count_entity_partitions().expect("entities");
        let beliefs = inv.count_belief_partitions().expect("beliefs");
        assert_eq!(
            memories
                .iter()
                .find(|r| r.scope == a)
                .map(|r| r.count)
                .unwrap_or(0),
            2
        );
        assert_eq!(
            entities
                .iter()
                .find(|r| r.scope == a)
                .map(|r| r.count)
                .unwrap_or(0),
            3
        );
        assert_eq!(
            beliefs
                .iter()
                .find(|r| r.scope == a)
                .map(|r| r.count)
                .unwrap_or(0),
            2
        );
        assert_eq!(
            memories
                .iter()
                .find(|r| r.scope == b)
                .map(|r| r.count)
                .unwrap_or(0),
            1
        );
        // B has no entities/beliefs: absent from those groups entirely.
        assert!(entities.iter().all(|r| r.scope != b));
        assert!(beliefs.iter().all(|r| r.scope != b));
    }

    #[test]
    fn scope_inventory_listing_is_exact_partition_only() {
        let inv = inventory();
        let a = scope("acme", None, Some("proj-a"), None, None);
        // Sibling partition differs in one supplied dimension, plus an
        // "(unset)" sibling the narrowing reads cannot address.
        let sibling = scope("acme", None, Some("proj-b"), None, None);
        let unset = scope("acme", None, None, None, None);
        write_memory(&inv, "belongs-to-a", a.clone());
        write_memory(&inv, "belongs-to-sibling", sibling.clone());
        write_memory(&inv, "belongs-to-unset", unset.clone());
        block_on(async {
            inv.knowledge
                .put_entity(entity("ent-a-1", "A entity", a.clone()))
                .await
                .expect("put entity")
        });
        block_on(async {
            inv.knowledge
                .put_entity(entity("ent-sib-1", "Sibling entity", sibling))
                .await
                .expect("put entity")
        });

        let memories = inv
            .list_partition_memories(&a, 10, 0)
            .expect("list memories");
        assert_eq!(memories.len(), 1, "only partition A's memory is listed");
        assert_eq!(memories[0].content.text, "belongs-to-a");

        let entities = inv
            .list_partition_entities(&a, 10, 0)
            .expect("list entities");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "A entity");

        // The "(unset)" partition is addressable exactly — the RISK-002 seam.
        let unset_memories = inv
            .list_partition_memories(&unset, 10, 0)
            .expect("list unset memories");
        assert_eq!(unset_memories.len(), 1);
        assert_eq!(unset_memories[0].content.text, "belongs-to-unset");
    }

    #[test]
    fn scope_inventory_listing_paginates_in_id_order() {
        let inv = inventory();
        let a = scope("acme", None, Some("paged"), None, None);
        for i in 1..=5 {
            write_memory(&inv, &format!("page memory {i}"), a.clone());
        }
        let all = inv.list_partition_memories(&a, 100, 0).expect("all");
        assert_eq!(all.len(), 5);
        let ids: Vec<String> = all.iter().map(|m| m.id.to_string()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "listing order is ascending id");

        let page1 = inv.list_partition_memories(&a, 2, 0).expect("page1");
        let page2 = inv.list_partition_memories(&a, 2, 2).expect("page2");
        let page3 = inv.list_partition_memories(&a, 2, 4).expect("page3");
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        assert_eq!(page3.len(), 1, "last page holds the remainder");
        let mut walked: Vec<String> = [page1, page2, page3]
            .concat()
            .iter()
            .map(|m| m.id.to_string())
            .collect();
        assert_eq!(walked.sort(), (), "walk compiles");
        assert_eq!(
            walked, sorted,
            "walking the pages covers exactly the listing"
        );
    }

    #[test]
    fn scope_inventory_recent_memories_are_newest_first() {
        let inv = inventory();
        let a = scope("acme", None, Some("recent"), None, None);
        for i in 1..=4 {
            write_memory(&inv, &format!("recent memory {i}"), a.clone());
        }
        let other = scope("acme", None, Some("other"), None, None);
        write_memory(&inv, "other partition memory", other);

        let recent = inv.list_recent_partition_memories(&a, 2).expect("recent");
        assert_eq!(recent.len(), 2, "bounded by limit");
        assert_eq!(
            recent[0].content.text, "recent memory 4",
            "newest write first"
        );
        assert_eq!(recent[1].content.text, "recent memory 3");
        assert!(
            recent
                .iter()
                .all(|m| m.content.text != "other partition memory"),
            "confined to the selected partition"
        );
    }

    #[test]
    fn scope_inventory_empty_store_has_no_partitions() {
        let inv = inventory();
        assert!(inv.count_memory_partitions().expect("memories").is_empty());
        assert!(inv.count_entity_partitions().expect("entities").is_empty());
        assert!(inv.count_belief_partitions().expect("beliefs").is_empty());
        let absent = inv
            .list_partition_memories(&scope("nobody", None, None, None, None), 10, 0)
            .expect("listing an absent partition");
        assert!(
            absent.is_empty(),
            "absent partition lists zero rows, not an error"
        );
    }
}
