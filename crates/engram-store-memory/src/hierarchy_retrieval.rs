//! Hierarchy context annotation for in-memory retrieval.
//!
//! This module uses already-materialized hierarchy nodes to explain matching
//! retrieval results. It does not create hierarchy records, expand candidates,
//! or infer aggregate summaries.

use std::collections::{BTreeMap, BTreeSet};

use engram_domain::*;

use crate::scope::scope_allows;

/// Adds hierarchy path context to retrieval results when hierarchical mode is
/// requested.
pub(crate) fn apply_hierarchy_context(
    results: &mut [RetrievalResult],
    nodes: Vec<HierarchyNode>,
    request: &RetrievalRequest,
) {
    if !request.modes.contains(&RetrievalMode::Hierarchical) {
        return;
    }

    let visible_nodes = nodes
        .into_iter()
        .filter(|node| {
            scope_allows(&node.scope, &request.scope) && node.status == HierarchyNodeStatus::Active
        })
        .map(|node| (node.id.to_string(), node))
        .collect::<BTreeMap<_, _>>();
    if visible_nodes.is_empty() {
        return;
    }

    let base_nodes = visible_nodes
        .values()
        .filter(|node| node.kind == HierarchyNodeKind::Base)
        .filter_map(|node| {
            let target_type = node.source_target_type.as_ref()?;
            let target_id = node.source_target_id.as_ref()?;
            Some((
                (
                    retrieval_target_label(target_type).to_owned(),
                    target_id.clone(),
                ),
                node.id.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    for result in results {
        let key = (
            retrieval_target_label(&result.target_type).to_owned(),
            result.target_id.clone(),
        );
        let Some(base_node_id) = base_nodes.get(&key) else {
            continue;
        };
        let path = parent_path(base_node_id, &visible_nodes);
        if path.is_empty() {
            continue;
        }
        result.score.hierarchical_fit = Some(1.0);
        if let Some(explanation) = &mut result.explanation {
            explanation.path = path;
            explanation.reason = format!("{} Hierarchy context attached.", explanation.reason);
        }
    }
}

fn parent_path(seed_id: &str, nodes: &BTreeMap<String, HierarchyNode>) -> Vec<String> {
    let mut path = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current_id = Some(seed_id.to_owned());

    while let Some(node_id) = current_id {
        if !visited.insert(node_id.clone()) {
            break;
        }
        let Some(node) = nodes.get(&node_id) else {
            break;
        };
        path.push(format!("{}:{}", node.kind_label(), node.name));
        current_id = node.parent_id.as_ref().map(ToString::to_string);
    }

    path
}

fn retrieval_target_label(target_type: &RetrievalTargetType) -> &'static str {
    match target_type {
        RetrievalTargetType::Memory => "memory",
        RetrievalTargetType::Event => "event",
        RetrievalTargetType::Chunk => "chunk",
        RetrievalTargetType::Document => "document",
        RetrievalTargetType::Entity => "entity",
        RetrievalTargetType::Relationship => "relationship",
        RetrievalTargetType::Concept => "concept",
        RetrievalTargetType::Belief => "belief",
        RetrievalTargetType::Contradiction => "contradiction",
        RetrievalTargetType::HierarchyNode => "hierarchy_node",
        RetrievalTargetType::HierarchyRelation => "hierarchy_relation",
    }
}

trait HierarchyNodeKindLabel {
    fn kind_label(&self) -> &'static str;
}

impl HierarchyNodeKindLabel for HierarchyNode {
    fn kind_label(&self) -> &'static str {
        match &self.kind {
            HierarchyNodeKind::Base => "base",
            HierarchyNodeKind::Aggregate => "aggregate",
            HierarchyNodeKind::Schema => "schema",
            HierarchyNodeKind::Topic => "topic",
            HierarchyNodeKind::Cluster => "cluster",
            HierarchyNodeKind::Domain => "domain",
        }
    }
}
