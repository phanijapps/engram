//! Git metadata detection for repository scanning.
//!
//! Detects git repository information (remote URL, branch, commit SHA)
//! for provenance tracking and stable source key generation.

use std::path::Path;

/// Detect git metadata (remote URL, branch, short SHA) if the root is a git repo.
pub fn detect_git(root: &Path) -> Option<(String, String, String)> {
    let run = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if s.is_empty() { None } else { Some(s) }
    };
    let remote = run(&["remote", "get-url", "origin"])?;
    let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let sha = run(&["rev-parse", "--short=10", "HEAD"])?;
    Some((remote, branch, sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_detect_git_in_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create a test file and commit
        fs::write(repo_path.join("test.txt"), "test content").unwrap();
        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Add a remote
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/test/test.git",
            ])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test git detection
        let git_info = detect_git(repo_path);
        assert!(git_info.is_some());
        let (remote, branch, sha) = git_info.unwrap();
        assert_eq!(remote, "https://github.com/test/test.git");
        assert!(!branch.is_empty());
        assert!(!sha.is_empty());
    }

    #[test]
    fn test_detect_git_in_non_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path();

        let git_info = detect_git(repo_path);
        assert!(git_info.is_none());
    }
}
