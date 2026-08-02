# PRD — way alignment checkpoints (WAY-5)

## Problem

senzu's phase process (grounding → spec → planning → ... → learnings) already
has a home in `way` — tasks carry a free-form `phase` field, visible in the
TUI. What's missing is the point of building that in the first place: senzu
exists so quality doesn't become sacrificial to speed under agent-assisted
development, and today that only works if nick is sitting there driving every
step. There's no way to kick a task off, let an agent work a phase
autonomously, and trust it'll surface itself the moment it hits something that
actually needs nick's judgment — architecture, key decisions, interface
design — versus grinding through the specifics on its own.

Nothing today distinguishes "an agent is working this task" from "this task is
stuck waiting on nick," and nothing records that distinction durably. A push
notification alone is ephemeral — miss it, and there's no record the task is
blocked. `way` needs to be the durable source of truth for that state, the
same way it's already the durable source of truth for phase and resume-state
prose.

Separately: senzu's phase sequence is nick's actual work process — the whole
reason it exists is to keep quality from being sacrificial to speed. Today
`way` stores `phase` as an opaque, unvalidated string (a deliberate design
principle from the original senzu-integration PRD). That means the sequence
is only followed because whichever agent/model/harness happens to be running
it chooses to follow its own instructions — nothing stops a weaker model, or
a different harness entirely, from skipping straight to "planning" with no
grounding pass, and `way` would have no way to know. If `way` is meant to be
a reflection of nick's actual process — freeing him to refine the quality
definitions while the harness drives execution — the process itself has to
be enforced somewhere that doesn't depend on which harness is doing the
driving. This PRD reverses that original non-interpretation principle,
deliberately and narrowly: `way` will validate phase *order*, still without
knowing or caring what any individual phase name *means*.

## Solution

1. A `way` task gains two new fields: `waiting_on` (a short reason, e.g. "prd
   needs architecture input/direction") and `waiting_on_since` (a unix
   timestamp, stamped alongside it). Presence of `waiting_on` means the task
   is blocked on nick; absence means it isn't.
2. `way session set-waiting <key> <reason>` / `way session clear-waiting
   <key>` write and clear it. `way session clear` (the full resume-state
   reset) clears it too, alongside phase/decisions/next — it's resume-state,
   not identity, so it's grouped with that, not with `claude_session_id`
   (which `clear` deliberately leaves alone).
3. The TUI shows a red leading dot on any task-list row where `waiting_on` is
   set (unread-indicator style, visible without opening the task), and a
   `WAITING` line in the detail pane showing the reason and how long it's
   been waiting, mirroring the existing `SESSION` line's layout.
4. `way` does not send notifications itself and gains no notification
   integration code. Whatever's actually running senzu (a live agent session)
   decides, in the moment, whether it's hit something that needs nick — the
   phase sequence itself is fixed and must be followed, but the interrupt
   point within a phase is the agent's own judgment call, not a fixed
   per-phase checkpoint. When it decides it needs nick, it records that via
   `way session set-waiting`, then notifies however it's able to (a push
   notification today; something else tomorrow). `way`'s CLI verb is the
   entire interface — already backend-agnostic, since it's a plain shell-out
   any agent runtime can make regardless of which notification tool it has.
   That also means senzu's own judgment logic (when to interrupt, how to word
   the message) lives entirely in senzu itself (a separate repo) — `way`
   never contains it.
5. A profile gains an ordered `phases` list — e.g. `grounding, spec, planning,
   execution, review, learnings` — the same idea as configurable pillars,
   since the sequence itself is one of nick's decisions, not a fixed built-in.
   An empty list (the default, including every profile that exists today)
   means no enforcement at all — current free-form behavior, unchanged.
6. `way session set-phase` validates against the active profile's `phases`
   list once one is configured: a task can move to the next phase, restate
   its current one, or move backward (revisiting an earlier phase is rigor,
   not a violation) freely. Jumping forward past an unvisited phase, or
   naming a phase not in the configured list, is rejected — unless `--force`
   is passed. `way` has no concept of *who* is invoking its own CLI, so
   `--force` is a plain flag available to any caller; that only nick is
   meant to reach for it is a boundary senzu's own instructions are
   responsible for holding, not something `way` can technically enforce.

## States

- Existing: task status (open/done, archived), `phase` (string, opaque
  content — `way` still never knows what "grounding" *means*).
- **New**: `waiting_on: Option<String>` / `waiting_on_since: Option<i64>` — a
  task is either blocked on nick or it isn't; `way` doesn't interpret the
  reason text any more than it interprets `phase`.
- **New**: a profile's `phases: Vec<String>` — ordered, empty by default. When
  non-empty, it's the vocabulary + sequence `way` validates a task's `phase`
  transitions against for tasks under that active profile.

## Behavior

- **Before**: a task's phase can change with no visible signal for whether
  nick's input is needed right now versus the agent just working through it.
  **After**: `waiting_on` is a first-class flag, visible in the CLI
  (`session show`, and by extension `way show`'s JSON) and in the TUI (badge +
  detail line), independent of which phase the task is in — blocked-ness can
  happen at any point in the sequence, not just at phase transitions.
- **Before**: `way session clear` resets phase/decisions/next/updated_at.
  **After**: it also resets `waiting_on`/`waiting_on_since`.
- **Before**: `way session set-phase` accepts any string, unconditionally.
  **After**: once the active profile has a non-empty `phases` list, a
  transition must be to the next phase, the current one again, or an earlier
  one; skipping ahead or naming an unconfigured phase is rejected unless
  `--force` is passed. Profiles with an empty `phases` list (every profile
  that exists before this ships) see no behavior change.

## Edge Cases

- `waiting_on` is set but the task is later marked `done`: no auto-clear:
  Since a task can be closed out from a blocked state without ever having
  the block explicitly resolved, and `way` doesn't infer intent here, this
  is left as-is — no auto-fixup. matches existing precedent (e.g. orphaned
  pillar values aren't auto-migrated either).
- `waiting_on_since` predates a later, unrelated `session_updated_at` bump
  (e.g. decisions get updated while still blocked): expected and correct —
  the two timestamps track different things on purpose (see Design
  Decisions Summary).
- The active profile is switched (`way profile use`) while a task set under
  the old profile has a phase not present in the new profile's `phases`
  list: same handling as an orphaned pillar — the stored value isn't
  auto-migrated or rejected retroactively, it just can't be advanced from
  without `--force` until it's fixed or the profile is switched back.
- Someone other than nick passes `--force`: `way` has no way to detect or
  prevent this — the flag is a plain CLI argument, available to any caller.
  Holding the "only nick" boundary is explicitly senzu's responsibility
  (its own instructions must never pass `--force` themselves), not a
  guarantee `way` makes.

## Observability

N/A, same as the rest of `way` — single-user local tool, no
logging/metrics infrastructure. `waiting_on_since` is captured specifically
so a *future* metrics effort (how much of nick's attention "quality work"
actually costs) has raw data to work from — deriving or surfacing anything
from it is explicitly not part of this work.

## Scope & Non-Goals

**in scope:**
- `waiting_on` / `waiting_on_since` fields, CLI verbs, TUI badge + detail line
- Profile-configurable ordered `phases` list, and `set-phase` order
  validation (with `--force` override) against it

**out of scope:**
- Any notification-sending code inside `way` itself — that's the consuming
  agent session's responsibility, always (mirrors the existing non-goal
  around `way` never resolving external pointers itself)
- senzu's own judgment logic for *when* to interrupt or *how* to word the
  message — lives entirely in senzu (dotfiles repo), no design exists here
- Deriving or displaying any metrics from `waiting_on_since` — captured now,
  designed later
- Slide-deck-style navigation through multiple queued prompts/questions on a
  single task — spun out as its own task, WAY-6, not designed here
- The senzu skill rewrite itself — separate repo, separate PRD (same
  boundary the senzu-integration PRD already drew)
- Editing an existing profile's `phases` list after creation — same gap
  already exists for `pillars`; not being closed for either here
- Validating or gating on anything *other* than phase order — e.g.
  requiring decisions/next prose to be present before advancing. Order is
  the only thing enforced; content requirements are a separate, undesigned
  question
- Enforcing who is allowed to pass `--force` — technically unenforceable by
  `way` (see Edge Cases); left to senzu's own instructions

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| interrupt trigger | fixed per-phase checkpoint vs. dynamic in-phase agent judgment | dynamic judgment, fixed phase order | nick wants to weigh in on real decisions, not every phase transition — most of a phase is "specifics," not worth interrupting for |
| notification ownership | `way` sends the notification itself vs. `way` only records state, agent notifies | agent notifies, `way` only records | keeps `way` free of notification-integration code, same principle as external-pointer resolution staying out of `way` |
| state shape | fold "blocked" into the existing `phase` string vs. new dedicated fields | new fields (`waiting_on` / `waiting_on_since`) | `phase` is deliberately opaque to `way`; blocked-ness has to be something `way` actually understands to drive a TUI badge, and it's orthogonal to phase anyway |
| timestamp | reuse `session_updated_at` vs. a dedicated `waiting_on_since` | dedicated | `session_updated_at` is bumped by unrelated writes (e.g. editing decisions), which would corrupt "how long has this been blocked" for future metrics |
| TUI treatment | defer to later vs. design now | design now (red leading dot + detail line) | "it's in the JSON" isn't enough — needs to be visible at a glance while scanning the task list |
| phase enforcement | leave as prompt-level discipline vs. way validates order in code | way validates order | nick wants the sequence to hold regardless of which model/harness runs it, not just when the agent happens to follow instructions well — that requires enforcement independent of the harness |
| phase vocabulary location | hardcode senzu's sequence in way vs. configurable per profile | configurable per profile | the sequence is one of nick's own decisions and may evolve — same reasoning that already moved pillars off a hardcoded enum |
| violation handling | silently allow vs. hard reject unconditionally vs. hard reject with an override | hard reject, with `--force` override | matches "must be followed" for an agent, while leaving nick a conscious way to skip a phase himself |

## Requirements

- **R1:** A `way` task can store `waiting_on` (reason) and `waiting_on_since`
  (unix timestamp), settable/clearable via the CLI, additive and
  backward-compatible with existing tasks.
- **R2:** `way session set-waiting <key> <reason>` sets both fields together;
  `way session clear-waiting <key>` clears both together.
- **R3:** `way session clear` (full resume-state reset) also clears
  `waiting_on`/`waiting_on_since`.
- **R4:** `way session show` prints `waiting_on`/`waiting_on_since` when
  present; the "no session state" guard accounts for it.
- **R5:** The TUI task list shows a distinct visual indicator (red leading
  dot) on any row with `waiting_on` set.
- **R6:** The TUI detail pane shows a `WAITING` line (reason + relative time)
  when `waiting_on` is set, styled consistently with the existing `SESSION`
  line.
- **R7:** `way` contains no code that sends notifications to any external
  system — this is a hard boundary, not a deferred feature.
- **R8:** A profile can store an ordered `phases: Vec<String>`
  (default empty), settable at profile-creation time, backward-compatible
  with every profile that exists today.
- **R9:** When the active profile's `phases` is non-empty, `way session
  set-phase` rejects a transition that skips ahead of an unvisited phase, or
  names a phase absent from the list. Restating the current phase or moving
  to an earlier one is always allowed. When `phases` is empty, behavior is
  unchanged from today (fully free-form).
- **R10:** `way session set-phase` accepts a `--force` flag that bypasses the
  R9 check unconditionally.
- **R11:** `way` makes no attempt to determine or restrict *who* invokes
  `--force` — that boundary is explicitly out of `way`'s reach (see Edge
  Cases).
