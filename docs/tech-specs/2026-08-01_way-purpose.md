# Tech Spec — way purpose pillar: Principle

## Overview + Design Principles

Adds `Principle` as a new first-class entity — the leanest one in `way` so
far. Same principles as everywhere else: additive, `Store` is the only
architectural boundary, redb JSON-blob persistence (app data, not config —
no bearing from [issue #20](https://github.com/iamnande/way/issues/20)),
CLI-first, no TUI in this MVP (provisional pending
[issue #21](https://github.com/iamnande/way/issues/21)).

Deliberately **not** a variant of `JournalEntry` — a separate type, per the
PRD's explicit decision — even though the shape (timestamped text) is
similar. No `status`/`standing`/`trajectory` either, unlike `Craft`/
`StabilityArea` — nothing here needs that much structure.

`Store` gains a seventh trait, `PrincipleStore`, joining `TaskStore`/
`ProfileStore`/`RoutineStore`/`PersonStore`/`CraftStore`/`StabilityStore`
under the existing `Store` supertrait.

---

## Data Model

```rust
// principle.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principle {
    pub id: u64,          // nanosecond timestamp, same id scheme as Task::id
    pub created_at: i64,  // unix seconds
    pub text: String,
}
```

### Redb table

```rust
const PRINCIPLES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("principles");
```

`RedbStore::open` gains this table open alongside the existing ones.
Append-only: no update method exists in the `Store` trait for `Principle`
beyond creation and outright removal (see PRD Non-Goals).

---

## Store Trait

```rust
pub trait PrincipleStore {
    fn add_principle(&self, text: String) -> Result<Principle>;
    fn list_principles(&self) -> Result<Vec<Principle>>;
    fn find_principle(&self, id: u64) -> Result<Option<Principle>>;
    fn remove_principle(&self, id: u64) -> Result<()>;
}

pub trait Store: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore + PrincipleStore {}
impl<T: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore + PrincipleStore> Store for T {}
```

No setter methods beyond `add`/`remove` — deliberately, matching the PRD's
append-only, no-edit-in-place decision.

---

## Interfaces

```
way principle add "<text>" [--stdin]
    Creates a principle. --stdin reads text from stdin instead (for
    longer entries), same convenience already established for
    `way journal add`.

way principle list
    Lists principles, reverse-chronological.

way principle show <id>

way principle remove <id>
    Deletes outright - no edit-in-place affordance.
```

All non-interactive (CLI-first). No search verb, no update verb (see PRD
Non-Goals).

---

## TUI

**Provisional**, same caveat as every prior pillar: no TUI affordance in
this MVP. [UX track #21](https://github.com/iamnande/way/issues/21) /
[comprehensive TUI support #26](https://github.com/iamnande/way/issues/26)
cover the eventual cross-cutting follow-up.

---

## Tests

Mirror existing `store.rs` conventions:

- `add_principle` + `find_principle` round-trip `text`/`created_at`.
- `list_principles` returns reverse-chronological order.
- `remove_principle` deletes outright — `find_principle` returns `None`
  afterward.
- No update path exists — nothing to test there beyond its absence.
- `Store` blanket impl: `PrincipleStore` joins the existing six-trait
  combination with no additional glue code, existing `Box<dyn Store>` call
  sites unaffected.

---

## Rollout

Single-commit, purely additive — new entity, new table, new trait
constituent. No coordination with any other repo, no dependency on
[issue #20](https://github.com/iamnande/way/issues/20) (redb-side app data,
not config).
