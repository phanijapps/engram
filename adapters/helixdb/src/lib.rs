//! Pinned Helix storage primitives. Domain port adapters are composed separately.
pub mod beliefs;
pub mod changes;
pub mod code_binding;
mod code_generation;
mod code_leases;
mod code_retirement;
pub mod codec;
pub mod config;
#[cfg(feature = "embedded")]
mod embedded;
pub mod error;
mod graph;
pub mod hierarchy;
mod identity;
pub mod inventory;
mod knowledge;
pub mod maintenance;
mod memory;
pub mod outcomes;
pub mod ownership;
pub mod procedures;
pub mod proposals;
pub mod receipts;
mod records;
pub mod retrieval;
mod runtime;
mod schema;
#[cfg(feature = "service")]
mod service;
pub mod session;
pub mod source_ledger;
pub mod transaction;
mod transport;
mod vocabulary;

pub mod vector;

use codec::Partition;
use error::{Error, Result};
use ownership::ScopeLease;
pub use runtime::retry_retained_cleanup;
use std::sync::Arc;
use transport::Transport;

/// A partition-bound primitive handle; clones retain its physical runtime owner.
#[derive(Clone)]
pub struct HelixStore {
    inner: Arc<StoreInner>,
    session: Option<Arc<session::SessionState>>,
    reentrant: bool,
    step_id: Option<String>,
}
struct StoreInner {
    transport: Arc<dyn Transport>,
    partition: Partition,
    lease: Option<ScopeLease>,
    reservation: std::sync::atomic::AtomicBool,
    cleanup_only: std::sync::atomic::AtomicBool,
    uncertain_receipts:
        std::sync::Mutex<std::collections::BTreeMap<String, receipts::PendingReceipt>>,
}
impl HelixStore {
    #[cfg(feature = "embedded")]
    pub fn open_embedded(config: config::EmbeddedConfig, partition: Partition) -> Result<Self> {
        partition.discriminator()?;
        if let Some(space) = &partition.embedding_space {
            vector::validate_config(space)?;
        }
        let read_only = config.read_only;
        let transport = Arc::new(embedded::Embedded::open(config, partition.clone())?);
        Self::finish_open(transport, partition, read_only)
    }
    #[cfg(feature = "service")]
    pub fn open_service(config: config::ServiceConfig, partition: Partition) -> Result<Self> {
        partition.discriminator()?;
        if let Some(space) = &partition.embedding_space {
            vector::validate_config(space)?;
        }
        let read_only = config.read_only;
        let transport = Arc::new(service::Service::open(config)?);
        Self::finish_open(transport, partition, read_only)
    }
    fn finish_open(
        transport: Arc<dyn Transport>,
        partition: Partition,
        read_only: bool,
    ) -> Result<Self> {
        partition.discriminator()?;
        let result = (|| {
            // Reject an incompatible marker before initialization or scope-lock mutation.
            let present = schema::check_marker(transport.as_ref(), &partition)?;
            if read_only {
                if !present {
                    return Err(Error::IncompatibleSchema);
                }
                return Ok(None);
            }
            // The unique scope-owner constraint is the sole bootstrap prerequisite.
            schema::ensure_index(transport.as_ref(), ownership::LOCK_LABEL, None)?;
            let lease = ownership::acquire(transport.as_ref(), &partition)?;
            schema::initialize(transport.as_ref(), &partition, &lease)?;
            Ok(Some(lease))
        })();
        match result {
            Ok(lease) => Ok(Self {
                inner: Arc::new(StoreInner {
                    transport,
                    partition,
                    lease,
                    reservation: std::sync::atomic::AtomicBool::new(false),
                    cleanup_only: std::sync::atomic::AtomicBool::new(false),
                    uncertain_receipts: std::sync::Mutex::new(std::collections::BTreeMap::new()),
                }),
                session: None,
                reentrant: false,
                step_id: None,
            }),
            Err(error) => {
                if transport.close().is_err() {
                    return Err(Error::CleanupAfterFailure {
                        cause: Box::new(error),
                    });
                }
                Err(error)
            }
        }
    }
    pub fn put_sentinel(&self, public_id: &str, payload: &str) -> Result<()> {
        if self.session.is_some() {
            return Err(Error::InvalidConfig(
                "sentinel writes cannot join domain sessions",
            ));
        }
        let _session = self.scope_session()?;
        let lease = self.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
        schema::write_payload(
            self.inner.transport.as_ref(),
            &self.inner.partition,
            lease,
            schema::SENTINEL_LABEL,
            public_id,
            payload.into(),
        )
    }
    pub fn get_sentinel(&self, public_id: &str) -> Result<Option<String>> {
        self.admit_logical_work()?;
        schema::read_payload(
            self.inner.transport.as_ref(),
            &self.inner.partition,
            schema::SENTINEL_LABEL,
            public_id,
        )
    }
    /// Reconcile an uncertain sentinel write using its persisted exact operation receipt.
    /// Successful reconciliation permits cleanup only, never implicit logical replay.
    pub fn reconcile_sentinel(&self, public_id: &str, operation_id: &str) -> Result<()> {
        self.admit_logical_work()?;
        schema::reconcile_sentinel(
            self.inner.transport.as_ref(),
            &self.inner.partition,
            public_id,
            operation_id,
        )
    }
    pub fn close(&self) -> Result<()> {
        self.inner.transport.close()
    }
}

#[cfg(all(test, any(feature = "embedded", feature = "service")))]
#[path = "../tests/support/data_fixture.rs"]
mod test_data_fixture;

#[cfg(test)]
extern crate self as engram_store_helixdb;
