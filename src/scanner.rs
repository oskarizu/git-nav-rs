//! Recursive git-repo discovery. Directory listing is sequential but the
//! recursion into children happens in a rayon parallel iterator, which is
//! the piece that actually mattered for wall-clock time on the old version.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;

pub fn find_repos(root: &Path, max_depth: u32) -> Vec<PathBuf> {
    let out = Mutex::new(Vec::new());
    walk(root, 0, max_depth, &out);
    let mut v = out.into_inner().unwrap_or_default();
    v.sort();
    v
}

fn walk(path: &Path, depth: u32, max_depth: u32, out: &Mutex<Vec<PathBuf>>) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };

    let children: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let meta = std::fs::symlink_metadata(&path).ok()?;
            if !meta.file_type().is_dir() {
                return None;
            }
            Some(path)
        })
        .collect();

    children.into_par_iter().for_each(|entry| {
        if entry.join(".git").exists() {
            if let Ok(mut guard) = out.lock() {
                guard.push(entry);
            }
            return;
        }
        walk(&entry, depth + 1, max_depth, out);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    #[test]
    fn finds_repos_at_various_depths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Direct child repo
        fs::create_dir_all(root.join("a/.git")).unwrap();
        // Nested repo
        fs::create_dir_all(root.join("b/c/.git")).unwrap();
        // Non-repo directory
        fs::create_dir_all(root.join("plain")).unwrap();
        // .git can be a file (for worktrees) — should still count
        fs::create_dir_all(root.join("d")).unwrap();
        touch(&root.join("d/.git"));

        let repos = find_repos(root, 4);
        let names: Vec<String> = repos
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"c".to_string()));
        assert!(names.contains(&"d".to_string()));
        assert!(!names.contains(&"plain".to_string()));
    }

    #[test]
    fn respects_max_depth() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("l1/l2/l3/deep/.git")).unwrap();
        let shallow = find_repos(root, 1);
        assert!(shallow.is_empty(), "depth 1 should not reach l1/l2/l3/deep");
        let deep = find_repos(root, 4);
        assert_eq!(deep.len(), 1);
    }

    #[test]
    fn skips_hidden_directories() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".hidden/.git")).unwrap();
        fs::create_dir_all(root.join("visible/.git")).unwrap();
        let repos = find_repos(root, 4);
        assert_eq!(repos.len(), 1);
    }

    #[test]
    fn does_not_descend_into_a_repo() {
        // If a nested "child" repo exists inside another repo, we should stop
        // at the outer one — a repo's contents are not further scanned.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("outer/.git")).unwrap();
        fs::create_dir_all(root.join("outer/inner/.git")).unwrap();
        let repos = find_repos(root, 4);
        assert_eq!(repos.len(), 1);
        assert!(repos[0].ends_with("outer"));
    }
}
