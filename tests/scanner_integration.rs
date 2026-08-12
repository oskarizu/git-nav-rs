//! Integration test that exercises scanner + git module against a real
//! on-disk git repo (created with `git init`), rather than the fake `.git`
//! dirs used in the unit tests.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn git_init(path: &std::path::Path) {
    let status = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(path)
        .status()
        .expect("git init failed to spawn");
    assert!(status.success(), "git init failed");
    // Set a local identity so `git commit` works without touching the
    // user's global git config.
    for (k, v) in [("user.email", "test@example.com"), ("user.name", "Test")] {
        let _ = Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["config", "--local", k, v])
            .status();
    }
}

#[test]
fn scan_then_gather_infos_returns_expected_repo() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let repo = root.join("my-repo");
    fs::create_dir(&repo).unwrap();
    git_init(&repo);
    fs::write(repo.join("README.md"), "hello").unwrap();

    let repos = git_nav::scanner::find_repos(root, 4);
    assert_eq!(repos.len(), 1);
    assert!(repos[0].ends_with("my-repo"));

    let infos = git_nav::git::gather_infos(&repos);
    assert_eq!(infos.len(), 1);
    let info = &infos[0];
    assert_eq!(info.name, "my-repo");
    // No commits yet, so sha stays as "-" and dirty picks up the untracked file.
    assert_eq!(info.sha, "-");
    assert_eq!(info.branch, "main");
    assert!(info.dirty >= 1);
    // No remote configured → org falls back to "-".
    assert_eq!(info.org, "-");
}

#[test]
fn nonexistent_root_yields_empty_list() {
    let tmp = TempDir::new().unwrap();
    let ghost = tmp.path().join("does-not-exist");
    let repos = git_nav::scanner::find_repos(&ghost, 4);
    assert!(repos.is_empty());
}
