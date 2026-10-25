//! Cross-file (cross-repo-shaped) endpoint join: a TS client `fetch` and a
//! Java Spring route in the SAME scan resolve into one `consumes` edge with
//! a bound endpoint object id — the React→REST mapping.

use std::fs;

use engram_domain::*;
use engram_ingest::{scan_repository, ScanOptions};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "t".into(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

#[test]
fn ts_fetch_joins_java_spring_endpoint() {
    let root = std::env::temp_dir().join(format!("engram-api-join-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("frontend")).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();
    let tsx = [
        "export function UserList() {",
        "  return null;",
        "}",
        "export async function loadUsers() {",
        "  const r = await fetch('/api/users/list');",
        "  return r.json();",
        "}",
    ]
    .join("\n");
    let java = [
        "@RestController",
        "@RequestMapping(\"/api/users\")",
        "public class UserController {",
        "    @GetMapping(\"/list\")",
        "    public java.util.List<String> listUsers() { return null; }",
        "}",
    ]
    .join("\n");
    fs::write(root.join("frontend").join("UserList.tsx"), tsx).unwrap();
    fs::write(root.join("backend").join("UserController.java"), java).unwrap();

    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let (summary, _) = scan_repository(
        &root,
        &ScanOptions {
            scope: scope(),
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            actor: Actor {
                id: Id::from("test"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            source_name: "api-join".into(),
            max_bytes: 0,
            manifest: Default::default(),
            scan_filter: Default::default(),
        },
        &store,
        |_| {},
    )
    .expect("scan");
    assert_eq!(summary.errors, 0);

    let rels = block_on(store.list_relationships(&scope())).expect("rels");
    let consume_edges: Vec<_> = rels.iter().filter(|r| r.predicate == "consumes").collect();
    assert!(
        consume_edges
            .iter()
            .any(|r| r.object.name.as_deref() == Some("GET /api/users/list")),
        "consumes edge to the endpoint: {consume_edges:?}"
    );
    assert!(
        consume_edges.iter().any(|r| r.object.id.is_some()),
        "endpoint object RESOLVED (cross-file join): {consume_edges:?}"
    );
    let routes: Vec<_> = rels.iter().filter(|r| r.predicate == "routes_to").collect();
    assert!(
        routes
            .iter()
            .any(|r| r.subject.name.as_deref() == Some("GET /api/users/list")),
        "spring route endpoint: {routes:?}"
    );

    let _ = fs::remove_dir_all(&root);
}
