//! Persistent config stored at `~/.config/git-nav/config.toml`.
//! First run prompts for the root directory and writes it there.

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const DEFAULT_DEPTH: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub root: PathBuf,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    DEFAULT_DEPTH
}

impl Config {
    pub fn expanded_root(&self) -> PathBuf {
        let s = self.root.to_string_lossy();
        let expanded = expand_tilde(&s);
        fs::canonicalize(&expanded).unwrap_or(expanded)
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return PathBuf::from(home());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return PathBuf::from(home()).join(rest);
    }
    PathBuf::from(path)
}

fn home() -> String {
    env::var("HOME").unwrap_or_else(|_| "/".into())
}

/// XDG-style config path on every platform (`~/.config/git-nav/config.toml`
/// unless `XDG_CONFIG_HOME` is set).
pub fn path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("git-nav/config.toml");
    }
    PathBuf::from(home()).join(".config/git-nav/config.toml")
}

pub fn load_or_init() -> crate::Result<Config> {
    let path = path();
    if path.exists() {
        let data = fs::read_to_string(&path)?;
        let cfg: Config = toml::from_str(&data)?;
        return Ok(cfg);
    }
    prompt_and_save()
}

pub fn prompt_and_save() -> crate::Result<Config> {
    let default_root = default_initial_root();
    let interactive = io::stdin().is_terminal() && io::stderr().is_terminal();
    let root = if interactive {
        let mut stderr = io::stderr();
        writeln!(stderr, "Welcome to git-nav! Let's set up.")?;
        write!(
            stderr,
            "Top-level directory to scan for git repos [{}]: ",
            default_root.display()
        )?;
        stderr.flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let raw = buf.trim();
        if raw.is_empty() {
            default_root
        } else {
            expand_tilde(raw)
        }
    } else {
        default_root
    };

    let cfg = Config {
        root,
        depth: DEFAULT_DEPTH,
    };
    save(&cfg)?;
    Ok(cfg)
}

pub fn save(cfg: &Config) -> crate::Result<()> {
    let path = path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)?;
    fs::write(&path, text)?;
    Ok(())
}

fn default_initial_root() -> PathBuf {
    let projects = PathBuf::from(home()).join("projects");
    if projects.exists() {
        projects
    } else {
        PathBuf::from(home())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_home() {
        // SAFETY: env mutation is unsafe in Rust 2024 because env vars are
        // process-global. This test only reads HOME after setting it, and
        // no other test in this module races on HOME.
        unsafe { std::env::set_var("HOME", "/tmp/faux-home") };
        assert_eq!(expand_tilde("~"), PathBuf::from("/tmp/faux-home"));
        assert_eq!(
            expand_tilde("~/projects"),
            PathBuf::from("/tmp/faux-home/projects")
        );
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    }

    #[test]
    fn config_serialization_roundtrip() {
        let cfg = Config {
            root: PathBuf::from("/tmp/repos"),
            depth: 3,
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.root, cfg.root);
        assert_eq!(parsed.depth, cfg.depth);
    }

    #[test]
    fn depth_defaults_when_missing() {
        let cfg: Config = toml::from_str("root = \"/tmp/x\"\n").unwrap();
        assert_eq!(cfg.depth, DEFAULT_DEPTH);
    }
}
