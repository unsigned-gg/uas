---
name: work-item
title: "{{ summary }}"
---
## Why

{{ why }}

## Spec

- LeanSpec: {{ spec | "specs/NNNN-<slug>.md (a new concept or an architecture change starts as a LeanSpec; AGENTS.md)" }}
- Architecture note: {{ arch_note | "docs/spec/<file>.md, if one exists" }}

## Scope

- In: {{ scope_in }}
- Out: {{ scope_out | "nothing else" }}

## Done when

{{ done_when }}

## Branch and PR (spec 0003)

- Branch `feat/<slug>` (or `fix/`, `docs/`), cut from `origin/development` in a sibling worktree
- PR targets `development`. The title has no ticket ID. The body says "Part of CER-N"
- Merge on green CI with a merge commit pinned to the head SHA. A promotion PR carries `development` to `main`

## Evidence (spec 0002)

- Tier: {{ evidence_tier | "asserted" }} (verified | reported | heuristic | asserted; only `verified` satisfies a gate)
- {{ evidence | "<command or receipt that proves it>" }}

## Gated steps after merge

- {{ gated | "none" }}

## Rollback

- {{ rollback | "<how to undo, in one step>" }}
