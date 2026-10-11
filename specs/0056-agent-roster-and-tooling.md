---
status: planned
created: 2026-10-11
tags: [agents, skills, makefile, lint, hygiene, roster]
priority: medium
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0050-build-handoff-pipeline, 0051-repository-guards, 0052-spec-check-rule-set]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0056-agent-roster-and-tooling

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

Three surfaces steer every lane in this repo and none has an owner: the persona files under
`.claude/agents/`, the skills under `.agents/skills/`, and the Makefile. Spec 0050 owns the
procedure that uses the personas. Spec 0002 names five Make targets. Spec 0052 owns the
`spec-check` rules. This spec owns the rest: the roster, the lint gate for persona files, the skill
roster, and the Make surface with one owning spec per target. Christian ruled on 2026-10-11 about
05:50Z that this spec is drafted (AskUserQuestion, lane w25:p1, label "Agent roster and tooling
surface") [reported: integrating lane brief]. Content decisions are the integrating lane's unless
marked `proposed`.

The surfaces are consistent today. The gap is enforcement: nothing in CI reads a persona file
(F4), and `AGENTS.md` lists three fewer `hygiene` members than the Makefile (F7).

### Facts

Measured 2026-10-11 on `origin/development` ebc7be8. Paths are relative to the repository root.

- **F1 [verified]:** 9 persona files: `ls .claude/agents/uas-*.md | wc -l` prints 9.
  `.claude/agents/README.md` lists the same 9 in its table (lines 11 to 21).
- **F2 [verified]:** body lines (`awk '/^---$/{d++;next} d>=2{c++}'`): architect 71, builder 67,
  critic 73, ebpf-builder 70, engineer 71, foreman 58, k8s-builder 77, planner 67, prober 60. All
  nine are within 40 to 110. `grep -nP '[^\x00-\x7F]'` finds nothing in any of them. Each holds
  `## Boundaries`, `## Tools and policy`, `## Escalation`, `## Output` in that order and one
  `Forbidden:` line, and each Output section states the 15-line cap.
- **F3 [verified]:** `.saga/teams.toml` lines 19 to 27 name 7 of the 9 in the `uas-estate` gang:
  foreman, architect, builder (twice), ebpf-builder, engineer, critic, prober. Planner and
  k8s-builder are not in it. The README says `--model` wins over a persona's `model:` line
  (lines 33 to 35).
- **F4 [verified]:** `prompt-linter` is a global read-only agent (the user-level agents directory,
  tools Read, Grep, Glob, no shell). It runs checks F1 to F8, P1 to P8 and A1 to A7. Nothing in
  the repo runs it: `grep -rn 'prompt-linter' .github Makefile lefthook.yml .claude/hooks` prints
  nothing. The README says to run it before a file is committed (lines 74 to 78).
- **F5 [verified]:** the linter's check F6 reads banned strings from `tests/agents.test.sh`. That
  file does not exist here (`ls tests` fails) or in the paas checkout (its `tests` directory
  lists five fixture directories and no such file). No banned
  list is defined anywhere, so F6 cannot pass or fail today.
- **F6 [verified]:** `.agents/skills/` holds 5 skills, each a `SKILL.md` of 36 to 79 lines whose
  description starts "Use when": ci-triage, custody-run, new-repo, release, spec-driven-dev.
  `.claude/skills/` holds 5 symlinks to them. `docs/runbooks/README.md` says "No runbooks yet."
- **F7 [verified]:** `Makefile` lines 15 to 17 list 32 names in `.PHONY`; 32 targets are defined
  (the two sets are equal); each defines a `##` help text. Line 49 gives `hygiene` 8 members. The
  `AGENTS.md` row (line 56) lists 5; spec 0002 line 40 lists 4. `ci.yml` lines 84 and 85 run all 8.
- **F8 [verified]:** the Make inputs are `JOBS ?= 2` (line 8), `NEXTEST_PROFILE`, `ORG`,
  `EBPF_TOOLCHAIN`, `SUDO`, and per target `VERSION`, `NAME`, `BASE`, `HEAD`, `PR_BODY_FILE`,
  `HEAD_REF`. The `xtask` targets take no `JOBS`: spec 0052 F2.

## Design

### Roster

Class: H handoff role (cited by spec 0050), G pane-gang role (`.saga/teams.toml`), both where
marked. Write scope is what the persona's Boundaries section allows.

| Persona | Class | Model | Tools | Write scope |
|---|---|---|---|---|
| `uas-engineer` | H, G | opus | Read, Grep, Glob, Bash, Write, Edit | `specs/` and brief files |
| `uas-planner` | H | sonnet | Read, Grep, Glob, Bash, Write | `engineering/plans/<spec>.md` |
| `uas-builder` | H, G | sonnet | Read, Grep, Glob, Bash, Write, Edit | files its brief names, own worktree |
| `uas-ebpf-builder` | H, G | sonnet | Read, Grep, Glob, Bash, Write, Edit | `crates/uas-ebpf`, own worktree |
| `uas-k8s-builder` | H | sonnet | Read, Grep, Glob, Bash, Write, Edit | `crates/uas-operator`, CRDs, fixtures |
| `uas-critic` | H, G | opus | Read, Grep, Glob, Bash | nothing |
| `uas-foreman` | G | sonnet | Read, Grep, Glob, Bash | nothing |
| `uas-architect` | G | inherit | Read, Grep, Glob, Bash, Write | the design-debt list |
| `uas-prober` | G | haiku | Read, Grep, Glob, Bash | nothing in any repo |

### Estate rules each persona meets

| ID | Rule | Checked by |
|---|---|---|
| E1 | Frontmatter keys only from name, description, tools, disallowedTools, model, maxTurns, effort, isolation, background, memory; `name` equals the file stem; `model` is haiku, sonnet, opus or inherit | `tooling-check` |
| E2 | Body of 40 to 110 lines after the closing frontmatter marker; pure ASCII | `tooling-check` |
| E3 | H2 order: Boundaries, Tools and policy, Escalation, Output; one `Forbidden:` list | `tooling-check` |
| E4 | `tools` lists only Read, Grep, Glob, Bash, Write, Edit, Agent, SendMessage, WebFetch, WebSearch, Monitor, LSP; a haiku persona lists no Write or Edit | `tooling-check` |
| E5 | The Output section caps the verdict at 15 lines or fewer | `tooling-check` |
| E6 | A README row and a roster row exist; each `persona` in `.saga/teams.toml` is a file stem | `tooling-check` |
| E7 | Capability-tool map, enforcement honesty, affirmative framing, no recitation leak (manual Parts III and IV) | `prompt-linter`, on request |

### Skills

| Skill | Purpose | Lines |
|---|---|---|
| `spec-driven-dev` | A new concept or architecture change: LeanSpec phases, `make spec-check` | 79 |
| `release` | Cut a version: `make release-notes`, the PR into development, the promotion PR | 63 |
| `ci-triage` | A red check: find the job, read 30 log lines, reproduce with the make target | 55 |
| `new-repo` | Make a repository from the template: `make new-repo`, then the checks | 79 |
| `custody-run` | Agent work under blackwall custody: task file, jailed run, settle, reconcile | 36 |

### Make surface

Every target in `.PHONY` is contract. Owner is the spec whose row decides the target's behaviour.
"This" means this spec. Inputs are Makefile variables or environment values read by the recipe.

| Targets | Owner | Inputs |
|---|---|---|
| `check`, `ci`, `smoke`, `template-check`, `fmt-check`, `clippy`, `test`, `doc-test`, `typos`, `machete` | 0002 (gate table, lines 39 to 43) | `JOBS`, `NEXTEST_PROFILE` |
| `hygiene` | 0002 (line 40), each other member under its owner below | `JOBS` |
| `spec-check` | 0052 | none |
| `toolchain-check`, `pin-check`, `pgt-install`, `deny` | 0004 (0002 names `deny` in `check`) | none |
| `oss-check`, `spdx-check`, `fetch-contracts` | 0048 | none |
| `hook-test` | 0051 D6 | none |
| `text-check` | 0053 D5 | `BASE`, `HEAD`, `PR_BODY_FILE`, `HEAD_REF` |
| `audit`, `sbom` | 0045 D1, D4 | none |
| `ebpf-build`, `ebpf-test` | 0014 D3, D8 | `JOBS`, `EBPF_TOOLCHAIN`, `SUDO` |
| `release-notes` | 0042 D4 (dirty-tree refusal), 0053 (the roll) | `VERSION` |
| `new-repo` | 0037 D3 | `NAME`, `ORG` |
| `build`, `doc`, `fmt`, `help`, `custody-check` | This | `JOBS` for `build` and `doc` |
| `tooling-check` | This (planned, Plan P1) | none |

`custody-check` runs `blackwall doctor`; spec 0022 uses that command as a test line and does not
decide the target, so this spec owns it (F7 lists no other owner).

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | This spec owns the roster, the persona lint gate, the skill roster and the Make-target index. Spec 0050 keeps the procedure, 0052 the `spec-check` rules, 0002 the gate table. This spec cites and does not restate them. | Three surfaces had no owner (Overview). | lane-decided: uas-engineer, 2026-10-11 |
| D2 | Personas fall in two classes: handoff roles that spec 0050 cites, and pane-gang roles that `.saga/teams.toml` names. A persona may be both. Foreman, architect and prober are gang-only; planner and k8s-builder are handoff-only. | The README table mixes them in one list (F3). | lane-decided: uas-engineer, 2026-10-11 |
| D3 | The estate agent-file rules E1 to E7 are the persona contract. Models stay as the README records them under Christian's 2026-10-10 ruling; this spec records the values and decides none. | Duplicated model decisions drift. | lane-decided: uas-engineer, 2026-10-11 |
| D4 | The lint gate has two layers. `make tooling-check` runs E1 to E6 deterministically. `prompt-linter` runs E7 and stays on request: a model reads it, so CI cannot. A PR that changes `.claude/agents/` carries the linter's TOTALS line in its body. | F4: no gate exists. The linter has no shell and cannot run in CI. | lane-decided: uas-engineer, 2026-10-11 |
| D5 | `tooling-check` joins the `hygiene` members. | `hygiene` is a required check; the change makes a persona defect block a merge. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D6 | `tooling-check` is a shell recipe of about 40 lines, in the style of `hook-test`, not an `xtask`. It has no banned-string list (F5). If paas publishes one, it becomes a variable of the recipe. | No Rust is needed for awk and grep checks; F6 of the linter has no source here. | lane-decided: uas-engineer, 2026-10-11 |
| D7 | A skill is a procedure an agent follows for a recurring kind of work, with a "Use when" trigger. A procedure for an operator with no agent trigger is a runbook under `docs/runbooks/`. A rule or a contract is a spec. | `docs/runbooks/` is empty and the skills are agent-facing (F6). | lane-decided: uas-engineer, 2026-10-11 |
| D8 | A new Make target ships with its `.PHONY` entry, a `##` help text and one row in the Make surface table above (or an amended row of its owner). `tooling-check` enforces the first and the row. | F7 shows the sets equal today; nothing keeps them equal. | lane-decided: uas-engineer, 2026-10-11 |
| D9 | Documentation tables that list `hygiene` members (`AGENTS.md` line 56, spec 0002 line 40) are corrected to the Makefile's 8, by a builder in Plan P3. P3 takes that fix over from spec 0002 line 105 and spec 0052 line 228, which each leave it to the other (amendment list in Notes). | F7: they list 5 and 4. | lane-decided: uas-engineer, 2026-10-11 |

### Requirements (EARS)

- **[REQ-0056-01]** (Ubiquitous): The file `.claude/agents/uas-*.md` of each persona shall carry frontmatter keys only from the allowed set of E1, a `name` equal to its file stem, and a `model` of haiku, sonnet, opus or inherit.
- **[REQ-0056-02]** (Ubiquitous): The persona file shall hold 40 to 110 body lines after the closing frontmatter marker and shall hold no byte outside ASCII.
- **[REQ-0056-03]** (Ubiquitous): The persona file shall hold the H2 sections Boundaries, Tools and policy, Escalation and Output in that order, and one `Forbidden:` list.
- **[REQ-0056-04]** (Ubiquitous): The persona file shall list only tools of E4, shall list no Write or Edit when its model is haiku, and shall state a verdict cap of 15 lines or fewer in its Output section.
- **[REQ-0056-05]** (Ubiquitous): The persona file shall have one row in `.claude/agents/README.md` and one in the roster table of this spec, and the `tools` line of the file shall appear in that roster row.
- **[REQ-0056-06]** (Unwanted behavior): IF `.saga/teams.toml` names a `persona` that is not the stem of a file under `.claude/agents/`, THEN `make tooling-check` shall exit non-zero naming it.
- **[REQ-0056-07]** (Event-driven): WHEN a pull request changes a file under `.claude/agents/`, the integrating lane shall run `prompt-linter` on each changed file before the commit and shall put its TOTALS line in the pull request body.
- **[REQ-0056-08]** (Ubiquitous): The target `tooling-check` shall run the checks E1 to E6 and shall be a member of `hygiene`.
- **[REQ-0056-09]** (Ubiquitous): The directory of each skill under `.agents/skills/` shall hold a `SKILL.md` whose `name` equals the directory name and whose `description` starts with "Use when", and `.claude/skills/<name>` shall be a symlink to that directory.
- **[REQ-0056-10]** (Unwanted behavior): IF a target defined in the Makefile is missing from `.PHONY`, lacks a `##` help text, or has no row in the Make surface table of this spec, THEN `make tooling-check` shall exit non-zero naming the target.
- **[REQ-0056-11]** (Event-driven): WHEN a pull request adds a Make target, it shall add the `.PHONY` entry, the help text and one row to the Make surface table, or amend the row of the spec that owns the target.
- **[REQ-0056-12]** (Ubiquitous): The `make hygiene` row of `AGENTS.md` shall name every member of the Makefile `hygiene` target.
- **[REQ-0056-13]** (Ubiquitous): The owner cell of each row of the Make surface table shall be non-empty and shall be This or name only specs that exist in `specs/`.

## Plan

- [ ] P1, one builder, `Makefile` only: add `tooling-check` (E1 to E6, REQ-0056-01 to 06, 08, 09, 10), add it to `.PHONY` and to `hygiene`, with one planted-defect test per row of the Test table. Red first: run the new recipe before the persona checks exist.
- [ ] P2, the integrating lane: record the linter step of REQ-0056-07 in `.claude/agents/README.md` (Lint section), after D5 is confirmed.
- [ ] P3, one builder, docs only: correct the `hygiene` rows of `AGENTS.md` and spec 0002, and amend spec 0002 line 105 and spec 0052 line 228 to name this spec as the owner (D9, REQ-0056-12).
- [ ] P4, the status moves per spec 0050 D14: `in-progress` with P1, `complete` when the Test command exits 0 on `development`.

## Test

Each plant runs in a scratch copy: `git -C /tmp/uas-w56 archive HEAD | tar -x -C "$SCRATCH"` (a copy,
never the checkout). The control is the unplanted copy: the command below prints `OK` and exits 0.
A row goes red when the check it names is removed from the command.

| Requirement | Plant, expected output |
|---|---|
| REQ-0056-01 | Add `owner: x` to a persona frontmatter: `FAIL frontmatter key owner`. Rename `name:` to another stem: `FAIL name`. Set `model: gpt`: `FAIL model`. |
| REQ-0056-02 | Append 50 lines to `uas-builder`: `FAIL body lines 117`. Insert one em dash (U+2014): `FAIL non-ASCII`. Delete 30 lines: `FAIL body lines` below 40. |
| REQ-0056-03 | Swap `## Escalation` and `## Output`: `FAIL heading order`. Delete the `Forbidden:` line: `FAIL Forbidden`. |
| REQ-0056-04 | Add `Write` to `tools:` of `uas-prober`: `FAIL haiku tools`. Add `Foo` to any `tools:`: `FAIL tool Foo`. Delete the cap sentence: `FAIL verdict cap`. |
| REQ-0056-05 | Copy a persona to `uas-extra.md`: `FAIL no README row`. Edit one tool in a persona: `FAIL tools not in roster`. |
| REQ-0056-06 | Set a `persona` value in `.saga/teams.toml` to `uas-nope`: `FAIL team persona uas-nope`. Control: the shipped file passes. |
| REQ-0056-07 | Process rule, no automated check: the reviewer reads the PR body for a `TOTALS:` line. Evidence tier `asserted` until P2 lands. |
| REQ-0056-08 | `make tooling-check` is absent today: `make: *** No rule to make target`. Red until P1. After P1, remove it from the `hygiene` line: `make -n hygiene` no longer prints its recipe. |
| REQ-0056-09 | Rename a skill directory: `FAIL skill name`. Break a symlink: `FAIL skill link`. Edit a description to start "Run": `FAIL skill trigger`. |
| REQ-0056-10 | Add `foo: ## x` to the Makefile without `.PHONY`: `FAIL not in .PHONY: foo`. Add it to `.PHONY`: `FAIL no row: foo`. Remove a `##`: `FAIL no help text`. Delete the `hook-test` row from the Make table while D6 prose still names the target: `FAIL no row: hook-test` (the check reads the table only). |
| REQ-0056-11 | Same plants as REQ-0056-10, seen from the PR side: the PR cannot go green. |
| REQ-0056-12 | Today: red. The command in the last block of this section prints three lines: `missing oss-check`, `missing spdx-check`, `missing hook-test`. Green after P3. |
| REQ-0056-13 | Blank the owner cell of the `hook-test` row: `FAIL owner cell`. Write `9999` in it: `FAIL owner spec 9999`. |

The command below runs today on `development` plus this spec. It is the body of P1 in shell
form, so the builder's recipe has a proven starting point.

```bash
bad=0
allowed=" name description tools disallowedTools model maxTurns effort isolation background memory "
for f in .claude/agents/uas-*.md; do
  stem=$(basename "$f" .md)
  n=$(awk '/^---$/{d++;next} d>=2{c++} END{print c+0}' "$f")
  { [ "$n" -ge 40 ] && [ "$n" -le 110 ]; } || { echo "FAIL body lines $n: $f"; bad=1; }
  LC_ALL=C grep -qP '[^\x00-\x7F]' "$f" && { echo "FAIL non-ASCII: $f"; bad=1; }
  for k in $(awk '/^---$/{d++;next} d==1{sub(/:.*/,"");print}' "$f"); do
    case "$allowed" in *" $k "*) ;; *) echo "FAIL frontmatter key $k: $f"; bad=1;; esac
  done
  grep -q "^name: $stem\$" "$f" || { echo "FAIL name: $f"; bad=1; }
  grep -qE '^model: (haiku|sonnet|opus|inherit)$' "$f" || { echo "FAIL model: $f"; bad=1; }
  h=$(grep '^## ' "$f" | cut -c4- | paste -sd,)
  [ "$h" = "Boundaries,Tools and policy,Escalation,Output" ] || { echo "FAIL heading order: $f"; bad=1; }
  grep -q '^Forbidden' "$f" || { echo "FAIL Forbidden: $f"; bad=1; }
  grep -qiE 'fifteen lines or fewer|15 lines or fewer' "$f" || { echo "FAIL verdict cap: $f"; bad=1; }
  t=$(sed -n 's/^tools: //p' "$f")
  for x in $(echo "$t" | tr -d ','); do
    case "$x" in Read|Grep|Glob|Bash|Write|Edit|Agent|SendMessage|WebFetch|WebSearch|Monitor|LSP) ;; *) echo "FAIL tool $x: $f"; bad=1;; esac
  done
  grep -q '^model: haiku$' "$f" && echo "$t" | grep -qE 'Write|Edit' && { echo "FAIL haiku tools: $f"; bad=1; }
  grep -qF "\`$stem\`" .claude/agents/README.md || { echo "FAIL no README row: $stem"; bad=1; }
  grep -qF "| \`$stem\` |" specs/0056-agent-roster-and-tooling.md || { echo "FAIL no roster row: $stem"; bad=1; }
  grep -qF "| $t |" specs/0056-agent-roster-and-tooling.md || { echo "FAIL tools not in roster: $stem"; bad=1; }
done
for p in $(sed -n 's/.*persona = "\([^"]*\)".*/\1/p' .saga/teams.toml); do
  [ -f ".claude/agents/$p.md" ] || { echo "FAIL team persona $p"; bad=1; }
done
for d in .agents/skills/*/; do
  s=$(basename "$d")
  grep -q "^name: $s\$" "$d/SKILL.md" || { echo "FAIL skill name: $s"; bad=1; }
  grep -q '^description: Use when' "$d/SKILL.md" || { echo "FAIL skill trigger: $s"; bad=1; }
  [ "$(readlink ".claude/skills/$s")" = "../../.agents/skills/$s" ] || { echo "FAIL skill link: $s"; bad=1; }
done
tbl=$(awk '/^\| Targets \| Owner/{f=1} f&&/^$/{exit} f' specs/0056-agent-roster-and-tooling.md | sed 1,2d)
ph=$(sed -n '/^\.PHONY/,/^$/p' Makefile | tr -d '\\' | sed 's/^\.PHONY://' | tr -s ' \t' '\n' | grep -v '^$')
for t in $(grep -oE '^[a-zA-Z0-9_-]+:( |$)' Makefile | tr -d ': '); do
  echo "$ph" | grep -qx "$t" || { echo "FAIL not in .PHONY: $t"; bad=1; }
  grep -qE "^$t:.*## " Makefile || { echo "FAIL no help text: $t"; bad=1; }
  echo "$tbl" | grep -qF "\`$t\`" || { echo "FAIL no row: $t"; bad=1; }
done
while IFS= read -r row; do
  own=$(echo "$row" | awk -F'|' '{print $3}')
  case "$own" in *This*|*[0-9][0-9][0-9][0-9]*) ;; *) echo "FAIL owner cell: $row"; bad=1;; esac
  for n in $(echo "$own" | grep -oE '[0-9]{4}'); do ls specs/$n-*.md >/dev/null 2>&1 || { echo "FAIL owner spec $n"; bad=1; }; done
done <<< "$tbl"
for t in $ph; do grep -qE "^$t:" Makefile || { echo "FAIL phony without target: $t"; bad=1; }; done
[ "$bad" = 0 ] && echo OK
```

Gap check, red until P3 (REQ-0056-12):

```bash
for m in $(sed -n 's/^hygiene: \(.*\) ##.*/\1/p' Makefile); do
  grep '`make hygiene`' AGENTS.md | grep -qF "\`$m\`" || echo "missing $m"
done
```

## Notes

- `tooling-check` is the one new target. Its row is in the Make surface table before it exists,
  so the table check passes once P1 lands and not before (REQ-0056-08 is red until then).
- The roster's tool strings are compared by exact text. A persona edit that reorders tools
  changes the roster row in the same PR.
- Skills reach Claude Code through symlinks (AGENTS.md lines 63 to 66). A skill added without
  the symlink is invisible to `/skills`.
- Open, for another lane: whether paas will publish `tests/agents.test.sh` (F5). Not blocking.
- Amendments this spec requires of others, done in P3: spec 0002 line 105 and spec 0052 line 228
  say the `hygiene` drift belongs to a 0052 note or a 0002 amendment; both change to name spec 0056.
