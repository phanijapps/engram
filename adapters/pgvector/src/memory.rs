//! Minimal Postgres-backed memory proof-of-concept (T2).
//!
//! Demonstrates the pattern: connect via tokio-postgres on the shared runtime,
//! write a memory JSONB row + scope columns, read it back scope-filtered.
//! The full `MemoryService` impl (matching every method of the trait) is the
//! follow-on; this proves the infrastructure works end-to-end.

use crate::connection::PgConnection;

/// A simple memory record stored in Postgres.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PgMemoryRow {
    pub id: String,
    pub content: String,
    pub tenant: String,
    pub workspace: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Writes a memory row into Postgres. Returns the stored row.
pub fn write_memory(conn: &PgConnection, row: &PgMemoryRow) -> Result<PgMemoryRow, String> {
    let json = serde_json::to_value(row).map_err(|e| e.to_string())?;
    conn.block_on(async {
        conn.client
            .execute(
                "INSERT INTO memories (id, record_json, tenant, workspace, created_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (id) DO UPDATE SET record_json = EXCLUDED.record_json",
                &[&row.id, &json, &row.tenant, &row.workspace, &row.created_at],
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(row.clone())
    })
}

/// Reads recent memories for a tenant, newest-first, capped at `limit`.
pub fn read_recent(
    conn: &PgConnection,
    tenant: &str,
    limit: i64,
) -> Result<Vec<PgMemoryRow>, String> {
    conn.block_on(async {
        let rows = conn
            .client
            .query(
                "SELECT record_json FROM memories WHERE tenant = $1 \
                 ORDER BY created_at DESC LIMIT $2",
                &[&tenant, &limit],
            )
            .await
            .map_err(|e| e.to_string())?;
        rows.iter()
            .map(|row| {
                let json: serde_json::Value = row.get(0);
                serde_json::from_value::<PgMemoryRow>(json).map_err(|e| e.to_string())
            })
            .collect()
    })
}
