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

Sleep tracking and medical appointments/providers are mentioned in the same
pillar section but are separate, structurally unrelated concerns — spun out
as their own first-principles passes:
[sleep tracking](https://github.com/iamnande/way/issues/22),
[medical appointments/providers](https://github.com/iamnande/way/issues/23).
This PRD's steel thread is routines/exercises **plus** logging that a
routine was actually followed on a given day — the README's own framing
("life is too short to choose to run in the rain consistently") implies
routines only matter if they get followed, so a definition with no record
of ever being done isn't a real MVP of this pillar.

---

## Solution

1. `way` gains two new entity types — `Routine` and `Exercise` — persisted in
   a new redb table via a new `RoutineStore` trait, same redb JSON-blob
   persistence as `Task`/`Profile`, not a new architectural layer. The
   existing `Store` trait is split into `TaskStore`/`ProfileStore` at the
   same time, unified with `RoutineStore` under a `Store` supertrait (see
   Design Decisions Summary).
2. A `Routine` has a name and an ordered list of `Exercise`s.
3. An `Exercise` belongs to exactly one `Routine` and carries `sets`, `reps`,
   `intensity`, `friction`, and a `duration`.
4. A `Routine`'s `duration`/`intensity`/`friction` are computed by aggregating
   its exercises — not stored redundantly, so they can never drift out of
   sync with their exercises.
5. A new `RoutineCompletion` entity records that a routine was actually
   followed on a given day: a simple dated log entry (which routine, when,
   an optional free-text note) — no per-exercise actuals-vs-planned capture
   in this MVP (e.g. "planned 3x10, actually did 3x8"). That's a real,
   valuable fast-follow (progressive-overload tracking) but a materially
   bigger feature than what's needed to answer this pillar's core question:
   are you actually following the routine, consistently, over time.

---

## States

- A `Routine` can be active or archived, mirroring `Task`'s existing
  archived/unarchived model. There is no "done" state — a routine is a
  reusable template, not a one-shot item.
- An `Exercise` has no independent state; it exists only as part of its
  parent `Routine` and is removed/edited via the routine.
- A `RoutineCompletion` is an append-only historical record — once logged,
  it isn't edited (a mistaken log is rare enough not to need a fix-up
  affordance in this MVP). It has no relationship to the routine's *current*
  exercise composition; it references the routine by name only.

---

## Behavior

- **Before**: `way` has no representation of exercises or routines at all.
- **After**: a user can define a routine, add/edit/remove exercises within
  it, and see the routine's aggregated duration/intensity/friction computed
  from its current exercises.
- Editing or removing an exercise recomputes the parent routine's aggregates
  on next read — there is no separate "recalculate" step.
- **Before**: no way to record that a routine was actually followed.
  **After**: a user can log a completion for a routine (today, or a specific
  past date, with an optional note) and view a routine's completion history
  reverse-chronological.

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
- `way routine log <name> [--date] [--note]` — record a completion; `--date`
  defaults to today (accepts a past date for backfilling), `--note` is
  optional free text.
- `way routine history <name>` — list completions for a routine,
  reverse-chronological.
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
- Completions are logged one at a time, any time after (or, via `--date`,
  for) the day they occurred; they accumulate indefinitely as a historical
  log with no archive/delete affordance in this MVP.

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
- Logging a completion for an archived routine: allowed — archived means
  "not currently active," not "never happened"; a completion is a historical
  fact independent of the routine's current active/archived state.
- Logging the same routine more than once on the same day: allowed (e.g. an
  AM and PM session) — no uniqueness constraint on (routine, date).
- Logging a completion for a routine name that doesn't exist: rejected with
  an error, same as any other command referencing an unknown routine.

---

## Observability

N/A — single-user local CLI/TUI tool, no logging/metrics/alerting
infrastructure in scope. Matches existing precedent; `way` has none today.

---

## Scope & Non-Goals

**in scope:**
- `Routine` and `Exercise` entity types, new redb table, new `RoutineStore` methods
- Splitting the existing `Store` trait into `TaskStore`/`ProfileStore`/`RoutineStore`, unified by a `Store` supertrait (no behavior change to existing methods)
- CLI CRUD for routines and their exercises
- Computed (not stored) routine-level duration/intensity/friction aggregation
- `RoutineCompletion` entity — a simple dated log (routine name, date, optional note), no per-exercise actuals

**out of scope:**
- Per-exercise actuals-vs-planned on a completion (progressive-overload
  tracking) — real fast-follow, bigger feature than this MVP needs
- Sleep tracking — spun out as [its own ticket](https://github.com/iamnande/way/issues/22)
- Medical appointments/providers — spun out as [its own ticket](https://github.com/iamnande/way/issues/23)
- TUI display/editing of routines and completions — CLI-first satisfies this
  PRD; TUI is a natural follow-on, and any TUI specifics are provisional
  pending the [UX track](https://github.com/iamnande/way/issues/21) anyway
- Associating a routine with a `pillar`/`Task` (e.g. auto-tagging a task as
  "body" when a routine exists) — routines stand alone as their own entity
  for now, same way `Profile` doesn't reference `Task`

---

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| entity shape | fit into existing `Task` fields (tags/description) vs. new `Routine`/`Exercise` types | new entity types, new table | structured, computable sets/reps/intensity/friction and aggregation don't fit flat string fields |
| `Store` trait boundary | keep bundling all entities into the single existing `Store` trait (as `Profile` was added) vs. split into focused per-entity traits (`TaskStore`/`ProfileStore`/`RoutineStore`) unified by a `Store` supertrait | split now | the single `Store` trait already mixed `Task` and `Profile` methods; adding a third unrelated entity without splitting would make it a growing god-trait. A supertrait (`Store: TaskStore + ProfileStore + RoutineStore`, blanket-impl'd) keeps `Box<dyn Store>` call sites in `cli.rs`/`app.rs` unchanged while giving each entity its own focused trait, matching a repository-per-entity shape |
| scope: definition vs. logging | routine-as-template only vs. template + dated completion log | template + completion log, one PRD | the README's own framing ties routines to being followed consistently; a template with no record of ever being done isn't a real MVP of this pillar |
| completion granularity | simple dated log vs. per-exercise actuals-vs-planned | simple dated log | answers this pillar's core question (are you following the routine, consistently) without the materially bigger feature of progression tracking; that's a real, separate fast-follow |
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
- **R8:** `way` supports a `RoutineCompletion` entity (routine name, date,
  optional note), persisted independently of `Routine`/`Exercise`, via new
  `RoutineStore` methods.
- **R9:** `way routine log <name> [--date] [--note]` records a completion;
  `way routine history <name>` lists a routine's completions
  reverse-chronological.
- **R10:** Logging a completion never requires the referenced routine to be
  active — archived routines can still be logged against.
