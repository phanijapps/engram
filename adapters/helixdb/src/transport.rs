//! One validated SDK operation-tree envelope for both transports.
use crate::error::{Error, Result};
use helix_db::{QueryRequest, QueryRequestType};
use serde_json::Value;

#[cfg(feature = "test-controls")]
pub(crate) type ReceiptValidator = std::sync::Arc<dyn Fn(&Value) -> bool + Send + Sync>;
#[cfg(feature = "test-controls")]
pub(crate) fn receipt_hash(value: &Value) -> Option<String> {
    use sha2::{Digest, Sha256};
    serde_json::to_vec(value).ok().map(|bytes| {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
}

#[derive(Clone)]
pub(crate) struct Operation {
    pub(crate) id: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) write: bool,
    pub(crate) mutates: bool,
}
impl Operation {
    pub(crate) fn new(request: QueryRequest) -> Result<Self> {
        Self::with_id(request, uuid::Uuid::new_v4().to_string())
    }
    pub(crate) fn with_id(request: QueryRequest, id: String) -> Result<Self> {
        let write = request.request_type() == QueryRequestType::Write;
        let mutates = match request.query() {
            helix_db::BatchQuery::Read(_) => false,
            helix_db::BatchQuery::Write(batch) => {
                helix_db::ReadBatch::try_from_parts(batch.entries.clone(), batch.returns.clone())
                    .is_err()
            }
        };
        let bytes = serde_json::to_vec(&request)
            .map_err(|_| Error::InvalidConfig("invalid operation tree"))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Error::InvalidConfig("operation tree exceeds bound"));
        }
        // Round-trip through the pinned parser, including its read/write mutation validation.
        let _: QueryRequest = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidConfig("invalid operation tree"))?;
        Ok(Self {
            id,
            bytes,
            write,
            mutates,
        })
    }
    pub(crate) fn unknown(&self) -> Error {
        if self.mutates {
            Error::OutcomeUnknown {
                operation_id: self.id.clone(),
            }
        } else {
            Error::Storage
        }
    }
}

pub(crate) trait Transport: Send + Sync {
    #[cfg(feature = "test-controls")]
    fn fixture_observed(&self, _observation: engram_runtime::effect::EffectObservation) {}
    #[cfg(feature = "test-controls")]
    fn fixture_bind_effects(
        &self,
        _operation_id: &str,
        _effects: Vec<engram_runtime::effect::EffectObservation>,
        _receipt_hash: Option<String>,
        _receipt_context: Value,
        _validate_receipt: ReceiptValidator,
    ) {
    }
    #[cfg(feature = "test-controls")]
    fn fixture_after_ack(
        &self,
        _operation_id: &str,
        _effects: Vec<engram_runtime::effect::EffectObservation>,
    ) {
    }
    fn execute(&self, operation: &Operation) -> Result<Value>;
    fn close(&self) -> Result<()>;
    fn reconcile_read(&self, operation: &Operation) -> Result<Value>;
    fn confirm_operation(&self, id: &str) -> Result<()>;
    fn register_cleanup(&self, lease: crate::ownership::ScopeLease) -> Result<()>;
}
