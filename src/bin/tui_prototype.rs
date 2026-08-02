// PROTOTYPE — wipe me. Throwaway TUI-variants prototype for wayfinder ticket
// #21 (UX track: TUI structure + keybindings). Answers: "what should way's
// TUI structure/keybindings look like, given it needs to scale past a
// single task list to 7 entity types, with a cleaner/friendlier feel?"
//
// Run: cargo run --bin tui-prototype
// Tab / Shift+Tab cycles variants. Opens on D by default — nick's reaction
// to round 1 (A/B/C): "don't like any of them, maybe some aspects of the
// first... I really like helix and zellij, default zellij is kinda nice."
// D synthesizes: Zellij's default numbered tab bar, Helix's minimal
// mode-badge status line + `:` command mode + `g`-prefix goto chords, and
// the clean/low-chrome list feel of yazi / taskwarrior-tui (both on the
// ratatui showcase). A/B/C kept for comparison, not because they won.
// Fixture data only, no store, no persistence. Not tested, not polished,
// not meant to be kept.

use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};

#[path = "../theme.rs"]
mod theme;

#[derive(Clone)]
struct Item {
    kind: &'static str,
    title: &'static str,
    pillar: &'static str,
    meta: &'static str,
    detail: &'static str,
}

fn fixture() -> Vec<Item> {
    vec![
        Item { kind: "obstacle", title: "spike: quality autonomous workflows", pillar: "craft", meta: "in-flight", detail: "Working WAY-5's alignment checkpoints. Currently at 'planning' phase." },
        Item { kind: "journal", title: "morning pages", pillar: "mind", meta: "today", detail: "Woke up thinking about the HOA board seat. Feels like a real chance to..." },
        Item { kind: "journal", title: "check-in: goals/progress", pillar: "mind", meta: "3 days ago", detail: "Q: What has your attention lately?\nA: Way's vision work, mostly." },
        Item { kind: "routine", title: "push day", pillar: "body", meta: "4 exercises", detail: "bench 4x8, ohp 3x10, dips 3x12, triceps 3x15 - ~42min, intensity 0.7" },
        Item { kind: "completion", title: "push day - logged", pillar: "body", meta: "yesterday", detail: "Felt strong on bench, dips were rough." },
        Item { kind: "person", title: "youngest", pillar: "relationships", meta: "child", detail: "Loves skating with dad. Currently into: gatorade glacier freeze." },
        Item { kind: "craft", title: "skateboarding", pillar: "craft", meta: "active", detail: "Standing: landed 50-50 stalls, sad 50-50 grinds on small transitions. Trajectory: build consistency, twice a week with the kids." },
        Item { kind: "craft", title: "woodworking", pillar: "craft", meta: "dormant", detail: "Standing: more tools than experience. Trajectory: long-horizon hobby to hone." },
        Item { kind: "stability", title: "safety net", pillar: "stability", meta: "active", detail: "Standing: building toward 6mo expenses. Trajectory: increase automatic transfer next quarter." },
        Item { kind: "stability", title: "retirement", pillar: "stability", meta: "needs attention", detail: "Standing: 401k contributions steady. IRR reenlistment decision pending, would change trajectory." },
        Item { kind: "principle", title: "meditations #1", pillar: "purpose", meta: "", detail: "You are not what you have mastered. You are what you are willing to risk becoming." },
        Item { kind: "obstacle", title: "fix: read/write lock handling", pillar: "craft", meta: "open", detail: "Concurrent opens on the same redb path shouldn't lock each other out." },
    ]
}

fn kind_glyph(kind: &str) -> &'static str {
    match kind {
        "obstacle" => "o",
        "journal" => "j",
        "routine" => "r",
        "completion" => "c",
        "person" => "p",
        "craft" => "k",
        "stability" => "s",
        "principle" => "P",
        _ => "?",
    }
}

fn pillar_color(pillar: &str) -> ratatui::style::Color {
    match pillar {
        "mind" => theme::AQUA,
        "body" => theme::GREEN,
        "relationships" => theme::ORANGE,
        "craft" => theme::FG,
        "stability" => theme::DIM,
        "purpose" => theme::RED,
        _ => theme::FG,
    }
}

const PILLARS: [&str; 7] = ["all", "mind", "body", "relationships", "craft", "stability", "purpose"];

struct App {
    variant: usize,
    items: Vec<Item>,
    selected: usize,
    // variant A
    pillar_filter: usize,
    command_mode: bool,
    command_input: String,
    split_detail: bool,
    // variant B
    detail_open: bool,
    help_open: bool,
    sidebar_group: usize,
    // variant C
    switcher_open: bool,
    switcher_input: String,
    // variant D
    d_command_mode: bool,
    d_command_input: String,
    d_goto_pending: bool,
    d_detail_open: bool,
}

impl App {
    fn new() -> Self {
        Self {
            variant: 3, // open on D — the synthesized, currently-best candidate
            items: fixture(),
            selected: 0,
            pillar_filter: 1, // start on "mind", not "all" - a real tab, not the catch-all
            command_mode: false,
            command_input: String::new(),
            split_detail: false,
            detail_open: false,
            help_open: false,
            sidebar_group: 0,
            switcher_open: false,
            switcher_input: String::new(),
            d_command_mode: false,
            d_command_input: String::new(),
            d_goto_pending: false,
            d_detail_open: false,
        }
    }

    fn filtered_a(&self) -> Vec<&Item> {
        if self.pillar_filter == 0 {
            self.items.iter().collect()
        } else {
            let p = PILLARS[self.pillar_filter];
            self.items.iter().filter(|i| i.pillar == p).collect()
        }
    }

    fn groups_b(&self) -> Vec<&'static str> {
        PILLARS[1..].to_vec()
    }

    fn group_items_b(&self) -> Vec<&Item> {
        let p = self.groups_b()[self.sidebar_group];
        self.items.iter().filter(|i| i.pillar == p).collect()
    }
}

fn main() -> Result<()> {
    let mut app = App::new();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if let Event::Key(key) = event::read()? {
            // global: variant cycling always wins, except while typing into a
            // text input (command bar / quick-switcher), where Tab still
            // cycles (no text field wants literal tabs here) but plain 'q'
            // should type instead of quit.
            let typing = app.command_mode || app.switcher_open || app.d_command_mode;

            match key.code {
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    app.variant = (app.variant + 3) % 4;
                    continue;
                }
                KeyCode::Tab => {
                    app.variant = (app.variant + 1) % 4;
                    continue;
                }
                KeyCode::BackTab => {
                    app.variant = (app.variant + 3) % 4;
                    continue;
                }
                _ => {}
            }

            if !typing && key.code == KeyCode::Char('q') {
                return Ok(());
            }

            match app.variant {
                0 => handle_variant_a(app, key.code),
                1 => handle_variant_b(app, key.code),
                2 => handle_variant_c(app, key.code),
                3 => handle_variant_d(app, key.code),
                _ => unreachable!(),
            }
        }
    }
}

// ---------- Variant A: single flowing pane, pillar tab strip, command bar ----------

fn handle_variant_a(app: &mut App, key: KeyCode) {
    if app.command_mode {
        match key {
            KeyCode::Esc => {
                app.command_mode = false;
                app.command_input.clear();
            }
            KeyCode::Enter => {
                app.command_mode = false;
                app.command_input.clear();
            }
            KeyCode::Backspace => {
                app.command_input.pop();
            }
            KeyCode::Char(c) => app.command_input.push(c),
            _ => {}
        }
        return;
    }
    let len = app.filtered_a().len().max(1);
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.selected = (app.selected + 1) % len,
        KeyCode::Char('k') | KeyCode::Up => app.selected = (app.selected + len - 1) % len,
        KeyCode::Char('[') => app.pillar_filter = (app.pillar_filter + PILLARS.len() - 1) % PILLARS.len(),
        KeyCode::Char(']') => app.pillar_filter = (app.pillar_filter + 1) % PILLARS.len(),
        KeyCode::Enter => app.split_detail = !app.split_detail,
        KeyCode::Char('/') => app.command_mode = true,
        _ => {}
    }
}

fn draw_variant_a(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // tab strip
            Constraint::Min(3),    // content (list [+ detail split])
            Constraint::Length(1), // status/command line
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("way", Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)))),
        chunks[0],
    );

    let mut tabs = Vec::new();
    for (i, p) in PILLARS.iter().enumerate() {
        let style = if i == app.pillar_filter {
            Style::default().fg(theme::AQUA).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::DIM)
        };
        tabs.push(Span::styled(format!(" {p} "), style));
    }
    frame.render_widget(Paragraph::new(Line::from(tabs)), chunks[1]);

    let content_area = chunks[2];
    let (list_area, detail_area) = if app.split_detail {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(content_area);
        (split[0], Some(split[1]))
    } else {
        (content_area, None)
    };

    let filtered = app.filtered_a();
    let lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == app.selected;
            let style = if selected {
                Style::default().bg(theme::SELECT_BG).fg(theme::FG)
            } else {
                Style::default().fg(theme::FG)
            };
            Line::from(vec![
                Span::styled(format!(" {} ", kind_glyph(item.kind)), Style::default().fg(pillar_color(item.pillar))),
                Span::styled(format!("{:<40}", item.title), style),
                Span::styled(format!("  {}", item.meta), Style::default().fg(theme::DIM)),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);

    if let Some(detail_area) = detail_area {
        frame.render_widget(Paragraph::new(Line::from(Span::styled("─".repeat(detail_area.width as usize), Style::default().fg(theme::DIM)))), Rect { height: 1, ..detail_area });
        if let Some(item) = filtered.get(app.selected) {
            let inner = Rect { y: detail_area.y + 1, height: detail_area.height.saturating_sub(1), ..detail_area };
            frame.render_widget(Paragraph::new(item.detail).wrap(ratatui::widgets::Wrap { trim: false }), inner);
        }
    }

    let status = if app.command_mode {
        Line::from(vec![Span::styled(format!("/{}", app.command_input), Style::default().fg(theme::AQUA))])
    } else {
        Line::from(Span::styled("j/k move  enter detail  [ ] pillar  / command  Tab variant  q quit", Style::default().fg(theme::DIM)))
    };
    frame.render_widget(Paragraph::new(status), chunks[3]);
}

// ---------- Variant B: sidebar + slide-over detail, on-demand help ----------

fn handle_variant_b(app: &mut App, key: KeyCode) {
    if app.help_open {
        if let KeyCode::Char('?') | KeyCode::Esc = key {
            app.help_open = false;
        }
        return;
    }
    if app.detail_open {
        match key {
            KeyCode::Esc => app.detail_open = false,
            KeyCode::Char('?') => app.help_open = true,
            _ => {}
        }
        return;
    }
    let groups_len = app.groups_b().len();
    let items_len = app.group_items_b().len().max(1);
    match key {
        KeyCode::Char('h') | KeyCode::Left => {
            app.sidebar_group = (app.sidebar_group + groups_len - 1) % groups_len;
            app.selected = 0;
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.sidebar_group = (app.sidebar_group + 1) % groups_len;
            app.selected = 0;
        }
        KeyCode::Char('j') | KeyCode::Down => app.selected = (app.selected + 1) % items_len,
        KeyCode::Char('k') | KeyCode::Up => app.selected = (app.selected + items_len - 1) % items_len,
        KeyCode::Enter => app.detail_open = true,
        KeyCode::Char('?') => app.help_open = true,
        _ => {}
    }
}

fn draw_variant_b(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(18), Constraint::Min(20)]).split(area);

    let groups = app.groups_b();
    let sidebar_lines: Vec<Line> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if i == app.sidebar_group {
                Line::from(vec![
                    Span::styled("▎", Style::default().fg(pillar_color(g))),
                    Span::styled(format!(" {g}"), Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(Span::styled(format!("  {g}"), Style::default().fg(theme::DIM)))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(sidebar_lines), cols[0]);

    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)]).split(cols[1]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!("{} › {}", groups[app.sidebar_group], "items"), Style::default().fg(theme::DIM)))),
        rows[0],
    );

    let group_items = app.group_items_b();
    let lines: Vec<Line> = group_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == app.selected;
            let style = if selected { Style::default().bg(theme::SELECT_BG).fg(theme::FG) } else { Style::default().fg(theme::FG) };
            Line::from(vec![
                Span::styled(format!(" {} ", kind_glyph(item.kind)), Style::default().fg(pillar_color(item.pillar))),
                Span::styled(format!("{:<40}", item.title), style),
                Span::styled(format!("  {}", item.meta), Style::default().fg(theme::DIM)),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[1]);

    frame.render_widget(Paragraph::new(Line::from(Span::styled("h/l group  j/k move  enter open  ? help  Tab variant  q quit", Style::default().fg(theme::DIM)))), rows[2]);

    if app.detail_open {
        if let Some(item) = group_items.get(app.selected) {
            let popup = centered_rect(area, 70, 60);
            frame.render_widget(Clear, popup);
            let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(pillar_color(item.pillar))).title(format!(" {} ", item.title));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            frame.render_widget(Paragraph::new(item.detail).wrap(ratatui::widgets::Wrap { trim: false }), inner);
        }
    }

    if app.help_open {
        let popup = centered_rect(area, 50, 50);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::AQUA)).title(" keybindings ");
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let help = "h/l   switch pillar group\nj/k   move selection\nenter open detail\nesc   close\n?     toggle this help\nTab   cycle prototype variant\nq     quit";
        frame.render_widget(Paragraph::new(help), inner);
    }
}

fn centered_rect(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - pct_y) / 2), Constraint::Percentage(pct_y), Constraint::Percentage((100 - pct_y) / 2)])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - pct_x) / 2), Constraint::Percentage(pct_x), Constraint::Percentage((100 - pct_x) / 2)])
        .split(vert[1])[1]
}

// ---------- Variant C: one-at-a-time focused card, fuzzy quick-switcher ----------

fn handle_variant_c(app: &mut App, key: KeyCode) {
    if app.switcher_open {
        match key {
            KeyCode::Esc => {
                app.switcher_open = false;
                app.switcher_input.clear();
            }
            KeyCode::Enter => {
                if let Some(idx) = PILLARS.iter().position(|p| p.starts_with(app.switcher_input.as_str()) && !app.switcher_input.is_empty()) {
                    app.pillar_filter = idx;
                    app.selected = 0;
                }
                app.switcher_open = false;
                app.switcher_input.clear();
            }
            KeyCode::Backspace => {
                app.switcher_input.pop();
            }
            KeyCode::Char(c) => app.switcher_input.push(c),
            _ => {}
        }
        return;
    }
    let len = app.filtered_a().len().max(1);
    match key {
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('l') | KeyCode::Right => app.selected = (app.selected + 1) % len,
        KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('h') | KeyCode::Left => app.selected = (app.selected + len - 1) % len,
        KeyCode::Char('g') => app.switcher_open = true,
        _ => {}
    }
}

fn draw_variant_c(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5), Constraint::Length(1)])
        .split(area);

    let filtered = app.filtered_a();
    let pillar_label = PILLARS[app.pillar_filter];
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!("way › {pillar_label}"), Style::default().fg(theme::DIM)))),
        rows[0],
    );

    if let Some(item) = filtered.get(app.selected) {
        let idx = format!("({}/{})", app.selected + 1, filtered.len());
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("{} ", kind_glyph(item.kind)), Style::default().fg(pillar_color(item.pillar))),
                Span::styled(item.title, Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {idx}"), Style::default().fg(theme::DIM)),
            ]),
            Line::from(""),
        ];
        for l in item.detail.lines() {
            lines.push(Line::from(Span::styled(l, Style::default().fg(theme::FG))));
        }
        let padded = Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[1] };
        frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), padded);
    }

    let status = if app.switcher_open {
        Line::from(vec![Span::styled(format!("go to: {}", app.switcher_input), Style::default().fg(theme::AQUA)), Span::styled("  (enter select, esc cancel)", Style::default().fg(theme::DIM))])
    } else {
        Line::from(Span::styled("j/k prev/next  g go-to  Tab variant  q quit", Style::default().fg(theme::DIM)))
    };
    frame.render_widget(Paragraph::new(status), rows[2]);

    if app.switcher_open {
        let popup = centered_rect(area, 40, 20);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme::AQUA)).title(" go to ");
        frame.render_widget(block, popup);
    }
}

// ---------- Variant D: Zellij tab bar + Helix status line/command mode ----------

fn handle_variant_d(app: &mut App, key: KeyCode) {
    if app.d_command_mode {
        match key {
            KeyCode::Esc => {
                app.d_command_mode = false;
                app.d_command_input.clear();
            }
            KeyCode::Enter => {
                app.d_command_mode = false;
                app.d_command_input.clear();
            }
            KeyCode::Backspace => {
                app.d_command_input.pop();
            }
            KeyCode::Char(c) => app.d_command_input.push(c),
            _ => {}
        }
        return;
    }
    if app.d_goto_pending {
        // helix-style chord: 'g' then a follow-up key completes the motion.
        // here: g + pillar's first letter jumps straight to that tab.
        app.d_goto_pending = false;
        if let KeyCode::Char(c) = key {
            if let Some(idx) = PILLARS.iter().position(|p| p.starts_with(c)) {
                app.pillar_filter = idx.max(1);
                app.selected = 0;
            }
        }
        return;
    }
    if app.d_detail_open {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => app.d_detail_open = false,
            _ => {}
        }
        return;
    }
    let len = app.filtered_a().len().max(1);
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.selected = (app.selected + 1) % len,
        KeyCode::Char('k') | KeyCode::Up => app.selected = (app.selected + len - 1) % len,
        KeyCode::Char(n @ '1'..='6') => {
            let idx = (n as u8 - b'0') as usize;
            if idx < PILLARS.len() {
                app.pillar_filter = idx;
                app.selected = 0;
            }
        }
        KeyCode::Char('g') => app.d_goto_pending = true,
        KeyCode::Enter => app.d_detail_open = true,
        KeyCode::Char(':') => app.d_command_mode = true,
        _ => {}
    }
}

fn draw_variant_d(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    // Zellij-style numbered tab bar: active tab is a solid colored block,
    // inactive tabs are dim plain text - no borders anywhere.
    let mut tabs = Vec::new();
    for (i, p) in PILLARS.iter().enumerate().skip(1) {
        if i == app.pillar_filter {
            tabs.push(Span::styled(format!(" {i} {p} "), Style::default().bg(pillar_color(p)).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)));
        } else {
            tabs.push(Span::styled(format!(" {i} {p} "), Style::default().fg(theme::DIM)));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(tabs)), rows[0]);

    let filtered = app.filtered_a();
    let lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == app.selected;
            let style = if selected { Style::default().bg(theme::SELECT_BG).fg(theme::FG) } else { Style::default().fg(theme::FG) };
            Line::from(vec![
                Span::styled(format!("  {} ", kind_glyph(item.kind)), Style::default().fg(pillar_color(item.pillar))),
                Span::styled(format!("{:<40}", item.title), style),
                Span::styled(format!("  {}", item.meta), Style::default().fg(theme::DIM)),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[1]);

    if app.d_detail_open {
        if let Some(item) = filtered.get(app.selected) {
            let popup = centered_rect(area, 72, 65);
            frame.render_widget(Clear, popup);
            let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(pillar_color(item.pillar))).title(format!(" {} ", item.title));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            frame.render_widget(Paragraph::new(item.detail).wrap(ratatui::widgets::Wrap { trim: false }), inner);
        }
    }

    // Helix-style status line: mode badge (left, colored block) + breadcrumb
    // (middle) + position (right). Swaps to a ':' command line when active.
    if app.d_command_mode {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" COMMAND ", Style::default().bg(theme::AQUA).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" :{}", app.d_command_input), Style::default().fg(theme::AQUA)),
            ])),
            rows[2],
        );
    } else if app.d_goto_pending {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" NORMAL ", Style::default().bg(theme::GREEN).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)),
                Span::styled(" g…  (m/b/r/c/s/p: jump to pillar)", Style::default().fg(theme::AQUA)),
            ])),
            rows[2],
        );
    } else {
        let pos = if filtered.is_empty() { "0/0".to_string() } else { format!("{}/{}", app.selected + 1, filtered.len()) };
        let mode_badge = Span::styled(" NORMAL ", Style::default().bg(theme::GREEN).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD));
        let breadcrumb = Span::styled(format!("  way › {}   ", PILLARS[app.pillar_filter]), Style::default().fg(theme::FG));
        let hint = Span::styled("1-6 tab  j/k move  enter open  g goto  : cmd  Tab variant  q quit   ", Style::default().fg(theme::DIM));
        frame.render_widget(
            Paragraph::new(Line::from(vec![mode_badge, breadcrumb, hint, Span::styled(pos, Style::default().fg(theme::DIM))])),
            rows[2],
        );
    }
}

// ---------- shared frame ----------

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(3), Constraint::Length(1)]).split(area);

    match app.variant {
        0 => draw_variant_a(frame, app, rows[0]),
        1 => draw_variant_b(frame, app, rows[0]),
        2 => draw_variant_c(frame, app, rows[0]),
        3 => draw_variant_d(frame, app, rows[0]),
        _ => unreachable!(),
    }

    let names = ["A — command palette", "B — sidebar + slide-over", "C — focused pager + quick-switch", "D — Zellij tabs + Helix status/command"];
    let indicator = Line::from(vec![
        Span::styled(" PROTOTYPE ", Style::default().bg(theme::RED).fg(theme::FG).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}  ", names[app.variant]), Style::default().bg(theme::SELECT_BG).fg(theme::FG)),
        Span::styled(" Tab/Shift+Tab: cycle ", Style::default().fg(theme::DIM)),
    ]);
    frame.render_widget(Paragraph::new(indicator), rows[1]);
}
