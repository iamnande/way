# PRD — way mind pillar: journal, check-ins, search

## Problem

`way`'s README names `mind` as the first pillar — "what has your attention?
where have you invested in your mental health?" — realized through writing a
journal near-daily, reading it back, getting prompted for a periodic check-in
against goals/progress/long-term-goal impact, and searching it when something's
forgotten. None of that exists in `way` today. The only trace of "mind" in the
codebase is `pillar`, a free-form tag on `Task` used to categorize dev work
items — unrelated to the actual pillar content. The doc previously assumed to
be the mind pillar's pairing (`way-alignment-checkpoints`) is in fact about
senzu phase-tracking (agent dev-workflow state) and has nothing to do with
journaling.

Nick has ASD/ADHD and is deliberately building a habit here; the design has to
account for that directly rather than bolt it on later. A blank page and "just
remember to write" are exactly the failure mode — cognitive load needs to be
reduced, not assumed away.

This PRD also settles `way`'s own identity question along the way: `way` is
nick's life app itself, not a project-tracker for a separate journaling
product. Every pillar, starting with this one, extends the same codebase
(`Task`/`Store`/TUI today; `JournalEntry` newly, here).

## Solution

1. A new first-class entity, `JournalEntry`, distinct from `Task` — journaling
   has none of `Task`'s work-tracking shape (status, phase, waiting-on,
   session linkage) and forcing it through that model would either pollute
   `Task` or require awkward workarounds.
2. Every entry has a `kind`: `Freeform` (write whenever) or `CheckIn` (written
   in response to a cadence-triggered prompt). Both are stored, read, and
   searched identically — a check-in is a journal entry with a different
   *origin*, not a different destination. This directly answers the README's
   `search` requirement: one place to look, not two.
3. Check-in is a **prompting mechanism**, not new content shape: on a
   configurable cadence, `way` can tell you a check-in is due (surfaced as
   state, not pushed), and walking through it hands you a fixed list of
   guiding questions (default: goals, progress, long-term-goal impact) instead
   of a blank page — directly targeting the executive-function friction nick
   is trying to design around.
4. `way` sends zero notifications itself — same hard boundary established in
   `way-alignment-checkpoints` (R7) for `waiting_on`. Check-in "due" is
   queryable state (CLI + a TUI indicator), not a push. Actually reminding
   nick is left to something external (OS-level notification/cron for now).
   A lightweight text/SMS nudge is real future work, explicitly deferred —
   see Non-Goals.
5. Check-in cadence (`checkin_cadence_days`) and the prompt list
   (`checkin_prompts`) are configurable with sane defaults, read from
   `~/.config/way/config.toml` — the newly-agreed home for all of `way`'s
   configuration (see [Config architecture](https://github.com/iamnande/way/issues/20)),
   not redb. `Profile.pillars`/`phases` migrate into the same file as part of
   landing this.
6. Search is basic substring matching over entry content — appropriate for a
   single-user, low-volume local dataset. No indexing, ranking, or fuzzy
   matching in MVP.

## States

- **New**: `JournalEntry { id, created_at, kind, content }` — `kind` is
  `Freeform` or `CheckIn`. `content` is free-form text in both cases; a
  check-in's content includes the prompts it answered (see Tech Spec for
  exact shape).
- **New**: `checkin_cadence_days: Option<u32>` (config, default `None` — no
  cadence enforced, matching the existing `phases`-empty-means-off
  convention) and `checkin_prompts: Vec<String>` (config, default: the three
  README questions — "What has your attention lately?", "How's progress
  against your goals?", "Any impact to your long-term goals?").
- **New**: "check-in due" is derived state — `now - last_checkin_entry.created_at
  >= checkin_cadence_days`, or due immediately if no check-in entry exists yet
  and a cadence is configured. Not stored; computed on read.

## Behavior

- **Before**: no journaling capability exists in `way`. **After**: `way
  journal add` opens `$EDITOR` (or reads stdin) for a freeform entry;
  `way journal checkin` walks the configured prompts (one at a time,
  interactive) and records the answers as a single `CheckIn` entry.
- **Before**: N/A. **After**: `way journal list` shows entries
  reverse-chronological; `way journal show <id>` shows one in full;
  `way journal search <query>` substring-matches over `content`, printing
  matching entries with their id/date.
- **Before**: N/A. **After**: the TUI gains a journal view (new mode,
  separate from the task list) for browsing/reading entries, and a small
  status indicator when a check-in is due — visible on opening the TUI,
  not pushed.
- **Before**: `Profile` (`pillars`, `phases`) lives in redb. **After**: it
  lives in `~/.config/way/config.toml`; redb holds `JournalEntry` (and
  existing `Task`) records only.

## Edge Cases

- No `checkin_cadence_days` configured: check-in is never "due" — `way
  journal checkin` still works on demand, it's just never prompted.
- `checkin_prompts` edited (added/removed/reordered) between check-ins: no
  migration concern — prompts are read fresh each time `way journal checkin`
  runs; past `CheckIn` entries keep whatever prompts they were answered
  against, embedded in their own `content`.
- Search query matches nothing: empty result set, not an error.
- `config.toml` missing entirely (fresh install): every field falls back to
  its default — no journaling-specific behavior change from a missing file
  vs. an empty one.
- A `CheckIn` triggered while one's already "due" from a previous missed
  cadence: no stacking/backfill logic — due-ness is always computed from the
  *most recent* check-in entry, so answering once clears it regardless of how
  overdue it was.

## Observability

N/A — same as the rest of `way`: single-user, local, no logging/metrics
infrastructure. Nothing here changes that.

## Scope & Non-Goals

**in scope:**
- `JournalEntry` data model, redb persistence, CLI verbs (`add`, `checkin`,
  `list`, `show`, `search`)
- Configurable check-in cadence + prompts, sane defaults
- "Check-in due" as queryable/displayed state (CLI + TUI), no push
- TUI journal view + due-indicator
- Migrating `Profile` (`pillars`, `phases`) from redb to `config.toml`
  (tracked jointly with [Config architecture](https://github.com/iamnande/way/issues/20))

**out of scope:**
- Any notification-sending code inside `way` (hard boundary, matches R7 from
  `way-alignment-checkpoints`) — reminding nick a check-in is due is an
  external concern (OS-level for now)
- A lightweight SMS/text-based journaling entry point — real, wanted, future
  work; not MVP
- Fuzzy/ranked search, tagging, or date-range filtering on journal entries —
  fast-follow only if plain substring search turns out to be insufficient
- Any UI/CLI for editing `config.toml` itself — it's hand-edited directly

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| journal entry model | reuse `Task` vs. new entity | new `JournalEntry` | `Task`'s shape (status/phase/waiting-on/session) has nothing to do with journaling; forcing it through would pollute both models |
| check-in representation | separate entity vs. `kind` field on `JournalEntry` | `kind` field | a check-in is still "something written, timestamped, searchable" — splitting it would mean searching two places, working against the README's own `search` requirement |
| check-in's actual job | content type vs. prompting/nudge mechanism | prompting mechanism | nick's stated intent is reducing blank-page friction (ADHD/ASD-aware), not a distinct kind of content |
| reminder delivery | `way` pushes vs. `way` records state only | records state only | matches existing R7 boundary (`way-alignment-checkpoints`); reminder delivery is a much bigger, separate problem (scheduling, OS integration) than journaling itself |
| cadence/prompts config | hardcoded vs. configurable | configurable, sane defaults | mirrors existing `phases` precedent — nick's own decisions evolve; hardcoding risks the same rigidity `phases` deliberately avoided |
| config storage | redb (existing `Profile` pattern) vs. `~/.config/way/config.toml` | `config.toml` | resolved separately as a cross-cutting decision (issue #20) — config is hand-tuned occasionally, redb is for growing per-record data |
| search implementation | fuzzy/indexed vs. plain substring | plain substring | single-user, low-volume local data; indexing/ranking is complexity the problem doesn't need yet |

## Requirements

- **R1:** `way` can persist `JournalEntry { id, created_at, kind, content }`
  records, independent of `Task`.
- **R2:** `way journal add` creates a `Freeform` entry from `$EDITOR` or stdin.
- **R3:** `way journal checkin` walks the configured `checkin_prompts` in
  order, recording a single `CheckIn` entry containing all prompt/answer
  pairs.
- **R4:** `way journal list` prints entries reverse-chronological (id, date,
  kind, first line/summary).
- **R5:** `way journal show <id>` prints one entry in full.
- **R6:** `way journal search <query>` substring-matches (case-insensitive)
  over `content`, printing matches with id/date.
- **R7:** `way` computes "check-in due" from `checkin_cadence_days` and the
  most recent `CheckIn` entry's timestamp; exposed via CLI and a TUI
  indicator. No `way`-originated notification/push exists anywhere in this
  feature.
- **R8:** `checkin_cadence_days: Option<u32>` and
  `checkin_prompts: Vec<String>` are read from `~/.config/way/config.toml`,
  with defaults applied when the file or fields are absent.
- **R9:** `Profile.pillars`/`phases` move from redb to `config.toml` as part
  of this work, landing jointly with the config-architecture change (#20).
- **R10:** The TUI gains a journal view (list + detail, mirroring the
  existing task list/detail split) and a due-indicator, without disrupting
  the existing task list view/keybindings.
