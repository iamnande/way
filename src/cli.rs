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
    /// Operator-level settings for how way itself behaves
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Extra args passed to `claude` when the TUI spawns a session
    /// (e.g. "--dangerously-skip-permissions"). Space-separated, no quoting support.
    SetClaudeArgs {
        #[arg(allow_hyphen_values = true)]
        args: String,
    },
    /// Show current config
    Show,
    /// Clear the claude launch args
    ClearClaudeArgs,
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// Set the task's phase - free-form, not tied to any one workflow's
    /// vocabulary (e.g. "grounding", "planning", or anything else)
    SetPhase { key: u32, phase: String },
    /// Read decisions prose from stdin and store it
    SetDecisions { key: u32 },
    /// Read next-step prose from stdin and store it
    SetNext { key: u32 },
    /// Attach an existing claude session UUID (e.g. one you started outside
    /// way) so 'c' resumes it instead of pinning a new one on next spawn
    SetClaudeId { key: u32, session_id: String },
    /// Detach the claude session id - next spawn starts a new one
    ClearClaudeId { key: u32 },
    /// Print phase/decisions/next/updated-at/claude-session-id as labeled plain text
    Show { key: u32 },
    /// Clear phase, decisions, next, and updated-at together (leaves
    /// claude-session-id alone - that's the live conversation's identity,
    /// not a summary of it)
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
            let tags = split_tags(tags);
            let mut task = store.add(title, description.unwrap_or_default(), tags)?;
            if let Some(p) = pillar {
                store.set_pillar(task.id, Some(p.clone()))?;
                task.pillar = Some(p.to_lowercase());
            }
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::Spawn { parent_key, title, description, tags, pillar } => {
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
        Command::Config { action } => run_config(action, store)?,
    }
    Ok(())
}

fn run_config(action: ConfigCommand, store: &dyn Store) -> Result<()> {
    match action {
        ConfigCommand::SetClaudeArgs { args } => {
            store.set_claude_launch_args(Some(args))?;
        }
        ConfigCommand::Show => {
            let args = store.claude_launch_args()?;
            println!("claude_launch_args: {}", args.as_deref().unwrap_or("(none)"));
        }
        ConfigCommand::ClearClaudeArgs => {
            store.set_claude_launch_args(None)?;
        }
    }
    Ok(())
}

fn run_session(action: SessionCommand, store: &dyn Store) -> Result<()> {
    match action {
        SessionCommand::SetPhase { key, phase } => {
            let task = find_by_key(store, key)?;
            store.set_session_phase(task.id, Some(phase))?;
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
        ProfileCommand::Add { name, pillars } => {
            let pillars = parse_pillar_spec(&pillars)?;
            store.add_profile(Profile { name, pillars, default_issue_system: None })?;
        }
    }
    Ok(())
}

fn split_tags(tags: Option<String>) -> Vec<String> {
    tags.map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}
