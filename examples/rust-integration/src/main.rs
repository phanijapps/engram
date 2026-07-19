//! Verified getting-started example: bootstrap Engram through the integration
//! facade, write one memory, and retrieve it. The docs/guides tutorial
//! documents this exact code path.

use chrono::Utc;
use engram_conformance::bootstrap_provider;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, Id, MemoryContent, MemoryKind, Metadata, Policy,
    Provenance, Requester, Retention, RetrievalRequest, Scope, Sensitivity, Visibility,
    WriteMemoryRequest,
};
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Configure Engram: where data lives, how scope is enforced, and which
    //    embedding provider the vector index is built for. The trusted_root must
    //    contain the storage_path (path confinement). A fresh per-run directory
    //    keeps the example idempotent so you can run it repeatedly.
    let storage_path =
        std::env::temp_dir().join(format!("engram-getting-started-{}", std::process::id()));
    let config = EngramConfig::new(
        storage_path,
        std::env::temp_dir(),
        engram_domain::types::ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "fastembed".to_string(),
            model: "bge-small-en-v1.5".to_string(),
            dimensions: 384,
            prompt_profile: "query".to_string(),
            normalization: None,
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    );

    // 2. Bootstrap. This constructs every adapter, runs a conformance fixture
    //    for each capability family, and attaches a handle only where the
    //    fixture passes. Capabilities you can actually use come back as Supported.
    let provider = bootstrap_provider(&config)?;
    println!("memory capability: {:?}", provider.capabilities().memory);

    // 3. Grab the memory handle and write one observation.
    let memory = provider
        .memory()
        .expect("memory handle is attached when the memory fixture passes");

    let scope = Scope {
        tenant: "my-agent".to_string(),
        subject: Some("session-1".to_string()),
        workspace: None,
        session: None,
        environment: Some("dev".to_string()),
    };
    let requester = Requester {
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: Some("MyAgent".to_string()),
            metadata: None,
        },
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
        on_behalf_of: None,
    };

    let written = memory
        .write_memory(WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "The user prefers concise answers with code examples.".to_string(),
                summary: None,
                entities: Vec::new(),
                language: Some("en".to_string()),
                format: None,
                structured: None,
                hash: None,
            },
            scope: scope.clone(),
            requester: requester.clone(),
            provenance: Provenance {
                source: "getting-started".to_string(),
                actor: requester.actor.clone(),
                observed_at: Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: Some(Sensitivity::Low),
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: Some(DeleteMode::Tombstone),
            },
            links: Vec::new(),
            idempotency_key: None,
        })
        .await?;

    println!("wrote memory id: {}", written.record.id);

    // 4. Retrieve it back.
    let context = memory
        .retrieve(RetrievalRequest {
            query: "user preferences".to_string(),
            scope: scope.clone(),
            requester: requester.clone(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(5),
            budget: None,
            include_explanations: None,
        })
        .await?;

    println!("retrieved {} item(s):", context.items.len());
    for item in &context.items {
        println!("  - {}", item.content);
    }

    // Mark `Metadata` as intentionally unused so the import stays valid for
    // readers who extend the example with custom metadata.
    let _: Option<Metadata> = None;
    Ok(())
}
