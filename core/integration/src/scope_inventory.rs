//! Engine-neutral scope-inventory port (job 012-admin-console-v1, STEP-001).
//!
//! [`ScopeInventory`] enumerates the distinct logical partitions present in a
//! store's governed data and lists one partition's records in isolation. A
//! partition is a distinct `(tenant, subject, workspace, session,
//! environment)` value-tuple present in the governed tables — a derived read
//! model, not a new entity (no registry, no schema change).
//!
//! # Unified partition semantics (the RISK-002 seam)
//!
//! Both adapters implement ONE engine-level rule, defined here and enforced by
//! the cross-recipe differential test:
//!
//! - **NULL is its own distinct value.** A record whose `workspace` is NULL
//!   belongs to a different partition than one whose `workspace` is
//!   `"default"`. Consumers render NULL dimensions as [`UNSET_LABEL`]
//!   (`"(unset)"`), never silently merged.
//! - **Partition reads match every dimension with NULL-equality** (exact-tuple
//!   matching, `IS NOT DISTINCT FROM` shape) — see [`partition_matches`]. This
//!   is deliberately different from the request-scope narrowing used by the
//!   scoped repository reads (absent request fields are wildcards there): a
//!   partition read can address an "(unset)" partition exactly, which the
//!   narrowing reads cannot express.
//! - **Deterministic order.** [`inventory`] returns partitions in ascending
//!   partition order ([`sort_summaries`]); the paginated listings return rows
//!   in ascending record-id order; [`partition_recent_memories`] returns
//!   newest-first. Engines must agree on these orders so results are
//!   comparable across recipes.
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds
//! no engine query text. The SQLite implementation lives in the adapters layer
//! (`engram-store-sqlite`), the pgvector implementation in
//! `engram-store-pgvector`; the trait impls are wired by the backend recipes.
//!
//! [`inventory`]: ScopeInventory::inventory
//! [`partition_recent_memories`]: ScopeInventory::partition_recent_memories

use async_trait::async_trait;
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, MemoryRecord, Scope};
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

/// Display label for a scope dimension whose stored value is NULL.
///
/// The label is part of the read-model contract (not a UI private) so both
/// engine recipes and every consumer render unset dimensions identically.
pub const UNSET_LABEL: &str = "(unset)";

/// Returns the display form of one scope-dimension value: the value verbatim,
/// or [`UNSET_LABEL`] when the dimension is unset (NULL).
pub fn dimension_label(value: Option<&str>) -> &str {
    value.unwrap_or(UNSET_LABEL)
}

/// Exact-tuple partition matching: the unified partition-read semantics.
///
/// Every dimension must be equal, treating `None` and `Some(value)` as
/// different values (NULL-equality). Unlike request-scope narrowing — where an
/// absent request field is a wildcard — a partition read never widens: a
/// record with `workspace = None` matches only the partition whose workspace
/// is unset.
pub fn partition_matches(record_scope: &Scope, partition: &Scope) -> bool {
    record_scope.tenant == partition.tenant
        && record_scope.subject == partition.subject
        && record_scope.workspace == partition.workspace
        && record_scope.session == partition.session
        && record_scope.environment == partition.environment
}

/// Sorts partition summaries into the canonical ascending partition order.
///
/// Order is by the display label of each dimension — tenant, subject,
/// workspace, session, environment — with unset dimensions ordered under
/// [`UNSET_LABEL`]. The order is defined once here so both engine recipes and
/// the parity differential test share one truth; `inventory` implementors must
/// return rows in this order.
pub fn sort_summaries(summaries: &mut [PartitionSummary]) {
    summaries.sort_by(|a, b| a.compare_partitions(b));
}

/// Merges per-kind grouped counts into one summary row per distinct
/// partition, zero-filling kinds the partition does not carry, and returns
/// the rows in the canonical order. One implementation for both engine
/// recipes — the adapters deliver `(scope, count)` groupings and never
/// re-derive merge or order semantics (RISK-002).
pub fn merge_partition_counts(
    memories: &[(Scope, u64)],
    entities: &[(Scope, u64)],
    beliefs: &[(Scope, u64)],
) -> Vec<PartitionSummary> {
    let mut merged: Vec<PartitionSummary> = Vec::new();
    let mut absorb = |rows: &[(Scope, u64)], kind: CountedKind| {
        for (scope, count) in rows {
            match merged.iter_mut().find(|summary| &summary.scope == scope) {
                Some(summary) => summary.add(kind, *count),
                None => {
                    let mut summary = PartitionSummary {
                        scope: scope.clone(),
                        memory_count: 0,
                        entity_count: 0,
                        belief_count: 0,
                    };
                    summary.add(kind, *count);
                    merged.push(summary);
                }
            }
        }
    };
    absorb(memories, CountedKind::Memory);
    absorb(entities, CountedKind::Entity);
    absorb(beliefs, CountedKind::Belief);
    sort_summaries(&mut merged);
    merged
}

/// The three governed kinds the inventory counts.
#[derive(Clone, Copy)]
enum CountedKind {
    Memory,
    Entity,
    Belief,
}

impl PartitionSummary {
    /// Adds `count` rows of one kind to this summary.
    fn add(&mut self, kind: CountedKind, count: u64) {
        match kind {
            CountedKind::Memory => self.memory_count += count,
            CountedKind::Entity => self.entity_count += count,
            CountedKind::Belief => self.belief_count += count,
        }
    }
}

/// Continuation offset for a page: advance only when the page came back full,
/// so an exactly-full final page yields a next offset whose follow-up page is
/// empty (terminating), and a short page ends the walk. Shared by both engine
/// recipes so pagination semantics never diverge.
pub fn next_page_offset<T>(items: &[T], page: &PartitionPage) -> Option<u64> {
    if items.len() < page.limit {
        None
    } else {
        Some(page.offset + items.len() as u64)
    }
}

/// One inventory row: a partition's exact scope tuple plus its record counts.
///
/// Counts cover the owner-named trio only (memories, knowledge entities,
/// beliefs); deeper per-kind breakdowns are out of scope for the console v1
/// read model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionSummary {
    /// The partition identity: exact values, with NULL as NULL.
    pub scope: Scope,
    /// Memories stored under exactly this partition.
    pub memory_count: u64,
    /// Knowledge entities stored under exactly this partition.
    pub entity_count: u64,
    /// Beliefs stored under exactly this partition.
    pub belief_count: u64,
}

impl PartitionSummary {
    /// Total governed records counted for this partition.
    pub fn total_count(&self) -> u64 {
        self.memory_count + self.entity_count + self.belief_count
    }

    /// Ordering key: the display labels of the five scope dimensions.
    fn label_key(&self) -> [std::borrow::Cow<'_, str>; 5] {
        [
            self.scope.tenant.as_str().into(),
            label_cow(self.scope.subject.as_deref()),
            label_cow(self.scope.workspace.as_deref()),
            label_cow(self.scope.session.as_deref()),
            label_cow(self.scope.environment.as_deref()),
        ]
    }

    /// Compares two summaries by the canonical partition order.
    fn compare_partitions(&self, other: &Self) -> std::cmp::Ordering {
        self.label_key().cmp(&other.label_key())
    }
}

/// Owned-or-borrowed display label for one dimension.
fn label_cow(value: Option<&str>) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Borrowed(dimension_label(value))
}

/// The largest accepted listing page.
pub const MAX_PARTITION_PAGE_LIMIT: usize = 500;

/// A bounded listing window into one partition.
///
/// Pages are offset-based over the listing's deterministic order (ascending
/// record id). `next_offset` on the result continues the walk; `None` means
/// the listing is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionPage {
    /// Page size: at least 1, at most [`MAX_PARTITION_PAGE_LIMIT`].
    pub limit: usize,
    /// Rows to skip before the page starts (0 = first page).
    pub offset: u64,
}

impl Default for PartitionPage {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

impl PartitionPage {
    /// Validates the window: a zero or oversized limit is a typed invalid
    /// request, never a silent clamp (clamping would let engines disagree
    /// about page boundaries and break partition-view parity).
    pub fn validate(&self) -> CoreResult<()> {
        if self.limit == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "partition page limit must be at least 1".to_string(),
            });
        }
        if self.limit > MAX_PARTITION_PAGE_LIMIT {
            return Err(CoreError::InvalidRequest {
                reason: format!(
                    "partition page limit {limit} exceeds the maximum {MAX_PARTITION_PAGE_LIMIT}",
                    limit = self.limit
                ),
            });
        }
        Ok(())
    }
}

/// One page of partition-scoped records plus the continuation offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartitionRecords<T> {
    /// The page's records in the listing's deterministic order.
    pub items: Vec<T>,
    /// Offset of the next page, or `None` when the listing is exhausted.
    pub next_offset: Option<u64>,
}

/// Scope-inventory port: distinct-partition enumeration plus exact-partition
/// listings.
///
/// Implementations MUST follow the unified partition semantics of this module
/// (NULL-distinct grouping, exact-tuple matching, deterministic orders). An
/// empty store yields an empty inventory; a partition that exists in no
/// governed table yields all-zero counts only if it is present in at least one
/// table's data — the inventory is derived from what the data contains, never
/// invented.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait ScopeInventory: Send + Sync {
    /// Enumerates every distinct partition present in the store's governed
    /// data with per-kind counts, in canonical ascending partition order
    /// ([`sort_summaries`]).
    async fn inventory(&self) -> CoreResult<Vec<PartitionSummary>>;

    /// Lists the memories of exactly one partition, ascending by record id,
    /// paginated by [`PartitionPage`].
    async fn partition_memories(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<MemoryRecord>>;

    /// Lists the knowledge entities of exactly one partition, ascending by
    /// record id, paginated by [`PartitionPage`].
    async fn partition_entities(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<KnowledgeEntity>>;

    /// Lists the knowledge relationships of exactly one partition, ascending
    /// by record id, paginated by [`PartitionPage`].
    async fn partition_relationships(
        &self,
        partition: &Scope,
        page: PartitionPage,
    ) -> CoreResult<PartitionRecords<KnowledgeRelationship>>;

    /// The most recently created memories of exactly one partition, newest
    /// first, bounded by `limit` (validated like a page limit: at least 1, at
    /// most [`MAX_PARTITION_PAGE_LIMIT`]).
    async fn partition_recent_memories(
        &self,
        partition: &Scope,
        limit: usize,
    ) -> CoreResult<Vec<MemoryRecord>>;
}

/// Validates a recent-additions bound using the same rule as [`PartitionPage`].
pub fn validate_recent_limit(limit: usize) -> CoreResult<()> {
    PartitionPage { limit, offset: 0 }.validate()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn scope_inventory_partition_matches_is_exact_tuple() {
        let partition = scope(
            "acme",
            Some("agent-1"),
            Some("proj"),
            Some("s-1"),
            Some("prod"),
        );
        assert!(
            partition_matches(
                &scope(
                    "acme",
                    Some("agent-1"),
                    Some("proj"),
                    Some("s-1"),
                    Some("prod")
                ),
                &partition
            ),
            "identical tuples match"
        );
        // Each dimension must match exactly; a differing value on any one
        // dimension is a different partition.
        assert!(!partition_matches(
            &scope(
                "other",
                Some("agent-1"),
                Some("proj"),
                Some("s-1"),
                Some("prod")
            ),
            &partition
        ));
        assert!(!partition_matches(
            &scope(
                "acme",
                Some("agent-2"),
                Some("proj"),
                Some("s-1"),
                Some("prod")
            ),
            &partition
        ));
        assert!(!partition_matches(
            &scope(
                "acme",
                Some("agent-1"),
                Some("other"),
                Some("s-1"),
                Some("prod")
            ),
            &partition
        ));
        assert!(!partition_matches(
            &scope(
                "acme",
                Some("agent-1"),
                Some("proj"),
                Some("s-2"),
                Some("prod")
            ),
            &partition
        ));
        assert!(!partition_matches(
            &scope(
                "acme",
                Some("agent-1"),
                Some("proj"),
                Some("s-1"),
                Some("staging")
            ),
            &partition
        ));
    }

    #[test]
    fn scope_inventory_unset_is_its_own_partition_value() {
        let unset_partition = scope("acme", None, None, None, None);
        // NULL never matches a supplied value on any optional dimension.
        assert!(!partition_matches(
            &scope("acme", Some("agent-1"), None, None, None),
            &unset_partition
        ));
        assert!(!partition_matches(
            &scope("acme", None, Some("default"), None, None),
            &unset_partition
        ));
        assert!(!partition_matches(
            &scope("acme", None, None, Some("s-1"), None),
            &unset_partition
        ));
        assert!(!partition_matches(
            &scope("acme", None, None, None, Some("prod")),
            &unset_partition
        ));
        // Two unset tuples do match: NULL equals NULL.
        assert!(partition_matches(
            &scope("acme", None, None, None, None),
            &unset_partition
        ));
    }

    #[test]
    fn scope_inventory_partition_reads_never_wildcard() {
        // The narrowing reads treat an absent request field as a wildcard; a
        // partition read must not: a supplied-tuple partition never matches a
        // record with extra values, and an unset partition never matches a
        // record with values (checked above). This test pins the contrast
        // that motivates the port (RISK-002): a per-adapter reuse of the
        // narrowing predicates cannot address "(unset)" partitions.
        let supplied = scope("acme", None, Some("proj"), None, None);
        assert!(!partition_matches(
            &scope(
                "acme",
                Some("agent-1"),
                Some("proj"),
                Some("s-9"),
                Some("prod")
            ),
            &supplied
        ));
    }

    #[test]
    fn scope_inventory_unset_label_is_fixed() {
        assert_eq!(UNSET_LABEL, "(unset)");
        assert_eq!(dimension_label(None), "(unset)");
        assert_eq!(dimension_label(Some("proj")), "proj");
        assert_eq!(dimension_label(Some("domain/sub")), "domain/sub");
    }

    #[test]
    fn scope_inventory_sort_order_is_deterministic() {
        let summary =
            |tenant: &str, subject: Option<&str>, workspace: Option<&str>| PartitionSummary {
                scope: scope(tenant, subject, workspace, None, None),
                memory_count: 0,
                entity_count: 0,
                belief_count: 0,
            };
        let mut rows = vec![
            summary("zzz", None, Some("b")),
            summary("acme", Some("z"), None),
            summary("acme", None, None),
            summary("acme", Some("a"), None),
            summary("acme", None, Some("default")),
        ];
        sort_summaries(&mut rows);
        let keys: Vec<String> = rows
            .iter()
            .map(|r| {
                format!(
                    "{}/{}/{}",
                    r.scope.tenant,
                    dimension_label(r.scope.subject.as_deref()),
                    dimension_label(r.scope.workspace.as_deref())
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                "acme/(unset)/(unset)",
                "acme/(unset)/default",
                "acme/a/(unset)",
                "acme/z/(unset)",
                "zzz/(unset)/b",
            ],
            "partitions sort by display label with unset under {UNSET_LABEL}"
        );
    }

    #[test]
    fn scope_inventory_page_validation_rejects_bad_windows() {
        assert!(PartitionPage::default().validate().is_ok());
        assert!(
            PartitionPage {
                limit: MAX_PARTITION_PAGE_LIMIT,
                offset: 99
            }
            .validate()
            .is_ok(),
            "the maximum limit is accepted"
        );
        let zero = PartitionPage {
            limit: 0,
            offset: 0,
        }
        .validate();
        assert!(zero.is_err(), "zero limit is rejected");
        let oversized = PartitionPage {
            limit: MAX_PARTITION_PAGE_LIMIT + 1,
            offset: 0,
        }
        .validate();
        assert!(oversized.is_err(), "oversized limit is rejected");
        assert!(validate_recent_limit(0).is_err());
        assert!(validate_recent_limit(MAX_PARTITION_PAGE_LIMIT).is_ok());
        assert!(validate_recent_limit(MAX_PARTITION_PAGE_LIMIT + 1).is_err());
    }

    #[test]
    fn scope_inventory_merge_zero_fills_and_orders() {
        let count = |workspace: Option<&str>| scope("acme", None, workspace, None, None);
        let memories = vec![(count(None), 1), (count(Some("proj-a")), 2)];
        let entities = vec![(count(Some("proj-a")), 3), (count(Some("proj-z")), 4)];
        let beliefs: Vec<(Scope, u64)> = vec![];
        let merged = merge_partition_counts(&memories, &entities, &beliefs);
        assert_eq!(merged.len(), 3, "union of partition tuples");
        let order: Vec<Option<String>> = merged.iter().map(|m| m.scope.workspace.clone()).collect();
        assert_eq!(
            order,
            vec![None, Some("proj-a".into()), Some("proj-z".into())]
        );
        assert_eq!(
            merged[0].memory_count, 1,
            "unset row zero-fills other kinds"
        );
        assert_eq!(merged[0].entity_count, 0);
        assert_eq!(merged[1].memory_count, 2);
        assert_eq!(merged[1].entity_count, 3);
        assert_eq!(
            merged[2].belief_count, 0,
            "belief-only-missing kind zero-fills"
        );
    }

    #[test]
    fn scope_inventory_total_count_sums_the_trio() {
        let row = PartitionSummary {
            scope: scope("acme", None, Some("proj"), None, None),
            memory_count: 3,
            entity_count: 5,
            belief_count: 7,
        };
        assert_eq!(row.total_count(), 15);
    }
}
