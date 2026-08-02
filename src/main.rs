mod app;
mod agent_session;
mod cli;
mod config;
mod craft;
mod journal;
mod multiplexer;
mod person;
mod principle;
mod routine;
mod stability;
mod store;
mod task;
mod theme;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use agent_session::spawn_agent_session;
use store::RedbStore;

fn data_path() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("could not resolve a data directory"))?;
    let dir = base.join("way");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("way.redb"))
}

/// Wakes the main loop. Both producers push into one channel so `run` blocks
/// on a single `recv()` — there is no timer and nothing is ever polled.
enum AppEvent {
    Terminal(Event),
    StoreChanged,
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let db_path = data_path()?;
    let store = RedbStore::open(&db_path)?;

    if let Some(command) = cli.command {
        return cli::run(command, &store);
    }

    let mut app = App::new(Box::new(store))?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();
    spawn_terminal_reader(tx.clone());
    let _watcher = spawn_store_watcher(db_path, tx)?;

    let result = run(&mut terminal, &mut app, rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Forwards every terminal input event onto `tx`. `event::read()` blocks
/// until the next key/resize/etc, so this thread costs nothing while idle.
fn spawn_terminal_reader(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if tx.send(AppEvent::Terminal(ev)).is_err() {
                break;
            }
        }
    });
}

/// Watches the store's directory for writes from *any* process — not just
/// this TUI's own edits, but one-shot `way` CLI invocations (e.g. a spawned
/// `claude` session running `way session set-phase`) landing while this TUI
/// sits idle waiting on a keypress. The watcher thread blocks on the OS's
/// own filesystem-change notifications and only wakes on a real write, so
/// `run` never has to poll the store on a timer to notice one.
///
/// This watches `RedbStore::marker_path`, a sidecar file bumped only on
/// successful writes — not the `.redb` file itself, whose mtime redb also
/// touches on plain reads. Watching the `.redb` file directly would make
/// every refresh re-trigger the watcher that caused it, forever.
///
/// Bursty writes (a single transaction can touch the marker more than once)
/// are coalesced into a single `AppEvent::StoreChanged` via a short
/// debounce window before waking the main loop.
fn spawn_store_watcher(db_path: PathBuf, tx: mpsc::Sender<AppEvent>) -> Result<RecommendedWatcher> {
    let marker_path = RedbStore::marker_path(&db_path);
    let watch_dir = marker_path.parent().ok_or_else(|| anyhow::anyhow!("db path has no parent directory"))?.to_path_buf();
    let file_name = marker_path.file_name().map(|n| n.to_owned());

    let (notify_tx, notify_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = notify_tx.send(res);
    })?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || loop {
        let event: notify::Event = match notify_rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(_)) => continue,
            Err(_) => break,
        };
        if !event.paths.iter().any(|p| p.file_name() == file_name.as_deref()) {
            continue;
        }
        while notify_rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
        if tx.send(AppEvent::StoreChanged).is_err() {
            break;
        }
    });

    Ok(watcher)
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App, rx: mpsc::Receiver<AppEvent>) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        match rx.recv() {
            Ok(AppEvent::Terminal(Event::Key(key))) => app.on_key(key.code, key.modifiers)?,
            Ok(AppEvent::Terminal(_)) => {}
            Ok(AppEvent::StoreChanged) => app.refresh()?,
            Err(_) => break,
        }

        if let Some(task) = app.pending_spawn.take() {
            spawn_agent_session(terminal, app, &task)?;
            app.refresh()?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::TaskStore;
    use uuid::Uuid;

    /// A write from another handle (standing in for a separate `way` CLI
    /// process, e.g. a spawned claude session calling `way session
    /// set-phase`) must produce a `StoreChanged` event without anything
    /// polling for it.
    ///
    /// Also guards the specific bug this design works around: redb touches
    /// the `.redb` file's mtime on every *open*, including plain reads, so
    /// a watcher pointed at that file directly would treat the TUI's own
    /// refresh-driven reads as content changes and re-trigger itself
    /// forever. Reading through the same handle several times first must
    /// produce nothing before the real write fires exactly one event.
    #[test]
    fn store_watcher_ignores_reads_and_fires_only_on_writes() {
        let dir = std::env::temp_dir().join(format!("way-watcher-test-{}-{}", std::process::id(), Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("way.redb");
        let store = RedbStore::open(&db_path).unwrap();

        let (tx, rx) = mpsc::channel();
        let _watcher = spawn_store_watcher(db_path.clone(), tx).unwrap();
        std::thread::sleep(Duration::from_millis(100));

        for _ in 0..5 {
            store.list().unwrap();
        }
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "reads must not produce a StoreChanged event");

        store.add("external write".to_string(), String::new(), vec![]).unwrap();
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("expected a StoreChanged event after a real write");
        assert!(matches!(event, AppEvent::StoreChanged));

        std::fs::remove_dir_all(&dir).ok();
    }
}
