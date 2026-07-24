# PRD — way as senzu's state layer

## Problem

senzu's resume-state mechanism (`/senzu compact` → a block in `~/.claude/CLAUDE.local.md`)
has two structural limits: it's single-machine (the file lives on whichever box the
session ran on) and single-slot (one global block — a second compact clobbers the
first, so only one task's session can be paused/resumed at a time).

senzu's issue routing (GitHub Discussions, Linear) assumes engineering work with a
real external issue tracker. senzu is also meant to apply to personal/life-scoped
work — the thing `way`'s pillar model already exists for — where there's no natural
external system to route to.

There's currently no single place showing what AI-assisted work is in flight,
paused, or done across both personal and work contexts. That state is scattered
across `CLAUDE.local.md`, GitHub Discussion threads, and Linear tickets, in
different formats, on different machines.

`way` today is a flat single-list task tracker with a TUI and a scriptable CLI
(add/list/show/done/pillar). It has no concept of session state, project lineage,
multiple pillar taxonomies, or external issue linkage.

---

## Solution

1. `way` tasks gain a structured session-state field (phase, decisions, completed,
   next), written at senzu's compact checkpoints. This becomes the durable,
   task-anchored source of truth for resume state, replacing `CLAUDE.local.md`'s
   role.
2. `way` gains **profiles** (e.g. personal, work), each with its own independently
   configurable list of pillars, replacing the hardcoded 6-pillar enum.
3. `way` gains **task lineage** — any task can spawn child tasks (spike → PRD →
   tickets, arbitrary depth), always as new `way` tasks, navigable as a tree.
4. `way` tasks gain an optional **external issue link** (a GitHub Discussion or
   Linear identifier), so senzu's existing phase-post behavior keeps posting to
   that external thread when one is present.
5. The `way` TUI gains a keybinding to spawn a `claude` session directly from the
   selected task, handing off the terminal and seeding context from the task.

Item 6 in the original discussion — rewriting the senzu skill itself to consume
`way`'s CLI — is **not** part of this PRD's scope. senzu lives in a separate repo
(dotfiles) and needs its own PRD once `way`'s side of this contract exists. This
document defines what `way` must expose to make that follow-on work possible.

---

## Configuration

- **Profile**: a name, a configured list of pillars (each pillar: name, glyph,
  color), and optionally a default external-issue-system association (github
  discussions / linear / none).
- **Active profile selection mechanism**: not decided in this PRD — needs a tech
  spec decision (candidates: a `way profile use <name>` command that persists a
  "current profile" pointer in the store, vs. a per-invocation `--profile` flag,
  vs. an env var). Flagged as an open requirement below (R7).
- Pillars are no longer a hardcoded enum; they become per-profile configured data.

---

## States

- Task status: open / done (existing), archived (existing) — unchanged by this
  work.
- **New**: a task either has no session-state, or has a stashed resume-state blob
  (written at compact time). `way` does not itself understand or validate senzu's
  phase model (grounding/spec/planning/.../learnings) — it stores and returns the
  blob senzu gives it. Phase semantics stay senzu's responsibility.

---

## Behavior

- **Before**: task creation/editing touches title, description, tags, pillar,
  status only.
  **After**: a task can additionally carry a parent link (lineage), an external
  issue link, and a resume-state blob. All three are optional; existing tasks and
  workflows are unaffected if unused.
- **Before**: resuming a senzu session means reading `~/.claude/CLAUDE.local.md`,
  which holds at most one session's state, machine-local.
  **After**: resuming means `way show <key>`, which returns that task's own
  resume-state — independent of machine, independent of how many other tasks also
  have paused sessions.
- **Before**: `way pillar <key> <name>` accepts one of 6 hardcoded values.
  **After**: accepted values depend on the active profile's configured pillar
  list.

---

## API

CLI surface this work needs to add (exact flag/argument shape is a tech-spec
decision, not fixed here):

- `way spawn <parent-key> <title> [...]` — create a child task under a parent.
- `way tree <key>` — show a task's full lineage (ancestors and descendants).
- `way link <key> <external-ref>` — attach a GitHub Discussion / Linear
  identifier to a task.
- `way session set <key> <state>` / `way session show <key>` — write/read the
  resume-state blob.
- `way profile list` / `way profile use <name>` / `way profile add <name>
  --pillars ...` — profile management.
- TUI: new keybinding to spawn a `claude` session from the selected task.

---

## Lifecycle

- **Session state**: created at a task's first `/senzu compact`, updated on
  subsequent compacts, cleared on `/senzu clear` — the same lifecycle
  `CLAUDE.local.md` has today, just relocated to the task.
- **Lineage**: child tasks are created only by explicit action (a spike matures
  into a PRD task; a PRD breaks into ticket tasks) — never automatic or implicit.
  The parent link is set at child-creation time and is immutable afterward;
  re-parenting is out of scope (see Non-Goals).
- **Profiles**: created once, pillar lists edited over time. Switching the active
  profile does not delete or reassign tasks tagged under another profile's
  pillars — see Edge Cases for what happens when a pillar is later removed.

---

## Edge Cases

- A task is tagged with a pillar that's later removed from its profile's
  configured list: the task keeps the stale pillar value. Not auto-migrated;
  displays as orphaned/inert data. No auto-fixup in v1.
- A task's external link points to a GitHub Discussion / Linear issue that no
  longer exists upstream: `way` does not validate the link at write or read
  time. Broken links are possible and undetected.
- Circular lineage (a task's ancestor chain loops back to itself): must be
  rejected at write time by `way spawn` / `way link`-equivalent parent-setting
  logic.
- Two machines each hold a stale copy of the same `way.redb` and diverge: no
  sync mechanism exists or is being added. Explicitly out of scope — `way`
  remains single-machine, single-writer (redb's own file lock already enforces
  single-process-at-a-time on one machine).
- A profile is deleted while tasks still reference its pillars: tasks are not
  deleted or reassigned; their pillar values become orphaned/display-only, same
  handling as the pillar-removed case above.

---

## Observability

N/A — single-user local CLI/TUI tool, no logging/metrics/alerting
infrastructure in scope. Matches existing precedent; `way` has none today.

---

## Scope & Non-Goals

**in scope:**
- Session-state field on tasks, readable/writable via the CLI
- Profiles with independently configurable per-profile pillar sets
- Task lineage (parent/child links, always new `way` tasks, cycle-rejected)
- External issue link field (reference only, no live validation or sync)
- TUI → `claude` session handoff (subprocess spawn/restore)

**out of scope:**
- The senzu skill rewrite itself (separate repo, separate PRD)
- Multi-machine sync/replication of `way`'s data store
- Automatic pillar migration when a profile's pillar list changes
- Live validation or bidirectional sync with GitHub Discussions/Linear
- Re-parenting a task's lineage after creation
- Metrics-driven refinement (named as a future aspiration; no design exists yet)
- Multi-user / shared access to a single `way` store

---

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| session-state storage | `CLAUDE.local.md` (status quo) vs. task-anchored field | task-anchored field | multi-task and multi-machine; visible directly in the TUI instead of hidden in a dotfile |
| pillar model | hardcoded enum (status quo) vs. per-profile configurable list | per-profile configurable list | personal and work contexts need genuinely different taxonomies, not just a shared bucket |
| lineage targets | tickets as external issues vs. tickets as new `way` tasks | always new `way` tasks | keeps lineage navigable locally regardless of which external system (or none) is involved |
| active profile selection | TBD | — | open question, deferred to tech spec (see R7) |

---

## Requirements

- **R1:** A `way` task can store a structured session-state blob (phase,
  decisions, completed, next), settable and readable via the CLI.
- **R2:** `way` supports named profiles, each with an independently configurable
  list of pillars (name, glyph, color).
- **R3:** A `way` task can reference a parent task; creating a child task links
  it to that parent; cycles are rejected at write time.
- **R4:** A `way` task can optionally store a free-form external issue reference
  string (e.g. `owner/repo#N` or `PROJ-123`), with no validation against the
  external system.
- **R5:** The `way` TUI can spawn a `claude` subprocess from the selected task,
  handing off the terminal and restoring the TUI cleanly on the child's exit.
- **R6:** All existing CLI commands and TUI behavior continue to work unchanged
  for tasks that don't use profiles, lineage, or session-state — these are
  additive, opt-in features, not breaking changes to what exists today.
- **R7:** A mechanism for selecting the active profile is defined and
  implemented (exact shape deferred to tech spec).
