//! Explicit administrative inventory over one selected logical store.
//! Recipe-side authorization remains mandatory; ordinary domain ports stay launch-scope bound.
use crate::{
    HelixStore,
    codec::Partition,
    error::{Error, Result},
    records::*,
    transport::Operation,
};
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, MemoryRecord, Scope};
use engram_runtime::{CoreError, CoreResult};
use helix_db::dsl::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeCountRow {
    pub scope: Scope,
    pub count: u64,
}
/// Validate the full exact tuple against both physical identity and typed projections.
fn validate_row(
    selected: &Partition,
    family: &str,
    row: &Value,
    expected: Option<&Scope>,
) -> Result<(Scope, Value)> {
    let payload: Value = serde_json::from_str(
        row.get("payload")
            .and_then(Value::as_str)
            .ok_or(Error::IncompatibleSchema)?,
    )
    .map_err(|_| Error::IncompatibleSchema)?;
    let scope: Scope = decode(
        payload
            .get("scope")
            .cloned()
            .ok_or(Error::IncompatibleSchema)?,
    )?;
    let partition = Partition {
        logical_store: selected.logical_store.clone(),
        scope: scope.clone(),
        embedding_space: selected.embedding_space.clone(),
    };
    let public_id = id(&payload)?;
    if expected.is_some_and(|s| s != &scope)
        || row.get("logical_store").and_then(Value::as_str) != Some(selected.logical_store.as_str())
        || row.get("partition").and_then(Value::as_str) != Some(partition.discriminator()?.as_str())
        || row.get("key").and_then(Value::as_str)
            != Some(partition.record_key(family, public_id)?.as_str())
        || row.get("public_id").and_then(Value::as_str) != Some(public_id)
    {
        return Err(Error::IncompatibleSchema);
    }
    for (name, value) in [
        ("tenant", Some(scope.tenant.as_str())),
        ("subject", scope.subject.as_deref()),
        ("workspace", scope.workspace.as_deref()),
        ("session", scope.session.as_deref()),
        ("environment", scope.environment.as_deref()),
    ] {
        if row
            .get(format!("scope_{name}_set"))
            .and_then(Value::as_bool)
            != Some(value.is_some())
            || row.get(format!("scope_{name}")).and_then(Value::as_str) != value
        {
            return Err(Error::IncompatibleSchema);
        }
    }
    Ok((scope, payload))
}
impl HelixStore {
    pub fn count_memory_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        self.count_family(MEMORY)
    }
    pub fn count_entity_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        self.count_family(ENTITY)
    }
    pub fn count_relationship_partitions(&self) -> CoreResult<Vec<ScopeCountRow>> {
        self.count_family(RELATIONSHIP)
    }
    fn inventory_rows(
        &self,
        family: &str,
        scope: Option<&Scope>,
        offset: usize,
        limit: usize,
        recent: bool,
    ) -> Result<Vec<(Scope, Value)>> {
        self.admit_logical_work()?;
        let mut predicates = vec![SourcePredicate::eq(
            "logical_store",
            self.inner.partition.logical_store.clone(),
        )];
        if let Some(scope) = scope {
            let partition = Partition {
                logical_store: self.inner.partition.logical_store.clone(),
                scope: scope.clone(),
                embedding_space: self.inner.partition.embedding_space.clone(),
            };
            predicates.push(SourcePredicate::eq("partition", partition.discriminator()?));
        }
        let order = if recent {
            vec![("createdAt", Order::Desc), ("public_id", Order::Asc)]
        } else {
            vec![("public_id", Order::Asc), ("partition", Order::Asc)]
        };
        let query = QueryRequest::read(
            read_batch()
                .var_as(
                    "rows",
                    g().n_with_label_where(family, SourcePredicate::and(predicates))
                        .order_by_multiple(order)
                        .skip(offset)
                        .limit(limit.min(8))
                        .value_map(None::<Vec<String>>),
                )
                .returning(["rows"]),
        );
        let response = self.inner.transport.execute(&Operation::new(query)?)?;
        crate::schema::rows(&response, "rows")?
            .iter()
            .map(|r| validate_row(&self.inner.partition, family, r, scope))
            .collect()
    }
    fn count_family(&self, family: &str) -> CoreResult<Vec<ScopeCountRow>> {
        let mut groups: BTreeMap<String, ScopeCountRow> = BTreeMap::new();
        let mut offset = 0;
        loop {
            let rows = self
                .inventory_rows(family, None, offset, 8, false)
                .map_err(core)?;
            let n = rows.len();
            for (scope, _) in rows {
                let partition = Partition {
                    logical_store: self.inner.partition.logical_store.clone(),
                    scope: scope.clone(),
                    embedding_space: None,
                };
                let row = groups
                    .entry(partition.discriminator().map_err(core)?)
                    .or_insert(ScopeCountRow { scope, count: 0 });
                row.count = row
                    .count
                    .checked_add(1)
                    .ok_or_else(|| core(Error::Storage))?;
            }
            if n < 8 {
                break;
            }
            offset += n;
        }
        // The recipe delegates canonical label sorting/zero filling to integration's existing helpers.
        Ok(groups.into_values().collect())
    }
    pub fn partition_memories(
        &self,
        scope: &Scope,
        limit: usize,
        offset: u64,
    ) -> CoreResult<Vec<MemoryRecord>> {
        self.partition_page(MEMORY, scope, limit, offset, false)
    }
    pub fn partition_entities(
        &self,
        scope: &Scope,
        limit: usize,
        offset: u64,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        self.partition_page(ENTITY, scope, limit, offset, false)
    }
    pub fn partition_relationships(
        &self,
        scope: &Scope,
        limit: usize,
        offset: u64,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.partition_page(RELATIONSHIP, scope, limit, offset, false)
    }
    fn partition_page<T: serde::de::DeserializeOwned>(
        &self,
        family: &str,
        scope: &Scope,
        limit: usize,
        offset: u64,
        recent: bool,
    ) -> CoreResult<Vec<T>> {
        if limit == 0
            || limit > 500
            || usize::try_from(offset)
                .ok()
                .and_then(|n| n.checked_add(limit))
                .is_none()
        {
            return Err(CoreError::InvalidRequest {
                reason: "invalid bounded partition page".into(),
            });
        }
        let mut rows = vec![];
        let mut cursor = offset as usize;
        while rows.len() < limit {
            let page = self
                .inventory_rows(family, Some(scope), cursor, limit - rows.len(), recent)
                .map_err(core)?;
            let n = page.len();
            for (_, v) in page {
                rows.push(decode(v).map_err(core)?);
            }
            if n < 8 {
                break;
            }
            cursor += n;
        }
        Ok(rows)
    }
    pub fn partition_recent_memories(
        &self,
        scope: &Scope,
        limit: usize,
    ) -> CoreResult<Vec<MemoryRecord>> {
        self.partition_page(MEMORY, scope, limit, 0, true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn malformed_inventory_partition_and_none_empty_scope_fail_closed() {
        let scope = Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("workspace".into()),
            session: None,
            environment: None,
        };
        let partition = Partition {
            logical_store: "store".into(),
            scope: scope.clone(),
            embedding_space: None,
        };
        let mut row = json!({"logical_store":"store","public_id":"id","key":partition.record_key(MEMORY,"id").unwrap(),"partition":partition.discriminator().unwrap(),"payload":serde_json::to_string(&json!({"id":"id","scope":scope})).unwrap(),"scope_tenant_set":true,"scope_tenant":"tenant","scope_subject_set":false,"scope_workspace_set":true,"scope_workspace":"workspace","scope_session_set":false,"scope_environment_set":false});
        assert!(validate_row(&partition, MEMORY, &row, Some(&scope)).is_ok());
        row["partition"] = "forged".into();
        assert!(validate_row(&partition, MEMORY, &row, None).is_err());
        row["partition"] = partition.discriminator().unwrap().into();
        let mut empty = scope.clone();
        empty.subject = Some(String::new());
        assert!(validate_row(&partition, MEMORY, &row, Some(&empty)).is_err());
        row["payload"] = serde_json::to_string(&json!({"id":"id","scope":empty}))
            .unwrap()
            .into();
        assert!(validate_row(&partition, MEMORY, &row, None).is_err());
    }
}
