//! Owned schema marker and index readiness; readers never initialize state.
use crate::{
    codec::{Marker, Partition},
    error::{Error, Result},
    ownership::ScopeLease,
    transport::{Operation, Transport},
};
use helix_db::dsl::prelude::*;
use serde_json::Value;

pub(crate) const MARKER_LABEL: &str = "EngramSchemaV1";
pub(crate) const SENTINEL_LABEL: &str = "EngramSentinelV1";

pub(crate) fn rows<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>> {
    value
        .get(name)
        .and_then(Value::as_array)
        .ok_or(Error::Storage)
}

fn payload_read_operation(partition: &Partition, label: &str, id: &str) -> Result<Operation> {
    let request = QueryRequest::read(
        read_batch()
            .var_as(
                "rows",
                g().n_with_label_where(
                    label,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq("key", partition.record_key(label, id)?),
                        SourcePredicate::eq("partition", partition.discriminator()?),
                    ]),
                )
                .value_map(Some(vec!["payload", "public_id", "partition"])),
            )
            .returning(["rows"]),
    );
    Operation::new(request)
}
fn payload_from_response(
    response: &Value,
    partition: &Partition,
    id: &str,
) -> Result<Option<String>> {
    let values = rows(response, "rows")?;
    if values.len() > 1 {
        return Err(Error::IncompatibleSchema);
    }
    values
        .first()
        .map(|value| {
            if value.get("public_id").and_then(Value::as_str) != Some(id)
                || value.get("partition").and_then(Value::as_str)
                    != Some(partition.discriminator()?.as_str())
            {
                return Err(Error::IncompatibleSchema);
            }
            value
                .get("payload")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(Error::IncompatibleSchema)
        })
        .transpose()
}

pub(crate) fn read_payload(
    transport: &dyn Transport,
    partition: &Partition,
    label: &str,
    id: &str,
) -> Result<Option<String>> {
    let response = transport.execute(&payload_read_operation(partition, label, id)?)?;
    payload_from_response(&response, partition, id)
}

pub(crate) fn write_payload(
    transport: &dyn Transport,
    partition: &Partition,
    lease: &ScopeLease,
    label: &str,
    id: &str,
    payload: String,
) -> Result<()> {
    if id.is_empty() || payload.len() > 512 * 1024 {
        return Err(Error::InvalidConfig(
            "bounded public identity/payload required",
        ));
    }
    let key = partition.record_key(label, id)?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let request = lease
        .guard()
        .var_as(
            "existing",
            g().n_with_label_where(label, SourcePredicate::eq("key", key.clone())),
        )
        .var_as_if(
            "updated",
            BatchCondition::VarMinSize("owner".into(), 1),
            g().n(NodeRef::var("existing"))
                .set_property("payload", payload.clone())
                .set_property("last_operation", operation_id.clone()),
        )
        // Both owner and absence conditions must hold, using a filtered owner variable.
        .var_as_if(
            "create_owner",
            BatchCondition::VarEmpty("existing".into()),
            g().n(NodeRef::var("owner")),
        )
        .var_as_if(
            "created",
            BatchCondition::VarMinSize("create_owner".into(), 1),
            g().add_n(
                label,
                vec![
                    ("key", PropertyInput::from(key)),
                    ("partition", PropertyInput::from(partition.discriminator()?)),
                    ("public_id", PropertyInput::from(id)),
                    ("payload", PropertyInput::from(payload)),
                    ("last_operation", PropertyInput::from(operation_id.clone())),
                ],
            ),
        )
        .returning(["updated", "created"]);
    let response = transport.execute(&Operation::with_id(
        QueryRequest::write(request),
        operation_id,
    )?)?;
    let changed = ["updated", "created"].iter().any(|name| {
        response
            .get(name)
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
    });
    if !changed {
        return Err(Error::Contention);
    }
    Ok(())
}

pub(crate) fn ensure_index(
    transport: &dyn Transport,
    label: &str,
    lease: Option<&ScopeLease>,
) -> Result<()> {
    let spec = IndexSpec::node_unique_equality(label, "key");
    let batch = match lease {
        Some(lease) => lease.guard().var_as_if(
            "index",
            BatchCondition::VarMinSize("owner".into(), 1),
            g().create_index_if_not_exists(spec),
        ),
        None => write_batch().var_as("index", g().create_index_if_not_exists(spec)),
    }
    .returning(["index"]);
    let response = transport.execute(&Operation::new(QueryRequest::write(batch))?)?;
    let receipt = response.get("index").ok_or(Error::Storage)?;
    if receipt.get("kind").and_then(Value::as_str) == Some("already_active") {
        return Ok(());
    }
    let operation = receipt
        .get("operation_id")
        .and_then(Value::as_str)
        .ok_or(Error::Storage)?
        .to_owned();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let request = QueryRequest::read(
            read_batch()
                .var_as("status", g().get_index_operation(operation.clone()))
                .returning(["status"]),
        );
        let status = transport.execute(&Operation::new(request)?)?;
        match status
            .get("status")
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str)
        {
            Some("succeeded") => return Ok(()),
            Some("queued" | "running") if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            _ => return Err(Error::IncompatibleSchema),
        }
    }
}

pub(crate) fn check_marker(transport: &dyn Transport, partition: &Partition) -> Result<bool> {
    check_marker_using(partition, |operation| transport.execute(operation))
}

pub(crate) fn check_marker_using(
    partition: &Partition,
    execute: impl FnOnce(&Operation) -> Result<Value>,
) -> Result<bool> {
    let response = execute(&payload_read_operation(partition, MARKER_LABEL, "schema")?)?;
    let Some(payload) = payload_from_response(&response, partition, "schema")? else {
        return Ok(false);
    };
    let marker: Marker = serde_json::from_str(&payload).map_err(|_| Error::IncompatibleSchema)?;
    marker.validate(partition)?;
    Ok(true)
}

pub(crate) fn initialize(
    transport: &dyn Transport,
    partition: &Partition,
    lease: &ScopeLease,
) -> Result<()> {
    ensure_index(transport, MARKER_LABEL, Some(lease))?;
    ensure_index(transport, SENTINEL_LABEL, Some(lease))?;
    if !check_marker(transport, partition)? {
        write_payload(
            transport,
            partition,
            lease,
            MARKER_LABEL,
            "schema",
            serde_json::to_string(&Marker::expected(partition)).map_err(|_| Error::Storage)?,
        )?;
    }
    if !check_marker(transport, partition)? {
        return Err(Error::IncompatibleSchema);
    }
    Ok(())
}

pub(crate) fn reconcile_sentinel(
    transport: &dyn Transport,
    partition: &Partition,
    id: &str,
    operation_id: &str,
) -> Result<()> {
    let request = QueryRequest::write(
        write_batch()
            .var_as(
                "rows",
                g().n_with_label_where(
                    SENTINEL_LABEL,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq("key", partition.record_key(SENTINEL_LABEL, id)?),
                        SourcePredicate::eq("partition", partition.discriminator()?),
                    ]),
                )
                .value_map(Some(vec!["public_id", "partition", "last_operation"])),
            )
            .returning(["rows"]),
    );
    let response = transport.reconcile_read(&Operation::with_id(request, operation_id.into())?)?;
    let values = rows(&response, "rows")?;
    if values.len() != 1
        || values[0].get("public_id").and_then(Value::as_str) != Some(id)
        || values[0].get("partition").and_then(Value::as_str)
            != Some(partition.discriminator()?.as_str())
        || values[0].get("last_operation").and_then(Value::as_str) != Some(operation_id)
    {
        return Err(Error::OutcomeUnknown {
            operation_id: operation_id.into(),
        });
    }
    transport.confirm_operation(operation_id)
}
