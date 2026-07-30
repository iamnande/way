# Tech Spec — way mind pillar: journal, check-ins, search

## Overview + Design Principles

This spec adds a new first-class entity (`JournalEntry`) and a new
configuration layer (`~/.config/way/config.toml`) to `way`. It follows the
same principles already established by `way-alignment-checkpoints`:
additive/backward-compatible, `Store` trait is the only architectural
boundary, no notification-sending code anywhere in `way`, CLI-first.

Two things are new to this codebase, introduced here and expected to be the
standing pattern going forward (not one-offs for journaling):

1. **Config lives outside redb.** `Profile` (`pillars`, `phases`) and this
   feature's `checkin_cadence_days`/`checkin_prompts` all move to a single
   `~/.config/way/config.toml`, loaded at startup. redb is reserved for
   actual growing/per-record app data (`Task`, and now `JournalEntry`).
   Tracked jointly with [issue #20](https://github.com/iamnande/way/issues/20).
2. **A second first-class entity beside `Task`.** `JournalEntry` gets its own
   redb table and its own `Store` methods — it is not a `Task` variant.

---

## Config

New dependency: `toml = "0.9"` (workspace-appropriate serde-compatible TOML
parser; matches the existing `serde`/`serde_json` pattern already in use).

```rust
// config.rs (new file)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<Profile>,   // moved from redb PROFILES_TABLE
    #[serde(default)]
    pub active_profile: Option<String>, // moved from redb SETTINGS_TABLE, no longer owner-scoped (see Migration)
    #[serde(default)]
    pub checkin_cadence_days: Option<u32>,   // None = no cadence enforced
    #[serde(default = "default_checkin_prompts")]
    pub checkin_prompts: Vec<String>,
}

fn default_checkin_prompts() -> Vec<String> {
    vec![
        "What has your attention lately?".to_string(),
        "How's progress against your goals?".to_string(),
        "Any impact to your long-term goals?".to_string(),
    ]
}

fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not resolve a config directory"))?;
    Ok(base.join("way").join("config.toml"))
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&raw)?)
}
```

`load_config()` is called once at startup (`main.rs`), same place `data_path()`
already resolves the redb file. A missing file, or a file missing individual
fields, resolves to defaults — no error either way; `way` never writes this
file itself in this feature (hand-edited only, per the PRD's non-goals).

### Migration (existing `Profile`/active-profile out of redb)

- `RedbStore::open` currently seeds `PROFILES_TABLE`/`SETTINGS_TABLE` and
  bootstraps a default "personal" profile. This moves to `Config::default()`
  seeding the same "personal" profile (via the existing
  `Profile::default_personal()`) when `profiles` is empty.
- `active_profile` was previously scoped per local-owner (multi-user
  forward-compat, never actually used). Since config is now a single flat
  file (inherently single-owner, matching how `way` already behaves in
  practice), `active_profile: Option<String>` drops the owner-scoping —
  simplification, not a regression, since nothing reads the multi-owner
  behavior today.
- `PROFILES_TABLE` and `SETTINGS_TABLE` (and the now-unused
  `active_profile_key`/`local_owner` helpers in `store.rs`) are removed once
  `Config` fully replaces them. `TABLE` (tasks) is untouched.
- One-time migration: none required to ship — this is nick's own single
  install, not a distributed tool with existing users' redb files to
  preserve. If a redb file has profile data from before this change, it's
  simply superseded by `config.toml`'s defaults on next run (acceptable,
  confirmed low-stakes for a single-user local tool).

---

## Data Model

```rust
// journal.rs (new file)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JournalEntryKind {
    Freeform,
    CheckIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: u64,
    pub created_at: i64,          // unix seconds
    pub kind: JournalEntryKind,
    pub content: String,
}
```

A `CheckIn` entry's `content` is the rendered prompt/answer pairs, one per
configured prompt, e.g.:

```
Q: What has your attention lately?
A: <answer>

Q: How's progress against your goals?
A: <answer>
```

This keeps `content: String` uniform across both kinds (no separate
structured-answers field) — substring search then works identically over
freeform and check-in entries alike, per the PRD's single-search-surface
requirement. Rendering is done once at write time by the CLI layer; `Store`
never parses `content` back apart.

---

## Interfaces

```
way journal add [--stdin]
    Opens $EDITOR on a scratch file (same pattern edtui/the TUI already
    pulls in for multi-line input) for a Freeform entry; --stdin reads
    content from stdin instead (for scripting/piping).

way journal checkin
    Walks Config.checkin_prompts in order, one interactive prompt at a
    time (stdin readline per question — no editor needed, answers are
    short). Renders the Q/A pairs into content, writes one CheckIn entry.

way journal list [--limit N]
    Reverse-chronological. Each row: id, date, kind, first line of content.

way journal show <id>
    Full content of one entry.

way journal search <query>
    Case-insensitive substring match over content. Prints matching entries
    (id, date, kind) - not full content, mirroring `list`'s row shape.

way journal status
    Prints whether a check-in is currently due (see Due Computation), and
    when the last one was. Also exposed as a field on `way show`-style JSON
    output for TUI/scripting consumption.
```

`Store` trait gains:

```rust
fn add_journal_entry(&self, kind: JournalEntryKind, content: String) -> Result<JournalEntry>;
fn list_journal_entries(&self) -> Result<Vec<JournalEntry>>;
fn find_journal_entry(&self, id: u64) -> Result<Option<JournalEntry>>;
fn search_journal_entries(&self, query: &str) -> Result<Vec<JournalEntry>>;
fn last_checkin(&self) -> Result<Option<JournalEntry>>;
```

`checkin_due(config: &Config, last: Option<&JournalEntry>) -> bool` is a free
function (not a `Store` method — it's pure logic over `Config` +
`Option<JournalEntry>`, no I/O), living in `journal.rs`:

```rust
pub fn checkin_due(config: &Config, last: Option<&JournalEntry>) -> bool {
    let Some(cadence_days) = config.checkin_cadence_days else { return false };
    match last {
        None => true, // never checked in, cadence is configured -> due
        Some(entry) => {
            let elapsed_days = (now_unix() - entry.created_at) / 86_400;
            elapsed_days >= cadence_days as i64
        }
    }
}
```

### Redb table

```rust
const JOURNAL_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("journal_entries");
```

Same id-allocation pattern already used for `Task` (next-id counter in
`SETTINGS_TABLE`-equivalent, or reusing whatever `Task` already does for
`id` generation — `JournalEntry.id` is independent of `Task.id`, own
sequence). `RedbStore::open` gains `write_txn.open_table(JOURNAL_TABLE)?;`
alongside the existing table opens (minus `PROFILES_TABLE`/`SETTINGS_TABLE`,
removed per Migration above).

---

## TUI

**Provisional** — [UX track: TUI structure + keybindings](https://github.com/iamnande/way/issues/21)
is a dedicated, separate effort to rework the TUI's overall structure and
keybinding scheme; nick isn't sold on either as they stand today. What
follows describes journal browsing/check-in due-signaling in terms of
today's existing patterns so this PRD/tech-spec isn't blocked on that track
resolving first, but the concrete keybinding/layout choices here are
expected to be revised once #21 lands, not treated as locked.

**New mode**, alongside the existing task-list mode — bound to a new key
(`J`, listed in the existing keybinding help line in `ui.rs`, e.g.
`"...  A archived  J journal  q quit"`).

**Journal list view**: mirrors `draw_list`'s existing row style — reverse
chronological, one line per entry (date, kind glyph, first-line preview).
**Journal detail view**: mirrors `draw_detail` — full `content`, styled as
plain multi-line text (no special parsing of the Q/A rendering).

**Due indicator**: a single-glyph badge (reusing `theme::RED`, same as the
`waiting_on` dot) shown in the app's status/header area — not per-row, since
it's not tied to any one task or journal entry — visible immediately on
opening the TUI when `checkin_due(...)` is true. Pressing the journal
keybinding while it's showing is the natural path into `checkin`, though the
TUI's `checkin` flow itself (interactive multi-prompt entry) can defer to
shelling out to the CLI verb initially, same as other multi-step flows in
`way` today, rather than building a bespoke TUI form for MVP.

---

## Tests

Mirror the existing `store.rs` test conventions:

- `add_journal_entry` + `find_journal_entry` round-trip id/kind/content.
- `list_journal_entries` returns reverse-chronological order.
- `search_journal_entries` is case-insensitive substring match; empty result
  for no match, not an error.
- `checkin_due`: `None` cadence -> never due; cadence configured + no prior
  check-in -> due; cadence configured + last check-in within window -> not
  due; last check-in past window -> due.
- `load_config`: missing file -> `Config::default()`; file with partial
  fields -> defaults fill the gaps (`#[serde(default)]` round-trip, same
  pattern already proven for `Task`/`Profile` fields elsewhere in this repo).
- Migration smoke test: fresh `RedbStore::open` no longer creates
  `PROFILES_TABLE`/`SETTINGS_TABLE`; `Config::default()` seeds the
  "personal" profile via `Profile::default_personal()`.

---

## Rollout

Single-commit, additive except for the `Profile`/active-profile migration
out of redb, which is a deliberate breaking change to internal storage —
acceptable since this is nick's own single local install, not a distributed
tool with other users' existing redb files to preserve compatibility with.
No coordination with any other repo required.
