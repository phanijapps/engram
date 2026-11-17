//! Partition-bound vectors and complete embedding-space validation.
use engram_domain::EmbeddingSpace;
use engram_runtime::{CoreError, CoreResult};

pub(crate) fn validate_space(
    expected: &EmbeddingSpace,
    actual: &EmbeddingSpace,
    vector: &[f32],
) -> CoreResult<()> {
    if vector.len() != expected.dimensions as usize || vector.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::InvalidRequest {
            reason: "dimension_mismatch or nonfinite vector".into(),
        });
    }
    if expected != actual {
        return Err(CoreError::InvalidRequest {
            reason: "embedding_space_mismatch".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_dimensions_foreign_model_is_rejected() {
        let expected = EmbeddingSpace::new("provider", "model-a", 2, "query", None::<String>);
        let foreign = EmbeddingSpace::new("provider", "model-b", 2, "query", None::<String>);
        assert!(
            validate_space(&expected, &foreign, &[1.0, 0.0]).is_err(),
            "foreign embedding identity must reject even with equal dimensions"
        );
    }
}

use crate::{HelixStore, error::Error, records::*, transport::Operation};
use async_trait::async_trait;
use engram_domain::Id;
use helix_db::dsl::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
#[derive(Clone)]
pub struct HelixVectorIndex {
    pub(crate) store: HelixStore,
    space: EmbeddingSpace,
}
pub(crate) fn validate_config(space: &EmbeddingSpace) -> crate::error::Result<()> {
    if space.provider.is_empty()
        || space.model.is_empty()
        || space.prompt_profile.is_empty()
        || space.dimensions == 0
        || space.dimensions > 65536
    {
        return Err(Error::InvalidConfig(
            "complete bounded embedding identity required",
        ));
    }
    Ok(())
}
pub(crate) fn family(space: &EmbeddingSpace) -> String {
    format!(
        "EngramVectorV1_{}",
        Sha256::digest(serde_json::to_vec(space).expect("embedding space serializes"))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
impl HelixStore {
    pub fn vector_index(&self) -> CoreResult<HelixVectorIndex> {
        let space = self
            .inner
            .partition
            .embedding_space
            .clone()
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "vectors disabled: no embedding space configured".into(),
            })?;
        validate_config(&space).map_err(core)?;
        Ok(HelixVectorIndex {
            store: self.clone(),
            space,
        })
    }
}
impl HelixVectorIndex {
    fn label(&self) -> String {
        family(&self.space)
    }
    fn checked(&self, v: Value) -> CoreResult<Vec<f32>> {
        let space: EmbeddingSpace = decode(
            v.get("space")
                .cloned()
                .ok_or_else(|| core(Error::IncompatibleSchema))?,
        )
        .map_err(core)?;
        let vector: Vec<f32> = decode(
            v.get("embedding")
                .cloned()
                .ok_or_else(|| core(Error::IncompatibleSchema))?,
        )
        .map_err(core)?;
        validate_space(&self.space, &space, &vector)?;
        Ok(vector)
    }
}
#[async_trait]
impl engram_retrieval::VectorIndex for HelixVectorIndex {
    fn embedding_space(&self) -> &EmbeddingSpace {
        &self.space
    }
    async fn insert(&self, id: &Id, space: &EmbeddingSpace, vector: Vec<f32>) -> CoreResult<()> {
        validate_space(&self.space, space, &vector)?;
        self.store.put(&self.label(),&json!({"id":id,"scope":self.store.inner.partition.scope,"space":space,"embedding":vector})).map_err(core)?;
        Ok(())
    }
    async fn search(
        &self,
        space: &EmbeddingSpace,
        vector: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>> {
        self.search_constrained(space, vector, limit, None)
    }
    async fn vector_for_target(&self, id: &Id) -> CoreResult<Option<Vec<f32>>> {
        self.store
            .get::<Value>(&self.label(), id.as_str())
            .map_err(core)?
            .map(|v| self.checked(v))
            .transpose()
    }
    async fn embedded_ids(&self) -> CoreResult<HashSet<Id>> {
        let mut ids = HashSet::new();
        for v in self.store.all::<Value>(&self.label()).map_err(core)? {
            self.checked(v.clone())?;
            ids.insert(Id::from(id(&v).map_err(core)?));
        }
        Ok(ids)
    }
    async fn delete_target(&self, id: &Id) -> CoreResult<()> {
        self.store
            .remove(&self.label(), id.as_str())
            .map_err(core)?;
        Ok(())
    }
    async fn gc_orphan_targets(&self, live: &[Id]) -> CoreResult<usize> {
        self.store.domain_atomic(|s| {
            let mut removed = 0;
            for v in s.all::<Value>(&self.label()).map_err(core)? {
                let id = Id::from(id(&v).map_err(core)?);
                if !live.contains(&id) {
                    s.stage(&self.label(), id.as_str(), None).map_err(core)?;
                    removed += 1
                }
            }
            Ok(removed)
        })
    }
    async fn clear(&self) -> CoreResult<()> {
        self.gc_orphan_targets(&[]).await?;
        Ok(())
    }
}

impl HelixVectorIndex {
    pub(crate) fn search_constrained(
        &self,
        space: &EmbeddingSpace,
        vector: Vec<f32>,
        limit: usize,
        allowed: Option<&[String]>,
    ) -> CoreResult<Vec<(Id, f32)>> {
        validate_space(&self.space, space, &vector)?;
        if limit == 0 || limit > 1000 {
            return Err(CoreError::InvalidRequest {
                reason: "invalid bounded vector limit".into(),
            });
        }
        self.store.admit_logical_work().map_err(core)?;
        let partition = self.store.inner.partition.discriminator().map_err(core)?;
        let traversal = if let Some(allowed) = allowed {
            if allowed.is_empty() {
                return Ok(vec![]);
            }
            g().n_with_label_where(
                self.label(),
                SourcePredicate::eq("partition", partition.clone()),
            )
            .where_(Predicate::is_in("public_id", allowed.to_vec()))
            .vector_search_with(
                self.label(),
                "embedding",
                PropertyInput::from(vector),
                limit,
                Some(partition.clone().into()),
            )
        } else {
            g().vector_search_nodes_with(
                self.label(),
                "embedding",
                PropertyInput::from(vector),
                limit,
                Some(partition.clone().into()),
            )
        };
        let query = QueryRequest::read(
            read_batch()
                .var_as(
                    "rows",
                    traversal.project_bindings(vec![
                        BindingProjection::current("public_id", "public_id"),
                        BindingProjection::current("partition", "partition"),
                        BindingProjection::current("key", "key"),
                        BindingProjection::current("$distance", "distance"),
                    ]),
                )
                .returning(["rows"]),
        );
        let response = self
            .store
            .inner
            .transport
            .execute(&Operation::new(query).map_err(core)?)
            .map_err(core)?;
        let mut out = vec![];
        for row in crate::schema::rows(&response, "rows").map_err(core)? {
            let id = row["public_id"]
                .as_str()
                .ok_or_else(|| core(Error::IncompatibleSchema))?;
            if row["partition"].as_str() != Some(partition.as_str())
                || row["key"].as_str()
                    != Some(
                        self.store
                            .inner
                            .partition
                            .record_key(&self.label(), id)
                            .map_err(core)?
                            .as_str(),
                    )
            {
                return Err(core(Error::IncompatibleSchema));
            }
            let distance = row["distance"]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| core(Error::IncompatibleSchema))?;
            let payload = self
                .store
                .get::<Value>(&self.label(), id)
                .map_err(core)?
                .ok_or_else(|| core(Error::IncompatibleSchema))?;
            self.checked(payload)?;
            out.push((Id::from(id), 1.0 - distance as f32));
        }
        Ok(out)
    }
}
