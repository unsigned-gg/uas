---
name: Work item
about: A unit of work with a spec, a scope and a done-when
title: ""
labels: ""
---

## Why

<!-- The problem, in one or two sentences. -->

## Spec

- LeanSpec: specs/NNNN-slug.md (a new concept or an architecture change starts as a LeanSpec; see AGENTS.md)
- Architecture note: docs/spec/<file>.md, if one exists

## Scope

- In:
- Out:

## Done when

<!-- Verifiable checks. Each one names a command. -->

## Branch and PR

- Branch `feat/<slug>` (or `fix/`, `docs/`), cut from `origin/main`
- PR targets `main`. The title has no ticket ID. The body says "Part of CER-N"
- Merge on green CI with a merge commit.

## Evidence

- Tier: asserted (verified | reported | heuristic | asserted; only `verified` satisfies a gate)
- <command or receipt that proves it>

## Gated steps after merge

- none

## Rollback

- <how to undo, in one step>
