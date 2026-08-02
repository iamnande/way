use std::io::Read;

use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::store::Store;
use crate::task::{PillarDef, Profile};

#[derive(Parser)]
#[command(name = "way", about = "way: a life task tracker")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Add a new task
    Add {
        title: String,
        #[arg(long)]
        description: Option<String>,
        /// Comma-separated tags
        #[arg(long)]
        tags: Option<String>,
        /// Must be a pillar name in the active profile
        #[arg(long)]
        pillar: Option<String>,
    },
    /// Create a task as a child of an existing task (lineage)
    Spawn {
        parent_key: u32,
        title: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        pillar: Option<String>,
    },
    /// List tasks as a JSON array
    List {
        #[arg(long)]
        pillar: Option<String>,
        #[arg(long, value_enum, default_value_t = StatusFilter::All)]
        status: StatusFilter,
        #[arg(long, value_enum, default_value_t = ViewFilter::Active)]
        view: ViewFilter,
    },
    /// Show one task by its WAY-N key
    Show { key: u32 },
    /// Show a task's full lineage: ancestors and descendants
    Tree { key: u32 },
    /// Mark a task done (idempotent)
    Done { key: u32 },
    /// Decide resume-vs-fresh for WAY-N from the store's current state and
    /// run claude accordingly. This is what a spawned zellij tab's command
    /// actually is - see `agent_session::launch_claude_for_task` for why
    /// that indirection (re-check at execution time, don't bake a frozen
    /// decision into the tab's argv) matters for zellij's own resurrection.
    Launch { key: u32 },
    /// Attach to way's dedicated multiplexer session, creating it if needed
    /// and immediately rerunning every tab's command if resurrecting one
    /// that was killed. Run this from a plain shell, not from inside an
    /// existing multiplexer session.
    Resume,
    /// Set a task's pillar, or "clear" to unset it
    Pillar { key: u32, pillar: String },
    /// Attach an external pointer (GH Discussion / Linear / PR / ticket id).
    /// A task may have more than one; adding a duplicate is a no-op.
    /// `way link <key> clear` removes all pointers.
    Link {
        key: u32,
        external_ref: String,
        /// Remove this specific pointer instead of adding it
        #[arg(long)]
        remove: bool,
    },
    /// Read/write a task's session resume-state
    Session {
        #[command(subcommand)]
        action: SessionCommand,
    },
    /// Manage profiles (each with its own configurable pillar set)
    Profile {
        #[command(subcommand)]
        action: ProfileCommand,
    },
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// Set the task's phase - not tied to any one workflow's vocabulary
    /// (e.g. "grounding", "planning", or anything else), but validated
    /// against the active profile's configured phase order when it has one
    SetPhase {
        key: u32,
        phase: String,
        /// Bypass phase-order validation
        #[arg(long)]
        force: bool,
    },
    /// Flag the task as blocked, waiting on nick, with a short reason
    SetWaiting { key: u32, reason: String },
    /// Clear the waiting-on-nick flag (leaves phase/decisions/next alone)
    ClearWaiting { key: u32 },
    /// Read decisions prose from stdin and store it
    SetDecisions { key: u32 },
    /// Read next-step prose from stdin and store it
    SetNext { key: u32 },
    /// Attach an existing claude session UUID (e.g. one you started outside
    /// way) so 'c' resumes it instead of pinning a new one on next spawn
    SetClaudeId { key: u32, session_id: String },
    /// Detach the claude session id - next spawn starts a new one
    ClearClaudeId { key: u32 },
    /// Print phase/decisions/next/updated-at/waiting-on/claude-session-id as
    /// labeled plain text
    Show { key: u32 },
    /// Clear phase, decisions, next, updated-at, and waiting-on together
    /// (leaves claude-session-id alone - that's the live conversation's
    /// identity, not a summary of it)
    Clear { key: u32 },
}

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// List all profiles
    List,
    /// Switch the active profile
    Use { name: String },
    /// Create a new profile. --pillars is "name:glyph:colorhex,name:glyph:colorhex,..."
    Add {
        name: String,
        #[arg(long)]
        pillars: String,
        /// Ordered phase vocabulary, e.g. "grounding,spec,planning". Omit
        /// (or leave empty) for no phase-order enforcement on this profile.
        #[arg(long)]
        phases: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum StatusFilter {
    All,
    Open,
    Done,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ViewFilter {
    Active,
    Archived,
}

fn is_clear(s: &str) -> bool {
    s.eq_ignore_ascii_case("clear") || s.eq_ignore_ascii_case("none")
}

fn find_by_key(store: &dyn Store, key: u32) -> Result<crate::task::Task> {
    store.find_by_key(key)?.ok_or_else(|| anyhow!("no task with key WAY-{key}"))
}

/// The WAY-N a spawned claude session is bound to, set by `way` itself at
/// spawn time (see `agent_session::spawn_agent_session_*`). Lets `add`/
/// `spawn` tell whether they're being called from inside a task-bound
/// session at all, rather than trusting the caller (agent or human) to
/// remember and say so.
fn active_key() -> Option<u32> {
    std::env::var(crate::agent_session::ACTIVE_KEY_ENV).ok().and_then(|s| s.parse().ok())
}

/// Lineage enforcement: while a session is bound to WAY-N, any task it
/// creates must attach under that lineage - a bare `add` (no parent at all)
/// or a `spawn` off some unrelated task are both refused. `new_parent` is
/// `None` for `add`, `Some(parent_key)` for `spawn`.
fn enforce_lineage(new_parent: Option<u32>) -> Result<()> {
    let Some(active) = active_key() else { return Ok(()) };
    match new_parent {
        None => bail!(
            "this session is bound to WAY-{active} - top-level `add` is disallowed while bound. \
            Use `way spawn {active} ...` for related work discovered along the way, \
            or update WAY-{active} directly (`way session ...`) if this *is* that task."
        ),
        Some(parent_key) if parent_key != active => {
            bail!("this session is bound to WAY-{active} - spawn must parent off WAY-{active}, not WAY-{parent_key}")
        }
        Some(_) => Ok(()),
    }
}

/// Duplicate-of guard: distinct from lineage enforcement above. A child that
/// merely restates its parent's title isn't a lineage branch, it's a
/// duplicate of the parent - the same failure mode `enforce_lineage` guards
/// against, reachable via the very fallback path its error message points
/// to. Scoped to this one parent, not a tracker-wide title-uniqueness rule.
fn refuse_clone_of_parent(parent: &crate::task::Task, title: &str) -> Result<()> {
    let key = parent.key;
    if title.trim().eq_ignore_ascii_case(parent.title.trim()) {
        bail!(
            "WAY-{key} already has this title - that's a duplicate of it, not a child of it. \
            Work WAY-{key} directly instead of spawning a clone."
        );
    }
    Ok(())
}

fn parse_pillar_spec(spec: &str) -> Result<Vec<PillarDef>> {
    spec.split(',')
        .map(|entry| {
            let parts: Vec<&str> = entry.split(':').collect();
            let [name, glyph, color] = parts.as_slice() else {
                bail!("invalid pillar spec '{entry}' (expected name:glyph:colorhex)");
            };
            if glyph.chars().count() != 1 {
                bail!("pillar glyph '{glyph}' must be exactly one character");
            }
            let hex = color.trim_start_matches('#');
            if hex.len() != 6 {
                bail!("pillar color '{color}' must be a 6-digit hex value");
            }
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| anyhow!("invalid hex color '{color}'"))?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| anyhow!("invalid hex color '{color}'"))?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| anyhow!("invalid hex color '{color}'"))?;
            Ok(PillarDef { name: name.to_lowercase(), glyph: glyph.to_string(), color: (r, g, b) })
        })
        .collect()
}

pub fn run(command: Command, store: &dyn Store) -> Result<()> {
    match command {
        Command::Add { title, description, tags, pillar } => {
            enforce_lineage(None)?;
            let tags = split_tags(tags);
            let mut task = store.add(title, description.unwrap_or_default(), tags)?;
            if let Some(p) = pillar {
                store.set_pillar(task.id, Some(p.clone()))?;
                task.pillar = Some(p.to_lowercase());
            }
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Spawn { parent_key, title, description, tags, pillar } => {
            enforce_lineage(Some(parent_key))?;
            let parent = find_by_key(store, parent_key)?;
            refuse_clone_of_parent(&parent, &title)?;
            let tags = split_tags(tags);
            let mut task = store.spawn_child(parent_key, title, description.unwrap_or_default(), tags)?;
            if let Some(p) = pillar {
                store.set_pillar(task.id, Some(p.clone()))?;
                task.pillar = Some(p.to_lowercase());
            }
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::List { pillar, status, view } => {
            let mut tasks = match view {
                ViewFilter::Active => store.list()?,
                ViewFilter::Archived => store.list_archived()?,
            };
            if let Some(p) = pillar {
                let p = p.to_lowercase();
                tasks.retain(|t| t.pillar.as_deref() == Some(p.as_str()));
            }
            tasks.retain(|t| match status {
                StatusFilter::All => true,
                StatusFilter::Open => !t.done,
                StatusFilter::Done => t.done,
            });
            println!("{}", serde_json::to_string_pretty(&tasks)?);
        }
        Command::Show { key } => {
            let task = find_by_key(store, key)?;
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Tree { key } => {
            let tree = store.tree(key)?;
            println!("{}", serde_json::to_string_pretty(&tree)?);
        }
        Command::Done { key } => {
            let mut task = find_by_key(store, key)?;
            if !task.done {
                store.toggle(task.id)?;
                task.done = true;
            }
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Launch { key } => {
            let task = find_by_key(store, key)?;
            crate::agent_session::launch_claude_for_task(store, &task)?;
        }
        Command::Resume => {
            crate::multiplexer::resume()?;
        }
        Command::Pillar { key, pillar } => {
            let mut task = find_by_key(store, key)?;
            let p = if is_clear(&pillar) { None } else { Some(pillar) };
            store.set_pillar(task.id, p.clone())?;
            task.pillar = p.map(|p| p.to_lowercase());
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Link { key, external_ref, remove } => {
            let task = find_by_key(store, key)?;
            if is_clear(&external_ref) {
                store.clear_external_refs(task.id)?;
            } else if remove {
                store.remove_external_ref(task.id, &external_ref)?;
            } else {
                store.add_external_ref(task.id, external_ref)?;
            }
            let task = find_by_key(store, key)?;
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Session { action } => run_session(action, store)?,
        Command::Profile { action } => run_profile(action, store)?,
    }
    Ok(())
}

fn run_session(action: SessionCommand, store: &dyn Store) -> Result<()> {
    match action {
        SessionCommand::SetPhase { key, phase, force } => {
            let task = find_by_key(store, key)?;
            store.set_session_phase(task.id, Some(phase), force)?;
        }
        SessionCommand::SetWaiting { key, reason } => {
            let task = find_by_key(store, key)?;
            store.set_waiting(task.id, Some(reason))?;
        }
        SessionCommand::ClearWaiting { key } => {
            let task = find_by_key(store, key)?;
            store.set_waiting(task.id, None)?;
        }
        SessionCommand::SetDecisions { key } => {
            let task = find_by_key(store, key)?;
            let mut blob = String::new();
            std::io::stdin().read_to_string(&mut blob)?;
            store.set_session_decisions(task.id, Some(blob))?;
        }
        SessionCommand::SetNext { key } => {
            let task = find_by_key(store, key)?;
            let mut blob = String::new();
            std::io::stdin().read_to_string(&mut blob)?;
            store.set_session_next(task.id, Some(blob))?;
        }
        SessionCommand::SetClaudeId { key, session_id } => {
            let task = find_by_key(store, key)?;
            store.set_claude_session_id(task.id, Some(session_id))?;
        }
        SessionCommand::ClearClaudeId { key } => {
            let task = find_by_key(store, key)?;
            store.set_claude_session_id(task.id, None)?;
        }
        SessionCommand::Show { key } => {
            let task = find_by_key(store, key)?;
            if task.phase.is_none()
                && task.session_decisions.is_none()
                && task.session_next.is_none()
                && task.claude_session_id.is_none()
                && task.waiting_on.is_none()
            {
                bail!("no session state for WAY-{key}");
            }
            if let Some(phase) = &task.phase {
                println!("phase: {phase}\n");
            }
            if let Some(decisions) = &task.session_decisions {
                println!("decisions:\n{decisions}\n");
            }
            if let Some(next) = &task.session_next {
                println!("next:\n{next}\n");
            }
            if let Some(reason) = &task.waiting_on {
                println!("waiting_on: {reason}");
                if let Some(since) = task.waiting_on_since {
                    println!("waiting_on_since (unix seconds): {since}\n");
                }
            }
            if let Some(updated_at) = task.session_updated_at {
                println!("updated_at (unix seconds): {updated_at}");
            }
            println!("claude_session_id: {}", task.claude_session_id.as_deref().unwrap_or("(none)"));
        }
        SessionCommand::Clear { key } => {
            let task = find_by_key(store, key)?;
            store.clear_session(task.id)?;
        }
    }
    Ok(())
}

fn run_profile(action: ProfileCommand, store: &dyn Store) -> Result<()> {
    match action {
        ProfileCommand::List => {
            println!("{}", serde_json::to_string_pretty(&store.list_profiles()?)?);
        }
        ProfileCommand::Use { name } => {
            store.use_profile(&name)?;
        }
        ProfileCommand::Add { name, pillars, phases } => {
            let pillars = parse_pillar_spec(&pillars)?;
            let phases = split_tags(phases);
            store.add_profile(Profile { name, pillars, default_issue_system: None, phases })?;
        }
    }
    Ok(())
}

fn split_tags(tags: Option<String>) -> Vec<String> {
    tags.map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}
