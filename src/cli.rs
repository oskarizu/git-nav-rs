//! Manual arg parsing — kept dependency-free so compile times stay small
//! and the binary size doesn't balloon from clap.

use std::path::PathBuf;

use crate::config::expand_tilde;

#[derive(Debug, Default)]
pub struct ParsedArgs {
    pub command: Command,
    pub root_override: Option<PathBuf>,
    pub depth_override: Option<u32>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Command {
    #[default]
    Run,
    Help,
    Version,
    Init {
        shell: String,
    },
    InstallShell,
    ConfigPath,
    Reset,
}

pub fn parse(args: &[String]) -> Result<ParsedArgs, String> {
    let mut out = ParsedArgs::default();
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                out.command = Command::Help;
                return Ok(out);
            }
            "-V" | "--version" => {
                out.command = Command::Version;
                return Ok(out);
            }
            "--init" => {
                let shell = args
                    .get(i + 1)
                    .ok_or_else(|| "--init needs a shell (bash|zsh|fish)".to_string())?
                    .clone();
                out.command = Command::Init { shell };
                return Ok(out);
            }
            "--install-shell" => {
                out.command = Command::InstallShell;
                return Ok(out);
            }
            "--config-path" => {
                out.command = Command::ConfigPath;
                return Ok(out);
            }
            "--reset" => {
                out.command = Command::Reset;
                return Ok(out);
            }
            "-r" | "--root" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--root needs a value".to_string())?
                    .clone();
                out.root_override = Some(expand_tilde(&v));
                i += 1;
            }
            "-d" | "--depth" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--depth needs a value".to_string())?
                    .clone();
                let n: u32 = v
                    .parse()
                    .map_err(|_| format!("--depth expects a number, got {v}"))?;
                out.depth_override = Some(n);
                i += 1;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        std::iter::once("git-nav")
            .chain(v.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn defaults_to_run() {
        let p = parse(&args(&[])).unwrap();
        assert_eq!(p.command, Command::Run);
        assert!(p.root_override.is_none());
        assert!(p.depth_override.is_none());
    }

    #[test]
    fn help_short_and_long() {
        assert_eq!(parse(&args(&["-h"])).unwrap().command, Command::Help);
        assert_eq!(parse(&args(&["--help"])).unwrap().command, Command::Help);
    }

    #[test]
    fn init_captures_shell() {
        let p = parse(&args(&["--init", "zsh"])).unwrap();
        assert_eq!(
            p.command,
            Command::Init {
                shell: "zsh".into()
            }
        );
    }

    #[test]
    fn root_and_depth_override() {
        let p = parse(&args(&["--root", "/tmp", "--depth", "2"])).unwrap();
        assert_eq!(p.depth_override, Some(2));
        assert_eq!(
            p.root_override.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
    }

    #[test]
    fn unknown_arg_errors() {
        assert!(parse(&args(&["--bogus"])).is_err());
    }

    #[test]
    fn depth_needs_number() {
        assert!(parse(&args(&["--depth", "abc"])).is_err());
    }
}
