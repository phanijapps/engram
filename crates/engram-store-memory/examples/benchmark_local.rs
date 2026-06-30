use std::time::Instant;

use chrono::{TimeZone, Utc};
use engram_core::MemoryService;
use engram_domain::*;
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

const MEMORY_COUNT: usize = 250;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let service = InMemoryMemoryService::new();

        let write_started = Instant::now();
        for index in 0..MEMORY_COUNT {
            service.write_memory(write_request(index)).await?;
        }
        let write_elapsed = write_started.elapsed();

        let retrieve_started = Instant::now();
        let context = service.retrieve(retrieval_request()).await?;
        let retrieve_elapsed = retrieve_started.elapsed();

        println!("engram local benchmark smoke");
        println!("adapter=in-memory");
        println!("memories_written={MEMORY_COUNT}");
        println!(
            "write_elapsed_ms={:.3}",
            write_elapsed.as_secs_f64() * 1000.0
        );
        println!("retrieved_items={}", context.items.len());
        println!(
            "retrieve_elapsed_ms={:.3}",
            retrieve_elapsed.as_secs_f64() * 1000.0
        );
        println!("note=local timing only; not a performance claim");

        Ok(())
    })
}

fn write_request(index: usize) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: format!("benchmark-token Engram local benchmark memory {index}"),
            summary: Some(format!("benchmark memory {index}")),
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: Some(format!("sha256:benchmark-memory-{index}")),
        },
        scope: scope(),
        requester: requester(vec!["memory.write"]),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: Some(format!("benchmark-memory-{index}")),
    }
}

fn retrieval_request() -> RetrievalRequest {
    RetrievalRequest {
        query: "benchmark-token".to_owned(),
        scope: scope(),
        requester: requester(vec!["memory.retrieve"]),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: vec![MemoryKind::Fact],
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: None,
            include_archived: Some(false),
        }),
        cues: Vec::new(),
        limit: Some(10),
        budget: Some(ContextBudget {
            max_items: Some(10),
            max_tokens: Some(2_000),
            max_bytes: None,
        }),
        include_explanations: Some(false),
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-benchmark".to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("local".to_owned()),
    }
}

fn requester(permissions: Vec<&str>) -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: permissions.into_iter().map(str::to_owned).collect(),
        on_behalf_of: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-benchmark"),
        kind: ActorKind::Agent,
        display_name: Some("Benchmark Agent".to_owned()),
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "local_benchmark_smoke".to_owned(),
        actor: actor(),
        observed_at: timestamp(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("synthetic".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Ephemeral,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Delete),
    }
}

fn timestamp() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("valid benchmark timestamp")
}
