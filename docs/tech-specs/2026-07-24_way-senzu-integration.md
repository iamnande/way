# Tech Spec — way as senzu's state layer

## Overview + Design Principles

This spec defines the data model and CLI surface for extending `way` from a flat
task tracker into a state layer for AI-assisted work sessions: profiles with
configurable pillars, task lineage, external issue references, and session-state
storage, plus a TUI-to-`claude` handoff.

Design principles:

1. **Additive and backward-compatible.** Every new field defaults such that
   existing tasks and existing CLI/TUI behavior are unaffected — same approach
   already used for `key` and `pillar` (`#[serde(default)]`).
2. **`way` stores data, it does not interpret senzu's phase semantics.**
   `decisions`/`next` are generic, universal slots — not senzu-specific phase
   vocabulary — so `way` still doesn't know or care what "grounding" or
   "planning" means. External pointers are opaque identifiers `way` never
   dereferences; resolving them live is always the consuming session's job.
3. **Reuse the existing `Store` trait boundary.** Every capability here is a new
   method on `Store`/`RedbStore` — no new architectural layer.
4. **No new persistence machinery.** Profiles are stored the same way tasks
   are (JSON blobs in redb), consistent with how the app already persists
   data. No ORM, no config-file format.
5. **CLI-first.** Every new capability must be reachable non-interactively
   before, or alongside, any TUI affordance for it.

---

## Data Model

```rust
// task.rs
pub struct Task {
    // existing fields unchanged: id, key, title, description, tags, done, archived
    pub pillar: Option<String>,             // was Option<Pillar>; now a profile-defined name, lowercase
    pub parent_key: Option<u32>,            // references another task's `key`, immutable after creation
    pub external_refs: Vec<String>,         // zero or more, e.g. "owner/repo#N" or "PROJ-123"; way never resolves these
    pub session_decisions: Option<String>,  // way's own record — nothing external to go stale against
    pub session_next: Option<String>,       // short-lived, expected to go stale fast — that's fine
    pub session_updated_at: Option<i64>,    // unix seconds; set whenever decisions or next is written; passive staleness signal only
}

pub struct PillarDef {
    pub name: String,   // e.g. "mind", "delivery" — lowercase, matched case-insensitively on input
    pub glyph: String,  // single-char display glyph
    pub color: (u8, u8, u8),
}

pub struct Profile {
    pub name: String,             // e.g. "personal", "work"
    pub pillars: Vec<PillarDef>,
    pub default_issue_system: Option<IssueSystem>, // None | GithubDiscussions | Linear — display/default only, not enforced
}
```

New redb tables: `profiles` (keyed by profile name → JSON `Profile`), and a
single settings record holding the active profile name (a tiny fixed-key table,
same pattern as the tasks table).

`Task.pillar` changes type from `Option<Pillar>` (enum) to `Option<String>`.
See § Core for why this needs only a normalization pass, not a hard migration.

**Revision note** (session-state / external refs): this replaces the
originally-shipped `external_ref: Option<String>` and `session_state:
Option<String>` (single opaque blob). Neither field has been used against real
(non-scratch) data yet — only isolated test stores — so this is a clean
rename/restructure, not a migration. No backward-compat shim is needed for
these two fields specifically (contrast with `key`/`pillar`, which *did* have
real data riding on them and got proper backfill/normalization passes).

---

## Interfaces

```
way spawn <parent-key> <title> [--description ...] [--tags ...] [--pillar ...]
    creates a task with parent_key = <parent-key>. Errors if parent-key doesn't exist.

way tree <key>
    JSON: { task, ancestors: [...], descendants: [...] }
    ancestors: full parent_key chain to root.
    descendants: full recursive tree below this task.

way link <key> <external-ref>            # add a pointer (dedup: adding the same ref twice is a no-op)
way link <key> <external-ref> --remove   # remove that specific pointer
way link <key> clear                     # remove all pointers

way session set-decisions <key>   # reads decisions prose from stdin
way session set-next <key>        # reads next-step prose from stdin
way session show <key>            # prints decisions/next/updated-at as labeled plain text; exit 1 if both unset
way session clear <key>           # clears decisions, next, and updated-at together

way profile list
way profile use <name>
way profile add <name> --pillars "name:glyph:colorhex,name:glyph:colorhex,..."

way pillar <key> <name>        # unchanged shape; now validates against the active profile
```

TUI: `c` keybinding on the selected task — suspends raw mode/alt screen, spawns
`claude` seeded with a prompt assembled by `way` itself before the process
starts (not a bare key the session has to go look up afterward — see the R1
revision). If `session_decisions`/`session_next` are set, the prompt frames it
as a re-attach and includes them verbatim; otherwise it's a fresh-start prompt
built from the task's own fields. External pointers are listed by identifier
only, never dereferenced by `way` — whoever's resuming resolves them live.
Waits for the child to exit, restores the TUI with a full redraw.

---

## Validation

- Pillar name on `way pillar` / `way add --pillar` / task creation: must match
  a name in the *active* profile's pillar list at assignment time (case-
  insensitive). Unknown name → reject. Already-assigned values are **not**
  re-validated on read if the profile changes later (see PRD edge cases —
  orphaned pillars stay as inert data, display-only).
- `way spawn <parent-key>`: parent must exist (`find_by_key`), else error.
- `way link`: non-empty string; no format/existence validation against GitHub/
  Linear. Adding a ref already present is a no-op, not an error (idempotent).
  `--remove` on a ref that isn't present is also a no-op.
- `way profile use <name>`: must reference an existing profile.
- `way profile add <name>`: name must not already exist; each pillar spec
  entry must have exactly `name:glyph:colorhex` with a single-character glyph
  and a valid hex color.

---

## Core

**Cycle-freedom by construction.** `parent_key` is set only at task-creation
time (`way spawn`) and is immutable afterward — the PRD already places
re-parenting out of scope. Since a child's parent must exist *before* the
child does, and links never change post-creation, a cycle is structurally
impossible. No runtime graph-cycle check is needed; R3's "cycles are rejected
at write time" is satisfied by this constraint rather than by an explicit
detection pass.

**Pillar migration is a normalization, not a schema migration.** A fieldless
Rust enum (`Pillar`) serializes via serde as a bare string matching the
variant name — e.g. `"pillar":"Mind"`. That's already structurally identical
to `Option<String>`; changing the Rust type doesn't break deserialization of
existing records. The only real migration need is case: old values are
`"Mind"`, `"Body"`, etc. (capitalized); new values are lowercase profile
pillar names. On store open, a `normalize_pillars` pass — same shape as the
existing `backfill_keys` pass — lowercases any legacy-capitalized pillar
string it finds. Idempotent, runs every open, no-op once nothing's left to
fix.

**Default profile bootstrap.** On a store with zero profiles (fresh install,
or first open after this upgrade), `way` auto-creates a `personal` profile
with the six legacy pillar names (mind/body/relationships/craft/stability/
purpose) and their existing colors from `theme.rs`, and sets it active. This
is what makes R6 hold: `way pillar 7 mind` keeps working immediately after
upgrading, with zero manual setup.

**`way tree`** operates on the already-in-memory task list (same as `list`/
`list_archived` today) — filter by `parent_key` for children, recurse for
descendants, walk `parent_key` pointers for ancestors. No index needed at
this scale.

**Prompt assembly happens in `way`, not after `claude` starts.** `main.rs`
builds the opening prompt from the full `Task` before spawning the subprocess:
if `session_decisions`/`session_next` are set, it's framed as a re-attach and
handed over verbatim (both are `way`'s own record — nothing external to have
gone stale); otherwise it's a fresh-start prompt from title/description/tags/
pillar. `external_refs` are listed by identifier only. This is already how
`spawn_claude_session`/`build_prompt` work as of the R1 revision fix — this
pass updates them to read the split fields instead of the single blob.

**`updated_at` is a shared timestamp, not per-field.** Writing either
`session_decisions` or `session_next` updates the same `session_updated_at` —
one passive signal for "how fresh is this," not two independently-tracked
ones. Simpler, and the PRD only asks for a staleness signal, not per-field
provenance.

---

## Query Layer

No new indexes. Reuses the existing full-table-scan-into-memory pattern
(`list_where`) that every other `Store` method already relies on — appropriate
for a single-user store where the whole dataset fits in memory.

---

## API Layer

New CLI entry points listed in § Interfaces, each following the exact shape
of the existing commands in `cli.rs`: parse args, call a `Store` trait method,
print JSON (or the raw blob for `session show`) to stdout, print an error to
stderr and exit 1 on failure.

---

## Files Changed

| file | change |
|---|---|
| `src/task.rs` | (done) `Task.pillar` → `Option<String>`; `PillarDef`/`Profile`/`IssueSystem` added. **This revision:** `external_ref: Option<String>` → `external_refs: Vec<String>`; `session_state: Option<String>` → `session_decisions`/`session_next`/`session_updated_at` |
| `src/store.rs` | (done) `profiles` table, active-profile pointer, `normalize_pillars`. **This revision:** `link_external` → `add_external_ref`/`remove_external_ref`/`clear_external_refs`; `set_session_state` → `set_session_decisions`/`set_session_next`/`clear_session` (all three touch `session_updated_at`) |
| `src/cli.rs` | (done) `spawn`, `tree`, `profile list/use/add`, active-profile-validated `pillar`. **This revision:** `link` gains `--remove` and dedup-on-add; `session set/show/clear` → `session set-decisions/set-next/show/clear` |
| `src/app.rs` | (done) `c` keybinding, `pending_spawn`. **This revision:** none — the suspend/spawn/restore mechanics don't change, only what `main.rs` reads off the `Task` to build the prompt |
| `src/ui.rs` | (done) profile-driven pillar colors, dynamic picker. **This revision:** detail pane gains visibility for `parent_key` (shown as `WAY-{n}`), `external_refs`, and a session summary (`updated_at` relative time + `next` preview) — currently these fields exist in the data model and CLI but are invisible in the TUI, which undercuts the stated point of moving state here ("visible directly in the TUI instead of hidden in a dotfile," per the PRD's own Design Decisions table) |
| `src/theme.rs` | unchanged |
| `src/main.rs` | (done) `spawn_claude_session`, context-first `build_prompt`. **This revision:** `build_prompt` reads `session_decisions`/`session_next` instead of the single blob, and lists `external_refs` |

---

## Integration Tests

- `pillar_migration_normalizes_legacy_capitalized_values` — write a legacy
  `"pillar":"Mind"` record directly (same technique as
  `backfills_missing_keys_on_open`), reopen, confirm it reads back as `"mind"`.
- `fresh_store_bootstraps_default_personal_profile` — open a brand-new store,
  confirm exactly one profile named `personal` exists with the six legacy
  pillar names, and it's active.
- `spawn_creates_linked_child_and_rejects_unknown_parent` — spawn a child
  under a valid parent, confirm `parent_key`; attempt to spawn under a
  nonexistent key, confirm it errors.
- `tree_returns_full_ancestor_chain_and_descendants` — build a 3-level
  lineage (spike → PRD → ticket), confirm `tree` on the middle task returns
  the spike as an ancestor and the ticket as a descendant.
- `session_decisions_and_next_round_trip_independently_and_share_updated_at` —
  set decisions via stdin, confirm `updated_at` is set; set next separately,
  confirm decisions is unchanged and `updated_at` advances; clear, confirm all
  three report none.
- `pillar_assignment_rejects_name_not_in_active_profile` — assigning an
  unknown pillar name errors; an already-assigned pillar that's later removed
  from the profile is *not* rejected on read (orphaned, display-only).
- `external_refs_add_is_idempotent_and_remove_targets_one_entry` — adding the
  same ref twice results in one entry, not two; `--remove` removes only the
  targeted ref, leaving others intact; `clear` empties the list.
- **Manual verification** (not automatable under the standing rule against
  driving the live binary): the `c` keybinding actually suspends and restores
  the terminal cleanly, and the re-attach prompt (when `session_decisions`/
  `session_next` are set) reads as a continuation rather than a cold start —
  nick runs this himself.

---

## Resolved Questions

| question | resolution |
|---|---|
| How is the active profile selected? (R7) | A persisted pointer inside the store itself, changed via `way profile use <name>` — not an env var or per-invocation flag. `way` always knows its own current context without external parameters. |
| Does `Task.pillar` need a hard schema migration? | No. The old enum's JSON representation is already a bare string, directly compatible with the new `Option<String>` type. Only a lowercase-normalization backfill is needed. |
| How is lineage cycle prevention implemented? | By construction — `parent_key` is immutable and set only at creation, and a parent must exist before a child references it. No runtime cycle check. |
| Does the TUI → `claude` handoff inject full context into the prompt? | **Superseded.** Originally: no, pass only the key. Revised (from real usage feedback): yes — `way` assembles the prompt itself before `claude` starts, re-attaching with `session_decisions`/`session_next` verbatim when present, or a fresh-start prompt from the task's fields otherwise. Already implemented as of the R1 revision fix; this pass updates it to the split fields. |
| How does `way session set-decisions`/`set-next` accept a multi-line blob? | Via stdin, not a CLI argument — avoids shell-quoting and newline fragility for senzu's multi-paragraph resume blocks. |
| Is `way tree` one level of children, or full lineage? | Full recursive descendants plus the full ancestor chain, matching the PRD's "full lineage" wording. |
| Why split `session_state` into `decisions`/`next` instead of one blob? | Real-usage feedback: a single cached blob presented as current has no way to signal partial staleness. Splitting lets `decisions` (durable) and `next` (expected to go stale fast, that's normal) carry different implicit trust levels, and keeps pointers (which *do* have a live source of truth) structurally separate from prose (which doesn't). |
| Why generalize `external_ref` to `external_refs: Vec<String>`, and why does `way` never resolve them? | A task's session may reference more than one promoted artifact (a PR, a ticket, a doc) at once. `way` never resolving them isn't a limitation being deferred — storing identifiers only, never a content copy, is what makes staleness structurally impossible for this part of the state: there's nothing cached to go stale. |

---

## Requirements Coverage

| requirement | covered by |
|---|---|
| R1 — decisions/next prose + `updated_at`, settable/readable via CLI, never treated as a cached copy | § Data Model (`session_decisions`/`session_next`/`session_updated_at`), § Interfaces (`way session set-decisions/set-next/show/clear`), § Core |
| R2 — profiles with independently configurable pillar sets | § Data Model (`Profile`, `PillarDef`), § Interfaces (`way profile *`), § Core (default bootstrap) |
| R3 — parent link, cycle-rejected at write time | § Data Model (`Task.parent_key`), § Interfaces (`way spawn`), § Validation, § Core (cycle-freedom by construction) |
| R4 — zero-or-more external pointers, never validated or resolved by `way` | § Data Model (`external_refs: Vec<String>`), § Interfaces (`way link` add/remove/clear), § Validation (idempotent add/remove) |
| R5 — TUI spawns `claude` subprocess, restores cleanly | § Interfaces (TUI keybinding), § Files Changed (`src/app.rs`), § Integration Tests (manual verification) |
| R6 — existing CLI/TUI behavior unchanged for unused features | § Design Principles (additive/backward-compatible), § Core (default profile bootstrap, pillar normalization) |
| R7 — active profile selection mechanism defined | § Resolved Questions, § Data Model (active-profile pointer) |
