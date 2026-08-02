# PRD — way stability pillar: StabilityArea

## Problem

The README's `stability` pillar is a one-line sketch: "how are your finances?
what's our housing plan (moving plan)? how can we improve resilience along
the way?" Nothing in `way` implements it; only `Pillar::Stability` exists,
as a label.

Gap-assessment surfaced four distinct facets: raw finance tracking (already
handled well by YNAB — real-time + historical income/expense), a safety-net
figure (derived from finances, tracked separately), current housing and its
repair/stability needs, an intended moving plan and its costs, and
retirement planning ("started, but needs attention"). Nick was initially
unsure whether these were separate pillars in their own right; on reflection
they aren't — they're all "a named area of financial/life security I'm
tracking the current state and trajectory of," structurally close to how
`craft`'s disciplines all answered the same three README questions in one
shape. (Whether `craft` and `stability` should go further and share one
literal entity type is a real, separate question, parked as
[its own ticket](https://github.com/iamnande/way/issues/28) rather than
decided here.)

Raw finance/transaction tracking is explicitly **out of scope** — YNAB
already solves it well, and rebuilding it in `way` would be large, redundant
work outside what this project is actually for. What nick wants from `way`
here is closer to *attention* — a nudge to actually engage with the budget
regularly — which is itself a general `way` capability (recurring/repeating
obstacles), not something specific to stability, and is spun out as
[its own ticket](https://github.com/iamnande/way/issues/29).

## Solution

1. A single first-class entity, `StabilityArea`, represents one area of
   financial/life security — mirroring `Craft`'s shape but with fields
   adapted to what actually applies here: `name`, `status` (`Active` |
   `Dormant` | `Historical` — same three values as `Craft`, same
   reasoning), `standing` (freeform: current state — a safety-net amount, a
   housing situation, a moving plan's status, retirement contribution
   levels), and `trajectory` (freeform: direction + why).
2. `Craft`'s `space` field (niche/focus within a discipline) is deliberately
   **not** carried over — "what is your niche" doesn't apply to a safety
   net or a housing situation the way it applies to a discipline. Adapting
   the shape to what's actually asked, not copying a pattern wholesale.
3. Initial areas nick names: "Safety Net," "Housing," "Moving Plan,"
   "Retirement" — created as data, not hardcoded; more areas can be added
   the same way `Craft` disciplines are.
4. No session/history log on `StabilityArea` in this PRD — the "attend to
   this regularly" need is the recurring-obstacle mechanism
   ([#29](https://github.com/iamnande/way/issues/29)), a separate,
   general capability, not a bespoke log bolted onto this entity.

## States

- `StabilityArea.status`: same three values and meaning as `Craft.status`
  — `Active` (currently attended to), `Dormant` (not currently active,
  contingent on something outside nick's control — e.g. a moving plan
  paused pending a job/housing-market decision), `Historical` (genuinely
  past, no realistic path back).
- Deletable outright (rare, deliberate) — no separate archive state, same
  reasoning as `Craft`.

## Behavior

- **Before**: no representation of stability areas in `way` at all.
- **After**: an area can be defined (name, status, standing, trajectory)
  and edited incrementally as its current state or plan changes —
  `standing`/`trajectory` are expected to be revisited over time (e.g.
  retirement planning "needs attention" implies it'll be edited as that
  attention happens), not fixed at creation.

## Edge Cases

- An area's `status` changes over time (e.g. a moving plan goes from
  `Active` to `Dormant` when a move falls through): no history of past
  status is kept in this MVP, same as `Craft`.
- Two areas with different names but conceptually overlapping content
  (e.g. someone later wants to split "Housing" into "Current Home" and
  "Moving Plan" separately): no merge/split tooling — areas are managed
  by hand, add/remove as needed.

## Observability

N/A — single-user local tool, no logging/metrics infrastructure, matches
existing precedent.

## Scope & Non-Goals

**in scope:**
- `StabilityArea` entity: `name`, `status`, `standing`, `trajectory`
- CLI CRUD

**out of scope:**
- Raw finance/transaction tracking (income/expense ledger) — YNAB already
  solves this; not rebuilt in `way`
- Any YNAB API integration — the safety-net figure and other `standing`
  values are manually updated text, not pulled from an external service
- Recurring/repeating obstacles (the "attend to your budget" nudge
  mechanism) — spun out as [its own ticket](https://github.com/iamnande/way/issues/29),
  general-purpose, not stability-specific
- Whether `StabilityArea` and `Craft` should actually be the same entity
  type — parked as [its own ticket](https://github.com/iamnande/way/issues/28)
- Session/history log per area — no current need identified beyond the
  recurring-obstacle mechanism already spun out
- TUI display/editing — CLI-first satisfies this PRD; any TUI work is
  provisional pending the [UX track](https://github.com/iamnande/way/issues/21)
- Search over areas — low record count, same reasoning as `Craft`/`Person`

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| entity shape | separate entity per facet (finances/housing/moving/retirement) vs. one shared `StabilityArea` | shared entity | all four answer "what's the current state and trajectory of this area of security," same shape as `Craft`'s disciplines — not truly separate pillars, per nick's own read |
| finance tracking | rebuild transaction/budget tracking in `way` vs. leave to YNAB | leave to YNAB | already solved well externally; rebuilding would be large, redundant work outside this project's actual value |
| attention mechanism | bespoke stability-specific reminder vs. general recurring-obstacle capability | general capability, spun out | nick's actual want (consistent attention) generalizes well beyond budgeting; solving it once benefits every pillar, not just this one |
| field shape | copy `Craft`'s exact fields (incl. `space`) vs. adapt | adapt, drop `space` | "niche/focus within a discipline" has no analog for a safety net or housing situation; copying a pattern wholesale isn't the same as recognizing a shared shape |
| craft/stability unification | decide now vs. park as its own question | park | nick was explicitly unsure ("idk how i feel about this"); forcing the call now would mean reopening craft's already-closed spec under time pressure rather than a considered pass |

## Requirements

- **R1:** `way` supports a `StabilityArea` entity (`id`, `name`, `status`,
  `standing`, `trajectory`), independent of `Task`.
- **R2:** `way stability add <name> --status <active|dormant|historical>`
  creates an area; `standing`/`trajectory` are optional at creation and
  editable afterward.
- **R3:** `way stability list [--status <...>]` / `way stability show <name>`
  list areas (optionally filtered by status) / show one in full.
- **R4:** An area's `status`/`standing`/`trajectory` can each be updated
  independently via the CLI.
- **R5:** `way stability remove <name>` deletes an area outright (no
  archive state).
- **R6:** All of the above is reachable non-interactively via the CLI
  (CLI-first convention).
