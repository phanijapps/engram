//! Adapter-neutral runners for accepted memory service contract examples.
//!
//! This module owns only orchestration over the public `MemoryService` trait:
//! load accepted requests, call the service, and return the observed domain
//! responses. It deliberately avoids repository access, event inspection,
//! adapter construction, timing, persistence, and assertion policy so concrete
//! crates can keep their own boundary-specific tests.

use std::sync::Arc;

use engram_core::{CoreError, CoreResult, MemoryService};
use engram_domain::*;

use crate::accepted_examples;

/// Result of running the accepted retrieval example after seeding its write.
#[derive(Debug)]
pub struct RetrievalContractOutcome {
    /// Response returned by the setup write.
    pub write: WriteMemoryResponse,
    /// Context returned by the retrieval request.
    pub context: ContextPayload,
}

/// Runs accepted write/retrieval examples against a `MemoryService`.
///
/// The runner intentionally knows nothing about storage, events, repositories,
/// or adapter construction. Tests for those concerns stay in concrete crates.
pub struct MemoryContractRunner {
    service: Arc<dyn MemoryService>,
}

impl MemoryContractRunner {
    /// Creates a runner over a shared memory service implementation.
    ///
    /// Callers decide how the service is constructed and whether it is fresh or
    /// shared. The runner only stores the trait object needed to execute
    /// accepted contract examples.
    pub fn new(service: Arc<dyn MemoryService>) -> Self {
        Self { service }
    }

    /// Writes the accepted v1 write-memory example through `MemoryService`.
    ///
    /// The parsed request comes from `accepted_examples`, and any adapter
    /// validation or policy failure is returned as the service's `CoreError`.
    pub async fn write_accepted_example(&self) -> CoreResult<WriteMemoryResponse> {
        let request = accepted_examples::write_memory_request().map_err(parse_error)?;
        self.write_request(request).await
    }

    /// Writes a caller-supplied request through the service contract.
    ///
    /// Tests use this for structurally valid negative fixtures that should be
    /// rejected by behavior validation instead of JSON deserialization.
    pub async fn write_request(
        &self,
        request: WriteMemoryRequest,
    ) -> CoreResult<WriteMemoryResponse> {
        self.service.write_memory(request).await
    }

    /// Writes and retrieves the accepted v1 examples as one contract flow.
    ///
    /// The write response and retrieval context are both returned so adapter
    /// tests can add focused assertions without reimplementing fixture loading.
    pub async fn retrieve_accepted_example(&self) -> CoreResult<RetrievalContractOutcome> {
        let write = self.write_accepted_example().await?;
        let request = accepted_examples::retrieval_request().map_err(parse_error)?;
        let context = self.service.retrieve(request).await?;
        Ok(RetrievalContractOutcome { write, context })
    }
}

fn parse_error(error: serde_json::Error) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!("fixture parse failed: {error}"),
    }
}
