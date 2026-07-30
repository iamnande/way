# Tech Spec — way workout routines + completion log

## Overview + Design Principles

This spec covers `way`'s side of the body pillar's steel thread: routine/
exercise definition plus a completion log. It follows the same principles
already established across `way`: additive/backward-compatible, `Store` is
the only architectural boundary, no new persistence machinery beyond redb,
CLI-first. Config (`~/.config/way/config.toml`, per
[issue #20](https://github.com/iamnande/way/issues/20)) has no bearing here —
routines, exercises, and completions are all app *data*, not configuration,
so they stay in redb.

One structural change, already decided in the PRD: the single `Store` trait
(today mixing `Task` and `Profile` methods) splits into `TaskStore`,
`ProfileStore`, and this work's new `RoutineStore`, unified by a `Store`
supertrait. No behavior change to existing methods — existing `Box<dyn
Store>` call sites in `cli.rs`/`app.rs` are unaffected.

---

## Data Model

```rust
// routine.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub name: String,
    pub sets: u32,
    pub reps: u32,
    pub intensity: f32,   // 0.0-1.0, subjective effort scale — matches how `friction` is already framed in the README (a felt quantity, not a physical unit)
    pub friction: f32,    // 0.0-1.0, same scale
    pub duration_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub name: String,          // unique, primary key (mirrors Profile's existing name-keyed pattern)
    pub exercises: Vec<Exercise>,
    pub archived: bool,
}

impl Routine {
    pub fn duration_secs(&self) -> u32 {
        self.exercises.iter().map(|e| e.duration_secs).sum()
    }
    pub fn intensity(&self) -> f32 {
        avg(self.exercises.iter().map(|e| e.intensity))
    }
    pub fn friction(&self) -> f32 {
        avg(self.exercises.iter().map(|e| e.friction))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineCompletion {
    pub id: u64,               // nanosecond timestamp, same id scheme as Task::id
    pub routine_name: String,  // loose reference by name, not a redb foreign key — same style as Task.pillar
    pub completed_on: i64,     // unix seconds, the date the routine was done (may be backdated via --date)
    pub note: Option<String>,
}
```

`Routine` is keyed by `name` in redb (mirrors `Profile`, which is also
name-keyed) — every existing PRD CLI verb (`way routine show <name>`, etc.)
already assumes name-addressing, so this isn't a new pattern, just making it
explicit. `RoutineCompletion` is keyed by its own `id` (append-only log,
many rows per routine over time — an id-keyed table is the right shape,
matching `Task`).

`intensity`/`friction` land on a `0.0-1.0` scale rather than inventing units
— both are inherently subjective/felt quantities per the README's own
framing, and a normalized scale keeps aggregation (`avg` across exercises)
straightforward without unit-conversion logic.

---

## Store Trait Split

```rust
pub trait TaskStore {
    // every existing Task-related method from today's Store trait, unchanged
    fn add(&self, title: String, description: String, tags: Vec<String>) -> Result<Task>;
    // ... (all other existing methods, verbatim)
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

pub trait Store: TaskStore + ProfileStore + RoutineStore {}
impl<T: TaskStore + ProfileStore + RoutineStore> Store for T {}
```

The blanket impl means `RedbStore` just implements the three focused traits;
`Store` falls out for free, and every existing `Box<dyn Store>` call site
keeps compiling unchanged.

### Redb tables

```rust
const ROUTINES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("routines");
const COMPLETIONS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("routine_completions");
```

`RedbStore::open` gains both table opens alongside the existing ones.
`add_routine` rejects a duplicate name (`Routine` names are unique, same as
`Profile` names today). `log_completion` generates `id` the same way
`add()` does for `Task` (`SystemTime::now()...as_nanos() as u64`), and does
**not** require the routine to be active — `find_routine` is checked for
existence only, not `archived` state, matching PRD Edge Cases/R10.

---

## Interfaces

```
way routine add <name>
    Creates an empty, active routine. Errors if the name already exists.

way routine list [--archived]
    Lists routines (active by default; --archived flips to archived-only,
    mirroring `way list`/`way list --archived`'s existing convention).

way routine show <name>
    Prints a routine's exercises and computed duration/intensity/friction.

way routine archive <name> / way routine unarchive <name>

way routine exercise add <routine> <name> --sets <u32> --reps <u32>
    --intensity <f32> --friction <f32> --duration <seconds>
    Appends an exercise to the routine.

way routine exercise remove <routine> <exercise-name>
    Removes the first exercise matching that name from the routine.

way routine log <name> [--date YYYY-MM-DD] [--note <text>]
    Records a completion. --date defaults to today; accepts a past date for
    backfilling. Errors if <name> doesn't resolve to an existing routine
    (active or archived).

way routine history <name>
    Lists completions for a routine, reverse-chronological (date, note if
    present).
```

All non-interactive, CLI-first — matches R6/the PRD's existing convention.
No TUI verb list here; see TUI section below.

---

## TUI

**Provisional** — same caveat as the mind pillar's tech spec: [UX track:
TUI structure + keybindings](https://github.com/iamnande/way/issues/21) is a
dedicated, separate effort reworking the TUI's structure and keybindings
wholesale. Routines/completions have **no TUI affordance in this MVP at
all** (matches the PRD's existing Non-Goals) — CLI-first fully satisfies
this work, and building a routine/completion view now risks it being thrown
away once #21 lands. TUI display is a natural, explicit follow-on once the
UX track resolves.

---

## Tests

Mirror the existing `store.rs` test conventions:

- `add_routine` rejects a duplicate name; `find_routine` round-trips name/
  exercises/archived.
- `add_exercise`/`remove_exercise` update the parent routine's stored
  exercise list; `Routine::duration_secs`/`intensity`/`friction` recompute
  from the current exercise set (not stored redundantly, per R3).
- A routine with zero exercises: aggregates are zero, not an error.
- Two exercises sharing a name in the same routine: both persist
  independently (no dedup).
- `archive_routine`/`unarchive_routine` mirror `Task`'s existing
  archive/unarchive round-trip test shape.
- `log_completion` on an archived routine succeeds (R10); on a nonexistent
  routine name errors.
- `log_completion` twice for the same routine/date both persist as separate
  rows (no uniqueness constraint on routine+date).
- `completion_history` returns reverse-chronological order.
- `Store` blanket impl: a type implementing `TaskStore + ProfileStore +
  RoutineStore` satisfies `Store` with no additional code, and existing
  `Box<dyn Store>` call sites in `cli.rs`/`app.rs` compile unchanged.

---

## Rollout

Single-commit, additive except for the `Store` trait split, which is an
internal refactor with no external behavior change (existing `Task`/
`Profile` methods unchanged, existing call sites unaffected). No
coordination with any other repo required. No dependency on
[issue #20](https://github.com/iamnande/way/issues/20) (config migration) —
this work is entirely redb-side app data, untouched by that change.
