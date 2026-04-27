//! `skillctl use` 的 TUI 多选界面。
//!
//! 见 REQ.md §8.3。

#![forbid(unsafe_code)]
#![allow(clippy::expect_used)] // TUI 入口边界小范围允许

use std::collections::HashSet;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;

/// TUI 入口候选项。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: SkillId,
    pub summary: String,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    /// 当前是否已被项目启用。
    pub enabled: bool,
    /// 当前的 tier（仅当 enabled 时有意义）。
    pub tier: Option<skillctl_core::Tier>,
}

#[derive(Debug)]
struct App {
    items: Vec<Candidate>,
    visible: Vec<usize>, // 过滤后剩余的 items 索引
    list: ListState,
    selected: HashSet<usize>,
    core_tagged: HashSet<usize>,
    filter: String,
    cancelled: bool,
}

impl App {
    fn new(items: Vec<Candidate>) -> Self {
        let mut selected = HashSet::new();
        let mut core_tagged = HashSet::new();
        for (i, c) in items.iter().enumerate() {
            if c.enabled {
                selected.insert(i);
            }
            if matches!(c.tier, Some(skillctl_core::Tier::Core)) {
                core_tagged.insert(i);
            }
        }
        let visible: Vec<usize> = (0..items.len()).collect();
        let mut list = ListState::default();
        if !visible.is_empty() {
            list.select(Some(0));
        }
        Self {
            items,
            visible,
            list,
            selected,
            core_tagged,
            filter: String::new(),
            cancelled: false,
        }
    }

    fn refresh_filter(&mut self) {
        let q = self.filter.to_ascii_lowercase();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                if q.is_empty() {
                    return true;
                }
                c.id.to_string().to_ascii_lowercase().contains(&q)
                    || c.summary.to_ascii_lowercase().contains(&q)
                    || c.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect();
        if self.visible.is_empty() {
            self.list.select(None);
        } else {
            self.list.select(Some(0));
        }
    }

    fn cursor(&self) -> Option<usize> {
        self.list.selected().and_then(|v| self.visible.get(v).copied())
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let cur = self.list.selected().unwrap_or(0) as isize;
        let new = (cur + delta).rem_euclid(self.visible.len() as isize) as usize;
        self.list.select(Some(new));
    }

    fn toggle_select(&mut self) {
        if let Some(idx) = self.cursor() {
            if self.selected.contains(&idx) {
                self.selected.remove(&idx);
                self.core_tagged.remove(&idx);
            } else {
                self.selected.insert(idx);
            }
        }
    }

    fn toggle_core(&mut self) {
        if let Some(idx) = self.cursor() {
            if self.core_tagged.contains(&idx) {
                self.core_tagged.remove(&idx);
            } else {
                self.selected.insert(idx);
                self.core_tagged.insert(idx);
            }
        }
    }
}

/// 来自 `skillctl add` 多 skill 仓库的待安装候选项。
#[derive(Debug, Clone)]
pub struct DiscoverItem {
    pub index: usize,
    pub proposed_id: String,
    pub description: String,
    pub path: String,
}

/// `skillctl add github:repo` 的 TUI：多选 → 返回选中的 index 列表。
pub fn select_for_install(items: Vec<DiscoverItem>) -> Result<Vec<usize>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let mut terminal = ratatui::init();
    let result = run_install_loop(&mut terminal, items);
    ratatui::restore();
    result
}

fn run_install_loop(
    terminal: &mut ratatui::DefaultTerminal,
    items: Vec<DiscoverItem>,
) -> Result<Vec<usize>> {
    let mut selected: HashSet<usize> = HashSet::new();
    let mut cursor: usize = 0;
    let mut cancelled = false;

    loop {
        terminal
            .draw(|frame| draw_install(frame, &items, &selected, cursor))
            .map_err(|e| Error::other(format!("tui draw: {e}")))?;
        let evt = crossterm::event::read().map_err(|e| Error::other(format!("tui event: {e}")))?;
        let Event::Key(key) = evt else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                cancelled = true;
                break;
            }
            KeyCode::Enter => break,
            KeyCode::Up => {
                if cursor > 0 {
                    cursor -= 1;
                }
            }
            KeyCode::Down => {
                if cursor + 1 < items.len() {
                    cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if selected.contains(&cursor) {
                    selected.remove(&cursor);
                } else {
                    selected.insert(cursor);
                }
            }
            KeyCode::Char('a') => {
                if selected.len() == items.len() {
                    selected.clear();
                } else {
                    selected = (0..items.len()).collect();
                }
            }
            _ => {}
        }
    }
    if cancelled {
        return Err(Error::other("cancelled"));
    }
    let mut out: Vec<usize> = selected.into_iter().collect();
    out.sort_unstable();
    Ok(out)
}

fn draw_install(
    frame: &mut ratatui::Frame,
    items: &[DiscoverItem],
    selected: &HashSet<usize>,
    cursor: usize,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let title = format!("skillctl add — discovered {} skills, select to install", items.len());
    let header =
        Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("skillctl add"));
    frame.render_widget(header, chunks[0]);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let mark = if selected.contains(&i) { "[x]" } else { "[ ]" };
            let line = Line::from(vec![
                Span::styled(mark, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(&it.proposed_id, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(&it.description, Style::default().fg(Color::Gray)),
                Span::raw("  "),
                Span::styled(format!("({})", &it.path), Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(cursor));
    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " {}/{} selected ",
            selected.len(),
            items.len()
        )))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let help =
        Paragraph::new("↑↓ move · Space toggle · a (de)select all · Enter install · Esc/q cancel")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help, chunks[2]);
}

/// 启动 TUI 多选。
///
/// 返回 (core_ids, extra_ids)。取消时返回 `Error::Other("cancelled")`。
pub fn select(candidates: Vec<Candidate>) -> Result<(Vec<SkillId>, Vec<SkillId>)> {
    if candidates.is_empty() {
        return Err(Error::other("no candidates available; run `skillctl add` first"));
    }
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, App::new(candidates));
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut ratatui::DefaultTerminal,
    mut app: App,
) -> Result<(Vec<SkillId>, Vec<SkillId>)> {
    loop {
        terminal
            .draw(|frame| draw(frame, &mut app))
            .map_err(|e| Error::other(format!("tui draw: {e}")))?;
        let evt = crossterm::event::read().map_err(|e| Error::other(format!("tui event: {e}")))?;
        let Event::Key(key) = evt else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Esc => {
                app.cancelled = true;
                break;
            }
            KeyCode::Char('q') if app.filter.is_empty() => {
                app.cancelled = true;
                break;
            }
            KeyCode::Enter => break,
            KeyCode::Up => app.move_cursor(-1),
            KeyCode::Down => app.move_cursor(1),
            KeyCode::Char(' ') if app.filter.is_empty() => app.toggle_select(),
            KeyCode::Char('c') if app.filter.is_empty() => app.toggle_core(),
            KeyCode::Char('/') => {
                app.filter.clear();
                app.filter.push(' ');
                app.filter.pop();
            }
            KeyCode::Backspace => {
                app.filter.pop();
                app.refresh_filter();
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                if app.filter.is_empty() && (ch == 'q' || ch == ' ' || ch == 'c') {
                    continue;
                }
                app.filter.push(ch);
                app.refresh_filter();
            }
            _ => {}
        }
    }
    if app.cancelled {
        return Err(Error::other("cancelled"));
    }
    let mut core = Vec::new();
    let mut extra = Vec::new();
    for (i, item) in app.items.iter().enumerate() {
        if !app.selected.contains(&i) {
            continue;
        }
        if app.core_tagged.contains(&i) {
            core.push(item.id.clone());
        } else {
            extra.push(item.id.clone());
        }
    }
    Ok((core, extra))
}

fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    // 顶部：filter
    let title = format!(
        "skillctl use — filter: {}{}",
        app.filter,
        if app.filter.is_empty() { " (type to filter)" } else { "" }
    );
    let header =
        Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("skillctl"));
    frame.render_widget(header, chunks[0]);

    // 列表
    let items: Vec<ListItem> = app
        .visible
        .iter()
        .map(|&idx| {
            let c = &app.items[idx];
            let mark = if app.core_tagged.contains(&idx) {
                "[C]"
            } else if app.selected.contains(&idx) {
                "[x]"
            } else {
                "[ ]"
            };
            let langs = if c.languages.is_empty() {
                String::new()
            } else {
                format!(" ({})", c.languages.join(","))
            };
            let line = Line::from(vec![
                Span::styled(mark, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(c.id.to_string(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(langs),
                Span::raw("  "),
                Span::styled(c.summary.clone(), Style::default().fg(Color::Gray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " skills ({} / {}) ",
            app.visible.len(),
            app.items.len()
        )))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, chunks[1], &mut app.list);

    // 底部：帮助行
    let help = Paragraph::new(
        "↑↓ move · Space toggle · c mark as core · type to filter · Enter save · Esc/q cancel",
    )
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help, chunks[2]);
}
