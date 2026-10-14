//! Exact-file attribution of previously unversioned code before replacement.
use super::schema::sql_error;
use engram_domain::CodeFileKey;
use engram_runtime::CoreResult;
use rusqlite::{Connection, params};

pub(super) fn attribute_legacy_records(
    conn: &Connection,
    file: &CodeFileKey,
    key: &str,
) -> CoreResult<()> {
    // Both file identity and all scope dimensions are exact. Unknown/missing
    // repository attribution is left unverified, never swept by a path alone.
    let graph_path = if cfg!(windows) {
        "replace(path,char(92),'/')"
    } else {
        "path"
    };
    let mut graphs=conn.prepare(&format!("SELECT id,json_extract(record_json,'$.metadata.documentId') FROM knowledge_graphs_records WHERE stable_source_key=?1 AND {graph_path}=?2 AND tenant=?3 AND subject IS ?4 AND workspace IS ?5 AND session IS ?6 AND environment IS ?7")).map_err(sql_error)?;
    let rows = graphs
        .query_map(
            params![
                file.repository,
                file.path,
                file.scope.tenant,
                file.scope.subject,
                file.scope.workspace,
                file.scope.session,
                file.scope.environment
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .map_err(sql_error)?;
    for row in rows {
        let (graph, document) = row.map_err(sql_error)?;
        attribute(conn, key, "graph", &graph)?;
        for (table, kind) in [
            ("knowledge_entities_records", "entity"),
            ("knowledge_relationships_records", "relationship"),
            ("knowledge_unresolved_refs_records", "unresolved_reference"),
        ] {
            let mut stmt=conn.prepare(&format!("SELECT id FROM {table} WHERE graph_id=?1 AND tenant=?2 AND subject IS ?3 AND workspace IS ?4 AND session IS ?5 AND environment IS ?6")).map_err(sql_error)?;
            let ids = stmt
                .query_map(
                    params![
                        graph,
                        file.scope.tenant,
                        file.scope.subject,
                        file.scope.workspace,
                        file.scope.session,
                        file.scope.environment
                    ],
                    |r| r.get::<_, String>(0),
                )
                .map_err(sql_error)?;
            for id in ids {
                attribute(conn, key, kind, &id.map_err(sql_error)?)?;
            }
        }
        if let Some(document) = document {
            attribute_document(conn, file, key, &document)?;
        }
    }
    let document_path = document_path_expression();
    let mut stmt=conn.prepare(&format!("SELECT d.id FROM knowledge_documents d JOIN knowledge_sources s ON s.id=d.source_id WHERE json_extract(d.record_json,'$.kind')='code' AND {document_path}=?1 AND json_extract(s.record_json,'$.metadata.stableSourceKey')=?2 AND s.tenant=?3 AND s.subject IS ?4 AND s.workspace IS ?5 AND s.session IS ?6 AND s.environment IS ?7")).map_err(sql_error)?;
    let ids = stmt
        .query_map(
            params![
                file.path,
                file.repository,
                file.scope.tenant,
                file.scope.subject,
                file.scope.workspace,
                file.scope.session,
                file.scope.environment
            ],
            |r| r.get::<_, String>(0),
        )
        .map_err(sql_error)?;
    for id in ids {
        attribute_document(conn, file, key, &id.map_err(sql_error)?)?;
    }
    Ok(())
}
fn attribute_document(
    conn: &Connection,
    file: &CodeFileKey,
    key: &str,
    document: &str,
) -> CoreResult<()> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_documents WHERE id=?1)",
            [document],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    if !exists {
        return Ok(());
    }
    let document_path = document_path_expression();
    let matches:bool=conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM knowledge_documents d JOIN knowledge_sources s ON s.id=d.source_id WHERE d.id=?1 AND {document_path}=?2 AND json_extract(d.record_json,'$.kind')='code' AND s.tenant=?3 AND s.subject IS ?4 AND s.workspace IS ?5 AND s.session IS ?6 AND s.environment IS ?7)"),params![document,file.path,file.scope.tenant,file.scope.subject,file.scope.workspace,file.scope.session,file.scope.environment],|r|r.get(0)).map_err(sql_error)?;
    if !matches {
        return Err(super::source_inventory::invalid(
            "legacy graph document attribution crosses file or scope",
        ));
    }

    attribute(conn, key, "document", document)?;
    let mut stmt=conn.prepare("SELECT id FROM knowledge_chunks_records WHERE document_id=?1 AND json_extract(record_json,'$.code') IS NULL").map_err(sql_error)?;
    let rows = stmt
        .query_map([document], |r| r.get::<_, String>(0))
        .map_err(sql_error)?;
    for row in rows {
        attribute(conn, key, "chunk", &row.map_err(sql_error)?)?;
    }
    Ok(())
}
fn attribute(conn: &Connection, key: &str, kind: &str, id: &str) -> CoreResult<()> {
    // Do not relabel records that already have typed source ownership.
    conn.execute("INSERT OR IGNORE INTO code_source_contributions(file_key,attempt_id,kind,record_id) SELECT ?1,'legacy:unverified',?2,?3 WHERE NOT EXISTS(SELECT 1 FROM code_source_contributions WHERE file_key=?1 AND kind=?2 AND record_id=?3)",params![key,kind,id]).map_err(sql_error)?;
    Ok(())
}

fn document_path_expression() -> &'static str {
    // Backslashes are separators only on Windows. Preserve literal Unix paths.
    if cfg!(windows) {
        "replace(json_extract(d.record_json,'$.path'),char(92),'/')"
    } else {
        "json_extract(d.record_json,'$.path')"
    }
}
