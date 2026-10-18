---
name: spec-driven-dev
description: Use when you add a new concept, change the architecture, or start a feature in this repo. Guides the LeanSpec five phases (Discover, Create, Refine, Implement, Verify) with this repo's specs/ directory and make spec-check.
---

# Spec-driven development

A new concept or an architecture change starts as a spec in `specs/`. This is a rule in `AGENTS.md`.

Read these first:
- `vendor/leanspec/SKILL.md` - the LeanSpec method.
- `.agents/skills/spec-driven-dev/spec-template.md` - the spec shape with EARS requirements syntax.
- `vendor/leanspec/first-principles.md` - why specs stay short.
- `specs/README.md` - the registry of current specs.

## Phase 1: Discover

1. Read `specs/README.md`. Look for a spec that already covers the work.
2. If one exists, update it. Do not make a second spec for the same subject.
3. Run `git fetch origin` and list open PRs. Check that nobody else builds the same thing.
4. Find the next number. Use the highest number in `specs/` plus one. Numbers are unique.

## Phase 2: Create

1. Make `specs/NNNN-kebab-slug.md`. Use 4 digits.
2. Start from `.agents/skills/spec-driven-dev/spec-template.md`.
3. Write the frontmatter: `status: planned`, `created: YYYY-MM-DD`. Add `tags`, `priority` and `depends_on` when they apply.
4. Write the H1 as `# NNNN-slug`. It must match the file name.
5. Add the five sections in this order: `## Overview`, `## Design`, `## Plan`, `## Test`, `## Notes`.
6. Add one row to `specs/README.md`. Use a relative link.

## Phase 3: Refine

1. In Overview, state what and why now. Date every number.
2. In Design, record each decision with who made it and when.
3. In Design, write all requirements using EARS syntax under `### Requirements (EARS)`:
   - **Ubiquitous**: `The <system> shall <action>.`
   - **Event-driven**: `WHEN <trigger>, the <system> shall <action>.`
   - **State-driven**: `WHILE <state>, the <system> shall <action>.`
   - **Unwanted behavior**: `IF <condition>, THEN the <system> shall <action>.`
   - **Optional feature**: `WHERE <feature>, the <system> shall <action>.`
   - **Complex**: `WHILE <state>, WHEN <trigger>, the <system> shall <action>.`
4. In Plan, write a checklist. Each item must be verifiable.
5. In Test, put at least one fenced command. State what a pass means.
6. Keep the spec under 300 lines. Above 400 lines the check fails. Split it into sub-spec files (`vendor/leanspec/sub-spec-files.md`).
7. Run the check:

```bash
make spec-check
```

## Phase 4: Implement

1. Set `status: in-progress`.
2. Cut a branch from `main`: `git switch -c feat/<slug> origin/main`.
3. Do one plan item at a time. Tick the item in the spec as you finish it.
4. Commit with a signed Conventional Commit. One logical change per commit.
5. Add one line under `## [Unreleased]` in `CHANGELOG.md` if the change is user-visible.

## Phase 5: Verify

1. Run the command in the spec's `## Test` section. Read the full output.
2. Run the repo gates:

```bash
make check
make hygiene
make smoke
```

3. Label your evidence. Only `verified` evidence satisfies a gate. See spec 0002.
4. Set `status: complete`. Update the row in `specs/README.md`.
5. Open the PR into `main`. Put the commands and results in the PR's Verified section.

## Do not

- Do not put implementation detail in a spec. State intent.
- Do not leave a broken relative link. `make spec-check` fails on it.
- Do not edit `vendor/leanspec/`.
