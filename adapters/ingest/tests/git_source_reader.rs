use std::{fs, path::Path, process::Command};

use chrono::Utc;
use engram_domain::*;
use engram_ingest::{FilesystemSourceReaderOptions, GitSourceReader};
use engram_knowledge::SourceReader;
use futures::executor::block_on;

#[test]
fn git_reader_discovers_only_tracked_supported_files() {
    let root = git_fixture("tracked-discovery");
    write_file(&root.join("src/lib.rs"), "pub fn tracked() {}\n");
    write_file(&root.join("README.md"), "# Tracked\n");
    write_file(&root.join("notes.txt"), "untracked but supported\n");
    write_file(&root.join("image.bin"), "\u{0}\u{1}");
    git(&root, ["add", "README.md", "src/lib.rs", "image.bin"]);

    let reader =
        GitSourceReader::new(&root, FilesystemSourceReaderOptions::default()).expect("git reader");
    let source = source(&root);

    let documents = block_on(reader.read_source(&source)).expect("read git source");

    assert_eq!(
        documents
            .iter()
            .map(|document| document.path.as_deref().expect("path"))
            .collect::<Vec<_>>(),
        vec!["README.md", "src/lib.rs"]
    );
    assert_eq!(documents[0].kind, SourceDocumentKind::Markdown);
    assert_eq!(documents[1].language.as_deref(), Some("rust"));
    assert!(
        documents
            .iter()
            .all(|document| document.source_id == source.id
                && document.policy == source.policy
                && document.provenance == source.provenance
                && document.content_hash.starts_with("sha256:"))
    );

    remove_fixture(&root);
}

#[test]
fn git_reader_reads_tracked_utf8_document() {
    let root = git_fixture("read-document");
    write_file(
        &root.join("docs/intro.md"),
        "# Intro\nTracked Git content.\n",
    );
    git(&root, ["add", "docs/intro.md"]);
    let reader =
        GitSourceReader::new(&root, FilesystemSourceReaderOptions::default()).expect("git reader");
    let source = source(&root);
    let documents = block_on(reader.read_source(&source)).expect("read git source");

    let text = block_on(reader.read_document(&documents[0])).expect("read tracked document");

    assert!(text.contains("Tracked Git content"));
    remove_fixture(&root);
}

#[test]
fn git_reader_rejects_untracked_and_escaped_paths() {
    let root = git_fixture("reject-paths");
    write_file(&root.join("tracked.txt"), "tracked\n");
    write_file(&root.join("untracked.txt"), "untracked\n");
    git(&root, ["add", "tracked.txt"]);
    let reader =
        GitSourceReader::new(&root, FilesystemSourceReaderOptions::default()).expect("git reader");

    let mut document = document("untracked.txt");
    let untracked = block_on(reader.read_document(&document)).expect_err("reject untracked");
    assert!(format!("{untracked}").contains("not tracked"));

    document.path = Some("../secret.txt".to_owned());
    let traversal = block_on(reader.read_document(&document)).expect_err("reject traversal");
    assert!(format!("{traversal}").contains("traversal"));

    remove_fixture(&root);
}

fn git_fixture(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "engram-git-source-reader-{name}-{}",
        std::process::id()
    ));
    remove_fixture(&root);
    fs::create_dir_all(&root).expect("create fixture root");
    git(&root, ["init"]);
    root
}

fn git<const N: usize>(root: &Path, args: [&str; N]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write fixture file");
}

fn remove_fixture(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn source(root: &Path) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from("source-git"),
        kind: SourceKind::GitRepository,
        scope: Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        name: "git fixture".to_owned(),
        uri: Some(root.display().to_string()),
        version: Some("worktree".to_owned()),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn document(path: &str) -> SourceDocument {
    SourceDocument {
        id: Id::from("document-fixture"),
        source_id: Id::from("source-git"),
        kind: SourceDocumentKind::Text,
        uri: None,
        path: Some(path.to_owned()),
        title: None,
        mime_type: Some("text/plain".to_owned()),
        language: None,
        version: None,
        content_hash: "sha256:test".to_owned(),
        provenance: provenance(),
        policy: policy(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "git fixture".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("git_source_reader".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-ingest"),
        kind: ActorKind::Agent,
        display_name: Some("Ingest Agent".to_owned()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}
