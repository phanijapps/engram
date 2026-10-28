use engram_ingest::git_source::{
    acquire_local_branch, read_resolved_commit, resolve_local_branch, retain_snapshot,
    validate_branch_ref,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;
fn run(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "Factory Fixture")
        .env("GIT_AUTHOR_EMAIL", "factory@example.invalid")
        .env("GIT_COMMITTER_NAME", "Factory Fixture")
        .env("GIT_COMMITTER_EMAIL", "factory@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git fixture operation failed: {:?}",
        args
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}
fn repo() -> (tempfile::TempDir, PathBuf) {
    let d = tempdir().unwrap();
    let r = d.path().join("repo");
    std::fs::create_dir(&r).unwrap();
    run(&r, &["init", "-b", "main"]);
    run(&r, &["config", "user.name", "Factory Fixture"]);
    run(&r, &["config", "user.email", "factory@example.invalid"]);
    std::fs::create_dir(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), b"pub fn committed() {}\r\n").unwrap();
    run(&r, &["add", "."]);
    run(&r, &["commit", "-m", "fixture"]);
    (d, r)
}
mod task_007 {
    use super::*;
    #[test]
    fn dirty_checkout_untouched() {
        let (d, r) = repo();
        std::fs::write(r.join("src/lib.rs"), b"dirty\n").unwrap();
        std::fs::write(r.join("untracked"), b"untracked").unwrap();
        let before = (
            std::fs::read(r.join("src/lib.rs")).unwrap(),
            std::fs::read(r.join("untracked")).unwrap(),
            std::fs::read(r.join(".git/index")).unwrap(),
            run(&r, &["rev-parse", "HEAD"]),
            run(&r, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        );
        let (c, f) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        let s = retain_snapshot(&d.path().join("snapshots"), c, &f).unwrap();
        assert_eq!(
            std::fs::read(s.root.join("src/lib.rs")).unwrap(),
            b"pub fn committed() {}\r\n"
        );
        let after = (
            std::fs::read(r.join("src/lib.rs")).unwrap(),
            std::fs::read(r.join("untracked")).unwrap(),
            std::fs::read(r.join(".git/index")).unwrap(),
            run(&r, &["rev-parse", "HEAD"]),
            run(&r, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        );
        assert_eq!(before, after);
    }
    #[test]
    fn local_no_origin() {
        let (_guard, r) = repo();
        assert!(run(&r, &["remote"]).is_empty());
        let (c, files) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        assert_eq!(c.branch_ref, "refs/heads/main");
        assert!(!files.is_empty());
    }
    #[test]
    fn exact_ref_validation() {
        for bad in [
            "main",
            "HEAD",
            "refs/tags/v1",
            "refs/heads/a..b",
            "refs/heads/a//b",
            "refs/heads/x.lock",
            "refs/heads/a b",
        ] {
            assert!(validate_branch_ref(bad).is_err(), "{bad}")
        }
    }
    #[test]
    fn case_distinct_refs() {
        let (_guard, r) = repo();
        run(&r, &["branch", "Main"]);
        let (lower, _) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        let (upper, _) = acquire_local_branch(&r, "refs/heads/Main").unwrap();
        assert_eq!(lower.oid, upper.oid);
        assert_ne!(lower.branch_ref, upper.branch_ref);
    }
    #[test]
    fn moved_ref_pinned_once() {
        let (_guard, r) = repo();
        let first = resolve_local_branch(&r, "refs/heads/main").unwrap();
        let first_oid = first.oid.clone();
        std::fs::write(r.join("src/lib.rs"), b"second\n").unwrap();
        run(&r, &["add", "."]);
        run(&r, &["commit", "-m", "move branch"]);
        let (second, _) = acquire_local_branch(&r, "refs/heads/main").unwrap();
        assert_ne!(first_oid, second.oid);
        assert_eq!(first_oid.len(), 40);
        let pinned_files = read_resolved_commit(&r, &first).unwrap();
        assert_eq!(
            pinned_files
                .iter()
                .find(|f| f.path == Path::new("src/lib.rs"))
                .unwrap()
                .bytes,
            b"pub fn committed() {}\r\n"
        );
    }
    #[test]
    fn missing_object() {
        let (_guard, r) = repo();
        assert!(acquire_local_branch(&r, "refs/heads/main").is_ok());
        let error = acquire_local_branch(&r, "refs/heads/missing").unwrap_err();
        assert!(matches!(
            error,
            engram_knowledge::CoreError::InvalidRequest { .. }
        ));
    }
}
