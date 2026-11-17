//! Explicit configured HTTP transport; no retries, redirects, ambient proxy or SDK preferences.
use crate::{
    config::ServiceConfig,
    error::{Error, Result},
    runtime::{Admission, Engine, RuntimeOwner},
    transport::{Operation, Transport},
};
use engram_runtime::storage_network_policy::{StorageAccess, with_storage_access};
use serde_json::Value;
use std::{io::Read, sync::Arc};

pub(crate) struct Service {
    owner: Arc<RuntimeOwner>,
}
impl Service {
    pub(crate) fn open(config: ServiceConfig) -> Result<Self> {
        // Canonical strict law is evaluated before credentials, client construction or network.
        with_storage_access(
            StorageAccess::ConfiguredService,
            config.vectors_enabled,
            || Ok(()),
        )
        .map_err(|_| Error::InvalidConfig("PLT-001 prohibits disabled-vector service access"))?;
        let admission = Admission::acquire()?;
        let url = reqwest::Url::parse(&config.endpoint)
            .map_err(|_| Error::InvalidConfig("invalid service endpoint"))?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(Error::InvalidConfig(
                "endpoint must be an explicit HTTP(S) origin without credentials",
            ));
        }
        if url.scheme() == "http"
            && !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
        {
            return Err(Error::InvalidConfig(
                "remote service endpoints require verified TLS",
            ));
        }
        if config.credential_env.is_empty()
            || !config
                .credential_env
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || config.timeout.is_zero()
            || config.timeout > std::time::Duration::from_secs(300)
        {
            return Err(Error::InvalidConfig(
                "explicit credential environment name and bounded timeout required",
            ));
        }
        let owner = admission.start(
            32,
            None,
            config.timeout + std::time::Duration::from_secs(1),
            move || {
                let credential = std::env::var(&config.credential_env).map_err(|_| {
                    Error::InvalidConfig("configured storage credential is unavailable")
                })?;
                if credential.is_empty() || credential.bytes().any(|b| b.is_ascii_control()) {
                    return Err(Error::InvalidConfig(
                        "configured storage credential is invalid",
                    ));
                }
                let client = reqwest::blocking::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .retry(reqwest::retry::never())
                    .timeout(config.timeout)
                    .connect_timeout(config.timeout.min(std::time::Duration::from_secs(10)))
                    .build()
                    .map_err(|_| Error::InvalidConfig("service TLS client construction failed"))?;
                Ok(Box::new(ServiceEngine {
                    client,
                    endpoint: url
                        .join("v2/query")
                        .map_err(|_| Error::InvalidConfig("invalid query route"))?,
                    credential,
                    read_only: config.read_only,
                    cleanup: Vec::new(),
                }))
            },
        )?;
        Ok(Self { owner })
    }
}
impl Transport for Service {
    fn execute(&self, operation: &Operation) -> Result<Value> {
        self.owner.execute(operation)
    }
    fn close(&self) -> Result<()> {
        self.owner.close()
    }
    fn reconcile_read(&self, operation: &Operation) -> Result<Value> {
        self.owner.reconcile_read(operation)
    }
    fn confirm_operation(&self, id: &str) -> Result<()> {
        self.owner.confirm_operation(id)
    }
    fn register_cleanup(&self, lease: crate::ownership::ScopeLease) -> Result<()> {
        self.owner.register_cleanup(lease)
    }
}
struct ServiceEngine {
    client: reqwest::blocking::Client,
    endpoint: reqwest::Url,
    credential: String,
    read_only: bool,
    cleanup: Vec<crate::ownership::ScopeLease>,
}
impl Engine for ServiceEngine {
    fn startup_status(&self) -> Result<()> {
        Ok(())
    }
    fn execute(&mut self, operation: &Operation) -> Result<Value> {
        if self.read_only && operation.write {
            return Err(Error::ReadOnly);
        }
        let request = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.credential)
            .header("content-type", "application/json")
            .header(
                "x-helix-require-writer",
                if operation.write { "true" } else { "false" },
            )
            .body(operation.bytes.clone());
        let request = if operation.write {
            request.header("x-helix-await-durable", "true")
        } else {
            request
        };
        let mut response = request.send().map_err(|e| {
            if e.is_connect() {
                Error::Storage
            } else {
                operation.unknown()
            }
        })?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| operation.unknown())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(operation.unknown());
        }
        if !status.is_success() {
            return Err(
                classify_rejection(status.as_u16(), &bytes).unwrap_or_else(|| operation.unknown())
            );
        }
        serde_json::from_slice(&bytes).map_err(|_| operation.unknown())
    }
    fn register_cleanup(&mut self, lease: crate::ownership::ScopeLease) -> Result<()> {
        if self.cleanup.len() >= 1024 {
            return Err(Error::InvalidConfig("scope cleanup budget exceeded"));
        }
        self.cleanup.push(lease);
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        while let Some(lease) = self.cleanup.pop() {
            if crate::ownership::release(&lease, &mut |operation| self.execute(operation)).is_err()
            {
                self.cleanup.push(lease);
                return Err(Error::CleanupUnconfirmed);
            }
        }
        Ok(())
    }
}

/// Definite rejection requires the complete pinned structured envelope, not HTTP status alone.
pub(crate) fn classify_rejection(status: u16, bytes: &[u8]) -> Option<Error> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Failure {
        error: helix_db::QueryErrorCode,
        msg: String,
        retryable: Option<bool>,
    }
    let failure: Failure = serde_json::from_slice(bytes).ok()?;
    if failure.msg.is_empty() {
        return None;
    }
    if status == 409
        && failure.error == helix_db::QueryErrorCode::TransactionConflict
        && failure.retryable.is_none()
    {
        Some(Error::Contention)
    } else if status == 429
        && failure.error.is_retryable_backpressure()
        && failure.retryable == Some(true)
    {
        Some(Error::Backpressure)
    } else {
        None
    }
}
