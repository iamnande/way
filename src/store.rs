use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use redb::{Database, DatabaseError, ReadableDatabase, ReadableTable, TableDefinition};
use serde::Serialize;

use crate::config::{load_config, save_config};
use crate::craft::{Craft, CraftSession, CraftStatus};
use crate::journal::{JournalEntry, JournalEntryKind};
use crate::person::{Person, RelationshipKind};
use crate::principle::Principle;
use crate::routine::{Exercise, Routine, RoutineCompletion};
use crate::stability::{StabilityArea, StabilityStatus};
use crate::task::{Profile, Task};

const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tasks");
const JOURNAL_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("journal_entries");
const ROUTINES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("routines");
const COMPLETIONS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("routine_completions");
const PEOPLE_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("people");
const CRAFTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("crafts");
const CRAFT_SESSIONS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("craft_sessions");
const STABILITY_AREAS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("stability_areas");
const PRINCIPLES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("principles");

fn now_unix() -> Result<i64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
}

fn next_id_ns() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64)
}

/// Best-effort local username, used to stamp `Task::owner` at creation. No
/// login/auth exists yet, so this is a convenience label, not an identity.
fn local_owner() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "unknown".to_string())
}

#[derive(Debug, Serialize)]
pub struct TaskTree {
    pub task: Task,
    pub ancestors: Vec<Task>,
    pub descendants: Vec<Task>,
}

pub trait TaskStore {
    fn add(&self, title: String, description: String, tags: Vec<String>) -> Result<Task>;
    fn spawn_child(&self, parent_key: u32, title: String, description: String, tags: Vec<String>) -> Result<Task>;
    fn list(&self) -> Result<Vec<Task>>;
    fn list_archived(&self) -> Result<Vec<Task>>;
    fn find_by_key(&self, key: u32) -> Result<Option<Task>>;
    fn tree(&self, key: u32) -> Result<TaskTree>;
    fn toggle(&self, id: u64) -> Result<()>;
    fn update_fields(&self, id: u64, title: String, description: String, tags: Vec<String>) -> Result<()>;
    fn archive(&self, id: u64) -> Result<()>;
    fn unarchive(&self, id: u64) -> Result<()>;
    fn set_pillar(&self, id: u64, pillar: Option<String>) -> Result<()>;
    fn add_external_ref(&self, id: u64, external_ref: String) -> Result<()>;
    fn remove_external_ref(&self, id: u64, external_ref: &str) -> Result<()>;
    fn clear_external_refs(&self, id: u64) -> Result<()>;
    /// `force` bypasses order validation against the active profile's
    /// configured `phases` list (no-op when that list is empty either way).
    fn set_session_phase(&self, id: u64, phase: Option<String>, force: bool) -> Result<()>;
    fn set_session_decisions(&self, id: u64, decisions: Option<String>) -> Result<()>;
    fn set_session_next(&self, id: u64, next: Option<String>) -> Result<()>;
    fn clear_session(&self, id: u64) -> Result<()>;
    fn set_claude_session_id(&self, id: u64, session_id: Option<String>) -> Result<()>;
    /// `Some` sets `waiting_on` + stamps `waiting_on_since` together; `None`
    /// clears both together.
    fn set_waiting(&self, id: u64, reason: Option<String>) -> Result<()>;
}

pub trait ProfileStore {
    fn list_profiles(&self) -> Result<Vec<Profile>>;
    fn active_profile(&self) -> Result<Profile>;
    fn use_profile(&self, name: &str) -> Result<()>;
    fn add_profile(&self, profile: Profile) -> Result<()>;
}

pub trait RoutineStore {
    fn add_routine(&self, name: String) -> Result<Routine>;
    fn list_routines(&self) -> Result<Vec<Routine>>;
    fn list_archived_routines(&self) -> Result<Vec<Routine>>;
    fn find_routine(&self, name: &str) -> Result<Option<Routine>>;
    fn archive_routine(&self, name: &str) -> Result<()>;
    fn unarchive_routine(&self, name: &str) -> Result<()>;
    fn add_exercise(&self, routine: &str, exercise: Exercise) -> Result<()>;
    fn remove_exercise(&self, routine: &str, exercise_name: &str) -> Result<()>;
    fn log_completion(&self, routine: &str, completed_on: i64, note: Option<String>) -> Result<RoutineCompletion>;
    fn completion_history(&self, routine: &str) -> Result<Vec<RoutineCompletion>>;
}

pub trait PersonStore {
    fn add_person(&self, name: String, relationship: RelationshipKind) -> Result<Person>;
    fn list_people(&self) -> Result<Vec<Person>>;
    fn find_person(&self, id: u64) -> Result<Option<Person>>;
    fn remove_person(&self, id: u64) -> Result<()>;
    fn set_person_birthdate(&self, id: u64, birthdate: Option<i64>) -> Result<()>;
    fn add_person_dream(&self, id: u64, dream: String) -> Result<()>;
    fn remove_person_dream(&self, id: u64, dream: &str) -> Result<()>;
    fn add_person_hobby(&self, id: u64, hobby: String) -> Result<()>;
    fn remove_person_hobby(&self, id: u64, hobby: &str) -> Result<()>;
    fn add_person_attention_area(&self, id: u64, area: String) -> Result<()>;
    fn remove_person_attention_area(&self, id: u64, area: &str) -> Result<()>;
    fn set_person_preference(&self, id: u64, key: String, value: String) -> Result<()>;
    fn remove_person_preference(&self, id: u64, key: &str) -> Result<()>;
    fn set_person_notes(&self, id: u64, notes: String) -> Result<()>;
}

pub trait CraftStore {
    fn add_craft(&self, name: String, status: CraftStatus) -> Result<Craft>;
    fn list_crafts(&self, status: Option<CraftStatus>) -> Result<Vec<Craft>>;
    fn find_craft(&self, name: &str) -> Result<Option<Craft>>;
    fn remove_craft(&self, name: &str) -> Result<()>;
    fn set_craft_status(&self, name: &str, status: CraftStatus) -> Result<()>;
    fn set_craft_space(&self, name: &str, space: String) -> Result<()>;
    fn set_craft_standing(&self, name: &str, standing: String) -> Result<()>;
    fn set_craft_trajectory(&self, name: &str, trajectory: String) -> Result<()>;
    fn log_craft_session(&self, craft: &str, logged_on: i64, note: Option<String>) -> Result<CraftSession>;
    fn craft_session_history(&self, craft: &str) -> Result<Vec<CraftSession>>;
}

pub trait StabilityStore {
    fn add_stability_area(&self, name: String, status: StabilityStatus) -> Result<StabilityArea>;
    fn list_stability_areas(&self, status: Option<StabilityStatus>) -> Result<Vec<StabilityArea>>;
    fn find_stability_area(&self, name: &str) -> Result<Option<StabilityArea>>;
    fn remove_stability_area(&self, name: &str) -> Result<()>;
    fn set_stability_status(&self, name: &str, status: StabilityStatus) -> Result<()>;
    fn set_stability_standing(&self, name: &str, standing: String) -> Result<()>;
    fn set_stability_trajectory(&self, name: &str, trajectory: String) -> Result<()>;
}

pub trait PrincipleStore {
    fn add_principle(&self, text: String) -> Result<Principle>;
    fn list_principles(&self) -> Result<Vec<Principle>>;
    fn find_principle(&self, id: u64) -> Result<Option<Principle>>;
    fn remove_principle(&self, id: u64) -> Result<()>;
}

pub trait JournalStore {
    fn add_journal_entry(&self, kind: JournalEntryKind, content: String) -> Result<JournalEntry>;
    fn list_journal_entries(&self) -> Result<Vec<JournalEntry>>;
    fn find_journal_entry(&self, id: u64) -> Result<Option<JournalEntry>>;
    fn search_journal_entries(&self, query: &str) -> Result<Vec<JournalEntry>>;
    fn last_checkin(&self) -> Result<Option<JournalEntry>>;
}

pub trait Store: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore + PrincipleStore + JournalStore {}
impl<T> Store for T where T: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore + PrincipleStore + JournalStore {}

/// Holds only the path, not an open `Database` — redb takes an OS file lock
/// for as long as a `Database` handle is alive, and only one process may hold
/// it at a time. `way` is spawned long-lived (the TUI) but also invoked
/// as a one-shot CLI from *inside* a `claude` session it spawned (`way
/// session set-phase`, etc.), so the parent's handle can't be held for its
/// whole lifetime — that would lock the child out for as long as the parent
/// runs. Each operation opens fresh and lets the `Database` drop (releasing
/// the lock) as soon as its transaction is done.
pub struct RedbStore {
    path: PathBuf,
}

impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let store = Self { path: path.as_ref().to_path_buf() };
        {
            let db = store.db()?;
            let write_txn = db.begin_write()?;
            write_txn.open_table(TABLE)?;
            write_txn.open_table(JOURNAL_TABLE)?;
            write_txn.open_table(ROUTINES_TABLE)?;
            write_txn.open_table(COMPLETIONS_TABLE)?;
            write_txn.open_table(PEOPLE_TABLE)?;
            write_txn.open_table(CRAFTS_TABLE)?;
            write_txn.open_table(CRAFT_SESSIONS_TABLE)?;
            write_txn.open_table(STABILITY_AREAS_TABLE)?;
            write_txn.open_table(PRINCIPLES_TABLE)?;
            write_txn.commit()?;
        }
        store.backfill_keys()?;
        store.normalize_pillars()?;
        // Profile bootstrap now lives in Config::ensure_bootstrapped
        // (config.toml), triggered on first load_config() call.
        Ok(store)
    }

    /// redb only lets one process hold an open `Database` handle at a time,
    /// so a collision here means another `way` process is mid-transaction,
    /// not that the store is unusable. That window is only ever a single
    /// transaction wide (see the type-level doc comment), so a short retry
    /// clears it instead of surfacing a spurious "already open" error.
    /// Sidecar file bumped on every successful write. redb touches the
    /// database file's mtime on every *open*, including plain reads (its
    /// `Database::create`/`open` do bookkeeping that writes to the file
    /// regardless), so a filesystem watcher pointed at the `.redb` file
    /// directly would re-trigger itself on the TUI's own read-driven
    /// refreshes, forever. This file only changes when data actually
    /// changes, so watching it gives a clean, loop-free signal.
    pub fn marker_path(db_path: &Path) -> PathBuf {
        db_path.with_extension("touch")
    }

    fn touch_marker(&self) -> Result<()> {
        std::fs::write(Self::marker_path(&self.path), now_unix()?.to_string())?;
        Ok(())
    }

    fn db(&self) -> Result<Database> {
        let mut wait = Duration::from_millis(2);
        for _ in 0..10 {
            match Database::create(&self.path) {
                Ok(db) => return Ok(db),
                Err(DatabaseError::DatabaseAlreadyOpen) => {
                    std::thread::sleep(wait);
                    wait *= 2;
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(Database::create(&self.path)?)
    }

    fn all_tasks(&self) -> Result<Vec<Task>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let tasks: Vec<Task> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice(v.value()).ok())
            .collect();
        Ok(tasks)
    }

    /// Assigns real keys to any tasks that predate the `key` field (which default
    /// to 0 via serde), so they don't all collide on the same display key.
    fn backfill_keys(&self) -> Result<()> {
        let mut unkeyed: Vec<Task> = self.all_tasks()?.into_iter().filter(|t| t.key == 0).collect();
        if unkeyed.is_empty() {
            return Ok(());
        }
        unkeyed.sort_by_key(|t| t.id); // creation order, oldest first
        let start = self.next_key()?;
        for (offset, mut task) in unkeyed.into_iter().enumerate() {
            task.key = start + offset as u32;
            self.put(&task)?;
        }
        Ok(())
    }

    /// Old pillar values were serialized from a fixed enum (`"Mind"`, `"Body"`, ...).
    /// Profile-defined pillar names are lowercase by convention; this brings any
    /// legacy-cased value in line so name matching stays consistent.
    fn normalize_pillars(&self) -> Result<()> {
        let stale: Vec<Task> = self
            .all_tasks()?
            .into_iter()
            .filter(|t| t.pillar.as_deref().is_some_and(|p| p != p.to_lowercase()))
            .collect();
        for mut task in stale {
            task.pillar = task.pillar.map(|p| p.to_lowercase());
            self.put(&task)?;
        }
        Ok(())
    }

    fn put(&self, task: &Task) -> Result<()> {
        let bytes = serde_json::to_vec(task)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.insert(task.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }

    fn list_where(&self, archived: bool) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> = self.all_tasks()?.into_iter().filter(|t| t.archived == archived).collect();
        tasks.sort_by_key(|t| t.id);
        Ok(tasks)
    }

    fn get(&self, id: u64) -> Result<Option<Task>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn next_key(&self) -> Result<u32> {
        let max = self.all_tasks()?.into_iter().map(|t| t.key).max().unwrap_or(0);
        Ok(max + 1)
    }
}

impl TaskStore for RedbStore {
    fn add(&self, title: String, description: String, tags: Vec<String>) -> Result<Task> {
        let id = next_id_ns()?;
        let key = self.next_key()?;
        let task = Task::new(id, key, title, description, tags, local_owner());
        self.put(&task)?;
        Ok(task)
    }

    fn spawn_child(&self, parent_key: u32, title: String, description: String, tags: Vec<String>) -> Result<Task> {
        let all = self.all_tasks()?;
        if !all.iter().any(|t| t.key == parent_key) {
            bail!("no task with key WAY-{parent_key}");
        }
        let id = next_id_ns()?;
        let key = self.next_key()?;
        let mut task = Task::new(id, key, title, description, tags, local_owner());
        task.parent_key = Some(parent_key);
        self.put(&task)?;
        Ok(task)
    }

    fn list(&self) -> Result<Vec<Task>> {
        self.list_where(false)
    }

    fn list_archived(&self) -> Result<Vec<Task>> {
        self.list_where(true)
    }

    fn find_by_key(&self, key: u32) -> Result<Option<Task>> {
        Ok(self.all_tasks()?.into_iter().find(|t| t.key == key))
    }

    fn tree(&self, key: u32) -> Result<TaskTree> {
        let all = self.all_tasks()?;
        let task = all.iter().find(|t| t.key == key).cloned().ok_or_else(|| anyhow!("no task with key WAY-{key}"))?;

        let mut ancestors = Vec::new();
        let mut current = task.parent_key;
        while let Some(pk) = current {
            match all.iter().find(|t| t.key == pk) {
                Some(parent) => {
                    current = parent.parent_key;
                    ancestors.push(parent.clone());
                }
                None => break,
            }
        }

        fn collect_descendants(all: &[Task], parent_key: u32, out: &mut Vec<Task>) {
            for t in all.iter().filter(|t| t.parent_key == Some(parent_key)) {
                out.push(t.clone());
                collect_descendants(all, t.key, out);
            }
        }
        let mut descendants = Vec::new();
        collect_descendants(&all, key, &mut descendants);

        Ok(TaskTree { task, ancestors, descendants })
    }

    fn toggle(&self, id: u64) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.done = !task.done;
            self.put(&task)?;
        }
        Ok(())
    }

    fn update_fields(&self, id: u64, title: String, description: String, tags: Vec<String>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.title = title;
            task.description = description;
            task.tags = tags;
            self.put(&task)?;
        }
        Ok(())
    }

    fn archive(&self, id: u64) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.archived = true;
            self.put(&task)?;
        }
        Ok(())
    }

    fn unarchive(&self, id: u64) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.archived = false;
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_pillar(&self, id: u64, pillar: Option<String>) -> Result<()> {
        let pillar = match pillar {
            Some(name) => {
                let profile = self.active_profile()?;
                let def = profile
                    .find_pillar(&name)
                    .ok_or_else(|| anyhow!("unknown pillar '{name}' for active profile '{}'", profile.name))?;
                Some(def.name.clone())
            }
            None => None,
        };
        if let Some(mut task) = self.get(id)? {
            task.pillar = pillar;
            self.put(&task)?;
        }
        Ok(())
    }

    fn add_external_ref(&self, id: u64, external_ref: String) -> Result<()> {
        if let Some(mut task) = self.get(id)?
            && !task.external_refs.iter().any(|r| r == &external_ref)
        {
            task.external_refs.push(external_ref);
            self.put(&task)?;
        }
        Ok(())
    }

    fn remove_external_ref(&self, id: u64, external_ref: &str) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            let before = task.external_refs.len();
            task.external_refs.retain(|r| r != external_ref);
            if task.external_refs.len() != before {
                self.put(&task)?;
            }
        }
        Ok(())
    }

    fn clear_external_refs(&self, id: u64) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.external_refs.clear();
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_session_phase(&self, id: u64, phase: Option<String>, force: bool) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            if let (Some(new_phase), false) = (&phase, force) {
                let profile = self.active_profile()?;
                if !profile.phases.is_empty() {
                    let new_idx = profile.phases.iter().position(|p| p == new_phase).ok_or_else(|| {
                        anyhow!(
                            "unknown phase '{new_phase}' for active profile '{}' (pass --force to set it anyway)",
                            profile.name
                        )
                    })?;
                    let cur_idx = task.phase.as_ref().and_then(|p| profile.phases.iter().position(|x| x == p));
                    let skips_ahead = match cur_idx {
                        None => new_idx > 0,
                        Some(cur) => new_idx > cur + 1,
                    };
                    if skips_ahead {
                        bail!(
                            "phase '{new_phase}' skips ahead of '{}' in profile '{}' (pass --force to override)",
                            task.phase.as_deref().unwrap_or("(none)"),
                            profile.name
                        );
                    }
                }
            }
            task.phase = phase;
            task.session_updated_at = Some(now_unix()?);
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_waiting(&self, id: u64, reason: Option<String>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.waiting_on = reason;
            task.waiting_on_since = if task.waiting_on.is_some() { Some(now_unix()?) } else { None };
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_session_decisions(&self, id: u64, decisions: Option<String>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.session_decisions = decisions;
            task.session_updated_at = Some(now_unix()?);
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_session_next(&self, id: u64, next: Option<String>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.session_next = next;
            task.session_updated_at = Some(now_unix()?);
            self.put(&task)?;
        }
        Ok(())
    }

    fn clear_session(&self, id: u64) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.phase = None;
            task.session_decisions = None;
            task.session_next = None;
            task.session_updated_at = None;
            task.waiting_on = None;
            task.waiting_on_since = None;
            self.put(&task)?;
        }
        Ok(())
    }

    fn set_claude_session_id(&self, id: u64, session_id: Option<String>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.claude_session_id = session_id;
            self.put(&task)?;
        }
        Ok(())
    }
}

impl ProfileStore for RedbStore {
    fn list_profiles(&self) -> Result<Vec<Profile>> {
        let mut config = load_config()?;
        config.profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(config.profiles)
    }

    fn active_profile(&self) -> Result<Profile> {
        load_config()?.active_profile()
    }

    fn use_profile(&self, name: &str) -> Result<()> {
        let mut config = load_config()?;
        if config.find_profile(name).is_none() {
            bail!("no profile named '{name}'");
        }
        config.active_profile = Some(name.to_string());
        save_config(&config)
    }

    fn add_profile(&self, profile: Profile) -> Result<()> {
        let mut config = load_config()?;
        if config.find_profile(&profile.name).is_some() {
            bail!("profile '{}' already exists", profile.name);
        }
        config.profiles.push(profile);
        save_config(&config)
    }
}

impl RoutineStore for RedbStore {
    fn add_routine(&self, name: String) -> Result<Routine> {
        let db = self.db()?;
        {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(ROUTINES_TABLE)?;
            if table.get(name.as_str())?.is_some() {
                bail!("routine '{name}' already exists");
            }
        }
        let routine = Routine { name: name.clone(), exercises: Vec::new(), archived: false };
        let bytes = serde_json::to_vec(&routine)?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(ROUTINES_TABLE)?;
            table.insert(name.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(routine)
    }

    fn list_routines(&self) -> Result<Vec<Routine>> {
        Ok(self.all_routines()?.into_iter().filter(|r| !r.archived).collect())
    }

    fn list_archived_routines(&self) -> Result<Vec<Routine>> {
        Ok(self.all_routines()?.into_iter().filter(|r| r.archived).collect())
    }

    fn find_routine(&self, name: &str) -> Result<Option<Routine>> {
        self.get_routine(name)
    }

    fn archive_routine(&self, name: &str) -> Result<()> {
        if let Some(mut r) = self.get_routine(name)? {
            r.archived = true;
            self.put_routine(&r)?;
        }
        Ok(())
    }

    fn unarchive_routine(&self, name: &str) -> Result<()> {
        if let Some(mut r) = self.get_routine(name)? {
            r.archived = false;
            self.put_routine(&r)?;
        }
        Ok(())
    }

    fn add_exercise(&self, routine: &str, exercise: Exercise) -> Result<()> {
        let mut r = self.get_routine(routine)?.ok_or_else(|| anyhow!("no routine named '{routine}'"))?;
        r.exercises.push(exercise);
        self.put_routine(&r)
    }

    fn remove_exercise(&self, routine: &str, exercise_name: &str) -> Result<()> {
        let mut r = self.get_routine(routine)?.ok_or_else(|| anyhow!("no routine named '{routine}'"))?;
        if let Some(pos) = r.exercises.iter().position(|e| e.name == exercise_name) {
            r.exercises.remove(pos);
            self.put_routine(&r)?;
        }
        Ok(())
    }

    fn log_completion(&self, routine: &str, completed_on: i64, note: Option<String>) -> Result<RoutineCompletion> {
        if self.get_routine(routine)?.is_none() {
            bail!("no routine named '{routine}'");
        }
        let completion = RoutineCompletion { id: next_id_ns()?, routine_name: routine.to_string(), completed_on, note };
        let bytes = serde_json::to_vec(&completion)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(COMPLETIONS_TABLE)?;
            table.insert(completion.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(completion)
    }

    fn completion_history(&self, routine: &str) -> Result<Vec<RoutineCompletion>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(COMPLETIONS_TABLE)?;
        let mut completions: Vec<RoutineCompletion> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice::<RoutineCompletion>(v.value()).ok())
            .filter(|c| c.routine_name == routine)
            .collect();
        completions.sort_by_key(|c| std::cmp::Reverse(c.completed_on));
        Ok(completions)
    }
}

impl RedbStore {
    fn all_routines(&self) -> Result<Vec<Routine>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(ROUTINES_TABLE)?;
        Ok(table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect())
    }

    fn get_routine(&self, name: &str) -> Result<Option<Routine>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(ROUTINES_TABLE)?;
        match table.get(name)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn put_routine(&self, routine: &Routine) -> Result<()> {
        let bytes = serde_json::to_vec(routine)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(ROUTINES_TABLE)?;
            table.insert(routine.name.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }
}

impl PersonStore for RedbStore {
    fn add_person(&self, name: String, relationship: RelationshipKind) -> Result<Person> {
        let person = Person::new(next_id_ns()?, name, relationship);
        self.put_person(&person)?;
        Ok(person)
    }

    fn list_people(&self) -> Result<Vec<Person>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(PEOPLE_TABLE)?;
        Ok(table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect())
    }

    fn find_person(&self, id: u64) -> Result<Option<Person>> {
        self.get_person(id)
    }

    fn remove_person(&self, id: u64) -> Result<()> {
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PEOPLE_TABLE)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }

    fn set_person_birthdate(&self, id: u64, birthdate: Option<i64>) -> Result<()> {
        self.edit_person(id, |p| p.birthdate = birthdate)
    }

    fn add_person_dream(&self, id: u64, dream: String) -> Result<()> {
        self.edit_person(id, |p| p.dreams_aspirations.push(dream))
    }

    fn remove_person_dream(&self, id: u64, dream: &str) -> Result<()> {
        self.edit_person(id, |p| p.dreams_aspirations.retain(|d| d != dream))
    }

    fn add_person_hobby(&self, id: u64, hobby: String) -> Result<()> {
        self.edit_person(id, |p| p.hobbies.push(hobby))
    }

    fn remove_person_hobby(&self, id: u64, hobby: &str) -> Result<()> {
        self.edit_person(id, |p| p.hobbies.retain(|h| h != hobby))
    }

    fn add_person_attention_area(&self, id: u64, area: String) -> Result<()> {
        self.edit_person(id, |p| p.attention_areas.push(area))
    }

    fn remove_person_attention_area(&self, id: u64, area: &str) -> Result<()> {
        self.edit_person(id, |p| p.attention_areas.retain(|a| a != area))
    }

    fn set_person_preference(&self, id: u64, key: String, value: String) -> Result<()> {
        self.edit_person(id, |p| match p.preferences.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = value,
            None => p.preferences.push((key, value)),
        })
    }

    fn remove_person_preference(&self, id: u64, key: &str) -> Result<()> {
        self.edit_person(id, |p| p.preferences.retain(|(k, _)| k != key))
    }

    fn set_person_notes(&self, id: u64, notes: String) -> Result<()> {
        self.edit_person(id, |p| p.notes = notes)
    }
}

impl RedbStore {
    fn get_person(&self, id: u64) -> Result<Option<Person>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(PEOPLE_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn put_person(&self, person: &Person) -> Result<()> {
        let bytes = serde_json::to_vec(person)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PEOPLE_TABLE)?;
            table.insert(person.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }

    fn edit_person(&self, id: u64, f: impl FnOnce(&mut Person)) -> Result<()> {
        if let Some(mut person) = self.get_person(id)? {
            f(&mut person);
            self.put_person(&person)?;
        }
        Ok(())
    }
}

impl CraftStore for RedbStore {
    fn add_craft(&self, name: String, status: CraftStatus) -> Result<Craft> {
        let db = self.db()?;
        {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(CRAFTS_TABLE)?;
            if table.get(name.as_str())?.is_some() {
                bail!("craft '{name}' already exists");
            }
        }
        let craft = Craft { name: name.clone(), status, space: String::new(), standing: String::new(), trajectory: String::new() };
        let bytes = serde_json::to_vec(&craft)?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRAFTS_TABLE)?;
            table.insert(name.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(craft)
    }

    fn list_crafts(&self, status: Option<CraftStatus>) -> Result<Vec<Craft>> {
        let all = self.all_crafts()?;
        Ok(match status {
            Some(s) => all.into_iter().filter(|c| c.status == s).collect(),
            None => all,
        })
    }

    fn find_craft(&self, name: &str) -> Result<Option<Craft>> {
        self.get_craft(name)
    }

    fn remove_craft(&self, name: &str) -> Result<()> {
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRAFTS_TABLE)?;
            table.remove(name)?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }

    fn set_craft_status(&self, name: &str, status: CraftStatus) -> Result<()> {
        self.edit_craft(name, |c| c.status = status)
    }

    fn set_craft_space(&self, name: &str, space: String) -> Result<()> {
        self.edit_craft(name, |c| c.space = space)
    }

    fn set_craft_standing(&self, name: &str, standing: String) -> Result<()> {
        self.edit_craft(name, |c| c.standing = standing)
    }

    fn set_craft_trajectory(&self, name: &str, trajectory: String) -> Result<()> {
        self.edit_craft(name, |c| c.trajectory = trajectory)
    }

    fn log_craft_session(&self, craft: &str, logged_on: i64, note: Option<String>) -> Result<CraftSession> {
        if self.get_craft(craft)?.is_none() {
            bail!("no craft named '{craft}'");
        }
        let session = CraftSession { id: next_id_ns()?, craft_name: craft.to_string(), logged_on, note };
        let bytes = serde_json::to_vec(&session)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRAFT_SESSIONS_TABLE)?;
            table.insert(session.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(session)
    }

    fn craft_session_history(&self, craft: &str) -> Result<Vec<CraftSession>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(CRAFT_SESSIONS_TABLE)?;
        let mut sessions: Vec<CraftSession> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice::<CraftSession>(v.value()).ok())
            .filter(|s| s.craft_name == craft)
            .collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.logged_on));
        Ok(sessions)
    }
}

impl RedbStore {
    fn all_crafts(&self) -> Result<Vec<Craft>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(CRAFTS_TABLE)?;
        Ok(table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect())
    }

    fn get_craft(&self, name: &str) -> Result<Option<Craft>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(CRAFTS_TABLE)?;
        match table.get(name)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn edit_craft(&self, name: &str, f: impl FnOnce(&mut Craft)) -> Result<()> {
        if let Some(mut craft) = self.get_craft(name)? {
            f(&mut craft);
            let bytes = serde_json::to_vec(&craft)?;
            let db = self.db()?;
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(CRAFTS_TABLE)?;
                table.insert(craft.name.as_str(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            self.touch_marker()?;
        }
        Ok(())
    }
}

impl StabilityStore for RedbStore {
    fn add_stability_area(&self, name: String, status: StabilityStatus) -> Result<StabilityArea> {
        let db = self.db()?;
        {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(STABILITY_AREAS_TABLE)?;
            if table.get(name.as_str())?.is_some() {
                bail!("stability area '{name}' already exists");
            }
        }
        let area = StabilityArea { id: next_id_ns()?, name: name.clone(), status, standing: String::new(), trajectory: String::new() };
        let bytes = serde_json::to_vec(&area)?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(STABILITY_AREAS_TABLE)?;
            table.insert(name.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(area)
    }

    fn list_stability_areas(&self, status: Option<StabilityStatus>) -> Result<Vec<StabilityArea>> {
        let all = self.all_stability_areas()?;
        Ok(match status {
            Some(s) => all.into_iter().filter(|a| a.status == s).collect(),
            None => all,
        })
    }

    fn find_stability_area(&self, name: &str) -> Result<Option<StabilityArea>> {
        self.get_stability_area(name)
    }

    fn remove_stability_area(&self, name: &str) -> Result<()> {
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(STABILITY_AREAS_TABLE)?;
            table.remove(name)?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }

    fn set_stability_status(&self, name: &str, status: StabilityStatus) -> Result<()> {
        self.edit_stability_area(name, |a| a.status = status)
    }

    fn set_stability_standing(&self, name: &str, standing: String) -> Result<()> {
        self.edit_stability_area(name, |a| a.standing = standing)
    }

    fn set_stability_trajectory(&self, name: &str, trajectory: String) -> Result<()> {
        self.edit_stability_area(name, |a| a.trajectory = trajectory)
    }
}

impl RedbStore {
    fn all_stability_areas(&self) -> Result<Vec<StabilityArea>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(STABILITY_AREAS_TABLE)?;
        Ok(table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect())
    }

    fn get_stability_area(&self, name: &str) -> Result<Option<StabilityArea>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(STABILITY_AREAS_TABLE)?;
        match table.get(name)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn edit_stability_area(&self, name: &str, f: impl FnOnce(&mut StabilityArea)) -> Result<()> {
        if let Some(mut area) = self.get_stability_area(name)? {
            f(&mut area);
            let bytes = serde_json::to_vec(&area)?;
            let db = self.db()?;
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(STABILITY_AREAS_TABLE)?;
                table.insert(area.name.as_str(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            self.touch_marker()?;
        }
        Ok(())
    }
}

impl PrincipleStore for RedbStore {
    fn add_principle(&self, text: String) -> Result<Principle> {
        let principle = Principle { id: next_id_ns()?, created_at: now_unix()?, text };
        let bytes = serde_json::to_vec(&principle)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PRINCIPLES_TABLE)?;
            table.insert(principle.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(principle)
    }

    fn list_principles(&self) -> Result<Vec<Principle>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(PRINCIPLES_TABLE)?;
        let mut principles: Vec<Principle> =
            table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect();
        principles.sort_by_key(|p| std::cmp::Reverse((p.created_at, p.id))); // id (ns-resolution) tiebreaks same-second entries
        Ok(principles)
    }

    fn find_principle(&self, id: u64) -> Result<Option<Principle>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(PRINCIPLES_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn remove_principle(&self, id: u64) -> Result<()> {
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PRINCIPLES_TABLE)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(())
    }
}

impl JournalStore for RedbStore {
    fn add_journal_entry(&self, kind: JournalEntryKind, content: String) -> Result<JournalEntry> {
        let entry = JournalEntry { id: next_id_ns()?, created_at: now_unix()?, kind, content };
        let bytes = serde_json::to_vec(&entry)?;
        let db = self.db()?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(JOURNAL_TABLE)?;
            table.insert(entry.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        self.touch_marker()?;
        Ok(entry)
    }

    fn list_journal_entries(&self) -> Result<Vec<JournalEntry>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(JOURNAL_TABLE)?;
        let mut entries: Vec<JournalEntry> =
            table.iter()?.filter_map(|entry| entry.ok()).filter_map(|(_, v)| serde_json::from_slice(v.value()).ok()).collect();
        entries.sort_by_key(|e| std::cmp::Reverse((e.created_at, e.id))); // id (ns-resolution) tiebreaks same-second entries
        Ok(entries)
    }

    fn find_journal_entry(&self, id: u64) -> Result<Option<JournalEntry>> {
        let db = self.db()?;
        let read_txn = db.begin_read()?;
        let table = read_txn.open_table(JOURNAL_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn search_journal_entries(&self, query: &str) -> Result<Vec<JournalEntry>> {
        let query = query.to_lowercase();
        Ok(self.list_journal_entries()?.into_iter().filter(|e| e.content.to_lowercase().contains(&query)).collect())
    }

    fn last_checkin(&self) -> Result<Option<JournalEntry>> {
        Ok(self.list_journal_entries()?.into_iter().find(|e| e.kind == JournalEntryKind::CheckIn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::craft::CraftStatus;
    use crate::journal::JournalEntryKind;
    use crate::person::RelationshipKind;
    use crate::routine::Exercise;
    use crate::stability::StabilityStatus;

    fn temp_path(label: &str) -> String {
        std::env::temp_dir().join(format!("way-store-test-{label}-{}.redb", std::process::id())).to_str().unwrap().to_string()
    }

    /// Points config.toml at an isolated scratch file for the duration of
    /// this test's thread, so profile-related tests don't read/clobber the
    /// real `~/.config/way/config.toml` or race other concurrently-running
    /// tests (thread-local, not an env var - cargo test runs `#[test]`s on
    /// separate OS threads, and env vars are process-global).
    fn isolate_config(label: &str) {
        let path = std::env::temp_dir().join(format!("way-config-test-{label}-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        crate::config::set_config_path_override(Some(path));
    }

    #[test]
    fn survives_reopen() {
        let path = temp_path("survives-reopen");
        let _ = std::fs::remove_file(&path);

        {
            let store = RedbStore::open(&path).unwrap();
            store.add("survives a restart".to_string(), String::new(), vec![]).unwrap();
        } // store (and its Database handle) dropped here, exactly like process exit

        let store = RedbStore::open(&path).unwrap();
        let tasks = store.list().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "survives a restart");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn old_records_without_pillar_still_deserialize() {
        isolate_config("no-pillar");
        let path = temp_path("no-pillar");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("assign me".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(task.pillar, None);

        store.set_pillar(task.id, Some("craft".to_string())).unwrap();
        let tasks = store.list().unwrap();
        assert_eq!(tasks[0].pillar, Some("craft".to_string()));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn keys_increment_and_are_never_reused() {
        let path = temp_path("keys");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let t1 = store.add("one".to_string(), String::new(), vec![]).unwrap();
        let t2 = store.add("two".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(t1.key, 1);
        assert_eq!(t2.key, 2);

        store.archive(t1.id).unwrap();
        let t3 = store.add("three".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(t3.key, 3, "key should not be reused after archiving t1");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn backfills_missing_keys_on_open() {
        let path = temp_path("backfill");
        let _ = std::fs::remove_file(&path);

        {
            let store = RedbStore::open(&path).unwrap();
            // Simulate legacy records that predate the key field (key defaults to 0).
            let mut a = store.add("first".to_string(), String::new(), vec![]).unwrap();
            a.key = 0;
            store.put(&a).unwrap();
            let mut b = store.add("second".to_string(), String::new(), vec![]).unwrap();
            b.key = 0;
            store.put(&b).unwrap();
        }

        let store = RedbStore::open(&path).unwrap();
        let keys: Vec<u32> = store.list().unwrap().iter().map(|t| t.key).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|k| *k != 0), "backfilled tasks must not stay at key 0");
        assert_ne!(keys[0], keys[1], "backfilled keys must be unique, not both WAY-0");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn pillar_migration_normalizes_legacy_capitalized_values() {
        let path = temp_path("normalize");
        let _ = std::fs::remove_file(&path);

        {
            let store = RedbStore::open(&path).unwrap();
            let mut task = store.add("legacy".to_string(), String::new(), vec![]).unwrap();
            task.pillar = Some("Mind".to_string()); // simulates the old fixed-enum serialization
            store.put(&task).unwrap();
        }

        let store = RedbStore::open(&path).unwrap();
        let task = store.find_by_key(1).unwrap().unwrap();
        assert_eq!(task.pillar, Some("mind".to_string()));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn fresh_store_bootstraps_default_personal_profile() {
        isolate_config("bootstrap");
        let path = temp_path("bootstrap");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let profiles = store.list_profiles().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "personal");
        assert_eq!(profiles[0].pillars.len(), 6);

        let active = store.active_profile().unwrap();
        assert_eq!(active.name, "personal");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn spawn_creates_linked_child_and_rejects_unknown_parent() {
        let path = temp_path("spawn");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let parent = store.add("spike".to_string(), String::new(), vec![]).unwrap();
        let child = store.spawn_child(parent.key, "prd".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(child.parent_key, Some(parent.key));

        let err = store.spawn_child(9999, "orphan".to_string(), String::new(), vec![]);
        assert!(err.is_err());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn tree_returns_full_ancestor_chain_and_descendants() {
        let path = temp_path("tree");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let spike = store.add("spike".to_string(), String::new(), vec![]).unwrap();
        let prd = store.spawn_child(spike.key, "prd".to_string(), String::new(), vec![]).unwrap();
        let ticket = store.spawn_child(prd.key, "ticket".to_string(), String::new(), vec![]).unwrap();

        let tree = store.tree(prd.key).unwrap();
        assert_eq!(tree.task.key, prd.key);
        assert_eq!(tree.ancestors.len(), 1);
        assert_eq!(tree.ancestors[0].key, spike.key);
        assert_eq!(tree.descendants.len(), 1);
        assert_eq!(tree.descendants[0].key, ticket.key);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn session_phase_decisions_and_next_round_trip_independently_and_share_updated_at() {
        isolate_config("session");
        let path = temp_path("session");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("has a session".to_string(), String::new(), vec![]).unwrap();

        store.set_session_decisions(task.id, Some("chose structured prose over a blob".to_string())).unwrap();
        let after_decisions = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(after_decisions.session_decisions, Some("chose structured prose over a blob".to_string()));
        assert_eq!(after_decisions.session_next, None);
        assert!(after_decisions.session_updated_at.is_some());
        let first_timestamp = after_decisions.session_updated_at.unwrap();

        store.set_session_next(task.id, Some("draft the tech spec".to_string())).unwrap();
        let after_next = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(after_next.session_decisions, Some("chose structured prose over a blob".to_string()), "setting next must not touch decisions");
        assert_eq!(after_next.session_next, Some("draft the tech spec".to_string()));
        assert!(after_next.session_updated_at.unwrap() >= first_timestamp, "updated_at is shared across both fields");

        store.set_session_phase(task.id, Some("grounding".to_string()), false).unwrap();
        let after_phase = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(after_phase.phase, Some("grounding".to_string()));
        assert_eq!(after_phase.session_decisions, Some("chose structured prose over a blob".to_string()), "setting phase must not touch decisions");
        assert_eq!(after_phase.session_next, Some("draft the tech spec".to_string()), "setting phase must not touch next");

        store.set_waiting(task.id, Some("needs architecture input".to_string())).unwrap();
        let after_waiting = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(after_waiting.waiting_on, Some("needs architecture input".to_string()));
        assert!(after_waiting.waiting_on_since.is_some());
        assert_eq!(after_waiting.phase, Some("grounding".to_string()), "setting waiting_on must not touch phase");

        store.clear_session(task.id).unwrap();
        let cleared = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(cleared.phase, None);
        assert_eq!(cleared.session_decisions, None);
        assert_eq!(cleared.session_next, None);
        assert_eq!(cleared.session_updated_at, None);
        assert_eq!(cleared.waiting_on, None);
        assert_eq!(cleared.waiting_on_since, None);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn waiting_on_round_trips_and_clear_waiting_clears_both_fields() {
        let path = temp_path("waiting");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("might get blocked".to_string(), String::new(), vec![]).unwrap();

        store.set_waiting(task.id, Some("prd needs architecture input/direction".to_string())).unwrap();
        let blocked = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(blocked.waiting_on, Some("prd needs architecture input/direction".to_string()));
        assert!(blocked.waiting_on_since.is_some());

        store.set_waiting(task.id, None).unwrap();
        let cleared = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(cleared.waiting_on, None);
        assert_eq!(cleared.waiting_on_since, None);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn waiting_on_alone_satisfies_the_session_show_guard() {
        let path = temp_path("waiting-guard");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("only waiting, no other session state".to_string(), String::new(), vec![]).unwrap();
        store.set_waiting(task.id, Some("needs a decision".to_string())).unwrap();

        let reloaded = store.find_by_key(task.key).unwrap().unwrap();
        assert!(reloaded.phase.is_none());
        assert!(reloaded.session_decisions.is_none());
        assert!(reloaded.session_next.is_none());
        assert!(reloaded.claude_session_id.is_none());
        assert!(reloaded.waiting_on.is_some(), "the CLI's no-session-state guard must treat this as having session state");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn phase_order_is_enforced_only_when_the_active_profile_configures_one() {
        isolate_config("phase-order");
        let path = temp_path("phase-order");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("follows a process".to_string(), String::new(), vec![]).unwrap();

        // default "personal" profile has phases: [] -> unconditionally unenforced
        store.set_session_phase(task.id, Some("anything-goes".to_string()), false).unwrap();

        store
            .add_profile(Profile {
                name: "work".to_string(),
                pillars: vec![],
                default_issue_system: None,
                phases: vec!["grounding".to_string(), "spec".to_string(), "planning".to_string()],
            })
            .unwrap();
        store.use_profile("work").unwrap();
        let task = store.add("follows senzu's process".to_string(), String::new(), vec![]).unwrap();

        store.set_session_phase(task.id, Some("grounding".to_string()), false).unwrap();
        store.set_session_phase(task.id, Some("spec".to_string()), false).unwrap();

        // backward move: allowed, revisiting is rigor not a violation
        store.set_session_phase(task.id, Some("grounding".to_string()), false).unwrap();
        store.set_session_phase(task.id, Some("spec".to_string()), false).unwrap();

        // skips "planning" straight past it (there's nothing after planning to skip to
        // here, so instead assert skipping spec entirely from a fresh task)
        let fresh = store.add("skips ahead".to_string(), String::new(), vec![]).unwrap();
        let skip_err = store.set_session_phase(fresh.id, Some("planning".to_string()), false);
        assert!(skip_err.is_err(), "grounding -> planning must skip spec and be rejected");
        store.set_session_phase(fresh.id, Some("planning".to_string()), true).unwrap(); // --force overrides
        let forced = store.find_by_key(fresh.key).unwrap().unwrap();
        assert_eq!(forced.phase, Some("planning".to_string()));

        let unknown_err = store.set_session_phase(task.id, Some("nonexistent".to_string()), false);
        assert!(unknown_err.is_err(), "a phase absent from the configured list must be rejected");
        store.set_session_phase(task.id, Some("nonexistent".to_string()), true).unwrap(); // --force overrides

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn external_refs_add_is_idempotent_and_remove_targets_one_entry() {
        let path = temp_path("refs");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("has refs".to_string(), String::new(), vec![]).unwrap();

        store.add_external_ref(task.id, "owner/repo#1".to_string()).unwrap();
        store.add_external_ref(task.id, "owner/repo#1".to_string()).unwrap(); // duplicate, should be a no-op
        store.add_external_ref(task.id, "PROJ-42".to_string()).unwrap();
        let with_refs = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(with_refs.external_refs, vec!["owner/repo#1".to_string(), "PROJ-42".to_string()]);

        store.remove_external_ref(task.id, "owner/repo#1").unwrap();
        let one_removed = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(one_removed.external_refs, vec!["PROJ-42".to_string()]);

        store.clear_external_refs(task.id).unwrap();
        let cleared = store.find_by_key(task.key).unwrap().unwrap();
        assert!(cleared.external_refs.is_empty());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn pillar_assignment_rejects_name_not_in_active_profile() {
        isolate_config("pillarcheck");
        let path = temp_path("pillarcheck");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let task = store.add("needs a pillar".to_string(), String::new(), vec![]).unwrap();

        let err = store.set_pillar(task.id, Some("not-a-real-pillar".to_string()));
        assert!(err.is_err());

        store.set_pillar(task.id, Some("craft".to_string())).unwrap();
        let reloaded = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(reloaded.pillar, Some("craft".to_string()));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn claude_session_id_persists_across_reopen() {
        let path = temp_path("session-id");
        let _ = std::fs::remove_file(&path);

        {
            let store = RedbStore::open(&path).unwrap();
            let task = store.add("resumable task".to_string(), String::new(), vec![]).unwrap();
            assert_eq!(task.claude_session_id, None);
            store.set_claude_session_id(task.id, Some("11111111-1111-1111-1111-111111111111".to_string())).unwrap();
        }

        let store = RedbStore::open(&path).unwrap();
        let task = store.find_by_key(1).unwrap().unwrap();
        assert_eq!(task.claude_session_id, Some("11111111-1111-1111-1111-111111111111".to_string()));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn owner_is_stamped_at_creation_and_survives_reopen() {
        let path = temp_path("owner");
        let _ = std::fs::remove_file(&path);

        {
            let store = RedbStore::open(&path).unwrap();
            let task = store.add("has an owner".to_string(), String::new(), vec![]).unwrap();
            assert_eq!(task.owner, local_owner());
            assert!(!task.owner.is_empty());
        }

        let store = RedbStore::open(&path).unwrap();
        let task = store.find_by_key(1).unwrap().unwrap();
        assert_eq!(task.owner, local_owner());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn old_records_without_owner_default_to_empty_string() {
        let path = temp_path("no-owner");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let mut task = store.add("predates owner".to_string(), String::new(), vec![]).unwrap();
        task.owner = String::new(); // simulate a legacy record serialized before this field existed
        store.put(&task).unwrap();

        let reloaded = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(reloaded.owner, "");

        std::fs::remove_file(&path).unwrap();
    }

    /// The core of WAY-3: a second, independent `RedbStore::open` on the same
    /// path must succeed and see prior writes, simulating a `way session
    /// set-phase` CLI call made by a claude session while the spawning `way`
    /// TUI process's own `RedbStore` handle is still alive.
    #[test]
    fn concurrent_opens_on_the_same_path_do_not_lock_each_other_out() {
        let path = temp_path("concurrent-opens");
        let _ = std::fs::remove_file(&path);

        let parent = RedbStore::open(&path).unwrap();
        let task = parent.add("obstacle".to_string(), String::new(), vec![]).unwrap();

        // parent's RedbStore handle is still alive here, unlike survives_reopen
        let child = RedbStore::open(&path).unwrap();
        child.set_session_phase(task.id, Some("in-flight".to_string()), false).unwrap();

        let seen_by_parent = parent.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(seen_by_parent.phase, Some("in-flight".to_string()));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn routine_aggregates_and_completion_history() {
        let path = temp_path("routine");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        store.add_routine("push day".to_string()).unwrap();
        assert!(store.add_routine("push day".to_string()).is_err(), "duplicate name must be rejected");

        store
            .add_exercise("push day", Exercise { name: "bench".to_string(), sets: 4, reps: 8, intensity: 0.8, friction: 0.6, duration_secs: 600 })
            .unwrap();
        let r = store.find_routine("push day").unwrap().unwrap();
        assert_eq!(r.exercises.len(), 1);
        assert_eq!(r.duration_secs(), 600);

        store.remove_exercise("push day", "bench").unwrap();
        let r = store.find_routine("push day").unwrap().unwrap();
        assert!(r.exercises.is_empty());

        store.archive_routine("push day").unwrap();
        assert!(store.list_routines().unwrap().is_empty());
        assert_eq!(store.list_archived_routines().unwrap().len(), 1);

        // logging a completion never requires the routine to be active (R10)
        store.log_completion("push day", 1000, Some("felt strong".to_string())).unwrap();
        store.log_completion("push day", 2000, None).unwrap();
        let history = store.completion_history("push day").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].completed_on, 2000, "reverse chronological");

        assert!(store.log_completion("nonexistent", 1000, None).is_err());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn person_crud_and_list_mutators() {
        let path = temp_path("person");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let a = store.add_person("youngest".to_string(), RelationshipKind::Child).unwrap();
        let b = store.add_person("youngest".to_string(), RelationshipKind::Child).unwrap();
        assert_ne!(a.id, b.id, "duplicate names are allowed - id-keyed, not name-keyed");

        store.add_person_hobby(a.id, "skateboarding".to_string()).unwrap();
        store.add_person_hobby(a.id, "drawing".to_string()).unwrap();
        store.remove_person_hobby(a.id, "drawing".to_string().as_str()).unwrap();
        let reloaded = store.find_person(a.id).unwrap().unwrap();
        assert_eq!(reloaded.hobbies, vec!["skateboarding".to_string()]);

        store.set_person_preference(a.id, "gatorade flavor".to_string(), "glacier freeze".to_string()).unwrap();
        store.set_person_preference(a.id, "gatorade flavor".to_string(), "blue".to_string()).unwrap();
        let reloaded = store.find_person(a.id).unwrap().unwrap();
        assert_eq!(reloaded.preferences, vec![("gatorade flavor".to_string(), "blue".to_string())], "upsert, not append");

        store.remove_person(a.id).unwrap();
        assert!(store.find_person(a.id).unwrap().is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn craft_status_filter_and_session_log() {
        let path = temp_path("craft");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        store.add_craft("skateboarding".to_string(), CraftStatus::Active).unwrap();
        store.add_craft("woodworking".to_string(), CraftStatus::Dormant).unwrap();
        assert!(store.add_craft("skateboarding".to_string(), CraftStatus::Active).is_err());

        assert_eq!(store.list_crafts(None).unwrap().len(), 2);
        assert_eq!(store.list_crafts(Some(CraftStatus::Active)).unwrap().len(), 1);

        store.set_craft_standing("skateboarding", "landed 50-50 stalls".to_string()).unwrap();
        let c = store.find_craft("skateboarding").unwrap().unwrap();
        assert_eq!(c.standing, "landed 50-50 stalls");

        // logging never requires Active (dormant woodworking still loggable)
        store.log_craft_session("woodworking", 1000, None).unwrap();
        store.log_craft_session("woodworking", 2000, None).unwrap();
        assert_eq!(store.craft_session_history("woodworking").unwrap().len(), 2);
        assert!(store.log_craft_session("nonexistent", 1000, None).is_err());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn stability_area_crud() {
        let path = temp_path("stability");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        store.add_stability_area("safety net".to_string(), StabilityStatus::Active).unwrap();
        store.set_stability_standing("safety net", "6mo expenses".to_string()).unwrap();
        store.set_stability_trajectory("safety net", "increase transfer".to_string()).unwrap();

        let area = store.find_stability_area("safety net").unwrap().unwrap();
        assert_eq!(area.standing, "6mo expenses");
        assert_eq!(area.trajectory, "increase transfer");

        store.set_stability_status("safety net", StabilityStatus::Dormant).unwrap();
        assert_eq!(store.list_stability_areas(Some(StabilityStatus::Dormant)).unwrap().len(), 1);
        assert_eq!(store.list_stability_areas(Some(StabilityStatus::Active)).unwrap().len(), 0);

        store.remove_stability_area("safety net").unwrap();
        assert!(store.find_stability_area("safety net").unwrap().is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn principle_append_only_and_delete() {
        let path = temp_path("principle");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        let p1 = store.add_principle("first".to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        let p2 = store.add_principle("second".to_string()).unwrap();

        let list = store.list_principles().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, p2.id, "reverse chronological");

        store.remove_principle(p1.id).unwrap();
        assert!(store.find_principle(p1.id).unwrap().is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn journal_entries_search_and_last_checkin() {
        let path = temp_path("journal");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        store.add_journal_entry(JournalEntryKind::Freeform, "woke up thinking about the HOA board".to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        let checkin = store.add_journal_entry(JournalEntryKind::CheckIn, "Q: attention?\nA: way's vision work".to_string()).unwrap();

        let results = store.search_journal_entries("hoa").unwrap();
        assert_eq!(results.len(), 1);
        assert!(store.search_journal_entries("nonexistent-term").unwrap().is_empty());

        let last = store.last_checkin().unwrap().unwrap();
        assert_eq!(last.id, checkin.id);

        std::fs::remove_file(&path).unwrap();
    }
}
