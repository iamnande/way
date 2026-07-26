mod app;
mod cli;
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
use uuid::Uuid;

use app::App;
use store::RedbStore;
use task::Task;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

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
