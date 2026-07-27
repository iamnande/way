use std::io;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use uuid::Uuid;

use crate::app::App;
use crate::store::Store;
use crate::task::Task;

/// Env var a spawned agent session is bound to its originating WAY-N through.
/// Set here at spawn time; read back by `cli::active_key` so `way add`/`spawn`
/// can tell whether they're being called from inside a task-bound session.
/// Named and scoped agent-agnostically on purpose - the spawn below happens
/// to hardcode `claude` today, but the binding itself isn't claude-specific.
pub const ACTIVE_KEY_ENV: &str = "WAY_ACTIVE_KEY";

/// Builds the opening prompt from the task itself — context assembled by
/// `way` before `claude` ever starts, not left for a cold session to go
/// fetch afterward. If the task carries decisions/next (a prior senzu
/// compact), this is a re-attach: that prose is `way`'s own record and is
/// handed over verbatim. External refs are listed by identifier only —
/// `way` never cached their content, so the resuming session is told to
/// resolve them live rather than trust anything as already known.
fn build_prompt(task: &Task) -> String {
    let refs = if task.external_refs.is_empty() { "(none)".to_string() } else { task.external_refs.join(", ") };
    let key = task.key;

    // Every prompt below seeds a brand-new agent session (even the "Resuming"
    // one - `--resume` only reuses an old session id, this text path is for a
    // fresh one), so nothing in the conversation yet tells the agent this task
    // already has a record. Without this line, the agent can mistake the
    // injected prompt for a raw task description and re-log it with `way add`,
    // producing a duplicate of the very task it was just handed.
    let already_tracked = format!(
        "(WAY-{key} already exists in the `way` tracker - do not `way add` or `way spawn` it again. \
        Update this same record as you go via `way session ...`, `way pillar`, `way link`, etc.)"
    );

    if task.session_decisions.is_some() || task.session_next.is_some() {
        let decisions = task.session_decisions.as_deref().unwrap_or("(none recorded)");
        let next = task.session_next.as_deref().unwrap_or("(none recorded)");
        return format!(
            "Resuming WAY-{key}: {}\n\n{already_tracked}\n\nDecisions so far:\n{decisions}\n\nNext:\n{next}\n\nExternal refs (resolve these live, don't assume anything about their current state): {refs}\n\nContinue from here.",
            task.title
        );
    }

    let description = if task.description.is_empty() { "(no description)".to_string() } else { task.description.clone() };
    let tags = if task.tags.is_empty() { "(none)".to_string() } else { task.tags.join(", ") };
    let pillar = task.pillar.as_deref().unwrap_or("(unassigned)");
    format!(
        "Starting WAY-{key}: {}\n\n{already_tracked}\n\n{description}\n\ntags: {tags}\npillar: {pillar}\nexternal refs: {refs}",
        task.title
    )
}

/// Decides resume-vs-fresh from the store's *current* state and runs claude
/// accordingly. This is the one place that decision gets made - both the
/// foreground path and `way launch` (see cli.rs) call through here rather
/// than each baking their own copy of the branch.
///
/// That single choke point matters beyond DRY: `way launch <key>` is what
/// gets embedded as a zellij pane's command, and zellij remembers/reruns
/// that literal argv verbatim if the pane is ever resurrected after zellij
/// itself was killed and restarted. If the resume-vs-fresh decision were
/// baked into that argv instead (e.g. a frozen `--session-id <id> "<prompt>"`
/// from the very first spawn), resurrection would replay it unchanged -
/// resending the entire original prompt as a "new" message on top of
/// whatever conversation already happened, or erroring on a reused
/// session-id. Re-deriving the decision here, at actual execution time,
/// means a resurrection replay of `way launch <key>` sees the session id
/// this function already persisted the first time and correctly resumes
/// instead.
pub fn launch_claude_for_task(store: &dyn Store, task: &Task) -> Result<()> {
    let mut cmd = std::process::Command::new("claude");
    cmd.env(ACTIVE_KEY_ENV, task.key.to_string());

    match &task.claude_session_id {
        Some(session_id) => {
            cmd.arg("--resume").arg(session_id);
        }
        None => {
            let new_id = Uuid::new_v4().to_string();
            store.set_claude_session_id(task.id, Some(new_id.clone()))?;
            cmd.arg("--session-id").arg(&new_id).arg(build_prompt(task));
        }
    }

    cmd.status()?;
    Ok(())
}

/// Dispatches to whichever spawn strategy fits the terminal `way` is
/// actually running in. Inside zellij, the agent gets its own tab and way's
/// own pane is never touched — "escaping" is just normal tab switching, so no
/// signal ever reaches the agent process. Outside zellij there's no
/// multiplexer to hand off to, so way falls back to the original behavior:
/// take over the terminal, block, and restore it after.
pub fn spawn_agent_session(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &App, task: &Task) -> Result<()> {
    if std::env::var_os("ZELLIJ").is_some() {
        return spawn_agent_session_zellij(task);
    }
    spawn_agent_session_foreground(terminal, app, task)
}

/// Runs the agent in its own zellij tab named `WAY-<key>`, created via
/// `zellij action new-tab -- <cmd>`, which returns as soon as the tab exists
/// rather than blocking until the command inside it exits. If that tab is
/// still open from a previous spawn, jumps back to it instead of starting a
/// second, disconnected session for the same task.
///
/// The tab's command is `way launch <key>`, not claude directly - see
/// `launch_claude_for_task` for why that indirection is load-bearing rather
/// than incidental.
fn spawn_agent_session_zellij(task: &Task) -> Result<()> {
    let tab_name = format!("WAY-{}", task.key);

    let existing = std::process::Command::new("zellij").args(["action", "query-tab-names"]).output()?;
    let tab_is_open = String::from_utf8_lossy(&existing.stdout).lines().any(|line| line.trim() == tab_name);

    if tab_is_open {
        std::process::Command::new("zellij").args(["action", "go-to-tab-name", &tab_name]).status()?;
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    // The absolute path to the currently-running binary, not a bare "way" -
    // this must resolve correctly even if `way` isn't on PATH, since it's
    // zellij (not a shell) execing this argv directly.
    let way_exe = std::env::current_exe()?;
    let args = vec![
        "action".to_string(),
        "new-tab".to_string(),
        "--name".to_string(),
        tab_name,
        "--cwd".to_string(),
        cwd.to_string_lossy().into_owned(),
        "--".to_string(),
        way_exe.to_string_lossy().into_owned(),
        "launch".to_string(),
        task.key.to_string(),
    ];

    std::process::Command::new("zellij").args(&args).status()?;
    Ok(())
}

/// Suspends the TUI, hands the real terminal to the agent subprocess, waits
/// for it to exit, then restores the TUI.
fn spawn_agent_session_foreground(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
    task: &Task,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = launch_claude_for_task(app.store(), task);

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    result
}
