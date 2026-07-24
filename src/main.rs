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

        if let Some(way_key) = app.pending_spawn.take() {
            spawn_claude_session(terminal, way_key)?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Suspends the TUI, hands the real terminal to a `claude` subprocess seeded
/// with just the task's WAY-N key, waits for it to exit, then restores the
/// TUI. Deliberately minimal — the session is expected to pull full context
/// itself via `way show`/`way session show`, not have it injected here.
fn spawn_claude_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, key: u32) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let prompt = format!("let's work on WAY-{key} — run `way show {key}` for context");
    let result = std::process::Command::new("claude").arg(prompt).status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    result?;
    Ok(())
}
