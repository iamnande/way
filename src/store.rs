use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::task::{Pillar, Task};

const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tasks");

pub trait Store {
    fn add(&self, title: String, description: String, tags: Vec<String>) -> Result<Task>;
    fn list(&self) -> Result<Vec<Task>>;
    fn list_archived(&self) -> Result<Vec<Task>>;
    fn toggle(&self, id: u64) -> Result<()>;
    fn update_fields(&self, id: u64, title: String, description: String, tags: Vec<String>) -> Result<()>;
    fn archive(&self, id: u64) -> Result<()>;
    fn unarchive(&self, id: u64) -> Result<()>;
    fn set_pillar(&self, id: u64, pillar: Option<Pillar>) -> Result<()>;
}

pub struct RedbStore {
    db: Database,
}

impl RedbStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = Database::create(path.as_ref())?;
        let write_txn = db.begin_write()?;
        write_txn.open_table(TABLE)?;
        write_txn.commit()?;
        let store = Self { db };
        store.backfill_keys()?;
        Ok(store)
    }

    /// Assigns real keys to any tasks that predate the `key` field (which default
    /// to 0 via serde), so they don't all collide on the same display key.
    fn backfill_keys(&self) -> Result<()> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let mut unkeyed: Vec<Task> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice::<Task>(v.value()).ok())
            .filter(|t| t.key == 0)
            .collect();
        drop(table);
        drop(read_txn);

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
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let mut tasks: Vec<Task> = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice(v.value()).ok())
            .filter(|t: &Task| t.archived == archived)
            .collect();
        tasks.sort_by_key(|t: &Task| t.id);
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
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let max = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .filter_map(|(_, v)| serde_json::from_slice::<Task>(v.value()).ok())
            .map(|t| t.key)
            .max()
            .unwrap_or(0);
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

    fn list(&self) -> Result<Vec<Task>> {
        self.list_where(false)
    }

    fn list_archived(&self) -> Result<Vec<Task>> {
        self.list_where(true)
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

    fn set_pillar(&self, id: u64, pillar: Option<Pillar>) -> Result<()> {
        if let Some(mut task) = self.get(id)? {
            task.pillar = pillar;
            self.put(&task)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_reopen() {
        let path = std::env::temp_dir().join(format!("way-store-test-{}.redb", std::process::id()));
        let path = path.to_str().unwrap();
        let _ = std::fs::remove_file(path);

        {
            let store = RedbStore::open(path).unwrap();
            store.add("survives a restart".to_string(), String::new(), vec![]).unwrap();
        } // store (and its Database handle) dropped here, exactly like process exit

        let store = RedbStore::open(path).unwrap();
        let tasks = store.list().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "survives a restart");

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_records_without_pillar_still_deserialize() {
        let path = std::env::temp_dir().join(format!("way-store-test-pillar-{}.redb", std::process::id()));
        let path = path.to_str().unwrap();
        let _ = std::fs::remove_file(path);

        let store = RedbStore::open(path).unwrap();
        let task = store.add("assign me".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(task.pillar, None);

        store.set_pillar(task.id, Some(Pillar::Craft)).unwrap();
        let tasks = store.list().unwrap();
        assert_eq!(tasks[0].pillar, Some(Pillar::Craft));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn keys_increment_and_are_never_reused() {
        let path = std::env::temp_dir().join(format!("way-store-test-keys-{}.redb", std::process::id()));
        let path = path.to_str().unwrap();
        let _ = std::fs::remove_file(path);

        let store = RedbStore::open(path).unwrap();
        let t1 = store.add("one".to_string(), String::new(), vec![]).unwrap();
        let t2 = store.add("two".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(t1.key, 1);
        assert_eq!(t2.key, 2);

        store.archive(t1.id).unwrap();
        let t3 = store.add("three".to_string(), String::new(), vec![]).unwrap();
        assert_eq!(t3.key, 3, "key should not be reused after archiving t1");

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn backfills_missing_keys_on_open() {
        let path = std::env::temp_dir().join(format!("way-store-test-backfill-{}.redb", std::process::id()));
        let path = path.to_str().unwrap();
        let _ = std::fs::remove_file(path);

        {
            let store = RedbStore::open(path).unwrap();
            // Simulate legacy records that predate the key field (key defaults to 0).
            let mut a = store.add("first".to_string(), String::new(), vec![]).unwrap();
            a.key = 0;
            store.put(&a).unwrap();
            let mut b = store.add("second".to_string(), String::new(), vec![]).unwrap();
            b.key = 0;
            store.put(&b).unwrap();
        }

        let store = RedbStore::open(path).unwrap();
        let keys: Vec<u32> = store.list().unwrap().iter().map(|t| t.key).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|k| *k != 0), "backfilled tasks must not stay at key 0");
        assert_ne!(keys[0], keys[1], "backfilled keys must be unique, not both WAY-0");

        std::fs::remove_file(path).unwrap();
    }
}
