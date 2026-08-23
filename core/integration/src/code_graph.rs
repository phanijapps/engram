//! Phase-2 code-graph query surface (RFC-0020 / engram-code spec T10):
//! `file_dependencies` and `explore`, composed over the `KnowledgeQuery`
//! port with the pure query functions from `engram-code`. One
//! implementation serves the Rust facade, both MCP servers, and the N-API
//! binding — transports compose, they never reimplement.

use engram_code::{explore as explore_query, file_dependencies as file_deps_query};
use engram_domain::Scope;
use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};

use crate::EngramProvider;

/// One file-level dependency edge (view over `FileDependency`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependencyView {
    pub from_path: String,
    pub import_path: String,
    pub resolved_to: Option<String>,
}

/// One `explore` node (view over `ExploreNode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreNodeView {
    pub name: String,
    pub kind: Option<String>,
    pub hop: u32,
}

impl EngramProvider {
    /// The file-level import graph in `scope`: every scanned file → the
    /// module paths it imports, resolved to defining files by stem suffix.
    pub async fn code_graph_file_dependencies(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<FileDependencyView>> {
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let paths: Vec<String> = entities
            .iter()
            .filter(|e| matches!(e.kind, engram_domain::EntityKind::File))
            .map(|e| e.name.clone())
            .collect();
        Ok(file_deps_query(&relationships, &paths)
            .into_iter()
            .map(|d| FileDependencyView {
                from_path: d.from_path,
                import_path: d.import_path,
                resolved_to: d.resolved_to,
            })
            .collect())
    }

    /// Natural-language explore: identifier-token seeding + bounded
    /// expansion over `calls`/`contains` edges. Defaults per the contract:
    /// depth 2, 24 nodes, 64 edges.
    pub async fn code_graph_explore(
        &self,
        scope: &Scope,
        query_text: &str,
        depth: Option<u32>,
        max_nodes: Option<usize>,
        max_edges: Option<usize>,
    ) -> CoreResult<Vec<ExploreNodeView>> {
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let names: Vec<(String, Option<String>)> = entities
            .iter()
            .map(|e| (e.name.clone(), Some(format!("{:?}", e.kind).to_lowercase())))
            .collect();
        Ok(explore_query(
            &relationships,
            &names,
            query_text,
            depth.unwrap_or(2),
            max_nodes.unwrap_or(24),
            max_edges.unwrap_or(64),
        )
        .into_iter()
        .map(|n| ExploreNodeView {
            name: n.name,
            kind: n.kind,
            hop: n.hop,
        })
        .collect())
    }
}
