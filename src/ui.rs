use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, ConfirmKind, Field, Mode, View};
use crate::backlog::BacklogRef;
use crate::craft::CraftStatus;
use crate::journal::JournalEntryKind;
use crate::person::RelationshipKind;
use crate::stability::StabilityStatus;
use crate::task::PillarDef;
use crate::theme;

fn pillar_def_color(def: &PillarDef) -> Color {
    Color::Rgb(def.color.0, def.color.1, def.color.2)
}

fn find_pillar_def<'a>(app: &'a App, name: &str) -> Option<&'a PillarDef> {
    app.active_profile.pillars.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

fn relative_time(unix_seconds: i64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(unix_seconds);
    let delta = (now - unix_seconds).max(0);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    draw_header(frame, app, outer[0]);
    if let Mode::Editing(field) = app.mode {
        draw_edit_form(frame, app, outer[1], field);
    } else {
        draw_backlog(frame, app, outer[1]);
        if let Mode::Detail = app.mode {
            draw_detail_popup(frame, app, outer[1]);
        }
    }
    draw_status_line(frame, app, outer[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let summary = match app.view {
        View::Active => {
            let open = app.tasks.iter().filter(|t| !t.done).count();
            format!(
                "way › backlog   {open} open obstacle{}  ·  {} across {} pillars",
                if open == 1 { "" } else { "s" },
                app.backlog.len(),
                6
            )
        }
        View::Archived => format!("way › archived   {} obstacle{}", app.tasks.len(), if app.tasks.len() == 1 { "" } else { "s" }),
    };
    frame.render_widget(Paragraph::new(Span::styled(summary, Style::default().fg(theme::DIM))), area);
}

fn kind_glyph(r: BacklogRef, app: &App) -> (&'static str, Color) {
    match r {
        BacklogRef::Task(i) => {
            let task = &app.tasks[i];
            if task.done { ("✓", theme::GREEN) } else { ("▸", theme::ORANGE) }
        }
        BacklogRef::Journal(_) => ("j", pillar_color(app, "mind")),
        BacklogRef::Routine(_) => ("r", pillar_color(app, "body")),
        BacklogRef::Person(_) => ("p", pillar_color(app, "relationships")),
        BacklogRef::Craft(_) => ("k", pillar_color(app, "craft")),
        BacklogRef::Stability(_) => ("s", pillar_color(app, "stability")),
        BacklogRef::Principle(_) => ("P", pillar_color(app, "purpose")),
    }
}

fn pillar_color(app: &App, name: &str) -> Color {
    find_pillar_def(app, name).map(pillar_def_color).unwrap_or(theme::FG)
}

fn row_title(r: BacklogRef, app: &App) -> String {
    match r {
        BacklogRef::Task(i) => app.tasks[i].title.clone(),
        BacklogRef::Journal(i) => {
            let entry = &app.journal[i];
            let kind = match entry.kind {
                JournalEntryKind::Freeform => "",
                JournalEntryKind::CheckIn => "check-in: ",
            };
            let first_line = entry.content.lines().next().unwrap_or("").trim();
            format!("{kind}{first_line}")
        }
        BacklogRef::Routine(i) => app.routines[i].name.clone(),
        BacklogRef::Person(i) => app.people[i].name.clone(),
        BacklogRef::Craft(i) => app.crafts[i].name.clone(),
        BacklogRef::Stability(i) => app.stability[i].name.clone(),
        BacklogRef::Principle(i) => truncate(app.principles[i].text.as_str(), 60),
    }
}

fn row_meta(r: BacklogRef, app: &App) -> String {
    match r {
        BacklogRef::Task(i) => {
            let task = &app.tasks[i];
            let mut meta = format!("WAY-{}", task.key);
            if task.waiting_on.is_some() {
                meta.push_str(" · waiting");
            }
            meta
        }
        BacklogRef::Journal(i) => relative_time(app.journal[i].created_at),
        BacklogRef::Routine(i) => format!("{} exercise{}", app.routines[i].exercises.len(), if app.routines[i].exercises.len() == 1 { "" } else { "s" }),
        BacklogRef::Person(i) => match app.people[i].relationship {
            RelationshipKind::Child => "child".to_string(),
            RelationshipKind::Partner => "partner".to_string(),
        },
        BacklogRef::Craft(i) => status_label(app.crafts[i].status),
        BacklogRef::Stability(i) => status_label_stability(app.stability[i].status),
        BacklogRef::Principle(i) => relative_time(app.principles[i].created_at),
    }
}

/// WAY-8 thread 1: a leading, at-a-glance marker - `waiting_on` takes
/// visual priority over mere session-liveness (it's the more actionable
/// signal, "needs you right now" vs. "a session happens to be open"), so
/// this is one glyph slot, not two competing ones. Only meaningful for
/// Task rows; the other six kinds have no session concept.
fn session_indicator(r: BacklogRef, app: &App) -> Span<'static> {
    let BacklogRef::Task(i) = r else { return Span::raw("  ") };
    let task = &app.tasks[i];
    if task.waiting_on.is_some() {
        return Span::styled("● ", Style::default().fg(theme::RED));
    }
    match app.session_status(task) {
        crate::app::SessionStatus::Live => Span::styled("● ", Style::default().fg(theme::AQUA)),
        crate::app::SessionStatus::Idle => Span::styled("○ ", Style::default().fg(theme::DIM)),
        crate::app::SessionStatus::None => Span::raw("  "),
    }
}

fn row_pillar_chip(r: BacklogRef, app: &App) -> Option<(String, Color)> {
    match r {
        BacklogRef::Task(i) => {
            let pillar = app.tasks[i].pillar.as_deref()?;
            let def = find_pillar_def(app, pillar)?;
            Some((def.name.clone(), pillar_def_color(def)))
        }
        BacklogRef::Journal(_) => Some(("mind".to_string(), pillar_color(app, "mind"))),
        BacklogRef::Routine(_) => Some(("body".to_string(), pillar_color(app, "body"))),
        BacklogRef::Person(_) => Some(("relationships".to_string(), pillar_color(app, "relationships"))),
        BacklogRef::Craft(_) => Some(("craft".to_string(), pillar_color(app, "craft"))),
        BacklogRef::Stability(_) => Some(("stability".to_string(), pillar_color(app, "stability"))),
        BacklogRef::Principle(_) => Some(("purpose".to_string(), pillar_color(app, "purpose"))),
    }
}

fn status_label(s: CraftStatus) -> String {
    match s {
        CraftStatus::Active => "active".to_string(),
        CraftStatus::Dormant => "dormant".to_string(),
        CraftStatus::Historical => "historical".to_string(),
    }
}

fn status_label_stability(s: StabilityStatus) -> String {
    match s {
        StabilityStatus::Active => "active".to_string(),
        StabilityStatus::Dormant => "dormant".to_string(),
        StabilityStatus::Historical => "historical".to_string(),
    }
}

fn rounded(border_color: Color) -> Block<'static> {
    Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border_color))
}

fn draw_backlog(frame: &mut Frame, app: &App, area: Rect) {
    let inner_width = area.width as usize;
    let items: Vec<ListItem> = app
        .backlog
        .iter()
        .map(|&r| {
            let (glyph, glyph_color) = kind_glyph(r, app);
            let title = row_title(r, app);
            let meta = row_meta(r, app);

            let mut spans = vec![session_indicator(r, app)];
            let mut prefix_len = 2;
            spans.push(Span::styled(format!(" {glyph} "), Style::default().fg(glyph_color)));
            prefix_len += 3;

            if let Some((label, color)) = row_pillar_chip(r, app) {
                let chip = format!("[{label}] ");
                prefix_len += chip.chars().count();
                spans.push(Span::styled(chip, Style::default().fg(color)));
            }

            let title_style = match r {
                BacklogRef::Task(i) if app.tasks[i].done => Style::default().fg(theme::DIM),
                _ => Style::default().fg(theme::FG),
            };
            let budget = inner_width.saturating_sub(prefix_len + meta.chars().count() + 2);
            spans.push(Span::styled(format!("{:<width$}", truncate(&title, budget), width = budget), title_style));
            spans.push(Span::styled(format!("  {meta}"), Style::default().fg(theme::DIM)));

            ListItem::new(Line::from(spans))
        })
        .collect();

    // WAY-5's flagged quirk, fixed here: no .fg() in highlight_style - ratatui
    // patches a Line's own per-span styles with this on the selected row, so
    // setting an explicit fg used to flatten every row's own color (the red
    // waiting-dot, pillar/kind chips, glyph colors) to theme::FG whenever it
    // happened to be selected. bg + bold is enough to mark the row without
    // erasing what it's actually telling you at a glance.
    let list = List::new(items).highlight_symbol("▎").highlight_style(Style::default().bg(theme::SELECT_BG).add_modifier(Modifier::BOLD));

    let mut state = ListState::default();
    if !app.backlog.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);

    if app.backlog.is_empty() {
        let message = match app.view {
            View::Active => "no obstacles yet — press 'a' to add",
            View::Archived => "nothing archived",
        };
        frame.render_widget(Paragraph::new(Span::styled(message, Style::default().fg(theme::DIM))), area);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 || s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn multiline(s: &str, style: Style) -> Vec<Line<'static>> {
    s.split('\n').map(|line| Line::from(Span::styled(line.to_string(), style))).collect()
}

fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD))
}

fn bar(value: f32, width: usize) -> String {
    let filled = ((value.clamp(0.0, 1.0) * width as f32).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Bespoke rendering per entity kind - each pillar's own shape, not one
/// generic wrapped-text box. Validated across the TUI-prototype rounds
/// (`prototype/tui-variants`) before landing here.
fn detail_lines(r: BacklogRef, app: &App) -> Vec<Line<'static>> {
    match r {
        BacklogRef::Task(i) => task_detail_lines(app, &app.tasks[i]),

        BacklogRef::Journal(i) => {
            let entry = &app.journal[i];
            let kind = match entry.kind {
                JournalEntryKind::Freeform => "freeform",
                JournalEntryKind::CheckIn => "check-in",
            };
            let mut lines = vec![
                Line::from(vec![Span::styled(format!(" {} ", kind), Style::default().bg(pillar_color(app, "mind")).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD))]),
                Line::from(""),
            ];
            lines.extend(multiline(&entry.content, Style::default().fg(theme::FG)));
            lines
        }

        BacklogRef::Routine(i) => {
            let routine = &app.routines[i];
            let mut lines = vec![
                Line::from(Span::styled(
                    format!("~{}s total · avg intensity {:.1} · avg friction {:.1}", routine.duration_secs(), routine.intensity(), routine.friction()),
                    Style::default().fg(theme::DIM),
                )),
                Line::from(""),
            ];
            for e in &routine.exercises {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<12}", e.name), Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{}x{}", e.sets, e.reps), Style::default().fg(theme::DIM)),
                    Span::styled(format!("  {}s", e.duration_secs), Style::default().fg(theme::DIM)),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    label("int "),
                    Span::styled(bar(e.intensity, 12), Style::default().fg(theme::ORANGE)),
                    Span::raw("  "),
                    label("fric "),
                    Span::styled(bar(e.friction, 12), Style::default().fg(theme::AQUA)),
                ]));
                lines.push(Line::from(""));
            }
            if routine.exercises.is_empty() {
                lines.push(Line::from(Span::styled("(no exercises yet)", Style::default().fg(theme::DIM))));
            }
            lines
        }

        BacklogRef::Person(i) => {
            let person = &app.people[i];
            let relationship = match person.relationship {
                RelationshipKind::Child => "child",
                RelationshipKind::Partner => "partner",
            };
            let mut lines = vec![
                Line::from(vec![Span::styled(format!(" {relationship} "), Style::default().bg(pillar_color(app, "relationships")).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD))]),
                Line::from(""),
            ];
            if !person.hobbies.is_empty() {
                lines.push(Line::from(label("HOBBIES")));
                lines.push(Line::from(person.hobbies.iter().map(|h| Span::styled(format!("[{h}] "), Style::default().fg(theme::AQUA))).collect::<Vec<_>>()));
                lines.push(Line::from(""));
            }
            if !person.dreams_aspirations.is_empty() {
                lines.push(Line::from(label("DREAMS")));
                lines.push(Line::from(person.dreams_aspirations.iter().map(|d| Span::styled(format!("[{d}] "), Style::default().fg(theme::GREEN))).collect::<Vec<_>>()));
                lines.push(Line::from(""));
            }
            if !person.preferences.is_empty() {
                lines.push(Line::from(label("PREFERENCES")));
                for (k, v) in &person.preferences {
                    lines.push(Line::from(Span::styled(format!("{k}: {v}"), Style::default().fg(theme::FG))));
                }
                lines.push(Line::from(""));
            }
            if !person.attention_areas.is_empty() {
                lines.push(Line::from(label("ATTENTION")));
                for a in &person.attention_areas {
                    lines.push(Line::from(Span::styled(a.clone(), Style::default().fg(theme::RED))));
                }
                lines.push(Line::from(""));
            }
            if !person.notes.is_empty() {
                lines.push(Line::from(label("NOTES")));
                lines.extend(multiline(&person.notes, Style::default().fg(theme::FG)));
            }
            lines
        }

        BacklogRef::Craft(i) => {
            let craft = &app.crafts[i];
            let (status_text, status_color) = match craft.status {
                CraftStatus::Active => ("active", theme::GREEN),
                CraftStatus::Dormant => ("dormant", theme::ORANGE),
                CraftStatus::Historical => ("historical", theme::DIM),
            };
            let mut lines = vec![
                Line::from(vec![Span::styled(format!(" {} ", status_text.to_uppercase()), Style::default().bg(status_color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD))]),
                Line::from(""),
            ];
            if !craft.space.is_empty() {
                lines.push(Line::from(label("SPACE")));
                lines.extend(multiline(&craft.space, Style::default().fg(theme::FG)));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(label("STANDING")));
            lines.extend(multiline(if craft.standing.is_empty() { "(none yet)" } else { &craft.standing }, Style::default().fg(theme::FG)));
            lines.push(Line::from(""));
            lines.push(Line::from(label("TRAJECTORY")));
            lines.extend(multiline(if craft.trajectory.is_empty() { "(none yet)" } else { &craft.trajectory }, Style::default().fg(theme::FG)));
            lines
        }

        BacklogRef::Stability(i) => {
            let area = &app.stability[i];
            let (status_text, status_color) = match area.status {
                StabilityStatus::Active => ("active", theme::GREEN),
                StabilityStatus::Dormant => ("dormant", theme::ORANGE),
                StabilityStatus::Historical => ("historical", theme::DIM),
            };
            let mut lines = vec![
                Line::from(vec![Span::styled(format!(" {} ", status_text.to_uppercase()), Style::default().bg(status_color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD))]),
                Line::from(""),
                Line::from(label("STANDING")),
            ];
            lines.extend(multiline(if area.standing.is_empty() { "(none yet)" } else { &area.standing }, Style::default().fg(theme::FG)));
            lines.push(Line::from(""));
            lines.push(Line::from(label("TRAJECTORY")));
            lines.extend(multiline(if area.trajectory.is_empty() { "(none yet)" } else { &area.trajectory }, Style::default().fg(theme::FG)));
            lines
        }

        BacklogRef::Principle(i) => {
            let principle = &app.principles[i];
            let mut lines = vec![Line::from(Span::styled("  “", Style::default().fg(pillar_color(app, "purpose")).add_modifier(Modifier::BOLD)))];
            for l in principle.text.lines() {
                lines.push(Line::from(Span::styled(format!("  {l}"), Style::default().fg(theme::FG).add_modifier(Modifier::ITALIC))));
            }
            lines.push(Line::from(Span::styled("  ”", Style::default().fg(pillar_color(app, "purpose")).add_modifier(Modifier::BOLD))));
            lines
        }
    }
}

fn task_detail_lines(app: &App, task: &crate::task::Task) -> Vec<Line<'static>> {
    let (status, status_color) = if task.done { ("done", theme::GREEN) } else { ("open", theme::ORANGE) };
    let tags = if task.tags.is_empty() { "(none)".to_string() } else { task.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ") };
    let description = if task.description.is_empty() { "(none)" } else { task.description.as_str() };
    let (pillar_label, pillar_style) = match &task.pillar {
        Some(name) => match find_pillar_def(app, name) {
            Some(def) => (def.name.clone(), Style::default().fg(pillar_def_color(def))),
            None => (format!("{name} (unknown — profile changed)"), Style::default().fg(theme::DIM)),
        },
        None => ("(unassigned — press 'p')".to_string(), Style::default().fg(theme::DIM)),
    };
    let refs = if task.external_refs.is_empty() { "(none)".to_string() } else { task.external_refs.join(", ") };

    let mut text = vec![
        Line::from(vec![
            Span::styled(format!("{:<8}", "STATUS"), Style::default().fg(theme::DIM)),
            Span::styled(format!(" {} ", status.to_uppercase()), Style::default().bg(status_color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![Span::styled(format!("{:<8}", "PILLAR"), Style::default().fg(theme::DIM)), Span::styled(pillar_label, pillar_style)]),
        Line::from(vec![Span::styled(format!("{:<8}", "TAGS"), Style::default().fg(theme::DIM)), Span::styled(tags, Style::default().fg(theme::AQUA))]),
        Line::from(vec![Span::styled(format!("{:<8}", "REFS"), Style::default().fg(theme::DIM)), Span::styled(refs, Style::default().fg(theme::AQUA))]),
    ];

    if let Some(parent_key) = task.parent_key {
        text.push(Line::from(vec![
            Span::styled(format!("{:<8}", "PARENT"), Style::default().fg(theme::DIM)),
            Span::styled(format!("WAY-{parent_key}"), Style::default().fg(theme::FG)),
        ]));
    }

    if task.phase.is_some() || task.session_decisions.is_some() || task.session_next.is_some() {
        let phase = task.phase.as_deref().unwrap_or("(no phase)");
        let when = task.session_updated_at.map(relative_time).unwrap_or_default();
        let preview = task.session_next.as_deref().unwrap_or("(no next step recorded)");
        text.push(Line::from(vec![
            Span::styled(format!("{:<8}", "SESSION"), Style::default().fg(theme::DIM)),
            Span::styled(format!("{phase} · {when} · next: {}", truncate(preview, 48)), Style::default().fg(theme::ORANGE)),
        ]));
    }

    if let Some(reason) = &task.waiting_on {
        let since = task.waiting_on_since.map(relative_time).unwrap_or_default();
        text.push(Line::from(vec![
            Span::styled(format!("{:<8}", "WAITING"), Style::default().fg(theme::DIM)),
            Span::styled(format!("{reason} · {since}"), Style::default().fg(theme::RED)),
        ]));
    }

    let (session_label, session_color) = match app.session_status(task) {
        crate::app::SessionStatus::Live => ("open now, somewhere".to_string(), theme::AQUA),
        crate::app::SessionStatus::Idle => ("not currently open".to_string(), theme::DIM),
        crate::app::SessionStatus::None => ("never started".to_string(), theme::DIM),
    };
    text.push(Line::from(vec![
        Span::styled(format!("{:<8}", "AGENT"), Style::default().fg(theme::DIM)),
        Span::styled(session_label, Style::default().fg(session_color)),
    ]));

    text.push(Line::from(""));
    text.extend(multiline(description, Style::default().fg(theme::FG)));
    text
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

fn draw_detail_popup(frame: &mut Frame, app: &App, area: Rect) {
    let Some(&r) = app.backlog.get(app.selected) else { return };
    let (glyph, glyph_color) = kind_glyph(r, app);
    let title = row_title(r, app);

    let popup = centered_rect(area, 78, 75);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(glyph_color))
        .title(Line::from(vec![
            Span::styled(format!(" {glyph} "), Style::default().fg(glyph_color).add_modifier(Modifier::BOLD)),
            Span::styled(truncate(&title, popup.width.saturating_sub(8) as usize), Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let padded = Rect { x: inner.x + 1, y: inner.y, width: inner.width.saturating_sub(2), height: inner.height };
    frame.render_widget(Paragraph::new(detail_lines(r, app)).wrap(Wrap { trim: false }), padded);
}

fn draw_edit_form(frame: &mut Frame, app: &mut App, area: Rect, field: Field) {
    let heading = match app.editing_key() {
        Some(key) => format!("editing WAY-{key}"),
        None => "new obstacle".to_string(),
    };
    let block = rounded(theme::AQUA).title(Span::styled(heading, Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let field_label_style = |f: Field| {
        if f == field {
            Style::default().fg(theme::AQUA).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::DIM)
        }
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title label
            Constraint::Length(1), // title editor
            Constraint::Length(1), // spacer
            Constraint::Length(1), // description label
            Constraint::Min(3),    // description editor
            Constraint::Length(1), // spacer
            Constraint::Length(1), // tags label
            Constraint::Length(1), // tags value
        ])
        .split(inner);

    let editor_theme = || edtui::EditorTheme::default().base(Style::default().fg(theme::FG)).hide_status_line().cursor_style(Style::default().bg(theme::FG).fg(theme::SELECT_BG));

    frame.render_widget(Paragraph::new(Span::styled("title", field_label_style(Field::Title))), rows[0]);
    frame.render_widget(edtui::EditorView::new(&mut app.title_editor).theme(editor_theme()).single_line(true).wrap(false), rows[1]);

    frame.render_widget(Paragraph::new(Span::styled("description", field_label_style(Field::Description))), rows[3]);
    frame.render_widget(edtui::EditorView::new(&mut app.description_editor).theme(editor_theme()).wrap(true), rows[4]);

    let mut tags_spans: Vec<Span> = app.draft_tags.iter().map(|t| Span::styled(format!("#{t} "), Style::default().fg(theme::AQUA))).collect();
    tags_spans.push(Span::styled(app.tag_input.clone(), Style::default().fg(theme::FG)));
    frame.render_widget(Paragraph::new(Span::styled("tags (enter/comma to add, backspace to remove)", field_label_style(Field::Tags))), rows[6]);
    frame.render_widget(Paragraph::new(Line::from(tags_spans)), rows[7]);

    match field {
        Field::Title => {
            if let Some(pos) = app.title_editor.cursor_screen_position() {
                frame.set_cursor_position((pos.x, pos.y));
            }
        }
        Field::Description => {
            if let Some(pos) = app.description_editor.cursor_screen_position() {
                frame.set_cursor_position((pos.x, pos.y));
            }
        }
        Field::Tags => {
            let chips_len: usize = app.draft_tags.iter().map(|t| t.chars().count() + 2).sum();
            let cursor_col = (chips_len + app.tag_input.chars().count()) as u16;
            let cursor_x = (rows[7].x + cursor_col).min(rows[7].x + rows[7].width.saturating_sub(1));
            frame.set_cursor_position((cursor_x, rows[7].y));
        }
    }
}

/// Helix-inspired: a mode badge (left, colored block) + short contextual
/// hint + position (right) - replacing the old always-visible full
/// keybinding legend. Validated across the TUI-prototype rounds.
fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    let pos = if app.backlog.is_empty() { "0/0".to_string() } else { format!("{}/{}", app.selected + 1, app.backlog.len()) };

    let (badge_text, badge_color, hint): (&str, Color, &str) = match app.mode {
        Mode::Normal => match app.view {
            View::Active => {
                if app.selected_task().is_some() {
                    ("NORMAL", theme::GREEN, "j/k move  enter detail  a add  e edit  t tags  s status  p pillar  d archive  c claude  A archived  q quit")
                } else {
                    ("NORMAL", theme::GREEN, "j/k move  enter detail  a add  A archived  q quit  (mutations for this kind: CLI only, for now)")
                }
            }
            View::Archived => ("NORMAL", theme::GREEN, "j/k move  enter detail  c claude  d restore  A active  q quit"),
        },
        Mode::Detail => ("DETAIL", theme::AQUA, "j/k move  enter/esc/q close"),
        Mode::Editing(Field::Tags) => ("EDIT", theme::AQUA, "type + enter/,: add tag   backspace (empty): remove last   tab: field   ctrl+s save   esc cancel"),
        Mode::Editing(_) => ("EDIT", theme::AQUA, "arrow keys to move   tab/shift+tab: field   ctrl+s: save   esc: cancel"),
        Mode::Confirm(kind) => {
            let color = match kind {
                ConfirmKind::Archive => theme::RED,
                ConfirmKind::Restore => theme::GREEN,
            };
            let badge = match kind {
                ConfirmKind::Archive => "ARCHIVE?",
                ConfirmKind::Restore => "RESTORE?",
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!(" {badge} "), Style::default().bg(color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  {}  ", confirm_prompt(app)), Style::default().fg(color)),
                    Span::styled("y/enter confirm  n/esc cancel", Style::default().fg(theme::DIM)),
                ])),
                area,
            );
            return;
        }
        Mode::PillarPick => {
            frame.render_widget(Paragraph::new(pillar_pick_line(app)), area);
            return;
        }
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {badge_text} "), Style::default().bg(badge_color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {hint}   "), Style::default().fg(theme::DIM)),
            Span::styled(pos, Style::default().fg(theme::DIM)),
        ])),
        area,
    );
}

fn pillar_pick_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, def) in app.active_profile.pillars.iter().enumerate() {
        spans.push(Span::styled(format!("{} {}  ", i + 1, def.name), Style::default().fg(pillar_def_color(def))));
    }
    spans.push(Span::styled("0 clear  ", Style::default().fg(theme::DIM)));
    spans.push(Span::styled("esc cancel", Style::default().fg(theme::DIM)));
    Line::from(spans)
}

fn confirm_prompt(app: &App) -> String {
    match app.selected_task() {
        Some(task) => format!("\"{}\"?", task.title),
        None => String::new(),
    }
}
