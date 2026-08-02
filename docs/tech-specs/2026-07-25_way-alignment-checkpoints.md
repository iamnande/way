# Tech Spec — way alignment checkpoints (WAY-5)

## Overview + Design Principles

This spec defines `way`'s side of "agent works a phase, pings nick when it
needs him": two new fields on `Task`, CLI verbs to set/clear/show them, and a
TUI badge. It reuses every design principle already established in the
senzu-integration tech spec — additive/backward-compatible fields, `way`
stores but never interprets senzu's semantics, the `Store` trait is the only
architectural boundary, no new persistence machinery, CLI-first.

One addition specific to this work: **`way` contains zero notification
code.** The `Store`/CLI layer's only job is recording and exposing a boolean-
ish fact (blocked or not, since when). Whichever agent runtime is driving
senzu decides when to set it and how to tell nick — that's entirely outside
`way`'s process boundary, same as external-pointer resolution already is.

**One deliberate reversal:** the senzu-integration tech spec's principle #2
("`way` stores data, it does not interpret senzu's phase semantics") no
longer holds in full. `way` still never interprets what a phase *name*
means — but it now validates phase *order* against a profile-configured
vocabulary, because that's the only way "the sequence must be followed"
holds regardless of which model/harness is driving it. This is scoped as
narrowly as possible: order only, nothing about phase content or what
must be true before advancing.

---

## Data Model

```rust
// task.rs
pub struct Task {
    // existing fields unchanged
    pub waiting_on: Option<String>,       // reason text; presence = blocked on nick. way never interprets it.
    pub waiting_on_since: Option<i64>,    // unix seconds, stamped whenever waiting_on is set; cleared together
}

pub struct Profile {
    // existing fields unchanged: name, pillars, default_issue_system
    pub phases: Vec<String>,   // ordered; empty = no enforcement (today's free-form behavior)
}
```

No new redb tables — these live on the existing `Task`/`Profile` JSON blobs,
same as `phase`/`pillars`/etc. `#[serde(default)]` on all three new fields
(`waiting_on`, `waiting_on_since`, `phases`), so existing stored tasks and
profiles deserialize with `None`/`vec![]` and need no migration. Every
profile that exists before this ships gets `phases: []`, which is exactly
"no enforcement" — zero behavior change until nick explicitly configures a
sequence for a given profile.

Deliberately *not* reusing `session_updated_at` for the timestamp:
`session_updated_at` is a shared "something in the session block changed"
signal, bumped by `set_session_phase`/`set_session_decisions`/
`set_session_next` alike. If `waiting_on_since` aliased it, editing decisions
prose while still blocked would silently reset "how long has this been
waiting" — the two need to vary independently.

---

## Interfaces

```
way session set-waiting <key> <reason>
    sets waiting_on = <reason>, waiting_on_since = now(). Positional
    one-liner, same shape as `set-phase <key> <phase>` — not a stdin blob
    like set-decisions/set-next, since these reasons read as short
    one-line prose ("prd needs architecture input/direction"), not prose
    blobs.

way session clear-waiting <key>
    sets waiting_on = None, waiting_on_since = None.

way session show <key>
    (existing) now also prints:
        waiting_on: <reason>
        waiting_on_since (unix seconds): <ts>
    when set. The existing "no session state" guard
    (phase/decisions/next/claude_session_id all None -> bail) also checks
    waiting_on, so a task that's *only* flagged waiting doesn't spuriously
    report "no session state for WAY-N".

way session clear <key>
    (existing) additionally clears waiting_on/waiting_on_since, alongside
    phase/decisions/next/updated_at. claude_session_id is still left alone
    — unchanged, that's identity not resume-state.

way session set-phase <key> <phase> [--force]
    (existing, gains --force) validated against the active profile's
    ordered `phases` list when it's non-empty (see Validation below).
    --force bypasses the check unconditionally; way has no notion of caller
    identity, so this is a plain flag, not a permission gate.

way profile add <name> --pillars "..." [--phases "grounding,spec,planning,..."]
    (existing, gains --phases) comma-separated, ordered, optional — omitted
    or empty means no enforcement for tasks under that profile.
```

`Store` trait gains two methods:

```rust
fn set_waiting(&self, id: u64, reason: Option<String>) -> Result<()>;
fn set_session_phase(&self, id: u64, phase: Option<String>, force: bool) -> Result<()>; // signature change
```

`set_waiting`: `Some(reason)` sets both fields together (stamping the
timestamp inside the same method, same pattern as `set_session_phase`
stamping `session_updated_at`); `None` clears both together. `set-waiting`
and `clear-waiting` are two CLI verbs over this one store method — same
ergonomic split as `SetClaudeId`/`ClearClaudeId` over `set_claude_session_id`.

`clear_session` (backing `way session clear`) gains two lines clearing
`waiting_on`/`waiting_on_since`, alongside the existing phase/decisions/next/
updated_at resets.

### Validation (set_session_phase)

Mirrors `set_pillar`'s existing `active_profile()?.find_pillar(...)` check:

```rust
fn set_session_phase(&self, id: u64, phase: Option<String>, force: bool) -> Result<()> {
    if let Some(mut task) = self.get(id)? {
        if let (Some(new_phase), false) = (&phase, force) {
            let profile = self.active_profile()?;
            if !profile.phases.is_empty() {
                let new_idx = profile.phases.iter().position(|p| p == new_phase)
                    .ok_or_else(|| anyhow!(
                        "unknown phase '{new_phase}' for active profile '{}' (pass --force to set it anyway)",
                        profile.name
                    ))?;
                let cur_idx = task.phase.as_ref()
                    .and_then(|p| profile.phases.iter().position(|x| x == p));
                let skips_ahead = match cur_idx {
                    None => new_idx > 0,
                    Some(cur) => new_idx > cur + 1,
                };
                if skips_ahead {
                    bail!(
                        "phase '{new_phase}' skips ahead of '{}' in profile '{}' (pass --force to override)",
                        task.phase.as_deref().unwrap_or("(none)"), profile.name
                    );
                }
            }
        }
        task.phase = phase;
        task.session_updated_at = Some(now_unix()?);
        self.put(&task)?;
    }
    Ok(())
}
```

Rule: a transition is valid if it restates the current phase, moves to the
immediate next phase in the configured list, or moves to any earlier phase
(revisiting is rigor, never a violation). Anything else — skipping an
unvisited phase, or naming a phase outside the configured list — is
rejected unless `force` is true. An empty `phases` list short-circuits the
whole check, so every profile that exists today is unaffected until nick
configures one.

---

## TUI

**List row** (`draw_list` in `ui.rs`): when `task.waiting_on.is_some()`, an
extra leading span — a single `●` glyph in `theme::RED` — is prepended before
the `WAY-{key}` span. Unset rows get a matching blank-width space instead, so
list alignment doesn't shift between blocked and unblocked rows.

```
● WAY-5 ▸ [c] spike: quality autonomous workflows
  WAY-4 ▸ [c] fix: read/write lock handling
```

`theme::RED` is already defined (currently only used for the archive/delete
confirm prompt) — reused here rather than adding a new color, consistent with
it already meaning "this needs your attention."

**Detail pane** (`draw_detail` in `ui.rs`): a new conditional block, same
shape as the existing `SESSION` line, inserted immediately after it:

```rust
if let Some(reason) = &task.waiting_on {
    let since = task.waiting_on_since.map(relative_time).unwrap_or_default();
    text.push(Line::from(vec![
        Span::styled(format!("{:<8}", "WAITING"), Style::default().fg(theme::DIM)),
        Span::styled(format!("{reason} · {since}"), Style::default().fg(theme::RED)),
    ]));
}
```

---

## Tests

Mirror the existing `session_phase_decisions_and_next_round_trip_independently_and_share_updated_at`
coverage in `store.rs`:

- `set_waiting` round-trips `waiting_on`/`waiting_on_since` and leaves
  phase/decisions/next untouched.
- `clear_session` clears `waiting_on`/`waiting_on_since` alongside the
  existing fields, leaves `claude_session_id` untouched.
- `clear_waiting` (via `set_waiting(id, None)`) clears both fields without
  touching phase/decisions/next.
- A task with only `waiting_on` set (no phase/decisions/next/claude_session_id)
  does not trip the "no session state" bail in `SessionCommand::Show`.
- With a profile configured `phases: ["grounding", "spec", "planning"]`:
  `None -> grounding` succeeds; `grounding -> spec` succeeds; `spec ->
  grounding` (backward) succeeds; `grounding -> planning` (skips `spec`)
  fails without `--force`, succeeds with it; `grounding -> "nonexistent"`
  fails without `--force`, succeeds with it.
- A profile with `phases: []` (every profile predating this change): any
  `set_session_phase` call succeeds unconditionally, matching current
  behavior exactly.

---

## Rollout

Single-commit, no flag/migration needed — additive fields, existing tasks
unaffected. No coordination with the senzu skill (dotfiles repo) required for
this commit to land; senzu adopting `set-waiting`/`clear-waiting` is a
follow-on change in that repo, out of scope here.
