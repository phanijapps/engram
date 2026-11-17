//! Exact durable receipt reconciliation; never replay an uncertain mutation.
use crate::{
    HelixStore,
    error::{Error, Result},
    records,
    transport::Operation,
};
use helix_db::dsl::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Clone)]
pub(crate) struct PendingReceipt {
    pub(crate) expected: Value,
    pub(crate) transport_unknown: bool,
}
pub(crate) fn receipt(
    store: &HelixStore,
    id: &str,
    mutations: &BTreeMap<(String, String), Option<Value>>,
) -> Result<Value> {
    let lease = store.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
    let bytes =
        serde_json::to_vec(&mutations.iter().collect::<Vec<_>>()).map_err(|_| Error::Storage)?;
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(
        json!({"id":id,"partition":store.inner.partition.discriminator()?,"owner":lease.token,"epoch":lease.epoch,"fingerprint":digest,"mutation_count":mutations.len()}),
    )
}
impl HelixStore {
    /// Read an acknowledged operation receipt within this exact partition.
    pub fn operation_receipt(&self, operation_id: &str) -> Result<Option<Value>> {
        self.get(records::RECEIPT, operation_id)
    }
    /// Returns the persisted exact receipt and permits owned cleanup only.
    pub fn reconcile_operation(&self, operation_id: &str) -> Result<Value> {
        let lease = self.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
        let partition = self.inner.partition.discriminator()?;
        let request = QueryRequest::write(
            write_batch()
                .var_as(
                    "rows",
                    g().n_with_label_where(
                        records::RECEIPT,
                        SourcePredicate::and(vec![
                            SourcePredicate::eq("partition", partition.clone()),
                            SourcePredicate::eq(
                                "key",
                                self.inner
                                    .partition
                                    .record_key(records::RECEIPT, operation_id)?,
                            ),
                        ]),
                    )
                    .value_map(None::<Vec<String>>),
                )
                .returning(["rows"]),
        );
        let unknown = || Error::OutcomeUnknown {
            operation_id: operation_id.into(),
        };
        let expected = self
            .inner
            .uncertain_receipts
            .lock()
            .map_err(|_| unknown())?
            .get(operation_id)
            .cloned()
            .ok_or_else(unknown)?;
        let operation = Operation::with_id(request, operation_id.into())?;
        // This is the sole adapter-level bypass: the exact partition/key receipt query is read-only.
        let response = if expected.transport_unknown {
            self.inner.transport.reconcile_read(&operation)
        } else {
            self.inner.transport.execute(&operation)
        }
        .map_err(|_| unknown())?;
        let rows = crate::schema::rows(&response, "rows").map_err(|_| unknown())?;
        if rows.len() != 1 {
            return Err(unknown());
        }
        let receipt: Value = serde_json::from_str(
            rows[0]
                .get("payload")
                .and_then(Value::as_str)
                .ok_or_else(unknown)?,
        )
        .map_err(|_| unknown())?;
        if receipt["id"] != operation_id
            || receipt["partition"] != partition
            || receipt["owner"] != lease.token
            || receipt["epoch"] != lease.epoch
            || receipt["fingerprint"]
                .as_str()
                .is_none_or(|v| v.len() != 64)
        {
            return Err(unknown());
        }
        if receipt != expected.expected {
            return Err(unknown());
        }
        if expected.transport_unknown {
            self.inner.transport.confirm_operation(operation_id)?;
        }
        self.inner
            .uncertain_receipts
            .lock()
            .map_err(|_| Error::Storage)?
            .remove(operation_id);
        Ok(receipt)
    }
}
