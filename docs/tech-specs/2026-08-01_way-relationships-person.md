# Tech Spec — way relationships pillar: Person

## Overview + Design Principles

Adds `Person` as a new first-class entity, following the same principles as
`JournalEntry` and `Routine`/`Exercise`: additive, `Store` is the only
architectural boundary, redb JSON-blob persistence (this is app data, not
config — no bearing from [issue #20](https://github.com/iamnande/way/issues/20)),
CLI-first, no TUI in this MVP (provisional pending
[issue #21](https://github.com/iamnande/way/issues/21), same as mind/body).

`Store` gains a fourth focused trait, `PersonStore`, joining `TaskStore`/
`ProfileStore`/`RoutineStore` under the existing `Store` supertrait
established in the body pillar's tech spec — no changes to that supertrait
mechanism, just one more constituent trait.

---

## Data Model

```rust
// person.rs (new file)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RelationshipKind {
    Child,
    Partner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: u64,                          // nanosecond timestamp, same scheme as Task::id
    pub name: String,                     // not unique - id-keyed, not name-keyed
    pub relationship: RelationshipKind,
    pub birthdate: Option<i64>,           // unix seconds
    pub dreams_aspirations: Vec<String>,
    pub hobbies: Vec<String>,
    pub preferences: Vec<(String, String)>,  // e.g. ("food", "sushi"), ("gatorade flavor", "glacier freeze")
    pub attention_areas: Vec<String>,
    pub notes: String,                    // freeform catch-all, empty string default
}
```

`preferences` as `Vec<(String, String)>` rather than a `HashMap` — small,
order-of-entry is meaningful for a human reading it back (most-recently-
added preference last), and JSON-blob serialization doesn't need map key
ordering guarantees. `dreams_aspirations`/`hobbies`/`attention_areas` are all
`Vec<String>` for the same reason: freeform, appended-to-over-time lists,
consistent shape across all three.

### Redb table

```rust
const PEOPLE_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("people");
```

`RedbStore::open` gains this table open alongside the existing ones. No
name-uniqueness constraint (unlike `Routine`/`Profile`) — two people can
share a name.

---

## Store Trait

```rust
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
    fn set_person_preference(&self, id: u64, key: String, value: String) -> Result<()>; // upsert by key
    fn remove_person_preference(&self, id: u64, key: &str) -> Result<()>;
    fn set_person_notes(&self, id: u64, notes: String) -> Result<()>;
}

pub trait Store: TaskStore + ProfileStore + RoutineStore + PersonStore {}
impl<T: TaskStore + ProfileStore + RoutineStore + PersonStore> Store for T {}
```

Each list/preference mutator is a narrow, single-purpose method — matches
the existing convention of targeted setters (`set_pillar`, `set_waiting`,
etc.) rather than one big "replace everything" update, so editing one hobby
doesn't require re-specifying the rest of the record.

---

## Interfaces

```
way person add <name> --relationship <child|partner> [--birthdate YYYY-MM-DD]
way person list
way person show <id>
way person remove <id>

way person set-birthdate <id> <YYYY-MM-DD>

way person add-dream <id> "<text>"       / way person remove-dream <id> "<text>"
way person add-hobby <id> "<text>"       / way person remove-hobby <id> "<text>"
way person add-attention <id> "<text>"   / way person remove-attention <id> "<text>"

way person set-preference <id> <key> <value>   (upsert)
way person remove-preference <id> <key>

way person notes <id>
    Opens $EDITOR pre-filled with the person's current notes; saving on
    close replaces the stored value. Same editor-based pattern already
    established for `way journal add`'s freeform entry.
```

All non-interactive-capable (CLI-first). No search verb (see PRD Non-Goals).

---

## TUI

**Provisional**, same caveat as mind/body: no TUI affordance in this MVP.
[UX track #21](https://github.com/iamnande/way/issues/21) is the dedicated
effort for TUI structure; building a people-list/detail view now risks
being thrown away once it lands.

---

## Tests

Mirror existing `store.rs` conventions:

- `add_person` + `find_person` round-trip all fields; `list_people` returns
  all added people regardless of `relationship`.
- Two people with the same `name` both persist independently (no
  uniqueness constraint).
- `add_person_dream`/`remove_person_dream` (and the hobby/attention-area
  equivalents) mutate only their own list, leaving other fields untouched.
- `set_person_preference` upserts: setting an existing key updates its
  value in place rather than appending a duplicate entry; `remove_person_preference`
  removes by key.
- `set_person_notes` replaces the full notes string.
- `remove_person` deletes the record outright — `find_person` returns
  `None` afterward (no archive/soft-delete).
- `Store` blanket impl: `PersonStore` joins the existing `TaskStore +
  ProfileStore + RoutineStore` combination with no additional glue code,
  existing `Box<dyn Store>` call sites unaffected.

---

## Rollout

Single-commit, purely additive — new entity, new table, new trait
constituent. No coordination with any other repo, no dependency on
[issue #20](https://github.com/iamnande/way/issues/20) (this is redb-side
app data, not config) or on the still-open
[family travel](https://github.com/iamnande/way/issues/24) /
[naming-philosophy](https://github.com/iamnande/way/issues/25) tickets.
