use std::path::Path;

use anyhow::Result;

/// Terminal multiplexer operations `way` needs to manage per-task tabs.
/// Zellij is the only implementation today, but nothing outside this file
/// should know that - the seam exists so a future swap (tmux, say) only
/// touches `Zellij`'s replacement, not every call site that spawns a task.
pub trait Multiplexer {
    /// Returns the full name of an existing tab whose name starts with
    /// `prefix`, if one is open. Matching by prefix (not exact equality)
    /// because a tab's name embeds task status, which can go stale between
    /// when the tab was created and when we're checking - the key prefix is
    /// the stable part.
    fn find_tab(&self, prefix: &str) -> Result<Option<String>>;
    fn go_to_tab(&self, name: &str) -> Result<()>;
    fn new_tab(&self, name: &str, cwd: &Path, argv: &[String]) -> Result<()>;
    /// Every currently-open tab name in one shell-out - for the TUI's
    /// "does this task have a live session somewhere" indicator (WAY-8),
    /// checked against every row at once rather than one `find_tab` call
    /// (one `zellij` subprocess) per row per redraw.
    fn list_open_tabs(&self) -> Result<Vec<String>>;
}

/// The single session all of way's task tabs live in, kept separate from
/// whatever session the user is otherwise using (their daily-driver
/// terminal). way always targets this session by name, regardless of which
/// session (if any) `way` itself is currently running inside - so spawning
/// a task works the same whether you're in your own session, in `way`'s
/// session already, or in no multiplexer at all.
pub const WAY_SESSION_NAME: &str = "way";

pub struct Zellij;

/// Name of the tab that always runs the way TUI itself - the first thing
/// you see attaching to way's session. Distinct on purpose from anything a
/// personal default layout is likely to already use (see `bootstrap` below).
const TUI_TAB_NAME: &str = "tui";

impl Zellij {
    /// Whether the zellij binary is usable at all - independent of whether
    /// `way` itself happens to be running inside a zellij pane right now,
    /// since way always targets its own dedicated session by name rather
    /// than whatever session (if any) is currently attached.
    pub fn is_installed() -> bool {
        std::process::Command::new("zellij").arg("--version").output().is_ok()
    }

    /// Connects to way's dedicated session, creating and bootstrapping it
    /// first if it doesn't exist yet. Safe to call every time - bootstrap
    /// only runs once, the first time the session doesn't already exist.
    pub fn connect() -> Result<Self> {
        let already_existed = Self::session_exists()?;
        std::process::Command::new("zellij").args(["attach", "--create-background", WAY_SESSION_NAME]).status()?;
        if !already_existed {
            Self::bootstrap()?;
        }
        Ok(Self)
    }

    fn session_exists() -> Result<bool> {
        let out = std::process::Command::new("zellij").args(["list-sessions", "--short"]).output()?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().any(|line| line.trim() == WAY_SESSION_NAME))
    }

    /// zellij has no CLI primitive to create a detached session with a
    /// specific layout - `attach --create-background` is the only headless
    /// creation path, and it always applies the user's own configured
    /// `default_layout`. So instead of fighting that, this leans into it:
    /// let the default layout populate the session's initial tabs, snapshot
    /// their names, add the one tab way actually wants (the TUI), then
    /// close every tab from that snapshot. Net effect matches "session
    /// starts with just the TUI running" without depending on any
    /// particular default layout's contents.
    fn bootstrap() -> Result<()> {
        // No client is attached at this point (the session was just created
        // headless), so "current tab"-based actions like plain `close-tab`
        // silently no-op - there's no client focus for them to act on. Every
        // tab here gets closed by its stable ID instead, which doesn't
        // depend on any client being attached.
        let junk_tab_ids = Self::tab_ids()?;

        let way_exe = std::env::current_exe()?;
        std::process::Command::new("zellij")
            .args(["-s", WAY_SESSION_NAME, "action", "new-tab", "--name", TUI_TAB_NAME, "--"])
            .arg(&way_exe)
            .status()?;

        for id in junk_tab_ids {
            std::process::Command::new("zellij").args(["-s", WAY_SESSION_NAME, "action", "close-tab-by-id", &id.to_string()]).status()?;
        }
        Ok(())
    }

    /// Stable tab IDs for every tab currently in way's session, via
    /// `list-tabs --json` rather than `query-tab-names` - IDs stay valid
    /// for addressed actions like `close-tab-by-id` regardless of whether
    /// any client is attached.
    fn tab_ids() -> Result<Vec<u64>> {
        let out = std::process::Command::new("zellij").args(["-s", WAY_SESSION_NAME, "action", "list-tabs", "--json"]).output()?;
        let parsed: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        let ids = parsed.as_array().map(|tabs| tabs.iter().filter_map(|t| t.get("tab_id")?.as_u64()).collect()).unwrap_or_default();
        Ok(ids)
    }
}

impl Multiplexer for Zellij {
    fn find_tab(&self, prefix: &str) -> Result<Option<String>> {
        Ok(self.list_open_tabs()?.into_iter().find(|name| name.starts_with(prefix)))
    }

    fn list_open_tabs(&self) -> Result<Vec<String>> {
        let out = std::process::Command::new("zellij")
            .args(["-s", WAY_SESSION_NAME, "action", "query-tab-names"])
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().map(|line| line.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }

    fn go_to_tab(&self, name: &str) -> Result<()> {
        std::process::Command::new("zellij").args(["-s", WAY_SESSION_NAME, "action", "go-to-tab-name", name]).status()?;
        Ok(())
    }

    fn new_tab(&self, name: &str, cwd: &Path, argv: &[String]) -> Result<()> {
        let mut args = vec![
            "-s".to_string(),
            WAY_SESSION_NAME.to_string(),
            "action".to_string(),
            "new-tab".to_string(),
            "--name".to_string(),
            name.to_string(),
            "--cwd".to_string(),
            cwd.to_string_lossy().into_owned(),
            "--".to_string(),
        ];
        args.extend(argv.iter().cloned());
        std::process::Command::new("zellij").args(&args).status()?;
        Ok(())
    }
}

/// `way resume`: attach to way's dedicated session, creating and
/// bootstrapping it first if it's never existed, immediately rerunning
/// every pane's command if resurrecting one that was killed. This is the
/// whole fix for "none of the obstacle tabs were usable" after restarting
/// zellij - the caller never needs to know about `--force-run-commands`,
/// and each rerun of `way launch <key>` re-derives resume-vs-fresh from the
/// store rather than replaying a stale prompt (see
/// `agent_session::launch_claude_for_task`).
///
/// Goes through `Zellij::connect()` first (not `attach --create` directly)
/// so a session that's never existed gets the same bootstrap - closing the
/// default-layout junk tabs - as any other first connection, instead of
/// landing on an unbootstrapped session because it happened to be attached
/// to before anything else ever spawned a task.
///
/// Takes over the current terminal; only meant to be run from a plain shell,
/// not from inside an existing multiplexer session.
pub fn resume() -> Result<()> {
    Zellij::connect()?;
    std::process::Command::new("zellij").args(["attach", "--force-run-commands", WAY_SESSION_NAME]).status()?;
    Ok(())
}
