use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct WriterLock {
    _file: File,
}
impl WriterLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(Error::InvalidConfig(
                "writer lock must not follow a symlink",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        let file = options.open(path).map_err(|_| Error::Storage)?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| Error::Contention)?;
        Ok(Self { _file: file })
    }
}

use crate::{
    codec::Partition,
    schema,
    transport::{Operation, Transport},
};
use helix_db::dsl::prelude::*;
use serde_json::Value;

pub(crate) const LOCK_LABEL: &str = "EngramScopeOwnerV1";
#[derive(Clone)]
pub(crate) struct ScopeLease {
    pub(crate) key: String,
    pub(crate) token: String,
    pub(crate) epoch: i64,
    pub(crate) partition: String,
}
impl ScopeLease {
    pub(crate) fn predicate(&self) -> SourcePredicate {
        SourcePredicate::and(vec![
            SourcePredicate::eq("key", self.key.clone()),
            SourcePredicate::eq("partition", self.partition.clone()),
            SourcePredicate::eq("owner", self.token.clone()),
            SourcePredicate::eq("epoch", self.epoch),
        ])
    }
    pub(crate) fn guard(&self) -> WriteBatch {
        write_batch().var_as(
            "owner",
            g().n_with_label_where(LOCK_LABEL, self.predicate()),
        )
    }
}

pub(crate) fn acquire(transport: &dyn Transport, partition: &Partition) -> Result<ScopeLease> {
    let key = partition.record_key(LOCK_LABEL, "scope-owner")?;
    let read = read_batch()
        .var_as(
            "lock",
            g().n_with_label_where(LOCK_LABEL, SourcePredicate::eq("key", key.clone()))
                .value_map(None::<Vec<String>>),
        )
        .returning(["lock"]);
    let current = transport.execute(&Operation::new(QueryRequest::read(read))?)?;
    let rows = schema::rows(&current, "lock")?;
    if rows.len() > 1 {
        return Err(Error::IncompatibleSchema);
    }
    let expected_partition = partition.discriminator()?;
    let previous = rows.first();
    if let Some(row) = previous {
        validate_owner_row(row, &key, &expected_partition)?;
    }
    if previous
        .and_then(|v| v.get("owner"))
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty())
    {
        return Err(Error::Contention);
    }
    let prior = previous
        .and_then(|v| v.get("epoch"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let lease = ScopeLease {
        key: key.clone(),
        token: uuid::Uuid::new_v4().to_string(),
        epoch: prior.checked_add(1).ok_or(Error::IncompatibleSchema)?,
        partition: expected_partition,
    };
    // Register cleanup before acquisition can have an uncertain outcome.
    transport.register_cleanup(lease.clone())?;
    let request = if previous.is_some() {
        write_batch()
            .var_as(
                "eligible",
                g().n_with_label_where(
                    LOCK_LABEL,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq("key", key),
                        SourcePredicate::eq("owner", ""),
                        SourcePredicate::eq("epoch", prior),
                    ]),
                ),
            )
            .var_as_if(
                "owned",
                BatchCondition::VarMinSize("eligible".into(), 1),
                g().n(NodeRef::var("eligible"))
                    .set_property("owner", lease.token.clone())
                    .set_property("epoch", lease.epoch),
            )
            .returning(["owned"])
    } else {
        write_batch()
            .var_as(
                "existing",
                g().n_with_label_where(LOCK_LABEL, SourcePredicate::eq("key", key.clone())),
            )
            .var_as_if(
                "owned",
                BatchCondition::VarEmpty("existing".into()),
                g().add_n(
                    LOCK_LABEL,
                    vec![
                        ("key", PropertyInput::from(key)),
                        ("owner", PropertyInput::from(lease.token.clone())),
                        ("epoch", PropertyInput::from(lease.epoch)),
                        ("partition", PropertyInput::from(partition.discriminator()?)),
                    ],
                ),
            )
            .returning(["owned"])
    };
    let response = transport.execute(&Operation::new(QueryRequest::write(request))?)?;
    if response
        .get("owned")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        return Err(Error::Contention);
    }
    Ok(lease)
}

/// Idempotent owned cleanup: inspect authoritative state before any release retry.
pub(crate) fn release(
    lease: &ScopeLease,
    execute: &mut impl FnMut(&Operation) -> Result<Value>,
) -> Result<()> {
    let read = QueryRequest::read(
        read_batch()
            .var_as(
                "lock",
                g().n_with_label_where(LOCK_LABEL, SourcePredicate::eq("key", lease.key.clone()))
                    .value_map(None::<Vec<String>>),
            )
            .returning(["lock"]),
    );
    let response = execute(&Operation::new(read)?)?;
    let rows = schema::rows(&response, "lock")?;
    if rows.len() > 1 {
        return Err(Error::CleanupUnconfirmed);
    }
    let Some(row) = rows.first() else {
        return Ok(());
    };
    validate_owner_row(row, &lease.key, &lease.partition)?;
    if row.get("owner").and_then(Value::as_str) != Some(lease.token.as_str())
        || row.get("epoch").and_then(Value::as_i64) != Some(lease.epoch)
    {
        // It is absent/released or belongs to someone else; never release another owner.
        return Ok(());
    }
    let request = lease
        .guard()
        .var_as_if(
            "released",
            BatchCondition::VarMinSize("owner".into(), 1),
            g().n(NodeRef::var("owner")).set_property("owner", ""),
        )
        .returning(["released"]);
    let response = execute(&Operation::new(QueryRequest::write(request))?)?;
    if response
        .get("released")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        return Err(Error::CleanupUnconfirmed);
    }
    Ok(())
}

fn validate_owner_row(row: &Value, key: &str, partition: &str) -> Result<()> {
    if row.get("key").and_then(Value::as_str) != Some(key)
        || row.get("partition").and_then(Value::as_str) != Some(partition)
        || row.get("owner").and_then(Value::as_str).is_none()
        || !row
            .get("epoch")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0)
    {
        return Err(Error::IncompatibleSchema);
    }
    Ok(())
}
