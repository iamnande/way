use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::Serialize;

use crate::task::{Profile, Task};

const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tasks");
const PROFILES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("profiles");
const SETTINGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("settings");
const ACTIVE_PROFILE_KEY: &str = "active_profile";
const CLAUDE_LAUNCH_ARGS_KEY: &str = "claude_launch_args";

fn now_unix() -> Result<i64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
}

#[derive(Debug, Serialize)]
pub struct TaskTree {
    pub task: Task,
    pub ancestors: Vec<Task>,
    pub descendants: Vec<Task>,
}

pub trait Store {
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
    fn set_session_decisions(&self, id: u64, decisions: Option<String>) -> Result<()>;
    fn set_session_next(&self, id: u64, next: Option<String>) -> Result<()>;
    fn clear_session(&self, id: u64) -> Result<()>;
    fn list_profiles(&self) -> Result<Vec<Profile>>;
    fn active_profile(&self) -> Result<Profile>;
    fn use_profile(&self, name: &str) -> Result<()>;
    fn add_profile(&self, profile: Profile) -> Result<()>;
    fn claude_launch_args(&self) -> Result<Option<String>>;
    fn set_claude_launch_args(&self, args: Option<String>) -> Result<()>;
    fn set_claude_session_id(&self, id: u64, session_id: Option<String>) -> Result<()>;
}

pub struct RedbStore {
    db: Database,
}

impl RedbStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = Database::create(path.as_ref())?;
        let write_txn = db.begin_write()?;
        write_txn.open_table(TABLE)?;
        write_txn.open_table(PROFILES_TABLE)?;
        write_txn.open_table(SETTINGS_TABLE)?;
        write_txn.commit()?;

        let store = Self { db };
        store.backfill_keys()?;
        store.normalize_pillars()?;
        store.bootstrap_default_profile()?;
        Ok(store)
    }

    fn all_tasks(&self) -> Result<Vec<Task>> {
        let read_txn = self.db.begin_read()?;
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

    /// Ensures a fresh store always has a usable active profile, so existing
    /// pillar commands keep working immediately after upgrading with no manual
    /// setup step.
    fn bootstrap_default_profile(&self) -> Result<()> {
        if !self.list_profiles()?.is_empty() {
            return Ok(());
        }
        let profile = Profile::default_personal();
        let name = profile.name.clone();
        self.add_profile(profile)?;
        self.use_profile(&name)?;
        Ok(())
    }

    fn put(&self, task: &Task) -> Result<()> {
        let bytes = serde_json::to_vec(task)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.insert(task.id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn list_where(&self, archived: bool) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> = self.all_tasks()?.into_iter().filter(|t| t.archived == archived).collect();
        tasks.sort_by_key(|t| t.id);
        Ok(tasks)
    }

    fn get(&self, id: u64) -> Result<Option<Task>> {
        let read_txn = self.db.begin_read()?;
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

impl Store for RedbStore {
    fn add(&self, title: String, description: String, tags: Vec<String>) -> Result<Task> {
        let id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64;
        let key = self.next_key()?;
        let task = Task::new(id, key, title, description, tags);
        self.put(&task)?;
        Ok(task)
    }

    fn spawn_child(&self, parent_key: u32, title: String, description: String, tags: Vec<String>) -> Result<Task> {
        let all = self.all_tasks()?;
        if !all.iter().any(|t| t.key == parent_key) {
            bail!("no task with key WAY-{parent_key}");
        }
        let id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64;
        let key = self.next_key()?;
        let mut task = Task::new(id, key, title, description, tags);
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
            task.session_decisions = None;
            task.session_next = None;
            task.session_updated_at = None;
            self.put(&task)?;
        }
        Ok(())
    }

    fn list_profiles(&self) -> Result<Vec<Profile>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PROFILES_TABLE)?;
        let mut profiles: Vec<Profile> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice(v.value()).ok())
            .collect();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    fn active_profile(&self) -> Result<Profile> {
        let read_txn = self.db.begin_read()?;
        let settings = read_txn.open_table(SETTINGS_TABLE)?;
        let name = settings
            .get(ACTIVE_PROFILE_KEY)?
            .map(|v| v.value().to_string())
            .ok_or_else(|| anyhow!("no active profile set"))?;
        let profiles = read_txn.open_table(PROFILES_TABLE)?;
        let bytes = profiles.get(name.as_str())?.ok_or_else(|| anyhow!("active profile '{name}' not found"))?;
        Ok(serde_json::from_slice(bytes.value())?)
    }

    fn use_profile(&self, name: &str) -> Result<()> {
        {
            let read_txn = self.db.begin_read()?;
            let profiles = read_txn.open_table(PROFILES_TABLE)?;
            if profiles.get(name)?.is_none() {
                bail!("no profile named '{name}'");
            }
        }
        let write_txn = self.db.begin_write()?;
        {
            let mut settings = write_txn.open_table(SETTINGS_TABLE)?;
            settings.insert(ACTIVE_PROFILE_KEY, name)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn add_profile(&self, profile: Profile) -> Result<()> {
        {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(PROFILES_TABLE)?;
            if table.get(profile.name.as_str())?.is_some() {
                bail!("profile '{}' already exists", profile.name);
            }
        }
        let bytes = serde_json::to_vec(&profile)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PROFILES_TABLE)?;
            table.insert(profile.name.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn claude_launch_args(&self) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let settings = read_txn.open_table(SETTINGS_TABLE)?;
        Ok(settings.get(CLAUDE_LAUNCH_ARGS_KEY)?.map(|v| v.value().to_string()))
    }

    fn set_claude_launch_args(&self, args: Option<String>) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut settings = write_txn.open_table(SETTINGS_TABLE)?;
            match args {
                Some(a) => {
                    settings.insert(CLAUDE_LAUNCH_ARGS_KEY, a.as_str())?;
                }
                None => {
                    settings.remove(CLAUDE_LAUNCH_ARGS_KEY)?;
                }
            }
        }
        write_txn.commit()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> String {
        std::env::temp_dir().join(format!("way-store-test-{label}-{}.redb", std::process::id())).to_str().unwrap().to_string()
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
    fn session_decisions_and_next_round_trip_independently_and_share_updated_at() {
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

        store.clear_session(task.id).unwrap();
        let cleared = store.find_by_key(task.key).unwrap().unwrap();
        assert_eq!(cleared.session_decisions, None);
        assert_eq!(cleared.session_next, None);
        assert_eq!(cleared.session_updated_at, None);

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
    fn claude_launch_args_round_trip_and_clear() {
        let path = temp_path("launch-args");
        let _ = std::fs::remove_file(&path);

        let store = RedbStore::open(&path).unwrap();
        assert_eq!(store.claude_launch_args().unwrap(), None);

        store.set_claude_launch_args(Some("--dangerously-skip-permissions".to_string())).unwrap();
        assert_eq!(store.claude_launch_args().unwrap(), Some("--dangerously-skip-permissions".to_string()));

        store.set_claude_launch_args(None).unwrap();
        assert_eq!(store.claude_launch_args().unwrap(), None);

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
}
