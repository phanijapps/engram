//! Current PLT-001 storage boundary, before the separately ratified amendment applies.
use engram_runtime::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAccess {
    LocalDisk,
    ConfiguredService,
}

/// Reject prohibited access before the caller resolves credentials or opens storage.
/// This does not authorize an unapplied configured-storage exception.
pub fn with_storage_access<T>(
    access: StorageAccess,
    vectors_enabled: bool,
    open: impl FnOnce() -> CoreResult<T>,
) -> CoreResult<T> {
    if !vectors_enabled && access == StorageAccess::ConfiguredService {
        return Err(CoreError::InvalidRequest {
            reason: "PLT-001 requires disabled-vector storage access to remain offline".into(),
        });
    }
    open()
}
