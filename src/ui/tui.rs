//! Fullscreen ratatui selector. Arrow keys navigate, type-to-filter narrows
//! the list, Enter picks, Esc/Ctrl-C quits. Rendered on stderr so stdout
//! stays clean for the caller shell function.

use std::io;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Terminal;

use crate::git::RepoInfo;
use crate::ui::fuzzy;

/// RAII guard that always restores the terminal on drop — including panic
/// unwinds. Skipping any of these on exit leaves the shell in a state that
/// some terminals (notably Ghostty) read as "a command is still running",
/// which then triggers a spurious close-window warning.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> crate::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stderr(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub fn select(infos: &[RepoInfo]) -> crate::Result<Option<usize>> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)?;
    run_loop(&mut terminal, infos)
    // TerminalGuard::drop restores cursor, alt-screen, and raw mode.
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
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title = format!(" git-nav ({} / {}) ", filtered.len(), infos.len());
    let filter_line = Line::from(vec![
        Span::styled("filter: ", Style::default().fg(Color::DarkGray)),
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
    ]);
    let filter_para = Paragraph::new(filter_line).block(
        Block::default().borders(Borders::ALL).title(Span::styled(
            title,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
    );
    f.render_widget(filter_para, chunks[0]);

    let header = Row::new(vec!["REPO", "ORG", "BRANCH", "SHA", "STATUS"])
        .style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(0);

    let rows: Vec<Row> = filtered.iter().map(|&i| build_row(&infos[i])).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(28),
            Constraint::Percentage(18),
            Constraint::Percentage(22),
            Constraint::Length(8),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ")
    .block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    f.render_stateful_widget(table, chunks[1], state);

    let hint = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" move   "),
        Span::styled("type", Style::default().fg(Color::Cyan)),
        Span::raw(" filter   "),
        Span::styled("enter", Style::default().fg(Color::Cyan)),
        Span::raw(" cd   "),
        Span::styled("esc", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]);
    let footer = Paragraph::new(hint).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

fn build_row(info: &RepoInfo) -> Row<'static> {
    Row::new(vec![
        Cell::from(info.name.clone()),
        Cell::from(info.org.clone()).style(Style::default().fg(Color::Blue)),
        Cell::from(info.branch.clone()).style(Style::default().fg(Color::Magenta)),
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
        spans.push(Span::styled(
            "clean",
            Style::default().fg(Color::Green),
        ));
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

    fn ri(name: &str, org: &str, branch: &str) -> RepoInfo {
        RepoInfo {
            name: name.into(),
            path: format!("/tmp/{name}"),
            branch: branch.into(),
            sha: "abc1234".into(),
            ahead: 0,
            behind: 0,
            dirty: 0,
            org: org.into(),
        }
    }

    #[test]
    fn empty_filter_returns_all() {
        let infos = vec![ri("a", "o", "main"), ri("b", "o", "main")];
        assert_eq!(apply_filter(&infos, ""), vec![0, 1]);
    }

    #[test]
    fn filter_matches_name_org_or_branch() {
        let infos = vec![
            ri("api-gateway", "acme", "main"),
            ri("billing-svc", "acme", "feat/x"),
            ri("website", "personal", "main"),
        ];
        assert_eq!(apply_filter(&infos, "bill"), vec![1]);
        assert_eq!(apply_filter(&infos, "person"), vec![2]);
        assert_eq!(apply_filter(&infos, "feat"), vec![1]);
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
}
