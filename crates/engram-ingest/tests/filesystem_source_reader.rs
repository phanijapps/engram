use std::{fs, path::Path};

use chrono::Utc;
use engram_core::SourceReader;
use engram_domain::*;
use engram_ingest::{FilesystemSourceReader, FilesystemSourceReaderOptions};
use futures::executor::block_on;

#[test]
fn filesystem_reader_discovers_supported_files_in_stable_order() {
    let root = fixture_root("stable-order");
    write_file(&root.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n");
    write_file(&root.join("README.md"), "# Engram\n");
    write_file(&root.join("notes.txt"), "source grounded text\n");
    write_file(&root.join(".hidden.md"), "hidden\n");
    write_file(&root.join("image.bin"), "\u{0}\u{1}");

    let reader = FilesystemSourceReader::new(&root, FilesystemSourceReaderOptions::default())
        .expect("filesystem reader");
    let source = source(&root);

    let documents = block_on(reader.read_source(&source)).expect("read source");

    assert_eq!(
        documents
            .iter()
            .map(|document| document.path.as_deref().expect("path"))
            .collect::<Vec<_>>(),
        vec!["README.md", "notes.txt", "src/lib.rs"]
    );
    assert_eq!(documents[0].kind, SourceDocumentKind::Markdown);
    assert_eq!(documents[2].kind, SourceDocumentKind::Code);
    assert_eq!(documents[2].language.as_deref(), Some("rust"));
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
fn filesystem_reader_reads_discovered_utf8_document() {
    let root = fixture_root("read-document");
    write_file(
        &root.join("docs/intro.md"),
        "# Intro\nMemory and knowledge stay separate.\n",
    );
    let reader = FilesystemSourceReader::new(&root, FilesystemSourceReaderOptions::default())
        .expect("filesystem reader");
    let source = source(&root);
    let documents = block_on(reader.read_source(&source)).expect("read source");

    let text = block_on(reader.read_document(&documents[0])).expect("read document");

    assert!(text.contains("Memory and knowledge"));
    remove_fixture(&root);
}

#[test]
fn filesystem_reader_rejects_path_traversal_and_absolute_paths() {
    let root = fixture_root("reject-paths");
    write_file(&root.join("docs/intro.md"), "safe\n");
    let reader = FilesystemSourceReader::new(&root, FilesystemSourceReaderOptions::default())
        .expect("filesystem reader");
    let mut document = document("docs/intro.md");

    document.path = Some("../secret.txt".to_owned());
    let traversal = block_on(reader.read_document(&document)).expect_err("reject traversal");
    assert!(format!("{traversal}").contains("traversal"));

    document.path = Some("/tmp/secret.txt".to_owned());
    let absolute = block_on(reader.read_document(&document)).expect_err("reject absolute");
    assert!(format!("{absolute}").contains("relative"));

    remove_fixture(&root);
}

#[test]
fn filesystem_reader_rejects_oversized_reads() {
    let root = fixture_root("oversized");
    write_file(&root.join("large.txt"), "too large");
    let reader = FilesystemSourceReader::new(
        &root,
        FilesystemSourceReaderOptions {
            max_file_bytes: 4,
            include_hidden: false,
        },
    )
    .expect("filesystem reader");

    let result = block_on(reader.read_source(&source(&root))).expect_err("reject oversized");

    assert!(format!("{result}").contains("max_file_bytes"));
    remove_fixture(&root);
}

fn fixture_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "engram-filesystem-source-reader-{name}-{}",
        std::process::id()
    ));
    remove_fixture(&root);
    fs::create_dir_all(&root).expect("create fixture root");
    root
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
        id: Id::from("source-filesystem"),
        kind: SourceKind::Filesystem,
        scope: Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        name: "filesystem fixture".to_owned(),
        uri: Some(root.display().to_string()),
        version: Some("v1".to_owned()),
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
        source_id: Id::from("source-filesystem"),
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
        source: "filesystem fixture".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("filesystem_source_reader".to_owned()),
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
