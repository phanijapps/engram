//! Versioned exact partition/public-identity encoding; None differs from an empty value.
use crate::error::{Error, Result};
use engram_domain::{EmbeddingSpace, Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const UPSTREAM_REVISION: &str = "2690416a40177b480014b8b5397426efbd1dc7b8";
#[derive(Clone, Debug)]
pub struct Partition {
    pub logical_store: String,
    pub scope: Scope,
    pub embedding_space: Option<EmbeddingSpace>,
}
impl Partition {
    pub fn discriminator(&self) -> Result<String> {
        if self.logical_store.is_empty() || self.scope.tenant.is_empty() {
            return Err(Error::InvalidConfig(
                "logical store and tenant are required",
            ));
        }
        let mut bytes = vec![1];
        for value in [
            Some(self.logical_store.as_str()),
            Some(self.scope.tenant.as_str()),
            self.scope.subject.as_deref(),
            self.scope.workspace.as_deref(),
            self.scope.session.as_deref(),
            self.scope.environment.as_deref(),
        ] {
            match value {
                None => bytes.push(0),
                Some(value) => {
                    let length = u32::try_from(value.len())
                        .map_err(|_| Error::InvalidConfig("partition component is too long"))?;
                    bytes.push(1);
                    bytes.extend(length.to_be_bytes());
                    bytes.extend(value.as_bytes());
                }
            }
        }
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
    pub fn record_key(&self, family: &str, public_id: &str) -> Result<String> {
        let mut input = self.discriminator()?.into_bytes();
        for value in [family, public_id] {
            input.extend((value.len() as u64).to_be_bytes());
            input.extend(value.as_bytes());
        }
        Ok(Sha256::digest(input)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Marker {
    pub(crate) schema_version: u32,
    pub(crate) encoding_version: u32,
    pub(crate) upstream_revision: String,
    pub(crate) embedding_space: Option<EmbeddingSpace>,
}
impl Marker {
    pub(crate) fn expected(partition: &Partition) -> Self {
        Self {
            schema_version: 1,
            encoding_version: 1,
            upstream_revision: UPSTREAM_REVISION.into(),
            embedding_space: partition.embedding_space.clone(),
        }
    }
    pub(crate) fn validate(&self, partition: &Partition) -> Result<()> {
        if self.schema_version != 1
            || self.encoding_version != 1
            || self.upstream_revision != UPSTREAM_REVISION
            || partition
                .embedding_space
                .as_ref()
                .is_some_and(|space| self.embedding_space.as_ref() != Some(space))
        {
            return Err(Error::IncompatibleSchema);
        }
        Ok(())
    }
}
