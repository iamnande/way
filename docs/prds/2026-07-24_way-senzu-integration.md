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

**Update (post-implementation, from real-world usage feedback):** the R1
session-state design as originally shipped — a single opaque blob, handed back
verbatim on resume — reproduces a known failure mode: a cached "here's where we
left off" summary that's gone stale because the work it references was
superseded elsewhere, with nothing to catch it. The fix isn't more caching
discipline, it's not caching the parts that have a live source of truth to
begin with. See the updated Solution/Requirements below.

---

## Solution

1. `way` tasks gain session-state, split by whether a live source of truth
   exists for it: lightweight `decisions`/`next` prose (way's own record — there
   is nothing external to reconcile it against, so it's stored as-is, with an
   `updated_at` timestamp as a passive staleness signal) plus zero or more
   external pointers (item 4) for anything that has a live source of truth
   elsewhere. Written at senzu's compact checkpoints, this becomes the durable,
   task-anchored source of truth for resume state, replacing `CLAUDE.local.md`'s
   role — without reproducing its staleness problem.
2. `way` gains **profiles** (e.g. personal, work), each with its own independently
   configurable list of pillars, replacing the hardcoded 6-pillar enum.
3. `way` gains **task lineage** — any task can spawn child tasks (spike → PRD →
   tickets, arbitrary depth), always as new `way` tasks, navigable as a tree.
4. `way` tasks gain zero or more **external pointers** (GitHub Discussion,
   Linear, PR, or ticket identifiers) instead of a single link. A pointer is an
   identifier only, never a cached copy of the artifact's content — resolving it
   live against the source system is the consuming session's job, not `way`'s
   (see Non-Goals). senzu's existing phase-post behavior keeps posting to the
   relevant external thread when one is present.
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
- **New**: a task carries `decisions`/`next` prose plus an `updated_at`
  timestamp (written at compact time), and separately, zero or more external
  pointers. `way` does not itself understand or validate senzu's phase model
  (grounding/spec/planning/.../learnings) — it stores and returns what senzu
  gives it. Phase semantics stay senzu's responsibility. `way` also never
  resolves a pointer's live content itself — see Non-Goals.

---

## Behavior

- **Before**: task creation/editing touches title, description, tags, pillar,
  status only.
  **After**: a task can additionally carry a parent link (lineage), zero or more
  external pointers, and resume-state prose. All optional; existing tasks and
  workflows are unaffected if unused.
- **Before**: resuming a senzu session means reading `~/.claude/CLAUDE.local.md`,
  which holds at most one session's state, machine-local.
  **After**: resuming means `way show <key>`, which returns that task's own
  decisions/next prose plus its external pointers — independent of machine,
  independent of how many other tasks also have paused sessions, and without
  presenting a stale cached copy as current: the prose is `way`'s own record
  (nothing external to go stale against), and the pointers are resolved live by
  whoever's resuming, not served from a cache.
- **Before**: `way pillar <key> <name>` accepts one of 6 hardcoded values.
  **After**: accepted values depend on the active profile's configured pillar
  list.

---

## API

CLI surface this work needs to add (exact flag/argument shape is a tech-spec
decision, not fixed here):

- `way spawn <parent-key> <title> [...]` — create a child task under a parent.
- `way tree <key>` — show a task's full lineage (ancestors and descendants).
- `way link <key> <external-ref>` — attach a pointer (GitHub Discussion, Linear,
  PR, or ticket identifier) to a task. A task may have more than one; exact
  add/remove/list shape is a tech-spec decision.
- `way session set <key> <state>` / `way session show <key>` — write/read the
  decisions/next prose. Pointers are read via `way link`/`way show`, not
  bundled into this prose.
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
- A task's external pointer references a GitHub Discussion / Linear issue that
  no longer exists upstream: `way` does not validate pointers at write or read
  time — it never resolves them at all. Broken pointers are possible and `way`
  itself will never detect them; detection happens (or doesn't) in whatever
  session dereferences the pointer live.
- A pointer can't be resolved live when a resuming session needs it (offline,
  no `gh`/Linear auth configured): `way` isn't involved in this failure at all
  since it never attempted resolution — this is entirely the consuming
  session's problem to surface.
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
- Session-state on tasks (decisions/next prose + `updated_at`), readable/writable via the CLI
- Zero-or-more external pointers per task (reference only, never resolved by `way`)
- Profiles with independently configurable per-profile pillar sets
- Task lineage (parent/child links, always new `way` tasks, cycle-rejected)
- TUI → `claude` session handoff (subprocess spawn/restore)

**out of scope:**
- The senzu skill rewrite itself (separate repo, separate PRD)
- Multi-machine sync/replication of `way`'s data store
- Automatic pillar migration when a profile's pillar list changes
- `way` performing live validation, resolution, or bidirectional sync against
  GitHub Discussions/Linear itself — pointers are stored inert; dereferencing
  them live is always the consuming session's responsibility, never `way`'s
- Re-parenting a task's lineage after creation
- Metrics-driven refinement (named as a future aspiration; no design exists yet)
- Committing in-flight/draft state (session prose, anything not yet promoted
  to an external system of record) to version control, ever — this isn't a
  deferred feature, it's a hard boundary. Keeping draft state *out* of git is
  the actual purpose of having `way` sit next to senzu rather than folding
  everything into commits; see Design Decisions Summary.
- Multi-user / shared access to a single `way` store **for now.** `way` is
  expected to grow toward multiple users sharing a set of tasks eventually
  (long-term direction, not scheduled work) — nothing here is designed
  specifically *for* that yet, but choices that would clearly need to be
  undone for it (e.g. treating pointers as identifiers rather than baking in
  single-owner assumptions) are being avoided where the cost of doing so is
  low.

---

## Design Decisions Summary

| decision | options considered | chosen | rationale |
|---|---|---|---|
| session-state storage | `CLAUDE.local.md` (status quo) vs. task-anchored field | task-anchored field | multi-task and multi-machine; visible directly in the TUI instead of hidden in a dotfile |
| pillar model | hardcoded enum (status quo) vs. per-profile configurable list | per-profile configurable list | personal and work contexts need genuinely different taxonomies, not just a shared bucket |
| lineage targets | tickets as external issues vs. tickets as new `way` tasks | always new `way` tasks | keeps lineage navigable locally regardless of which external system (or none) is involved |
| active profile selection | TBD | — | open question, deferred to tech spec (see R7) |
| session-state shape (post-implementation revision, from real-world feedback) | opaque single blob (as shipped) vs. structured decisions/next prose + separate external pointers | structured, split by whether a live source of truth exists | a cached copy of something with a live source of truth goes stale with nothing to catch it; prose with no external referent has nothing to go stale *against*, so a blob is fine there but must be separable from pointers |
| pointer resolution ownership | `way` resolves pointers live (fetches from GH/Linear) vs. `way` stores identifiers only | identifiers only, resolution is the consuming session's job | keeps `way` free of API credentials/integration code for external systems (unchanged design principle); also means there's never a cached copy to go stale in the first place — simpler than caching-plus-verification |
| in-flight state persistence | git-backed (reviewable like PRD/tech-spec) vs. database-only, deliberately uncommitted | database-only, never committed | committing in-flight/draft work is antithetical to `way`'s purpose — draft state needs a place to live specifically *outside* version control, not the same reviewability model as finished artifacts |

---

## Requirements

- **R1:** A `way` task can store decisions/next resume-state prose plus an
  `updated_at` timestamp, settable and readable via the CLI. This prose is
  `way`'s own record and is never treated as a cached copy of something with a
  live source of truth elsewhere.
- **R2:** `way` supports named profiles, each with an independently configurable
  list of pillars (name, glyph, color).
- **R3:** A `way` task can reference a parent task; creating a child task links
  it to that parent; cycles are rejected at write time.
- **R4:** A `way` task can store zero or more free-form external pointer
  strings (e.g. `owner/repo#N` or `PROJ-123`). `way` never validates or
  resolves them against the external system — they're stored as inert
  identifiers; live resolution is always the consuming session's
  responsibility.
- **R5:** The `way` TUI can spawn a `claude` subprocess from the selected task,
  handing off the terminal and restoring the TUI cleanly on the child's exit.
- **R6:** All existing CLI commands and TUI behavior continue to work unchanged
  for tasks that don't use profiles, lineage, or session-state — these are
  additive, opt-in features, not breaking changes to what exists today.
- **R7:** A mechanism for selecting the active profile is defined and
  implemented (exact shape deferred to tech spec).
