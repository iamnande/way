# PRD — way purpose pillar: Principle

## Problem

The README's `purpose` pillar is the most meta of the six — one line: "why
do we invest in any & all of the above items, perpetually, along the way?"
It doesn't sit alongside the other five as a sixth instance of the same
kind of thing; it's the "why" underneath them. Nothing in `way` implements
it; only `Pillar::Purpose` exists, as a label.

The README already has a `## principles` section, undocumented anywhere in
`way` itself — a handful of guidance statements ("you are not what you have
mastered. you are what you are willing to risk becoming.") meant to be
consulted when a decision is unclear. Gap-assessment (and confirmation from
nick) settled that `purpose`, for him, **is** the principles given a real
home — "the summation into a meaningful why behind it all" — not a
separate concept requiring its own additional artifact.

## Solution

1. A single first-class entity, `Principle` — deliberately the leanest
   shape a first-class entity can have: `id`, `created_at`, `text`. No
   `status`/`standing`/`trajectory` the way `Craft`/`StabilityArea` have
   them — a principle isn't "a named area with current state," it's a
   single, append-only reflection. Closer in spirit to `CraftSession`/
   `RoutineCompletion`, but even simpler: no parent entity to reference.
2. Nick's own framing — "my own variation of Meditations" — sets the
   model directly: principles accumulate over time, one at a time, each a
   self-contained piece of writing. Not edited after the fact (a
   Meditations-style entry is a record of that moment's reflection, not a
   living document); deletable outright if one was a mistake.
3. Explicitly **not** built by reusing `mind`'s `JournalEntry` with a new
   `kind` — nick wants principles to be first-class, not folded into
   journaling, even though the underlying shape (timestamped text) is
   similar. Two small, clearly-named things, not one entity serving two
   purposes.
4. No search, no TUI — same reasoning and deferrals as every prior pillar
   (low volume; TUI provisional pending
   [issue #21](https://github.com/iamnande/way/issues/21)).

## States

- No lifecycle beyond existing or not — no active/archived/status field.
  A principle simply exists once written; it's either there or deleted.

## Behavior

- **Before**: the README's principles live only as static markdown, with no
  way to add to them or consult them from `way` itself.
- **After**: a principle can be added at any time (append-only), and the
  full set can be listed to read back — directly serving the README's own
  "unsure about a decision along the way? read the principles."

## Edge Cases

- No principles exist yet (fresh install): `way principle list` returns
  empty, not an error.
- A principle is added in error: `way principle remove <id>` deletes it
  outright — no edit-in-place affordance, matching the "written once,
  reflects a moment" framing.

## Observability

N/A — single-user local tool, no logging/metrics infrastructure, matches
existing precedent.

## Scope & Non-Goals

**in scope:**
- `Principle` entity: `id`, `created_at`, `text`
- CLI: add, list, show, remove

**out of scope:**
- A separate "purpose statement" distinct from the principles list — nick
  described purpose as the summation of the principles, not an additional
  artifact
- Reusing `JournalEntry`/mind's infrastructure — deliberately a separate,
  first-class entity instead
- Editing a principle after it's written — append-only by design
- Linking a principle to a specific pillar/tag — principles read as
  general, not pillar-scoped, in how nick described them
- Search — low volume; revisit via
  [comprehensive search](https://github.com/iamnande/way/issues/27) if that
  changes
- TUI — provisional pending the [UX track](https://github.com/iamnande/way/issues/21)

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| purpose vs. principles | separate purpose-statement artifact vs. purpose = the principles themselves | purpose = principles | nick's own framing: "the summation into a meaningful why," not a distinct second thing |
| entity reuse | fold into `JournalEntry` (new `kind`) vs. new first-class `Principle` | new first-class entity | nick explicitly wants principles first-class, not journal-adjacent, despite the similar underlying shape |
| entity shape | mirror `Craft`/`StabilityArea` (status/standing/trajectory) vs. minimal (id/created_at/text) | minimal | a principle isn't a named area with ongoing state — it's a single Meditations-style reflection; matching a heavier pattern here would add scope nick explicitly asked to avoid |
| mutability | editable vs. append-only + delete-only | append-only + delete-only | mirrors the "written once, reflects that moment" nature of the Meditations framing nick named directly |

## Requirements

- **R1:** `way` supports a `Principle` entity (`id`, `created_at`, `text`),
  independent of `Task` and `JournalEntry`.
- **R2:** `way principle add "<text>"` (or `$EDITOR`/stdin for longer
  entries, mirroring `way journal add`) creates a principle.
- **R3:** `way principle list` lists principles reverse-chronological;
  `way principle show <id>` shows one in full.
- **R4:** `way principle remove <id>` deletes a principle outright — no
  edit-in-place affordance.
- **R5:** All of the above is reachable non-interactively via the CLI
  (CLI-first convention).
