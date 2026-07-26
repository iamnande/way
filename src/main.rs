mod app;
mod cli;
mod store;
mod task;
mod theme;
mod ui;

use std::io;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use uuid::Uuid;

use app::App;
use store::RedbStore;
use task::Task;

fn data_path() -> Result<std::path::PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("could not resolve a data directory"))?;
    let dir = base.join("way");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("way.redb"))
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let store = RedbStore::open(data_path()?)?;

    if let Some(command) = cli.command {
        return cli::run(command, &store);
    }

    let mut app = App::new(Box::new(store))?;

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
        terminal.draw(|frame| ui::draw(frame, app))?;

        if let Event::Key(key) = event::read()? {
            app.on_key(key.code, key.modifiers)?;
        }

        if let Some(task) = app.pending_spawn.take() {
            spawn_claude_session(terminal, app, &task)?;
            app.refresh()?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Builds the opening prompt from the task itself — context assembled by
/// `way` before `claude` ever starts, not left for a cold session to go
/// fetch afterward. If the task carries decisions/next (a prior senzu
/// compact), this is a re-attach: that prose is `way`'s own record and is
/// handed over verbatim. External refs are listed by identifier only —
/// `way` never cached their content, so the resuming session is told to
/// resolve them live rather than trust anything as already known.
fn build_prompt(task: &Task) -> String {
    let refs = if task.external_refs.is_empty() { "(none)".to_string() } else { task.external_refs.join(", ") };

    if task.session_decisions.is_some() || task.session_next.is_some() {
        let decisions = task.session_decisions.as_deref().unwrap_or("(none recorded)");
        let next = task.session_next.as_deref().unwrap_or("(none recorded)");
        return format!(
            "Resuming WAY-{}: {}\n\nDecisions so far:\n{decisions}\n\nNext:\n{next}\n\nExternal refs (resolve these live, don't assume anything about their current state): {refs}\n\nContinue from here.",
            task.key, task.title
        );
    }

    let description = if task.description.is_empty() { "(no description)".to_string() } else { task.description.clone() };
    let tags = if task.tags.is_empty() { "(none)".to_string() } else { task.tags.join(", ") };
    let pillar = task.pillar.as_deref().unwrap_or("(unassigned)");
    format!("Starting WAY-{}: {}\n\n{description}\n\ntags: {tags}\npillar: {pillar}\nexternal refs: {refs}", task.key, task.title)
}

/// Dispatches to whichever spawn strategy fits the terminal `way` is
/// actually running in. Inside zellij, claude gets its own tab and way's own
/// pane is never touched — "escaping" is just normal tab switching, so no
/// signal ever reaches the claude process. Outside zellij there's no
/// multiplexer to hand off to, so way falls back to the original behavior:
/// take over the terminal, block, and restore it after.
fn spawn_claude_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &App, task: &Task) -> Result<()> {
    if std::env::var_os("ZELLIJ").is_some() {
        return spawn_claude_session_zellij(app, task);
    }
    spawn_claude_session_foreground(terminal, app, task)
}

/// Runs claude in its own zellij tab named `WAY-<key>`, created via
/// `zellij action new-tab -- <cmd>`, which returns as soon as the tab exists
/// rather than blocking until the command inside it exits. If that tab is
/// still open from a previous spawn, jumps back to it instead of starting a
/// second, disconnected session for the same task.
fn spawn_claude_session_zellij(app: &App, task: &Task) -> Result<()> {
    let tab_name = format!("WAY-{}", task.key);

    let existing = std::process::Command::new("zellij").args(["action", "query-tab-names"]).output()?;
    let tab_is_open = String::from_utf8_lossy(&existing.stdout).lines().any(|line| line.trim() == tab_name);

    if tab_is_open {
        std::process::Command::new("zellij").args(["action", "go-to-tab-name", &tab_name]).status()?;
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    let mut args = vec![
        "action".to_string(),
        "new-tab".to_string(),
        "--name".to_string(),
        tab_name,
        "--cwd".to_string(),
        cwd.to_string_lossy().into_owned(),
        "--".to_string(),
        "claude".to_string(),
    ];

    match &task.claude_session_id {
        Some(session_id) => {
            args.push("--resume".to_string());
            args.push(session_id.clone());
        }
        None => {
            let new_id = Uuid::new_v4().to_string();
            app.set_claude_session_id(task.id, Some(new_id.clone()))?;
            args.push("--session-id".to_string());
            args.push(new_id);
            args.push(build_prompt(task));
        }
    }

    std::process::Command::new("zellij").args(&args).status()?;
    Ok(())
}

/// Suspends the TUI, hands the real terminal to a `claude` subprocess, waits
/// for it to exit, then restores the TUI. If the task already has a
/// `claude_session_id` (a prior spawn from `way`), this is a true resume —
/// `claude --resume <id>` drops back into the exact same conversation, no
/// injected prompt, matching what "re-attach" is actually supposed to mean.
/// Otherwise a fresh session is pinned to a new UUID via `--session-id` (so
/// it can be resumed next time) and seeded with the assembled prompt.
fn spawn_claude_session_foreground(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
    task: &Task,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let mut cmd = std::process::Command::new("claude");

    let result = match &task.claude_session_id {
        Some(session_id) => {
            cmd.arg("--resume").arg(session_id);
            cmd.status()
        }
        None => {
            let new_id = Uuid::new_v4().to_string();
            app.set_claude_session_id(task.id, Some(new_id.clone()))?;
            cmd.arg("--session-id").arg(&new_id).arg(build_prompt(task));
            cmd.status()
        }
    };

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    result?;
    Ok(())
}
