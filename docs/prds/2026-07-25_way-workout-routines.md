# PRD — workout routine support

## Problem

The README's `body` pillar already sketches the vocabulary: we create `exercise
routines`, filled with `exercises`, each carrying `sets`, `reps`, `intensity`,
`friction`, combining into an `exercise duration`, which aggregate up to a
`routine duration`, `intensity`, and `friction`. None of that exists in `way`
today — `Task` is a flat entity (title, description, tags, pillar, lineage,
external refs, session state) with no structured, computable fields for this
data, and no way to represent an exercise's attributes or a routine's
composition of exercises.

Sleep tracking, medical appointments/providers, and logging *when* a routine
was actually followed are mentioned in the same pillar section but are
separate concerns, out of scope here (see Non-Goals).

---

## Solution

1. `way` gains two new entity types — `Routine` and `Exercise` — persisted in
   a new redb table, following the same pattern `Profile` established: new
   `Store` trait methods, same redb JSON-blob persistence, not a new
   architectural layer.
2. A `Routine` has a name and an ordered list of `Exercise`s.
3. An `Exercise` belongs to exactly one `Routine` and carries `sets`, `reps`,
   `intensity`, `friction`, and a `duration`.
4. A `Routine`'s `duration`/`intensity`/`friction` are computed by aggregating
   its exercises — not stored redundantly, so they can never drift out of
   sync with their exercises.
5. This PRD covers routine *definition* only — a template/plan of exercises
   and their combined metrics. Logging that a routine was actually followed
   on a given day (a dated completion record) is explicitly a follow-on, not
   part of this work (see Non-Goals).

---

## States

- A `Routine` can be active or archived, mirroring `Task`'s existing
  archived/unarchived model. There is no "done" state — a routine is a
  reusable template, not a one-shot item.
- An `Exercise` has no independent state; it exists only as part of its
  parent `Routine` and is removed/edited via the routine.

---

## Behavior

- **Before**: `way` has no representation of exercises or routines at all.
- **After**: a user can define a routine, add/edit/remove exercises within
  it, and see the routine's aggregated duration/intensity/friction computed
  from its current exercises.
- Editing or removing an exercise recomputes the parent routine's aggregates
  on next read — there is no separate "recalculate" step.

---

## API

CLI surface this work needs to add (exact flag/argument shape is a tech-spec
decision, not fixed here):

- `way routine add <name>` — create a routine.
- `way routine list` / `way routine show <name>` — list routines / show one
  routine with its exercises and aggregated metrics.
- `way routine archive <name>` / `way routine unarchive <name>`.
- `way routine exercise add <routine> <name> --sets --reps --intensity
  --friction` — add an exercise to a routine.
- `way routine exercise remove <routine> <exercise>` — remove an exercise
  from a routine.
- Per the CLI-first convention, all of the above must work non-interactively
  before or alongside any TUI affordance. TUI display of routines is not
  required by this PRD (see Non-Goals) but nothing here should preclude it
  later.

---

## Lifecycle

- Routines are created explicitly, named by the user.
- Exercises are added to a routine one at a time and can be edited or
  removed; a routine's aggregates always reflect its current exercise set.
- Routines are archived, not deleted, mirroring `Task`'s archive model —
  consistent with `way`'s existing "nothing disappears silently" behavior.

---

## Edge Cases

- A routine with zero exercises: aggregated duration/intensity/friction are
  zero/empty rather than an error — an empty routine is a valid (if
  incomplete) draft state.
- Two exercises in the same routine share a name: allowed: e.g. two separate
  sets of "sprints" at different intensities in the same routine are a
  realistic case, not a duplicate to reject.
- An exercise's `intensity`/`friction` representation (scale, units) is a
  tech-spec decision, not resolved here.

---

## Observability

N/A — single-user local CLI/TUI tool, no logging/metrics/alerting
infrastructure in scope. Matches existing precedent; `way` has none today.

---

## Scope & Non-Goals

**in scope:**
- `Routine` and `Exercise` entity types, new redb table, new `Store` methods
- CLI CRUD for routines and their exercises
- Computed (not stored) routine-level duration/intensity/friction aggregation

**out of scope:**
- Logging/dated completion records of when a routine was actually followed
  (a real, expected follow-on — the README's "life is too short to run in
  the rain" framing implies routines get followed repeatedly over time, but
  that's a second entity/PRD, not bundled into definition support)
- Sleep tracking (separate `body`-pillar concern, mentioned in the same
  README section but unrelated data shape)
- Medical appointments/providers (same — separate `body`-pillar concern)
- TUI display/editing of routines (CLI-first satisfies this PRD; TUI is a
  natural follow-on, not required here)
- Associating a routine with a `pillar`/`Task` (e.g. auto-tagging a task as
  "body" when a routine exists) — routines stand alone as their own entity
  for now, same way `Profile` doesn't reference `Task`

---

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| entity shape | fit into existing `Task` fields (tags/description) vs. new `Routine`/`Exercise` types | new entity types, new table | structured, computable sets/reps/intensity/friction and aggregation don't fit flat string fields |
| `Store` trait boundary | keep bundling all entities into the single existing `Store` trait (as `Profile` was added) vs. split into focused per-entity traits (`TaskStore`/`ProfileStore`/`RoutineStore`) unified by a `Store` supertrait | split now | the single `Store` trait already mixed `Task` and `Profile` methods; adding a third unrelated entity without splitting would make it a growing god-trait. A supertrait (`Store: TaskStore + ProfileStore + RoutineStore`, blanket-impl'd) keeps `Box<dyn Store>` call sites in `cli.rs`/`app.rs` unchanged while giving each entity its own focused trait, matching a repository-per-entity shape |
| scope: definition vs. logging | routine-as-template only vs. template + dated completion log | template only | keeps this PRD to one data shape; a completion log is a distinct entity (dated, append-only) that deserves its own PRD rather than being bundled in |
| aggregate storage | store routine-level duration/intensity/friction redundantly vs. compute on read | compute on read | avoids the aggregate ever drifting out of sync with its exercises after an edit |

---

## Requirements

- **R1:** `way` supports a `Routine` entity with a name and an ordered list
  of `Exercise`s, persisted in a new redb table via new `Store` methods.
- **R2:** Each `Exercise` carries `sets`, `reps`, `intensity`, `friction`,
  and `duration`, and belongs to exactly one `Routine`.
- **R3:** A `Routine`'s duration/intensity/friction are computed by
  aggregating its current exercises, not stored redundantly.
- **R4:** Routines can be created, listed, shown, archived, and unarchived
  via the CLI, matching `Task`'s existing archive semantics.
- **R5:** Exercises can be added to and removed from a routine via the CLI.
- **R6:** All of the above is reachable non-interactively via the CLI before
  or alongside any TUI affordance (CLI-first convention).
- **R7:** The existing `Store` trait is split into focused per-entity traits
  (`TaskStore`, `ProfileStore`, and the new `RoutineStore`), unified by a
  `Store` supertrait so existing `Box<dyn Store>` call sites in `cli.rs` and
  `app.rs` require no changes. No behavior change to existing `Task`/`Profile`
  methods — this is a trait-boundary refactor only.
