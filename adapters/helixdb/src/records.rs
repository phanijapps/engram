//! Typed family projections and partition-filtered access to canonical records.
use crate::{
    HelixStore,
    error::{Error, Result},
    schema,
    transport::Operation,
};
use helix_db::dsl::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) const MEMORY: &str = "EngramMemoryV1";
pub(crate) const EVENT: &str = "EngramEventV1";
pub(crate) const SOURCE: &str = "EngramSourceV1";
pub(crate) const DOCUMENT: &str = "EngramDocumentV1";
pub(crate) const CHUNK: &str = "EngramChunkV1";
pub(crate) const ENTITY: &str = "EngramEntityV1";
pub(crate) const RELATIONSHIP: &str = "EngramRelationshipV1";
pub(crate) const GRAPH: &str = "EngramGraphV1";
pub(crate) const UNRESOLVED: &str = "EngramUnresolvedV1";
pub(crate) const ONTOLOGY: &str = "EngramOntologyV1";
pub(crate) const CLASS: &str = "EngramClassV1";
pub(crate) const PROPERTY: &str = "EngramPropertyV1";
pub(crate) const AXIOM: &str = "EngramAxiomV1";
pub(crate) const SCHEME: &str = "EngramSchemeV1";
pub(crate) const CONCEPT: &str = "EngramConceptV1";
pub(crate) const CONCEPT_RELATION: &str = "EngramConceptRelationV1";
pub(crate) const LEDGER: &str = "EngramSourceLedgerV1";
pub(crate) const ENDPOINT: &str = "EngramEndpointV1";
pub(crate) const RECEIPT: &str = "EngramReceiptV1";
pub(crate) const EDGE: &str = "EngramDirectedV1";
pub(crate) const FAMILIES: &[&str] = &[
    MEMORY,
    EVENT,
    SOURCE,
    DOCUMENT,
    CHUNK,
    ENTITY,
    RELATIONSHIP,
    GRAPH,
    UNRESOLVED,
    ONTOLOGY,
    CLASS,
    PROPERTY,
    AXIOM,
    SCHEME,
    CONCEPT,
    CONCEPT_RELATION,
    LEDGER,
    ENDPOINT,
    RECEIPT,
];
pub(crate) fn core(error: Error) -> engram_runtime::CoreError {
    engram_runtime::CoreError::Adapter {
        adapter: "helixdb".into(),
        message: error.to_string(),
    }
}
pub(crate) fn missing(kind: &'static str, id: impl ToString) -> engram_runtime::CoreError {
    engram_runtime::CoreError::NotFound {
        target_type: kind,
        target_id: id.to_string(),
    }
}
pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error::IncompatibleSchema)
}
pub(crate) fn value<T: Serialize>(record: &T) -> Result<Value> {
    serde_json::to_value(record).map_err(|_| Error::Storage)
}
pub(crate) fn id(value: &Value) -> Result<&str> {
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or(Error::InvalidConfig("record identity required"))
}

impl HelixStore {
    pub(crate) fn check_scope(&self, scope: &engram_domain::Scope) -> Result<()> {
        if &self.inner.partition.scope != scope {
            return Err(Error::InvalidConfig(
                "record scope differs from complete store partition",
            ));
        }
        Ok(())
    }
    pub(crate) fn visible_scope(&self, scope: &engram_domain::Scope) -> bool {
        &self.inner.partition.scope == scope
    }
    pub(crate) fn select_values(
        &self,
        family: &str,
        field: Option<(&str, &str)>,
    ) -> Result<Vec<Value>> {
        self.scan_window(family, field, None, None)
    }
    pub(crate) fn scan_window(
        &self,
        family: &str,
        field: Option<(&str, &str)>,
        after: Option<&str>,
        window: Option<(usize, usize)>,
    ) -> Result<Vec<Value>> {
        self.admit_logical_work()?;
        let partition = self.inner.partition.discriminator()?;
        let mut predicates = vec![SourcePredicate::eq("partition", partition.clone())];
        if let Some((name, val)) = field {
            predicates.push(SourcePredicate::eq(name, val));
        }
        if let Some(expected) = &self.step_id {
            let current = self
                .session
                .as_ref()
                .ok_or(Error::Contention)?
                .pending
                .lock()
                .map_err(|_| Error::Storage)?;
            if !current.as_ref().is_some_and(|step| &step.id == expected) {
                return Err(Error::Contention);
            }
        }
        let staged = self
            .session
            .as_ref()
            .is_some_and(|s| s.pending.lock().is_ok_and(|p| p.is_some()));
        let mut traversal = g().n_with_label_where(family, SourcePredicate::and(predicates));
        if let Some(after) = after {
            traversal = traversal.where_(Predicate::gt("public_id", after));
        }
        traversal = traversal.order_by("public_id", Order::Asc);
        if !staged {
            if let Some((offset, limit)) = window {
                traversal = traversal.skip(offset).limit(limit);
            }
        }
        let request = QueryRequest::read(
            read_batch()
                .var_as("rows", traversal.value_map(None::<Vec<String>>))
                .returning(["rows"]),
        );
        let response = self.inner.transport.execute(&Operation::new(request)?)?;
        let mut records = BTreeMap::new();
        for row in schema::rows(&response, "rows")? {
            if row.get("partition").and_then(Value::as_str) != Some(partition.as_str()) {
                return Err(Error::IncompatibleSchema);
            }
            let public_id = row
                .get("public_id")
                .and_then(Value::as_str)
                .ok_or(Error::IncompatibleSchema)?;
            if row.get("key").and_then(Value::as_str)
                != Some(self.inner.partition.record_key(family, public_id)?.as_str())
            {
                return Err(Error::IncompatibleSchema);
            }
            let payload: Value = serde_json::from_str(
                row.get("payload")
                    .and_then(Value::as_str)
                    .ok_or(Error::IncompatibleSchema)?,
            )
            .map_err(|_| Error::IncompatibleSchema)?;
            if let Some(scope) = payload.get("scope") {
                self.check_scope(&decode(scope.clone())?)?;
            }
            if id(&payload)? != public_id || records.insert(public_id.to_owned(), payload).is_some()
            {
                return Err(Error::IncompatibleSchema);
            }
        }
        if let Some(session) = &self.session {
            if let Some(step) = session.pending.lock().map_err(|_| Error::Storage)?.as_ref() {
                for ((label, public_id), mutation) in &step.mutations {
                    if label != family {
                        continue;
                    }
                    match mutation {
                        Some(v)
                            if field.is_none_or(|(name, expected)| {
                                projected(v, name) == Some(expected.to_owned())
                            }) =>
                        {
                            records.insert(public_id.clone(), v.clone());
                        }
                        _ => {
                            records.remove(public_id);
                        }
                    }
                }
            }
        }
        let records = records
            .into_iter()
            .filter(|(id, _)| after.is_none_or(|a| id.as_str() > a))
            .map(|(_, v)| v);
        Ok(if staged {
            if let Some((offset, limit)) = window {
                records.skip(offset).take(limit).collect()
            } else {
                records.collect()
            }
        } else {
            records.collect()
        })
    }
    pub(crate) fn all<T: DeserializeOwned>(&self, family: &str) -> Result<Vec<T>> {
        self.select_values(family, None)?
            .into_iter()
            .map(decode)
            .collect()
    }
    pub(crate) fn filtered<T: DeserializeOwned>(
        &self,
        family: &str,
        field: &str,
        val: &str,
    ) -> Result<Vec<T>> {
        self.select_values(family, Some((field, val)))?
            .into_iter()
            .map(decode)
            .collect()
    }
    pub(crate) fn get<T: DeserializeOwned>(
        &self,
        family: &str,
        public_id: &str,
    ) -> Result<Option<T>> {
        let mut rows = self.select_values(family, Some(("public_id", public_id)))?;
        if rows.len() > 1 {
            return Err(Error::IncompatibleSchema);
        }
        rows.pop().map(decode).transpose()
    }
    pub(crate) fn put<T: Serialize + Clone>(&self, family: &str, record: &T) -> Result<T> {
        let payload = value(record)?;
        self.atomic(|store| {
            store.stage(family, id(&payload)?, Some(payload.clone()))?;
            Ok(())
        })?;
        Ok(record.clone())
    }
    pub(crate) fn stage(
        &self,
        family: &str,
        public_id: &str,
        payload: Option<Value>,
    ) -> Result<()> {
        if !FAMILIES.contains(&family) || public_id.is_empty() {
            return Err(Error::InvalidConfig(
                "unknown family or empty record identity",
            ));
        }
        if let Some(v) = &payload {
            if id(v)? != public_id
                || serde_json::to_vec(v).map_err(|_| Error::Storage)?.len() > 512 * 1024
            {
                return Err(Error::InvalidConfig("invalid bounded record"));
            }
            if let Some(scope) = v.get("scope") {
                self.check_scope(&decode(scope.clone())?)?;
            }
        }
        let state = self.session.as_ref().ok_or(Error::Contention)?;
        #[cfg(test)]
        {
            let pause = state.stage_pause.lock().map_err(|_| Error::Storage)?.take();
            if let Some(pause) = pause {
                pause.entered.wait();
                pause.release.wait();
            }
        }
        let expected = self.step_id.as_ref().ok_or(Error::Contention)?;
        let mut pending = state.pending.lock().map_err(|_| Error::Storage)?;
        pending
            .as_mut()
            .filter(|step| &step.id == expected)
            .ok_or(Error::Contention)?
            .mutations
            .insert((family.to_owned(), public_id.to_owned()), payload);
        Ok(())
    }
    pub(crate) fn remove(&self, family: &str, public_id: &str) -> Result<bool> {
        self.atomic(|store| {
            let exists = store.get::<Value>(family, public_id)?.is_some();
            if exists {
                store.stage(family, public_id, None)?;
            }
            Ok(exists)
        })
    }
    pub(crate) fn endpoint(&self, reference: &engram_domain::EntityRef) -> Result<String> {
        let identity = serde_json::to_string(&(
            reference.id.as_ref().map(ToString::to_string),
            if reference.id.is_none() {
                Some((&reference.name, &reference.kind))
            } else {
                None
            },
        ))
        .map_err(|_| Error::Storage)?;
        let key = self.inner.partition.record_key(ENDPOINT, &identity)?;
        if self.get::<Value>(ENDPOINT, &key)?.is_none() {
            self.stage(
                ENDPOINT,
                &key,
                Some(json!({"id":key,"entity_id":reference.id.as_ref().map(ToString::to_string)})),
            )?;
        }
        Ok(key)
    }
}
pub(crate) fn projected(v: &Value, field: &str) -> Option<String> {
    if field == "public_id" {
        return v.get("id").and_then(Value::as_str).map(str::to_owned);
    }
    if field == "stableSourceKey" {
        return v
            .pointer("/metadata/stableSourceKey")
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    if field == "subject_id" || field == "object_id" {
        return v
            .get(field.split('_').next()?)
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    v.get(field).and_then(Value::as_str).map(str::to_owned)
}
/// Scalar projections preserve nullable state explicitly and keep indexed fields queryable.
pub(crate) fn properties(
    store: &HelixStore,
    family: &str,
    public_id: &str,
    v: &Value,
) -> Result<Vec<(String, PropertyInput)>> {
    let mut fields: BTreeMap<String, PropertyInput> = BTreeMap::new();
    for (key, val) in v.as_object().ok_or(Error::Storage)? {
        let input = match val {
            Value::String(s) => Some(PropertyInput::from(s.clone())),
            Value::Bool(b) => Some(PropertyInput::from(*b)),
            Value::Number(n) => n.as_i64().map(PropertyInput::from),
            _ => None,
        };
        if let Some(input) = input {
            fields.insert(key.clone(), input);
        }
    }
    for name in ["stableSourceKey", "path", "repositoryId", "generationId"] {
        if let Some(v) = v
            .get("metadata")
            .and_then(|m| m.get(name))
            .and_then(Value::as_str)
        {
            fields.insert(name.into(), v.into());
        }
    }
    for name in ["subject", "object"] {
        if let Some(id) = v
            .get(name)
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
        {
            fields.insert(format!("{name}_id"), PropertyInput::from(id));
        }
    }
    for name in ["sourceRefs", "evidence"] {
        if let Some(refs) = v.get(name).and_then(Value::as_array) {
            fields.insert(
                format!("{name}_ids"),
                PropertyInput::from(
                    refs.iter()
                        .filter_map(|r| {
                            r.get("targetId").and_then(Value::as_str).map(str::to_owned)
                        })
                        .collect::<Vec<_>>(),
                ),
            );
        }
    }
    for (name, component) in [
        ("tenant", Some(store.inner.partition.scope.tenant.as_str())),
        ("subject", store.inner.partition.scope.subject.as_deref()),
        (
            "workspace",
            store.inner.partition.scope.workspace.as_deref(),
        ),
        ("session", store.inner.partition.scope.session.as_deref()),
        (
            "environment",
            store.inner.partition.scope.environment.as_deref(),
        ),
    ] {
        fields.insert(format!("scope_{name}_set"), component.is_some().into());
        if let Some(component) = component {
            fields.insert(format!("scope_{name}"), component.into());
        }
    }
    fields.insert(
        "logical_store".into(),
        store.inner.partition.logical_store.clone().into(),
    );
    fields.insert(
        "key".into(),
        store.inner.partition.record_key(family, public_id)?.into(),
    );
    fields.insert(
        "partition".into(),
        store.inner.partition.discriminator()?.into(),
    );
    fields.insert("public_id".into(), public_id.into());
    fields.insert(
        "payload".into(),
        serde_json::to_string(v).map_err(|_| Error::Storage)?.into(),
    );
    Ok(fields.into_iter().collect())
}
