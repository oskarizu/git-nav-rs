//! Repo metadata gathered by shelling out to `git`. Every repo's info is
//! collected on its own rayon thread — this is where the "slow first start"
//! win comes from: N sequential `git status` calls become max(N, #cores).

use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub sha: String,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: u32,
    pub org: String,
}

impl RepoInfo {
    fn placeholder(path: &Path) -> Self {
        RepoInfo {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            path: path.to_string_lossy().to_string(),
            branch: "-".into(),
            sha: "-".into(),
            ahead: 0,
            behind: 0,
            dirty: 0,
            org: "-".into(),
        }
    }
}

pub fn gather_infos(paths: &[PathBuf]) -> Vec<RepoInfo> {
    paths.par_iter().map(|p| get_repo_info(p)).collect()
}

pub fn get_repo_info(path: &Path) -> RepoInfo {
    let mut info = RepoInfo::placeholder(path);

    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain=v2", "--branch"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            parse_status(&stdout, &mut info);
        }
    }

    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["remote", "get-url", "origin"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            info.org = parse_org(stdout.trim());
        }
    }
    info
}

pub(crate) fn parse_status(stdout: &str, info: &mut RepoInfo) {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            if rest != "(initial)" {
                info.sha = rest.chars().take(7).collect();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.head ") {
            info.branch = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for p in rest.split_whitespace() {
                if let Some(n) = p.strip_prefix('+') {
                    info.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = p.strip_prefix('-') {
                    info.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with(['1', '2', 'u', '?']) {
            info.dirty += 1;
        }
    }
}

/// Extract the org/owner segment from a git remote URL. Handles ssh
/// (`git@host:org/repo.git`) and URL (`https://host/org/repo.git`) forms.
pub fn parse_org(url: &str) -> String {
    if url.is_empty() {
        return "-".into();
    }
    let mut u = url.trim_end_matches('/').to_string();
    if let Some(stripped) = u.strip_suffix(".git") {
        u = stripped.to_string();
    }

    let path_part: String = if let Some(scheme_idx) = u.find("://") {
        let after = &u[scheme_idx + 3..];
        match after.find('/') {
            Some(i) => after[i + 1..].to_string(),
            None => after.to_string(),
        }
    } else if let Some(colon_idx) = u.find(':') {
        u[colon_idx + 1..].to_string()
    } else {
        u.clone()
    };

    let parts: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 2 {
        parts[parts.len() - 2].to_string()
    } else {
        "-".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_org_ssh_form() {
        assert_eq!(parse_org("git@github.com:acme/foo.git"), "acme");
    }

    #[test]
    fn parse_org_https_form() {
        assert_eq!(parse_org("https://github.com/acme/foo.git"), "acme");
    }

    #[test]
    fn parse_org_ssh_url_form() {
        assert_eq!(parse_org("ssh://git@github.com/acme/foo"), "acme");
    }

    #[test]
    fn parse_org_trailing_slash() {
        assert_eq!(parse_org("https://gitlab.com/acme/foo/"), "acme");
    }

    #[test]
    fn parse_org_missing_returns_dash() {
        assert_eq!(parse_org(""), "-");
        assert_eq!(parse_org("nonsense"), "-");
    }

    #[test]
    fn parse_status_full_output() {
        let s = "# branch.oid 1234567890abcdef\n\
                 # branch.head main\n\
                 # branch.ab +2 -1\n\
                 1 .M N... 100644 100644 100644 abc def foo\n\
                 ? untracked\n";
        let mut info = RepoInfo::placeholder(std::path::Path::new("/tmp/x"));
        parse_status(s, &mut info);
        assert_eq!(info.branch, "main");
        assert_eq!(info.sha, "1234567");
        assert_eq!(info.ahead, 2);
        assert_eq!(info.behind, 1);
        assert_eq!(info.dirty, 2);
    }

    #[test]
    fn parse_status_initial_commit() {
        let s = "# branch.oid (initial)\n# branch.head main\n";
        let mut info = RepoInfo::placeholder(std::path::Path::new("/tmp/x"));
        parse_status(s, &mut info);
        assert_eq!(info.sha, "-");
        assert_eq!(info.branch, "main");
    }
}
