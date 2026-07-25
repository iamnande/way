use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, ConfirmKind, Field, Mode, View};
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
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    draw_header(frame, app, outer[0]);
    draw_list(frame, app, main[0]);
    draw_detail(frame, app, main[1]);
    draw_bottom(frame, app, outer[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let open = app.tasks.iter().filter(|t| !t.done).count();
    let done = app.tasks.iter().filter(|t| t.done).count();
    let summary = match app.view {
        View::Active => format!("everyday · {open} open · {done} done"),
        View::Archived => format!("everyday — archived · {} item{}", app.tasks.len(), if app.tasks.len() == 1 { "" } else { "s" }),
    };
    let header = Paragraph::new(Span::styled(summary, Style::default().fg(theme::DIM))).block(titled(theme::GREEN, "way".to_string()));
    frame.render_widget(header, area);
}

fn rounded(border_color: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
}

fn titled(border_color: Color, title: String) -> Block<'static> {
    rounded(border_color).title(Span::styled(
        title,
        Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
    ))
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let inner_width = area.width.saturating_sub(2) as usize; // borders only, prefix computed per row
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let (mark, mark_color) = if task.done { ("✓", theme::GREEN) } else { ("▸", theme::ORANGE) };
            let title_style = if task.done {
                Style::default().fg(theme::DIM)
            } else {
                Style::default().fg(theme::FG)
            };

            let key = format!("WAY-{} ", task.key);
            let mut spans = vec![
                Span::styled(key.clone(), Style::default().fg(theme::DIM)),
                Span::styled(format!("{mark} "), Style::default().fg(mark_color)),
            ];
            let mut prefix_len = key.chars().count() + mark.chars().count() + 1;
            if let Some(pillar) = &task.pillar
                && let Some(def) = find_pillar_def(app, pillar)
            {
                let tag = format!("[{}] ", def.glyph);
                prefix_len += tag.chars().count();
                spans.push(Span::styled(tag, Style::default().fg(pillar_def_color(def))));
            }

            let title = truncate(&task.title, inner_width.saturating_sub(prefix_len));
            spans.push(Span::styled(title, title_style));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = match app.view {
        View::Active => format!("way / everyday ({}/{})", position(app), app.tasks.len()),
        View::Archived => format!("way / everyday — archived ({}/{})", position(app), app.tasks.len()),
    };
    let border_color = match app.view {
        View::Active => theme::GREEN,
        View::Archived => theme::DIM,
    };

    let list = List::new(items)
        .block(titled(border_color, title))
        .highlight_style(
            Style::default()
                .bg(theme::SELECT_BG)
                .fg(theme::FG)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn position(app: &App) -> usize {
    if app.tasks.is_empty() {
        0
    } else {
        app.selected + 1
    }
}

fn multiline(s: &str, style: Style) -> Vec<Line<'static>> {
    s.split('\n').map(|line| Line::from(Span::styled(line.to_string(), style))).collect()
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 || s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Mode::Editing(field) = app.mode {
        draw_edit_form(frame, app, area, field);
        return;
    }

    let Some(task) = app.tasks.get(app.selected) else {
        let message = match app.view {
            View::Active => "no tasks yet — press 'a' to add",
            View::Archived => "nothing archived",
        };
        let empty = Paragraph::new(Span::styled(message, Style::default().fg(theme::DIM))).block(titled(theme::DIM, "details".to_string()));
        frame.render_widget(empty, area);
        return;
    };
    let block = titled(theme::DIM, format!("WAY-{}", task.key));

    let (status, status_color) = if task.done { ("done", theme::GREEN) } else { ("open", theme::ORANGE) };
    let tags = if task.tags.is_empty() {
        "(none)".to_string()
    } else {
        task.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ")
    };
    let description = if task.description.is_empty() {
        "(none)"
    } else {
        task.description.as_str()
    };
    let (pillar_label, pillar_style) = match &task.pillar {
        Some(name) => match find_pillar_def(app, name) {
            Some(def) => (def.name.clone(), Style::default().fg(pillar_def_color(def))),
            None => (format!("{name} (unknown — profile changed)"), Style::default().fg(theme::DIM)),
        },
        None => ("(unassigned — press 'p')".to_string(), Style::default().fg(theme::DIM)),
    };

    let refs = if task.external_refs.is_empty() { "(none)".to_string() } else { task.external_refs.join(", ") };

    let mut text = vec![
        Line::from(Span::styled(
            task.title.clone(),
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{:<8}", "STATUS"), Style::default().fg(theme::DIM)),
            Span::styled(
                format!(" {} ", status.to_uppercase()),
                Style::default().bg(status_color).fg(theme::SELECT_BG).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<8}", "PILLAR"), Style::default().fg(theme::DIM)),
            Span::styled(pillar_label, pillar_style),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<8}", "TAGS"), Style::default().fg(theme::DIM)),
            Span::styled(tags, Style::default().fg(theme::AQUA)),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<8}", "REFS"), Style::default().fg(theme::DIM)),
            Span::styled(refs, Style::default().fg(theme::AQUA)),
        ]),
    ];

    if let Some(parent_key) = task.parent_key {
        text.push(Line::from(vec![
            Span::styled(format!("{:<8}", "PARENT"), Style::default().fg(theme::DIM)),
            Span::styled(format!("WAY-{parent_key}"), Style::default().fg(theme::FG)),
        ]));
    }

    if task.session_decisions.is_some() || task.session_next.is_some() {
        let when = task.session_updated_at.map(relative_time).unwrap_or_default();
        let preview = task.session_next.as_deref().unwrap_or("(no next step recorded)");
        text.push(Line::from(vec![
            Span::styled(format!("{:<8}", "SESSION"), Style::default().fg(theme::DIM)),
            Span::styled(format!("{when} · next: {}", truncate(preview, 48)), Style::default().fg(theme::ORANGE)),
        ]));
    }

    text.push(Line::from(""));
    text.extend(multiline(description, Style::default().fg(theme::FG)));

    let detail = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    frame.render_widget(detail, area);
}

fn draw_edit_form(frame: &mut Frame, app: &mut App, area: Rect, field: Field) {
    let heading = match app.editing_key() {
        Some(key) => format!("editing WAY-{key}"),
        None => "new task".to_string(),
    };
    let block = titled(theme::AQUA, heading);
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

    let editor_theme = || {
        edtui::EditorTheme::default()
            .base(Style::default().fg(theme::FG))
            .hide_status_line()
            .cursor_style(Style::default().bg(theme::FG).fg(theme::SELECT_BG))
    };

    frame.render_widget(Paragraph::new(Span::styled("title", field_label_style(Field::Title))), rows[0]);
    frame.render_widget(
        edtui::EditorView::new(&mut app.title_editor).theme(editor_theme()).single_line(true).wrap(false),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(Span::styled("description", field_label_style(Field::Description))),
        rows[3],
    );
    frame.render_widget(
        edtui::EditorView::new(&mut app.description_editor).theme(editor_theme()).wrap(true),
        rows[4],
    );

    let mut tags_spans: Vec<Span> = app
        .draft_tags
        .iter()
        .map(|t| Span::styled(format!("#{t} "), Style::default().fg(theme::AQUA)))
        .collect();
    tags_spans.push(Span::styled(app.tag_input.clone(), Style::default().fg(theme::FG)));
    frame.render_widget(
        Paragraph::new(Span::styled(
            "tags (enter/comma to add, backspace to remove)",
            field_label_style(Field::Tags),
        )),
        rows[6],
    );
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

fn draw_bottom(frame: &mut Frame, app: &App, area: Rect) {
    let (border_color, content) = match app.mode {
        Mode::Normal => (
            theme::DIM,
            Line::from(Span::styled(
                match app.view {
                    View::Active => "a add  e edit  s status  t tags  p pillar  c claude  d archive  A archived  j/k move  q quit",
                    View::Archived => "c claude  d restore  A active  j/k move  q quit",
                },
                Style::default().fg(theme::DIM),
            )),
        ),
        Mode::Editing(field) => (
            theme::AQUA,
            Line::from(Span::styled(
                match field {
                    Field::Tags => "type + enter/,: add tag   backspace (empty): remove last   tab: field   ctrl+s save   esc/ctrl+c cancel",
                    Field::Title | Field::Description => {
                        "arrow keys to move   tab/shift+tab: field   ctrl+s: save   esc/ctrl+c: cancel"
                    }
                },
                Style::default().fg(theme::DIM),
            )),
        ),
        Mode::Confirm(kind) => {
            let color = match kind {
                ConfirmKind::Archive => theme::RED,
                ConfirmKind::Restore => theme::GREEN,
            };
            (
                color,
                Line::from(Span::styled(
                    confirm_prompt(app, kind),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )),
            )
        }
        Mode::PillarPick => (theme::FG, pillar_pick_line(app)),
    };

    let bottom = Paragraph::new(content).block(rounded(border_color));
    frame.render_widget(bottom, area);
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

fn confirm_prompt(app: &App, kind: ConfirmKind) -> String {
    let verb = match kind {
        ConfirmKind::Archive => "archive",
        ConfirmKind::Restore => "restore",
    };
    match app.tasks.get(app.selected) {
        Some(task) => format!("{verb} \"{}\"? (y/n)", task.title),
        None => String::new(),
    }
}
