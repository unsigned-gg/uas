<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plans

One file per spec, written by `uas-planner` from a landed spec. The procedure of record is spec 0050
(`specs/0050-build-handoff-pipeline.md`); `.claude/agents/README.md`, "Build handoff", is the short form.

File name: `<NNNN>-<slug>.md`, the spec file stem.

## Plan shape

Sections in this order:

1. Header: spec, status, base sha, planner, date, and the three tiers (`verified`, `reported`, `asserted`).
2. `Findings the builders must know`.
3. `Plan items to packages`: each spec Plan item mapped to a package.
4. `Requirement coverage`: each requirement mapped to the package that closes it.
5. `Waves`: packages per wave, with a "Disjoint because" column. A wave holds at most three builders
   and their files do not overlap. Rows of this table start with a word, not a number.
6. One block per package: `### P<n>: <title> (wave <k>[, needs ...])`.
7. `Open`: what the planner could not settle.

## Package shape

Each package block holds, in this order:

- **Persona:** the agent that builds it.
- **Files:** paths that exist, or `new` on each new file.
- **Change:** what to write, to the line where the plan can.
- **Done-when:** a command with an exit code, taken from the spec Test section or a Makefile gate, that
  fails before the change.
- **Red-first:** the control: run Done-when before the change and watch it fail, then break the result
  once on purpose and watch it fail again.

A package with no runnable Done-when is BLOCKED. It names the spec line that blocks it.

## Landed packages

When a package lands, its heading gains `(DONE <date>, #<PR>)`. The plan file stays as the record.

## Spec status move

The plan's last package is the status move. The spec status goes from `planned` to `in-progress` with
the first landed wave, and to `complete` when the plan is done and the spec Test command exits 0 on
`development`. The frontmatter `status`, the body status line and the registry row in `specs/README.md`
change in one commit.
