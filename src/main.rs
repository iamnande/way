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

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
