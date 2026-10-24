//! Regression tests for consolidation dry-run semantics on the PRODUCTION
//! wiring (MCCL-audit defect F1).
//!
//! The `ExecutorConsolidationService` adapter previously ignored
//! `ConsolidationRequest.dry_run` and executed unconditionally, so MCP
//! `maintain {"dry_run": true}` mutated the store while reporting a preview.
//! The explicit-apply contract (planner/validation semantics) is now honored:
//! only `dry_run == Some(false)` executes; `Some(true)` and `None` are
//! plan-only previews.

use engram_conformance::wiring::bootstrap_provider;
use engram_domain::*;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
use engram_memory::MemoryRepository;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-dryrun".to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn request(dry_run: Option<bool>) -> ConsolidationRequest {
    ConsolidationRequest {
        scope: scope(),
        requester: Requester {
            actor: Actor {
                id: Id::from("test-agent"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            roles: vec![],
            permissions: vec![],
            on_behalf_of: None,
        },
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Hybrid),
        dry_run,
    }
}

fn bootstrapped() -> (std::path::PathBuf, engram_integration::EngramProvider) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "engram-dryrun-regression-{}-{}",
        std::process::id(),
        n
    ));
    let config = EngramConfig::new(
        dir.clone(),
        std::env::temp_dir(),
        engram_domain::ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".to_string(),
            model: "test_model".to_string(),
            dimensions: 384,
            prompt_profile: "query".to_string(),
            normalization: None,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    );
    let provider = bootstrap_provider(&config).expect("bootstrap provider");
    (dir, provider)
}

fn active_memory_count(provider: &engram_integration::EngramProvider) -> usize {
    let memory = provider.require_memory().expect("memory handle");
    let page = block_on(MemoryRepository::list_memories_paged(
        memory.as_ref(),
        &scope(),
        None,
        100,
    ))
    .expect("list memories");
    page.items.len()
}

#[test]
fn dry_run_true_is_a_plan_only_preview_that_writes_nothing() {
    let (_dir, provider) = bootstrapped();
    let before = active_memory_count(&provider);
    let consolidation = provider.require_consolidation().expect("consolidation");

    let run = block_on(consolidation.consolidate(request(Some(true)))).expect("dry run");

    // A completed run reporting planned work with zero writes.
    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    assert!(
        run.tasks
            .iter()
            .all(|t| t.items_written.unwrap_or(0) == 0
                || t.status != ConsolidationTaskStatus::Completed),
        "dry-run tasks must not report writes: {:?}",
        run.tasks
    );
    let after = active_memory_count(&provider);
    assert_eq!(before, after, "dry_run=true must not mutate the store");
}

#[test]
fn dry_run_none_defaults_to_plan_only_explicit_apply_contract() {
    let (_dir, provider) = bootstrapped();
    let before = active_memory_count(&provider);
    let consolidation = provider.require_consolidation().expect("consolidation");

    let run = block_on(consolidation.consolidate(request(None))).expect("preview");

    assert_eq!(run.status, ConsolidationRunStatus::Completed);
    let after = active_memory_count(&provider);
    assert_eq!(before, after, "dry_run=None must not mutate the store");
}

#[test]
fn dry_run_false_still_executes_the_pipeline() {
    let (_dir, provider) = bootstrapped();
    let consolidation = provider.require_consolidation().expect("consolidation");

    // Explicit apply must keep working (no behavior change for real callers).
    let run = block_on(consolidation.consolidate(request(Some(false)))).expect("apply run");
    assert_eq!(run.status, ConsolidationRunStatus::Completed);
}
