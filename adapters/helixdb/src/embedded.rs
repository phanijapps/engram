//! Real telemetry-free local engine, entirely owned by its dedicated worker.
use crate::{
    config::{EmbeddedConfig, confined_path},
    error::{Error, Result},
    ownership::WriterLock,
    runtime::{Admission, Engine, RuntimeOwner},
    transport::{Operation, Transport},
};
use helix_db::{HelixDB, HelixDbSource};
use serde_json::Value;
use std::sync::Arc;

#[cfg(test)]
mod native_diagnostic {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static PHASES: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
    pub(super) fn record(id: &str, phase: &str, error: Option<String>) {
        if !ENABLED.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut phases) = PHASES.lock() {
            if phases.len() < 24 {
                phases.push(serde_json::json!({"operation_id":id,"phase":phase,
                    "native_error":error.map(|s| s.chars().take(512).collect::<String>())}));
            }
        }
    }
    pub(crate) fn begin() {
        PHASES.lock().unwrap().clear();
        ENABLED.store(true, Ordering::Release);
    }
    pub(crate) fn finish() -> Vec<serde_json::Value> {
        ENABLED.store(false, Ordering::Release);
        std::mem::take(&mut *PHASES.lock().unwrap())
    }
}
#[cfg(test)]
pub(crate) use native_diagnostic::{
    begin as begin_native_diagnostic, finish as finish_native_diagnostic,
};

pub(crate) struct Embedded {
    owner: Arc<RuntimeOwner>,
}
impl Embedded {
    pub(crate) fn open(config: EmbeddedConfig, partition: crate::codec::Partition) -> Result<Self> {
        if config.queue_capacity == 0 || config.queue_capacity > 1024 {
            return Err(Error::InvalidConfig("invalid runtime queue capacity"));
        }
        let admission = Admission::acquire()?;
        let path = confined_path(&config.trusted_root, &config.path)?;
        if config.read_only {
            if !path.is_dir() {
                return Err(Error::InvalidConfig("read-only store must already exist"));
            }
        } else {
            std::fs::create_dir_all(&path).map_err(|_| Error::Storage)?;
        }
        reject_unsafe_tree(&path.join("engram-v1"))?;
        // Revalidate internal lock path before opening it; never follow a redirected lock.
        let lock = if config.read_only {
            None
        } else {
            let lock_path = path.join("engram-writer.lock");
            let checked = confined_path(&config.trusted_root, &lock_path)?;
            if checked != lock_path
                || std::fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(Error::InvalidConfig("writer lock must not be a symlink"));
            }
            Some(WriterLock::acquire(&lock_path)?)
        };
        let capacity = config.queue_capacity;
        let owner = admission.start(
            capacity,
            lock,
            std::time::Duration::from_secs(30),
            move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .max_blocking_threads(4)
                    .thread_stack_size(16 * 1024 * 1024)
                    .enable_all()
                    .build()
                    .map_err(|_| Error::Storage)?;
                Ok(Box::new(LocalEngine::start(
                    runtime,
                    path,
                    config.read_only,
                    &partition,
                )))
            },
        )?;
        Ok(Self { owner })
    }
}
impl Transport for Embedded {
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

struct LocalEngine {
    runtime: tokio::runtime::Runtime,
    db: Option<HelixDB>,
    opaque_open: bool,
    startup_error: Option<Error>,
    path: std::path::PathBuf,
    read_only: bool,
    cleanup: Vec<crate::ownership::ScopeLease>,
}
impl LocalEngine {
    fn start(
        runtime: tokio::runtime::Runtime,
        path: std::path::PathBuf,
        read_only: bool,
        partition: &crate::codec::Partition,
    ) -> Self {
        let mut engine = Self {
            runtime,
            db: None,
            opaque_open: false,
            startup_error: None,
            path,
            read_only,
            cleanup: Vec::new(),
        };
        let source = || HelixDbSource::Disk {
            root: engine.path.clone(),
            database: "engram-v1".into(),
        };
        let existing = engine.path.join("engram-v1").exists();
        if !read_only && existing {
            let input = source();
            match engine.runtime.block_on(HelixDB::open_reader_for_server(
                input.clone(),
                input.embedded_default_config(),
            )) {
                Ok(reader) => engine.db = Some(reader),
                Err(_) => {
                    engine.opaque_open = true;
                    return engine;
                }
            }
            // Parent retains the OS writer lock. This path never constructs a native writer.
            engine.read_only = true;
            let checked = crate::schema::check_marker_using(partition, |operation| {
                engine.query_current(operation)
            });
            let closed = engine
                .runtime
                .block_on(engine.db.as_ref().expect("owned preflight reader").close());
            if closed.is_err() {
                engine.startup_error = Some(match checked {
                    Err(error) => Error::CleanupAfterFailure {
                        cause: Box::new(error),
                    },
                    Ok(_) => Error::CleanupUnconfirmed,
                });
                return engine;
            }
            engine.db.take();
            if let Err(error) = checked {
                engine.startup_error = Some(error);
                return engine;
            }
            engine.read_only = false;
        }
        let input = HelixDbSource::Disk {
            root: engine.path.clone(),
            database: "engram-v1".into(),
        };
        let tuning = input.embedded_default_config();
        let opened = engine.runtime.block_on(async {
            if read_only {
                HelixDB::open_reader_for_server(input, tuning).await
            } else {
                HelixDB::open_for_server(input, tuning).await
            }
        });
        match opened {
            Ok(db) => engine.db = Some(db),
            Err(_) => engine.opaque_open = true,
        }
        engine
    }
    fn query_current(&mut self, operation: &Operation) -> Result<Value> {
        let db = self.db.as_ref().ok_or(Error::CleanupUnconfirmed)?;
        #[cfg(test)]
        native_diagnostic::record(&operation.id, "query_begin", None);
        let bytes = self
            .runtime
            .block_on(db.query_json(&operation.bytes))
            .map_err(|error| {
                #[cfg(test)]
                native_diagnostic::record(&operation.id, "query_error", Some(format!("{error:?}")));
                if error.is_index_backpressure() {
                    Error::Backpressure
                } else if error.is_transaction_conflict() {
                    Error::Contention
                } else {
                    operation.unknown()
                }
            })?;
        #[cfg(test)]
        native_diagnostic::record(&operation.id, "query_ok", None);
        if operation.write {
            #[cfg(test)]
            native_diagnostic::record(&operation.id, "flush_begin", None);
            self.runtime.block_on(db.flush_writer()).map_err(|error| {
                #[cfg(test)]
                native_diagnostic::record(&operation.id, "flush_error", Some(format!("{error:?}")));
                let _ = error;
                operation.unknown()
            })?;
            #[cfg(test)]
            native_diagnostic::record(&operation.id, "flush_ok", None);
        }
        #[cfg(test)]
        native_diagnostic::record(&operation.id, "parse_begin", None);
        serde_json::from_slice(&bytes)
            .map_err(|error| {
                #[cfg(test)]
                native_diagnostic::record(&operation.id, "parse_error", Some(format!("{error:?}")));
                let _ = error;
                operation.unknown()
            })
            .inspect(|_| {
                #[cfg(test)]
                native_diagnostic::record(&operation.id, "parse_ok", None);
            })
    }
}

impl Engine for LocalEngine {
    fn startup_status(&self) -> Result<()> {
        if self.opaque_open {
            Err(Error::CleanupUnconfirmed)
        } else if let Some(error) = &self.startup_error {
            Err(error.clone())
        } else {
            Ok(())
        }
    }
    fn execute(&mut self, operation: &Operation) -> Result<Value> {
        if self.opaque_open {
            return Err(Error::CleanupUnconfirmed);
        }
        if self.read_only && operation.write {
            return Err(Error::ReadOnly);
        }
        if self.read_only {
            // A fresh read handle observes acknowledged published state without opening a writer.
            self.runtime
                .block_on(self.db.as_ref().ok_or(Error::CleanupUnconfirmed)?.close())
                .map_err(|_| Error::CleanupUnconfirmed)?;
            self.db.take();
            let source = HelixDbSource::Disk {
                root: self.path.clone(),
                database: "engram-v1".into(),
            };
            match self.runtime.block_on(HelixDB::open_reader_for_server(
                source.clone(),
                source.embedded_default_config(),
            )) {
                Ok(db) => self.db = Some(db),
                Err(_) => {
                    // Upstream can swallow close-on-open failure; keep this runtime owned indefinitely.
                    self.opaque_open = true;
                    return Err(Error::CleanupUnconfirmed);
                }
            }
        }
        self.query_current(operation)
    }

    fn register_cleanup(&mut self, lease: crate::ownership::ScopeLease) -> Result<()> {
        if self.cleanup.len() >= 1024 {
            return Err(Error::InvalidConfig("scope cleanup budget exceeded"));
        }
        self.cleanup.push(lease);
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        if self.opaque_open {
            return Err(Error::CleanupUnconfirmed);
        }
        while let Some(lease) = self.cleanup.pop() {
            if crate::ownership::release(&lease, &mut |operation| self.execute(operation)).is_err()
            {
                self.cleanup.push(lease);
                return Err(Error::CleanupUnconfirmed);
            }
        }

        if self.db.is_none() {
            return Ok(());
        }
        if !self.read_only {
            self.runtime
                .block_on(
                    self.db
                        .as_ref()
                        .ok_or(Error::CleanupUnconfirmed)?
                        .flush_writer(),
                )
                .map_err(|_| Error::CleanupUnconfirmed)?;
        }
        self.runtime
            .block_on(self.db.as_ref().ok_or(Error::CleanupUnconfirmed)?.close())
            .map_err(|_| Error::CleanupUnconfirmed)
    }
}

fn reject_unsafe_tree(path: &std::path::Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(Error::Storage),
    };
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err(Error::InvalidConfig(
            "engine-owned storage contains an unsafe link or special file",
        ));
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|_| Error::Storage)? {
            reject_unsafe_tree(&entry.map_err(|_| Error::Storage)?.path())?;
        }
    }
    Ok(())
}
