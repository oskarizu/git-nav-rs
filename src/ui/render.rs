//! Plain-text table + ANSI helpers used as a non-TTY fallback.

use crate::git::RepoInfo;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";

pub fn print_table(infos: &[RepoInfo]) {
    let headers = ["#", "REPO", "ORG", "BRANCH", "SHA", "STATUS"];
    let rows: Vec<[String; 6]> = infos
        .iter()
        .enumerate()
        .map(|(i, info)| {
            [
                (i + 1).to_string(),
                info.name.clone(),
                info.org.clone(),
                info.branch.clone(),
                info.sha.clone(),
                format_status_ansi(info),
            ]
        })
        .collect();

    let mut widths = [0usize; 6];
    for (c, h) in headers.iter().enumerate() {
        widths[c] = vlen(h);
    }
    for row in &rows {
        for c in 0..6 {
            widths[c] = widths[c].max(vlen(&row[c]));
        }
    }

    let format_row = |row: &[String]| -> String {
        row.iter()
            .enumerate()
            .map(|(c, cell)| {
                let pad = widths[c].saturating_sub(vlen(cell));
                format!("{}{}", cell, " ".repeat(pad))
            })
            .collect::<Vec<_>>()
            .join("  ")
    };

    let header_row: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
    eprintln!("{BOLD}{}{RESET}", format_row(&header_row));
    eprintln!(
        "{}",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in &rows {
        eprintln!("{}", format_row(row));
    }
}

pub fn format_status_ansi(info: &RepoInfo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if info.dirty > 0 {
        parts.push(format!("{YELLOW}{} changed{RESET}", info.dirty));
    } else {
        parts.push(format!("{GREEN}clean{RESET}"));
    }
    if info.ahead > 0 {
        parts.push(format!("{CYAN}\u{2191}{}{RESET}", info.ahead));
    }
    if info.behind > 0 {
        parts.push(format!("{RED}\u{2193}{}{RESET}", info.behind));
    }
    parts.join(" ")
}

pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for nc in chars.by_ref() {
                if nc == 'm' {
                    break;
                }
            }
            continue;
        }
        result.push(c);
    }
    result
}

pub fn vlen(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_ansi_sequences() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(
            strip_ansi("\x1b[1;33mbold-yellow\x1b[0mtail"),
            "bold-yellowtail"
        );
    }

    #[test]
    fn vlen_ignores_ansi() {
        assert_eq!(vlen("\x1b[31mabc\x1b[0m"), 3);
    }
}
