//! Pinned Helix storage primitives. Domain port adapters are composed separately.
pub mod codec;
pub mod config;
#[cfg(feature = "embedded")]
mod embedded;
pub mod error;
pub mod ownership;
mod runtime;
mod schema;
#[cfg(feature = "service")]
mod service;
mod transport;

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
}
struct StoreInner {
    transport: Arc<dyn Transport>,
    partition: Partition,
    lease: Option<ScopeLease>,
}
impl HelixStore {
    #[cfg(feature = "embedded")]
    pub fn open_embedded(config: config::EmbeddedConfig, partition: Partition) -> Result<Self> {
        partition.discriminator()?;
        let read_only = config.read_only;
        let transport = Arc::new(embedded::Embedded::open(config, partition.clone())?);
        Self::finish_open(transport, partition, read_only)
    }
    #[cfg(feature = "service")]
    pub fn open_service(config: config::ServiceConfig, partition: Partition) -> Result<Self> {
        partition.discriminator()?;
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
                }),
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
