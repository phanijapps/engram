//! One validated SDK operation-tree envelope for both transports.
use crate::error::{Error, Result};
use helix_db::{QueryRequest, QueryRequestType};
use serde_json::Value;

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
    fn execute(&self, operation: &Operation) -> Result<Value>;
    fn close(&self) -> Result<()>;
    fn reconcile_read(&self, operation: &Operation) -> Result<Value>;
    fn confirm_operation(&self, id: &str) -> Result<()>;
    fn register_cleanup(&self, lease: crate::ownership::ScopeLease) -> Result<()>;
}
