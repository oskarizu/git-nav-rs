//! git-nav: a fast interactive navigator for local git repositories.
//!
//! The binary walks a configured root directory, gathers status for every
//! git repo it finds (in parallel), then hands the list off to a fullscreen
//! TUI selector. On selection, the chosen repo's path is printed to stdout
//! so a shell wrapper can `cd` into it.

pub mod cli;
pub mod config;
pub mod git;
pub mod scanner;
pub mod shell;
pub mod ui;

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use cli::{Command, ParsedArgs};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run() -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().collect();
    let parsed = cli::parse(&args)?;

    match parsed.command {
        Command::Help => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            println!("git-nav {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Command::Init { shell: sh } => {
            print!("{}", shell::init_snippet(&sh));
            Ok(ExitCode::SUCCESS)
        }
        Command::InstallShell => {
            let path = shell::install(None)?;
            eprintln!("Installed git-nav shell function to {}.", path.display());
            eprintln!("Reload your shell (e.g. `exec $SHELL -l`) and use `gnav`.");
            Ok(ExitCode::SUCCESS)
        }
        Command::ConfigPath => {
            println!("{}", config::path().display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Reset => {
            let cfg = config::prompt_and_save()?;
            eprintln!(
                "Config saved: root={}, depth={}",
                cfg.root.display(),
                cfg.depth
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Run => run_interactive(parsed),
    }
}

fn run_interactive(parsed: ParsedArgs) -> Result<ExitCode> {
    let mut cfg = config::load_or_init()?;
    if let Some(r) = parsed.root_override {
        cfg.root = r;
    }
    if let Some(d) = parsed.depth_override {
        cfg.depth = d;
    }

    let root = cfg.expanded_root();
    if !root.exists() {
        return Err(format!("root does not exist: {}", root.display()).into());
    }

    let repos = scanner::find_repos(&root, cfg.depth);
    if repos.is_empty() {
        eprintln!("No git repos found under {}", root.display());
        return Ok(ExitCode::SUCCESS);
    }

    let mut infos = git::gather_infos(&repos);
    infos.sort_by_key(|a| a.name.to_lowercase());

    let stdout_is_tty = io::stdout().is_terminal();
    let stderr_is_tty = io::stderr().is_terminal();

    let selection = if stderr_is_tty {
        ui::tui::select(&infos)?
    } else {
        ui::render::print_table(&infos);
        prompt_number(&infos)?
    };

    let Some(idx) = selection else {
        return Ok(ExitCode::SUCCESS);
    };

    let path = &infos[idx].path;
    if stdout_is_tty {
        eprintln!();
        eprintln!("Selected: {path}");
        eprintln!();
        eprintln!("cd didn't happen because git-nav was run directly.");
        eprintln!("Install the shell wrapper once with:  git-nav --install-shell");
        eprintln!("Or eval it in your rc file:           eval \"$(git-nav --init zsh)\"");
    } else {
        println!("{path}");
    }
    Ok(ExitCode::SUCCESS)
}

fn prompt_number(infos: &[git::RepoInfo]) -> Result<Option<usize>> {
    eprint!("\nSelect # to cd into (Enter to quit): ");
    io::stderr().flush().ok();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return Ok(None);
    }
    let choice = input.trim();
    if choice.is_empty() {
        return Ok(None);
    }
    match choice.parse::<usize>() {
        Ok(n) if n >= 1 && n <= infos.len() => Ok(Some(n - 1)),
        _ => {
            eprintln!("Invalid selection.");
            Ok(None)
        }
    }
}

fn print_help() {
    println!(
        "git-nav {} — jump between local git repos",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("USAGE:");
    println!("    git-nav [OPTIONS]");
    println!("    git-nav --init <bash|zsh|fish>");
    println!("    git-nav --install-shell");
    println!();
    println!("OPTIONS:");
    println!("    -r, --root <PATH>     Override configured root");
    println!("    -d, --depth <N>       Override configured max depth (default 4)");
    println!("        --config-path     Print the config file path and exit");
    println!("        --reset           Re-run the first-time setup prompt");
    println!("        --init <SHELL>    Print shell function for `eval`");
    println!("        --install-shell   Append the shell function to your rc file");
    println!("    -h, --help            Show this help");
    println!("    -V, --version         Show version");
}
