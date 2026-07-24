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
   Session-state is an opaque string blob from `way`'s point of view. senzu can
   change its internal resume-block format without `way` needing a migration.
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
    pub pillar: Option<String>,        // was Option<Pillar>; now a profile-defined name, lowercase
    pub parent_key: Option<u32>,       // references another task's `key`, immutable after creation
    pub external_ref: Option<String>,  // free-form, e.g. "owner/repo#N" or "PROJ-123"
    pub session_state: Option<String>, // opaque blob, senzu's resume-block format
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

---

## Interfaces

```
way spawn <parent-key> <title> [--description ...] [--tags ...] [--pillar ...]
    creates a task with parent_key = <parent-key>. Errors if parent-key doesn't exist.

way tree <key>
    JSON: { task, ancestors: [...], descendants: [...] }
    ancestors: full parent_key chain to root.
    descendants: full recursive tree below this task.

way link <key> <external-ref>
way link <key> clear

way session set <key>          # reads the blob from stdin, not a positional arg
way session show <key>         # prints the raw blob to stdout; exit 1 if none set
way session clear <key>

way profile list
way profile use <name>
way profile add <name> --pillars "name:glyph:colorhex,name:glyph:colorhex,..."

way pillar <key> <name>        # unchanged shape; now validates against the active profile
```

TUI: new keybinding (`c`) on the selected task — suspends raw mode/alt screen,
runs `claude "let's work on WAY-{key}: {title} — run \`way show {key}\` for
context"`, waits for exit, restores the TUI with a full redraw. v1 passes only
the key in the prompt; it does not inject full task/session context itself —
see § Resolved Questions.

---

## Validation

- Pillar name on `way pillar` / `way add --pillar` / task creation: must match
  a name in the *active* profile's pillar list at assignment time (case-
  insensitive). Unknown name → reject. Already-assigned values are **not**
  re-validated on read if the profile changes later (see PRD edge cases —
  orphaned pillars stay as inert data, display-only).
- `way spawn <parent-key>`: parent must exist (`find_by_key`), else error.
- `way link`: non-empty string; no format/existence validation against GitHub/
  Linear.
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
| `src/task.rs` | `Task.pillar` → `Option<String>`; add `parent_key`, `external_ref`, `session_state`; add `PillarDef`, `Profile`, `IssueSystem`; remove/retire the hardcoded `Pillar` enum (kept only as the constant list used by default-profile bootstrap) |
| `src/store.rs` | new `profiles` table + active-profile pointer; `normalize_pillars` startup pass (alongside existing `backfill_keys`); new `Store` methods: `spawn_child`, `tree`, `link_external`, `set_session_state`, `clear_session_state`, `list_profiles`, `use_profile`, `add_profile`, `active_profile` |
| `src/cli.rs` | new subcommands: `spawn`, `tree`, `link`, `session set/show/clear`, `profile list/use/add`; `pillar` / `add --pillar` validate against the active profile instead of a hardcoded enum |
| `src/app.rs` | new `c` keybinding: suspend terminal, spawn `claude` subprocess, restore terminal on exit |
| `src/ui.rs` | pillar color/glyph lookup becomes profile-data-driven; the pillar picker becomes dynamic (N options from the active profile) instead of fixed digits 1–6 |
| `src/theme.rs` | unchanged structurally; color values now sourced via profile pillar defs rather than a hardcoded per-`Pillar`-variant match |
| `src/main.rs` | unchanged — CLI dispatch is already generic over subcommands |

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
- `session_state_round_trips_and_clears` — set a multi-line blob via stdin,
  read it back verbatim, clear it, confirm a subsequent read reports none.
- `pillar_assignment_rejects_name_not_in_active_profile` — assigning an
  unknown pillar name errors; an already-assigned pillar that's later removed
  from the profile is *not* rejected on read (orphaned, display-only).
- **Manual verification** (not automatable under the standing rule against
  driving the live binary): the `c` keybinding actually suspends and restores
  the terminal cleanly — nick runs this himself.

---

## Resolved Questions

| question | resolution |
|---|---|
| How is the active profile selected? (R7) | A persisted pointer inside the store itself, changed via `way profile use <name>` — not an env var or per-invocation flag. `way` always knows its own current context without external parameters. |
| Does `Task.pillar` need a hard schema migration? | No. The old enum's JSON representation is already a bare string, directly compatible with the new `Option<String>` type. Only a lowercase-normalization backfill is needed. |
| How is lineage cycle prevention implemented? | By construction — `parent_key` is immutable and set only at creation, and a parent must exist before a child references it. No runtime cycle check. |
| Does the TUI → `claude` handoff inject full context into the prompt? | No. v1 passes only the WAY-N key; the session is expected to call `way show`/`way session show` itself. Richer prompt construction is senzu-side work, explicitly out of scope here. |
| How does `way session set` accept a multi-line blob? | Via stdin, not a CLI argument — avoids shell-quoting and newline fragility for senzu's multi-paragraph resume blocks. |
| Is `way tree` one level of children, or full lineage? | Full recursive descendants plus the full ancestor chain, matching the PRD's "full lineage" wording. |

---

## Requirements Coverage

| requirement | covered by |
|---|---|
| R1 — session-state blob, settable/readable via CLI | § Data Model (`Task.session_state`), § Interfaces (`way session set/show/clear`), § Core |
| R2 — profiles with independently configurable pillar sets | § Data Model (`Profile`, `PillarDef`), § Interfaces (`way profile *`), § Core (default bootstrap) |
| R3 — parent link, cycle-rejected at write time | § Data Model (`Task.parent_key`), § Interfaces (`way spawn`), § Validation, § Core (cycle-freedom by construction) |
| R4 — external issue reference, no validation | § Data Model (`Task.external_ref`), § Interfaces (`way link`) |
| R5 — TUI spawns `claude` subprocess, restores cleanly | § Interfaces (TUI keybinding), § Files Changed (`src/app.rs`), § Integration Tests (manual verification) |
| R6 — existing CLI/TUI behavior unchanged for unused features | § Design Principles (additive/backward-compatible), § Core (default profile bootstrap, pillar normalization) |
| R7 — active profile selection mechanism defined | § Resolved Questions, § Data Model (active-profile pointer) |
