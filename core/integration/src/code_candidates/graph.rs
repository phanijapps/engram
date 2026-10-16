//! Expansion only from already retrieved, authorized canonical anchors.
use super::{LaneHit, resolution::Resolver, trace};
use engram_domain::*;
use engram_retrieval::CodeIntelligenceConfig;
use engram_runtime::{CoreError, CoreResult};
use std::collections::HashSet;

pub(super) async fn expand(
    resolver: &mut Resolver<'_>,
    mut frontier: Vec<KnowledgeChunk>,
    config: &CodeIntelligenceConfig,
) -> CoreResult<Vec<LaneHit>> {
    if config.retrieval.graph_hops == 0 || frontier.is_empty() {
        return Ok(vec![]);
    }
    let Some(graph) = resolver.provider.graph().cloned() else {
        return Err(CoreError::ProviderUnavailable {
            provider: "anchored code graph expansion".into(),
        });
    };
    let cap = config.retrieval.candidates_per_lane as usize;
    let mut neighbors = config.retrieval.graph_neighbor_limit as usize;
    // Empty adjacency lists must not allow unbounded source calls.
    let mut calls = neighbors;
    let mut visited = HashSet::new();
    let mut seen_segments = HashSet::new();
    let mut seen_parents = HashSet::new();
    let mut result = Vec::new();
    for _ in 0..config.retrieval.graph_hops {
        let mut next = Vec::new();
        for parent in frontier {
            for reference in parent.entities {
                let Some(entity_id) = reference.id else {
                    continue;
                };
                if neighbors == 0 || calls == 0 || result.len() == cap {
                    return Ok(result);
                }
                if !visited.insert(entity_id.clone())
                    || !resolver.owns(CodeSourceRecordKind::Entity, entity_id.as_str())
                {
                    continue;
                }
                let Some(entity) = resolver
                    .provider
                    .require_knowledge()?
                    .get_entity(&entity_id, &resolver.request.scope)
                    .await?
                else {
                    continue;
                };
                let Some(graph_id) = entity.graph_id else {
                    continue;
                };
                if !resolver.owns(CodeSourceRecordKind::Graph, graph_id.as_str()) {
                    continue;
                }
                calls -= 1;
                let edges = graph
                    .neighbors(
                        &graph_id,
                        &entity_id,
                        &resolver.request.scope,
                        Some(neighbors as u32),
                    )
                    .await?;
                for edge in edges.into_iter().take(neighbors) {
                    neighbors -= 1;
                    let Some(target) = edge.object.id.as_ref() else {
                        continue;
                    };
                    if edge.scope != resolver.request.scope
                        || edge.graph_id.as_ref() != Some(&graph_id)
                        || edge.subject.id.as_ref() != Some(&entity_id)
                        || !resolver.owns(CodeSourceRecordKind::Relationship, edge.id.as_str())
                        || !resolver.owns(CodeSourceRecordKind::Entity, target.as_str())
                    {
                        continue;
                    }
                    for resolved in resolver.entity(target.as_str(), cap - result.len()).await? {
                        if seen_segments.insert(resolved.segment.id.clone()) {
                            if seen_parents.insert(resolved.parent.id.clone()) {
                                next.push(resolved.parent.clone());
                            }
                            result.push(LaneHit {
                                resolved,
                                trace: trace("graph_expansion", result.len() + 1, 1.0),
                            });
                            if result.len() == cap {
                                return Ok(result);
                            }
                        }
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Ok(result)
}
