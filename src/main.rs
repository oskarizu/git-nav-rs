use std::process::ExitCode;

fn main() -> ExitCode {
    match git_nav::run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("git-nav: {e}");
            ExitCode::FAILURE
        }
    }
}
