use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::store::Store;
use crate::task::{Pillar, Task};

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
        /// mind, body, relationships, craft, stability, or purpose
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
    /// Mark a task done (idempotent)
    Done { key: u32 },
    /// Set a task's pillar, or "clear" to unset it
    Pillar { key: u32, pillar: String },
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

fn parse_pillar(s: &str) -> Result<Pillar> {
    match s.to_lowercase().as_str() {
        "mind" => Ok(Pillar::Mind),
        "body" => Ok(Pillar::Body),
        "relationships" => Ok(Pillar::Relationships),
        "craft" => Ok(Pillar::Craft),
        "stability" => Ok(Pillar::Stability),
        "purpose" => Ok(Pillar::Purpose),
        other => bail!("unknown pillar '{other}' (expected mind/body/relationships/craft/stability/purpose)"),
    }
}

fn find_by_key(store: &dyn Store, key: u32) -> Result<Task> {
    let mut tasks = store.list()?;
    tasks.extend(store.list_archived()?);
    tasks.into_iter().find(|t| t.key == key).ok_or_else(|| anyhow!("no task with key WAY-{key}"))
}

pub fn run(command: Command, store: &dyn Store) -> Result<()> {
    match command {
        Command::Add { title, description, tags, pillar } => {
            let tags = tags
                .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            let mut task = store.add(title, description.unwrap_or_default(), tags)?;
            if let Some(p) = pillar {
                let p = parse_pillar(&p)?;
                store.set_pillar(task.id, Some(p))?;
                task.pillar = Some(p);
            }
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
        Command::List { pillar, status, view } => {
            let mut tasks = match view {
                ViewFilter::Active => store.list()?,
                ViewFilter::Archived => store.list_archived()?,
            };
            if let Some(p) = pillar {
                let p = parse_pillar(&p)?;
                tasks.retain(|t| t.pillar == Some(p));
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
            let p = if pillar.eq_ignore_ascii_case("clear") || pillar.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(parse_pillar(&pillar)?)
            };
            store.set_pillar(task.id, p)?;
            task.pillar = p;
            println!("{}", serde_json::to_string_pretty(&task)?);
        }
    }
    Ok(())
}
