//! Bounded journal-mode setup before knowledge schema migration.
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, Error, ErrorCode};
use std::time::{Duration, Instant};

pub(super) fn journal_mode(
    conn: &Connection,
    pragma: &str,
    timeout_ms: Option<u64>,
) -> CoreResult<()> {
    let deadline = Instant::now();
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(0));
    loop {
        match conn.query_row(pragma, [], |_| Ok(())) {
            Ok(()) => return Ok(()),
            // Journal-mode transitions can fail immediately under concurrent
            // openers. Retry the completed statement, never a partial migration.
            Err(Error::SqliteFailure(error, _))
                if matches!(
                    error.code,
                    ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
                ) && deadline.elapsed() < timeout =>
            {
                std::thread::sleep(
                    Duration::from_millis(10).min(timeout.saturating_sub(deadline.elapsed())),
                );
            }
            Err(error) => {
                return Err(CoreError::Adapter {
                    adapter: "engram-store-knowledge-sqlite".into(),
                    message: format!("journal-mode setup: {error}"),
                });
            }
        }
    }
}
