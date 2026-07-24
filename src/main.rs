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
            spawn_claude_session(terminal, &task)?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Builds the opening prompt from the task itself — context assembled by
/// `way` before `claude` ever starts, not left for a cold session to go
/// fetch afterward. If the task carries session-state (a prior senzu
/// compact), this is a re-attach: the stored resume-state is handed over
/// verbatim, opaque to `way`, so the session picks up where it left off
/// instead of re-grounding from zero.
fn build_prompt(task: &Task) -> String {
    if let Some(state) = &task.session_state {
        return format!("Resuming WAY-{}: {}\n\nPrior session state:\n{}\n\nContinue from here.", task.key, task.title, state);
    }

    let description = if task.description.is_empty() { "(no description)".to_string() } else { task.description.clone() };
    let tags = if task.tags.is_empty() { "(none)".to_string() } else { task.tags.join(", ") };
    let pillar = task.pillar.as_deref().unwrap_or("(unassigned)");
    format!("Starting WAY-{}: {}\n\n{description}\n\ntags: {tags}\npillar: {pillar}", task.key, task.title)
}

/// Suspends the TUI, hands the real terminal to a `claude` subprocess seeded
/// with the assembled prompt, waits for it to exit, then restores the TUI.
fn spawn_claude_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, task: &Task) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = std::process::Command::new("claude").arg(build_prompt(task)).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    result?;
    Ok(())
}
