//! Shell integration: emits and installs a `gnav` wrapper that captures
//! the path git-nav prints on stdout and `cd`s into it.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MARKER_START: &str = "# >>> git-nav shell integration >>>";
const MARKER_END: &str = "# <<< git-nav shell integration <<<";

pub fn init_snippet(shell: &str) -> String {
    let exe = current_exe();
    match shell {
        "fish" => format!(
            "function gnav\n    set -l dest ({exe} $argv)\n    if test -n \"$dest\"\n        cd $dest\n    end\nend\n"
        ),
        _ => format!(
            "gnav() {{\n  local dest\n  dest=\"$({exe} \"$@\")\" || return\n  [ -n \"$dest\" ] && cd \"$dest\"\n}}\n"
        ),
    }
}

fn current_exe() -> String {
    env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "git-nav".into())
}

/// Idempotently append (or refresh) the wrapper function in the user's rc
/// file. Returns the path of the file that was written.
pub fn install(shell_override: Option<&str>) -> crate::Result<PathBuf> {
    let shell = shell_override
        .map(String::from)
        .or_else(|| {
            env::var("SHELL").ok().and_then(|s| {
                Path::new(&s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(String::from)
            })
        })
        .unwrap_or_else(|| "bash".into());

    let rc = rc_file_for(&shell)?;
    let snippet = init_snippet(&shell);
    let block = format!("{MARKER_START}\n{snippet}{MARKER_END}");

    let existing = fs::read_to_string(&rc).unwrap_or_default();
    let updated = replace_or_append(&existing, &block);
    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&rc, updated)?;
    Ok(rc)
}

fn rc_file_for(shell: &str) -> crate::Result<PathBuf> {
    let home = env::var("HOME")?;
    let path = match shell {
        "zsh" => PathBuf::from(&home).join(".zshrc"),
        "bash" => PathBuf::from(&home).join(".bashrc"),
        "fish" => PathBuf::from(&home).join(".config/fish/config.fish"),
        other => return Err(format!("unsupported shell: {other}").into()),
    };
    Ok(path)
}

fn replace_or_append(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(MARKER_START), existing.find(MARKER_END)) {
        let end = end + MARKER_END.len();
        let mut out = String::new();
        out.push_str(existing[..start].trim_end_matches('\n'));
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(block);
        out.push('\n');
        out.push_str(existing[end..].trim_start_matches('\n'));
        return out;
    }
    let mut out = existing.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_covers_all_shells() {
        assert!(init_snippet("zsh").contains("gnav"));
        assert!(init_snippet("bash").contains("gnav"));
        assert!(init_snippet("fish").contains("function gnav"));
    }

    #[test]
    fn append_when_missing() {
        let out = replace_or_append("existing content", "BLOCK");
        assert!(out.contains("existing content"));
        assert!(out.trim_end().ends_with("BLOCK"));
    }

    #[test]
    fn append_to_empty_file() {
        let out = replace_or_append("", "BLOCK");
        assert_eq!(out.trim(), "BLOCK");
    }

    #[test]
    fn replace_when_present() {
        let block = format!("{MARKER_START}\nOLD_SNIPPET\n{MARKER_END}");
        let existing = format!("before\n{block}\nafter\n");
        let new = format!("{MARKER_START}\nNEW_SNIPPET\n{MARKER_END}");
        let out = replace_or_append(&existing, &new);
        assert!(out.contains("NEW_SNIPPET"));
        assert!(!out.contains("OLD_SNIPPET"));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn rc_file_maps_correctly() {
        // SAFETY: see config::tests — HOME is only read after we set it.
        unsafe { std::env::set_var("HOME", "/tmp/faux-home") };
        assert!(rc_file_for("zsh").unwrap().ends_with(".zshrc"));
        assert!(rc_file_for("bash").unwrap().ends_with(".bashrc"));
        assert!(
            rc_file_for("fish")
                .unwrap()
                .ends_with(".config/fish/config.fish")
        );
        assert!(rc_file_for("csh").is_err());
    }
}
