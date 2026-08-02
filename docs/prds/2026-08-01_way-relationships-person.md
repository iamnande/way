# PRD — way relationships pillar: Person (children + partner)

## Problem

The README's `relationships` pillar asks "where can we invest in the kids? my
partner? my communities?" and names four sub-concerns: children (each with a
birthdate, dreams/aspirations, hobbies, "preferred X," areas requiring extra
attention), a partner ("capture everything, invest"), family travel (trips
with a plan), and communities (never elaborated beyond the framing
question). None of this exists in `way` today — the only trace is
`Pillar::Relationships`, a label, and `spawn_child`/`parent_key` on `Task`,
which is dev-task lineage, unrelated to actual children.

Communities gained a concrete anchor mid-gap-assessment (nick was just
elected to his HOA board, ~120 homes) but resolved to **no new data model**:
that responsibility rides on `way`'s existing `Task` + `pillar` tagging,
already sufficient. Family travel is structurally unrelated to a person
(a dated itinerary, not a profile) and is spun out as
[its own ticket](https://github.com/iamnande/way/issues/24). This PRD's
steel thread is children + partner only.

## Solution

1. A single first-class entity, `Person`, covers both children and a
   partner — a `relationship: RelationshipKind` field (`Child` | `Partner`)
   distinguishes them, rather than two separate types or a polymorphic enum
   with variant-specific fields. Nothing named so far is a *structural*
   difference between the two (just different likely content) — a flat,
   shared model avoids modeling a distinction that can't yet be stated
   concretely. If a genuine structural difference surfaces later, it's an
   additive field, not a redesign.
2. `Person` carries the README's named attributes directly: `birthdate`,
   `dreams_aspirations`, `hobbies`, `preferences` (key/value — "food":
   "sushi", "gatorade flavor": "glacier freeze"), `attention_areas`, plus a
   catch-all `notes` for anything that doesn't fit a structured field
   (this is where any partner-specific or child-specific concern that never
   graduates into its own field lives).
3. No search, no cadence/prompting mechanism (unlike the mind pillar's
   journal) — this is a small, low-volume set of records (a handful of
   people, not a growing daily log), so list/show is sufficient.

## States

- A `Person` has no lifecycle state beyond existing or not — no
  active/archived split (unlike `Routine`/`Task`). People aren't
  archived; if a record needs removing, it's deleted outright (rare,
  deliberate, not a common operation).
- `relationship` is fixed at creation — not something that changes over
  time in practice (a `Child` doesn't become a `Partner`).

## Behavior

- **Before**: no representation of people in `way` at all.
- **After**: a person can be added with a name and relationship, and edited
  incrementally over time as more is learned (add a hobby, update
  attention-areas, jot a note) — the README's own framing ("since their
  birthdate, they've grown...") implies this is a living record added to
  over time, not filled out once and left alone.

## Edge Cases

- Two people share the same name (e.g. two kids, or a name collision with
  no relation): allowed — `Person` is id-keyed, not name-keyed, so this
  never conflicts.
- A field genuinely doesn't apply (e.g. no known birthdate): left `None`/
  empty; nothing here is required beyond `name` and `relationship`.
- Something relationship-specific comes up that doesn't fit a structured
  field: goes in `notes` — no schema change forced by one-off content.

## Observability

N/A — single-user local tool, no logging/metrics infrastructure, matches
existing precedent.

## Scope & Non-Goals

**in scope:**
- `Person` entity: `relationship`, `birthdate`, `dreams_aspirations`,
  `hobbies`, `preferences`, `attention_areas`, `notes`
- CLI CRUD (add, edit, list, show, remove)

**out of scope:**
- Family travel — spun out as [its own ticket](https://github.com/iamnande/way/issues/24)
- Communities/HOA responsibilities — resolved to use `way`'s existing
  `Task` + `pillar` tagging directly; no new modeling needed here
- Search over people — low record count makes `list`/`show` sufficient;
  revisit only if this stops being true
- TUI display/editing — CLI-first satisfies this PRD; any TUI work is
  provisional pending the [UX track](https://github.com/iamnande/way/issues/21)
- A true polymorphic (enum-with-variant-data) model distinguishing `Child`-
  only and `Partner`-only structured fields — no concrete structural
  difference has been named yet; revisit if one is (see Design Decisions)

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| entity shape | separate `Child`/`Partner` types vs. one shared `Person` | shared `Person` | both are "someone I have a relationship with whose details I want to remember" — the README's attribute list applies to both, no real structural difference named |
| polymorphism | Rust enum with variant-specific fields vs. flat struct + `relationship` tag + freeform `notes` | flat struct | no concrete partner-only or child-only structured field has been named — only that "problems differ," which is content, not shape. Modeling a distinction that can't be stated concretely is premature; an additive field later is cheap if one shows up |
| communities | model HOA/community involvement as new `Person`-adjacent entity vs. reuse existing `Task`/`pillar` | reuse existing `Task`/`pillar` | HOA board work (~120 homes) is administrative/operational, not "a person I know" — nick explicitly chose to drive it through `way`'s existing task-tracking capability instead of new modeling |
| family travel | fold into this PRD vs. spin out | spin out | structurally unrelated to a person (dated itinerary vs. a profile) |
| lifecycle | archived/active like `Routine`/`Task` vs. no lifecycle state | no lifecycle state | people aren't archived the way a workout routine is retired; removal (rare) is outright deletion, not a state flag |

## Requirements

- **R1:** `way` supports a `Person` entity (`id`, `name`, `relationship`,
  `birthdate`, `dreams_aspirations`, `hobbies`, `preferences`,
  `attention_areas`, `notes`), independent of `Task`.
- **R2:** `way person add <name> --relationship <child|partner>` creates a
  person; only `name`/`relationship` are required, everything else optional
  at creation.
- **R3:** `way person edit <id>` supports incrementally updating any field
  (adding a hobby, updating attention-areas, appending to notes) without
  requiring the others to be re-specified.
- **R4:** `way person list` / `way person show <id>` list all people /
  show one in full.
- **R5:** `way person remove <id>` deletes a person outright (no archive
  state).
- **R6:** All of the above is reachable non-interactively via the CLI
  (CLI-first convention).
