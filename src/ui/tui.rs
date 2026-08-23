//! Inline ratatui selector. Arrow keys navigate, type-to-filter narrows
//! the list, Enter picks, Esc/Ctrl-C quits. Rendered as a bottom-anchored
//! inline viewport on stderr, so the shell's scrollback context stays put
//! and stdout stays clean for the caller shell function.

use std::io;

use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, Clear, ClearType, disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::git::RepoInfo;
use crate::ui::fuzzy;

/// Chrome rows around the table (filter line + table header line + hint).
const CHROME_ROWS: u16 = 4;
/// Never render smaller than this — below it the widget is unusable.
const MIN_HEIGHT: u16 = 6;
/// Never render taller than this even on huge terminals.
const MAX_HEIGHT: u16 = 20;

/// RAII guard that always restores the terminal on drop — including panic
/// unwinds. Skipping raw-mode teardown leaves the shell in a state some
/// terminals (notably Ghostty) read as "a command is still running", which
/// triggers a spurious close-window warning.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> crate::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stderr(), Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), Show);
        let _ = disable_raw_mode();
    }
}

/// RAII helper that temporarily points stdout at `/dev/tty`.
///
/// Ratatui's inline viewport calls `crossterm::cursor::position()`, which
/// writes a CPR query (`\x1b[6n`) to stdout and reads the response from
/// stdin. When the shell wrapper runs `$(git-nav …)`, stdout is a pipe
/// (not the tty), so the query vanishes and crossterm times out. We
/// redirect stdout to `/dev/tty` for the picker's lifetime, then restore
/// it so `println!("{path}")` still lands in the wrapper's pipe.
#[cfg(unix)]
struct StdoutToTty {
    saved: libc::c_int,
}

#[cfg(unix)]
impl StdoutToTty {
    fn install() -> crate::Result<Self> {
        use std::fs::OpenOptions;
        use std::io::Write as _;
        use std::os::unix::io::AsRawFd;

        io::stdout().flush().ok();
        let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let tty = match OpenOptions::new().write(true).open("/dev/tty") {
            Ok(f) => f,
            Err(e) => {
                unsafe { libc::close(saved) };
                return Err(e.into());
            }
        };
        let rc = unsafe { libc::dup2(tty.as_raw_fd(), libc::STDOUT_FILENO) };
        if rc < 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(e.into());
        }
        Ok(Self { saved })
    }
}

#[cfg(unix)]
impl Drop for StdoutToTty {
    fn drop(&mut self) {
        use std::io::Write as _;
        io::stdout().flush().ok();
        unsafe {
            libc::dup2(self.saved, libc::STDOUT_FILENO);
            libc::close(self.saved);
        }
    }
}

pub fn select(infos: &[RepoInfo]) -> crate::Result<Option<usize>> {
    // Order matters: the stdout redirect must exist before ratatui's
    // first draw (which queries cursor position), and must outlive the
    // terminal guard so cleanup ANSI sequences also land on the tty.
    #[cfg(unix)]
    let _stdout_redirect = StdoutToTty::install()?;
    let _guard = TerminalGuard::enter()?;

    let (_, term_h) = terminal::size().unwrap_or((80, 24));
    let height = picker_height(term_h, infos.len());

    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )?;

    let result = run_loop(&mut terminal, infos);
    drop(terminal);

    // Reclaim the picker's rows so the shell prompt returns to the line
    // where it started — `terminal.clear()` on an inline viewport only
    // wipes the content, it doesn't remove the reserved space. Walking
    // the cursor up to the viewport's top row and clearing from there
    // to the end of the screen collapses the whole picker area, so the
    // next output (a shell prompt) begins exactly where the picker did.
    let _ = execute!(
        io::stderr(),
        MoveToColumn(0),
        MoveUp(height.saturating_sub(1)),
        Clear(ClearType::FromCursorDown),
    );

    result
}

fn picker_height(term_h: u16, repo_count: usize) -> u16 {
    let ideal = (repo_count as u16).saturating_add(CHROME_ROWS + 1);
    let capped = ideal.clamp(MIN_HEIGHT, MAX_HEIGHT);
    let cap_by_term = term_h.saturating_sub(1).max(MIN_HEIGHT);
    capped.min(cap_by_term)
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    infos: &[RepoInfo],
) -> crate::Result<Option<usize>> {
    let mut filter = String::new();
    let mut state = TableState::default();
    state.select(Some(0));

    loop {
        let filtered = apply_filter(infos, &filter);
        clamp_selection(&mut state, filtered.len());

        terminal.draw(|f| draw(f, infos, &filtered, &filter, &mut state))?;

        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event::read()?
        else {
            continue;
        };
        if kind != KeyEventKind::Press {
            continue;
        }

        match code {
            KeyCode::Esc => return Ok(None),
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
            KeyCode::Enter => {
                if let Some(cur) = state.selected() {
                    if let Some(&idx) = filtered.get(cur) {
                        return Ok(Some(idx));
                    }
                }
            }
            KeyCode::Up => move_selection(&mut state, -1, filtered.len()),
            KeyCode::Down => move_selection(&mut state, 1, filtered.len()),
            KeyCode::PageUp => move_selection(&mut state, -10, filtered.len()),
            KeyCode::PageDown => move_selection(&mut state, 10, filtered.len()),
            KeyCode::Home => {
                if filtered.is_empty() {
                    state.select(None);
                } else {
                    state.select(Some(0));
                }
            }
            KeyCode::End => {
                if filtered.is_empty() {
                    state.select(None);
                } else {
                    state.select(Some(filtered.len() - 1));
                }
            }
            KeyCode::Backspace => {
                filter.pop();
            }
            KeyCode::Char(c) => filter.push(c),
            _ => {}
        }
    }
}

fn apply_filter(infos: &[RepoInfo], filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..infos.len()).collect();
    }
    infos
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            fuzzy::matches(filter, &r.name)
                || fuzzy::matches(filter, &r.org)
                || fuzzy::matches(filter, &r.branch)
                || fuzzy::matches(filter, &r.author)
        })
        .map(|(i, _)| i)
        .collect()
}

fn clamp_selection(state: &mut TableState, len: usize) {
    if len == 0 {
        state.select(None);
        return;
    }
    let cur = state.selected().unwrap_or(0);
    state.select(Some(cur.min(len - 1)));
}

fn move_selection(state: &mut TableState, delta: i32, len: usize) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0) as i32;
    let next = (cur + delta).clamp(0, len as i32 - 1) as usize;
    state.select(Some(next));
}

fn draw(
    f: &mut ratatui::Frame,
    infos: &[RepoInfo],
    filtered: &[usize],
    filter: &str,
    state: &mut TableState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter line
            Constraint::Min(2),    // table (with top separator)
            Constraint::Length(2), // hint (with top separator)
        ])
        .split(f.area());

    let counter = format!("{}/{}", filtered.len(), infos.len());
    let filter_line = Paragraph::new(Line::from(vec![
        Span::styled(" filter: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            filter,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "▏",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
        Span::raw("  "),
        Span::styled(counter, Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(filter_line, chunks[0]);

    let header = Row::new(vec!["REPO", "ORG", "BRANCH", "AUTHOR", "SHA", "STATUS"]).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = filtered.iter().map(|&i| build_row(&infos[i])).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(24),
            Constraint::Percentage(14),
            Constraint::Percentage(20),
            Constraint::Percentage(16),
            Constraint::Length(8),
            Constraint::Percentage(16),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ")
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_stateful_widget(table, chunks[1], state);

    let hint = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" move   "),
        Span::styled("type", Style::default().fg(Color::Cyan)),
        Span::raw(" filter   "),
        Span::styled("enter", Style::default().fg(Color::Cyan)),
        Span::raw(" cd   "),
        Span::styled("esc", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]))
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(hint, chunks[2]);
}

fn build_row(info: &RepoInfo) -> Row<'static> {
    Row::new(vec![
        Cell::from(info.name.clone()),
        Cell::from(info.org.clone()).style(Style::default().fg(Color::Blue)),
        Cell::from(info.branch.clone()).style(Style::default().fg(Color::Magenta)),
        Cell::from(info.author.clone()).style(Style::default().fg(Color::Gray)),
        Cell::from(info.sha.clone()).style(Style::default().fg(Color::DarkGray)),
        Cell::from(status_line(info)),
    ])
}

fn status_line(info: &RepoInfo) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if info.dirty > 0 {
        spans.push(Span::styled(
            format!("{} changed", info.dirty),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        spans.push(Span::styled("clean", Style::default().fg(Color::Green)));
    }
    if info.ahead > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("↑{}", info.ahead),
            Style::default().fg(Color::Cyan),
        ));
    }
    if info.behind > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("↓{}", info.behind),
            Style::default().fg(Color::Red),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ri(name: &str, org: &str, branch: &str, author: &str) -> RepoInfo {
        RepoInfo {
            name: name.into(),
            path: format!("/tmp/{name}"),
            branch: branch.into(),
            sha: "abc1234".into(),
            ahead: 0,
            behind: 0,
            dirty: 0,
            org: org.into(),
            author: author.into(),
        }
    }

    #[test]
    fn empty_filter_returns_all() {
        let infos = vec![ri("a", "o", "main", "-"), ri("b", "o", "main", "-")];
        assert_eq!(apply_filter(&infos, ""), vec![0, 1]);
    }

    #[test]
    fn filter_matches_name_org_or_branch() {
        let infos = vec![
            ri("api-gateway", "acme", "main", "-"),
            ri("billing-svc", "acme", "feat/x", "-"),
            ri("website", "personal", "main", "-"),
        ];
        assert_eq!(apply_filter(&infos, "bill"), vec![1]);
        assert_eq!(apply_filter(&infos, "person"), vec![2]);
        assert_eq!(apply_filter(&infos, "feat"), vec![1]);
    }

    #[test]
    fn filter_matches_author() {
        let infos = vec![
            ri("api-gateway", "acme", "main", "Oscar García"),
            ri("billing-svc", "acme", "feat/x", "Ada Lovelace"),
        ];
        assert_eq!(apply_filter(&infos, "oscar"), vec![0]);
        assert_eq!(apply_filter(&infos, "ada"), vec![1]);
    }

    #[test]
    fn clamp_selection_handles_empty() {
        let mut s = TableState::default();
        s.select(Some(5));
        clamp_selection(&mut s, 0);
        assert_eq!(s.selected(), None);
    }

    #[test]
    fn clamp_selection_caps_at_last() {
        let mut s = TableState::default();
        s.select(Some(10));
        clamp_selection(&mut s, 3);
        assert_eq!(s.selected(), Some(2));
    }

    #[test]
    fn move_selection_clamps_edges() {
        let mut s = TableState::default();
        s.select(Some(0));
        move_selection(&mut s, -1, 5);
        assert_eq!(s.selected(), Some(0));
        move_selection(&mut s, 100, 5);
        assert_eq!(s.selected(), Some(4));
    }

    #[test]
    fn picker_height_scales_with_repo_count() {
        // small: chrome + 1 extra + repos, but clamped to MIN
        assert_eq!(picker_height(50, 1), MIN_HEIGHT);
        // medium: fits without clamping
        assert_eq!(picker_height(50, 10), 10 + CHROME_ROWS + 1);
        // large: clamped at MAX
        assert_eq!(picker_height(50, 100), MAX_HEIGHT);
    }

    #[test]
    fn picker_height_respects_short_terminal() {
        // Terminal too short to fit our ideal — cap to term_h - 1.
        let h = picker_height(8, 20);
        assert_eq!(h, 7);
    }
}
