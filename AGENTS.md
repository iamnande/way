# AGENTS.md

This file is the source of truth for agent instructions in this repo. It's kept provider-agnostic (not Claude-specific) to support cross-model and cross-tool integration — `CLAUDE.md` just points here.

## Agent skills

### Issue tracker

Issues live as GitHub Issues in `iamnande/way`, managed via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default label vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout — `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
