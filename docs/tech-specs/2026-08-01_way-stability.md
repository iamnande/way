# Tech Spec — way stability pillar: StabilityArea

## Overview + Design Principles

Adds `StabilityArea` as a new first-class entity, following the same
principles as `Craft`: additive, `Store` is the only architectural
boundary, redb JSON-blob persistence (app data, not config — no bearing
from [issue #20](https://github.com/iamnande/way/issues/20)), CLI-first, no
TUI in this MVP (provisional pending
[issue #21](https://github.com/iamnande/way/issues/21)).

`StabilityArea` deliberately adapts rather than copies `Craft`'s shape —
same `status` enum and general "name + current state + trajectory" idea,
but no `space` field (no analog for a safety net or housing situation) and
no session/history log (that need is the separate, general
[recurring-obstacle](https://github.com/iamnande/way/issues/29) capability,
not bolted onto this entity). Whether `Craft` and `StabilityArea` should
eventually collapse into one shared entity type is tracked separately in
[issue #28](https://github.com/iamnande/way/issues/28) — this spec treats
them as two distinct types for now, so that migration (if it happens) is a
deliberate follow-on, not something half-done here.

`Store` gains a sixth trait, `StabilityStore`, joining `TaskStore`/
`ProfileStore`/`RoutineStore`/`PersonStore`/`CraftStore` under the existing
`Store` supertrait.

---

## Data Model

```rust
// stability.rs (new file)
// Reuses the same three-value shape as CraftStatus (see craft.rs) - not
// literally the same Rust type, since issue #28 hasn't resolved whether
// Craft and StabilityArea should ever share code.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StabilityStatus {
    Active,
    Dormant,     // not currently active, contingent on something outside nick's control
    Historical,  // genuinely past, no realistic path back
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityArea {
    pub id: u64,
    pub name: String,          // unique - primary key, same as Craft/Routine
    pub status: StabilityStatus,
    pub standing: String,      // freeform: current state (safety-net amount, housing situation, moving-plan status, retirement contributions)
    pub trajectory: String,    // freeform: direction of change + why
}
```

### Redb table

```rust
const STABILITY_AREAS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("stability_areas");
```

`RedbStore::open` gains this table open alongside the existing ones.
`add_stability_area` rejects a duplicate name, matching `add_craft`/
`add_routine`.

---

## Store Trait

```rust
pub trait StabilityStore {
    fn add_stability_area(&self, name: String, status: StabilityStatus) -> Result<StabilityArea>;
    fn list_stability_areas(&self, status: Option<StabilityStatus>) -> Result<Vec<StabilityArea>>;
    fn find_stability_area(&self, name: &str) -> Result<Option<StabilityArea>>;
    fn remove_stability_area(&self, name: &str) -> Result<()>;
    fn set_stability_status(&self, name: &str, status: StabilityStatus) -> Result<()>;
    fn set_stability_standing(&self, name: &str, standing: String) -> Result<()>;
    fn set_stability_trajectory(&self, name: &str, trajectory: String) -> Result<()>;
}

pub trait Store: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore {}
impl<T: TaskStore + ProfileStore + RoutineStore + PersonStore + CraftStore + StabilityStore> Store for T {}
```

Narrow single-field setters, same convention as `Craft`/`Person`.

---

## Interfaces

```
way stability add <name> --status <active|dormant|historical>
    Creates an area. Errors if the name already exists.

way stability list [--status <active|dormant|historical>]

way stability show <name>

way stability remove <name>
    Deletes outright - no archive state.

way stability set-status <name> <active|dormant|historical>
way stability set-standing <name> "<text>"
way stability set-trajectory <name> "<text>"
```

All non-interactive (CLI-first). No search verb, no session-log verbs (see
PRD Non-Goals).

---

## TUI

**Provisional**, same caveat as every prior pillar: no TUI affordance in
this MVP. [UX track #21](https://github.com/iamnande/way/issues/21) /
[comprehensive TUI support #26](https://github.com/iamnande/way/issues/26)
cover the eventual cross-cutting follow-up.

---

## Tests

Mirror existing `store.rs` conventions (directly parallel to `Craft`'s test
shape, minus session-log tests):

- `add_stability_area` rejects a duplicate name; `find_stability_area`
  round-trips all fields.
- `list_stability_areas(None)` returns all areas; `list_stability_areas(Some(status))`
  filters correctly.
- `set_stability_status`/`set_stability_standing`/`set_stability_trajectory`
  each mutate only their own field.
- `remove_stability_area` deletes outright — `find_stability_area` returns
  `None` afterward.
- `Store` blanket impl: `StabilityStore` joins the existing five-trait
  combination with no additional glue code, existing `Box<dyn Store>` call
  sites unaffected.

---

## Rollout

Single-commit, purely additive — new entity, new table, new trait
constituent. No coordination with any other repo, no dependency on
[issue #20](https://github.com/iamnande/way/issues/20) (redb-side app data,
not config). Independent of the still-open
[craft/stability unification](https://github.com/iamnande/way/issues/28)
and [recurring-obstacle](https://github.com/iamnande/way/issues/29)
questions — this ships as its own distinct entity regardless of how those
resolve later.
