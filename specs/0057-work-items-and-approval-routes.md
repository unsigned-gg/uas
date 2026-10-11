---
status: planned
created: 2026-10-11
tags: [work-items, approval, codeowners, tasks, engineering]
priority: medium
depends_on: [0003-branch-model-and-promotion, 0025-wave-execution-on-the-custody-chain, 0037-repository-migration-to-uas, 0048-licence-and-publication, 0050-build-handoff-pipeline, 0053-changelog-and-commit-text-policy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0057-work-items-and-approval-routes

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

Work enters this repo through four surfaces: an issue template, a PR template, a task file for
blackwall, and a CODEOWNERS file. Who may merge or stop a change is written in five bullets of
`AGENTS.md` and in specs 0003 and 0050. No spec owns the whole contract, and no check reads any of
it. This spec names each surface, fixes the route of each class of action, and adds one check,
`work-check`. Christian ruled on 2026-10-11 about 05:50Z (AskUserQuestion, lane w25:p1, label
"Work items and approval routes") that this spec is drafted [reported: lane brief]. Content decisions
are the integrating lane's unless marked proposed.

### Facts (read 2026-10-11 at `ebc7be8`, `origin/development`)

- **F1 [verified]:** `ls .github/ISSUE_TEMPLATE` prints `config.yml` and `work-item.md`. `config.yml`
  holds one line, `blank_issues_enabled: false` (`cat -n`). `work-item.md` has front matter `name`,
  `about`, `title`, `labels` (lines 1 to 6) and eight H2 sections: Why, Spec, Scope, Done when, Branch
  and PR, Evidence, Gated steps after merge, Rollback (lines 8, 12, 17, 22, 26, 32, 37, 41). There is
  no bug or question template.
- **F2 [verified]:** `.github/pull_request_template.md` holds five H2 sections (Problem, What this does,
  Verified, Gated steps after merge, Rollback) and a last line `Part of CER-N`. The text `CER-N` does not
  match the 0053 D7 pattern `Part of (CER-[0-9]+|spec [0-9]{4})`, so an unedited template carries no ticket.
- **F3 [reported: spec 0045 D7, AGENTS.md rule 6, 0053 D7]:** A tracked item is a Linear ticket on team
  CER, not a GitHub issue. The repo holds no Linear configuration, so the mapping below cannot be read here.
- **F4 [verified: `cat tasks/*/task.toml`]:** Three task files exist. Keys used: `provider`, `model`,
  `budget_tokens`, `conventional_subject` and a `[vars]` table (`ticket`). None sets `read_paths` or
  `write_paths`. `tasks/wave-smoke-build/task.toml` has no `conventional_subject`. Every `tasks/<name>/`
  holds a `prompt.md` (`ls tasks/*`).
- **F5 [reported: soma@46701a3:blackwall/crates/blackwall-cli/src/task.rs, a local checkout that is
  on no remote branch; `blackwall --version` prints 1.2.0 and the checkout's crate version is 1.2.0]:**
  `parse_task_toml` reads `read_paths` and `write_paths` into the task metadata (lines 283 to 290). A
  search of `blackwall-cli/src` finds no use of them outside the metadata `Display` (lines 93 to 102);
  the jail takes its paths from the provider. Its final match arm is `_ => {}`, so an unknown key
  is dropped with no error. It also reads `[providers] chain` and `[cicatrix]`, which uas does not use.
  `tasks/README.md` line 11 says the same of the two path keys.
- **F6 [verified: `cat .github/CODEOWNERS`]:** Three patterns, each owned by `@todie`: `*`, `/.claude/`,
  `/.github/`. Seven comment lines record two co-approvers by role and say they are left out of the
  patterns so GitHub does not auto-request them. `MAINTAINERS.md` has three rows, all `todie`; the two
  paths it names exist (`ls -d crates/uas-ebpf/ebpf crates/uas-sandbox`).
- **F7 [verified: `gh api repos/unsigned-gg/uas/branches/development/protection`]:** Required approvals
  0, `require_code_owner_reviews` false, `required_signatures` false, six required checks (`secrets`,
  `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`, `fork-gate`). Squash and rebase merge are off
  (`gh api repos/unsigned-gg/uas`). So CODEOWNERS requests a review and gates nothing.
- **F8 [verified: `git grep` for each name outside its own file]:** `engineering/` holds `plans/` (twelve
  files, README included), `session-notes/` (only `.gitkeep`), `suggestions.md` (an empty template, "None
  yet"), `roadmap-improvements.md` and `cluster-survey-atlas.md` (mostly "Not probed yet"). Nothing links
  to `suggestions.md`, `roadmap-improvements.md` or `session-notes`. One plan cites the atlas. All three
  top-level files date from 25a4e81, 2026-10-10.

## Design

### Work items

A work item is the Linear ticket. The GitHub issue template is its shape, kept in the repo so a person
without Linear access files the same eight sections. The PR is the unit of delivery.

| Template section | Linear ticket | PR template section |
|---|---|---|
| Why | Description, first block | Problem |
| Spec, Scope | Description, next blocks | What this does |
| Done when, Evidence | Description, acceptance block | Verified |
| Branch and PR | Not copied | Not copied |
| Gated steps after merge | Description, last block | Gated steps after merge |
| Rollback | Description, last block | Rollback |

A PR cites its ticket as `Part of CER-N` or, when it has none, `Part of spec NNNN` (0053 D7). A spec
that is its own cause needs no ticket, so the second form names the spec the change implements.

### Task files

`tasks/<name>/` is a blackwall task. The name is the step id (0025 D11). `prompt.md` is required.
`task.toml` is optional and non-binding: blackwall reads it as hints and a wrong key never fails a run.
The allowed top-level keys are `provider`, `model`, `budget_tokens`, `conventional_subject`, and the
table `[vars]`. `model` for a wave Builder is `sonnet` (0025 D6, F12).

### Approval routes

Estate rules, cited without their paths: land-on-green and prod-is-dev (both 2026-10-06), and the
dispatch codes (ratified 2026-07-24; 2xx means the agent prepares and an operator green-lights).
Land-on-green is the standing green light for route 1 and 2: the lane still acts at class 200.

| Route | Action | Who acts | Who gates | Class |
|---|---|---|---|---|
| 1 | Merge a lane's own feature PR into `development` | The lane | Required checks SUCCESS on the exact head; no approval | 200-GIT, lit by ruling |
| 2 | Merge the promotion PR to `main` | The lane | Same, merge commit pinned to the head (0003) | 200-GIT, lit by ruling |
| 3 | Change branch protection, merge methods, required checks, signing | Operator | Operator; the lane prepares the API call and stops | 220-GIT |
| 4 | Secrets, Terraform, DNS, platform apps | Operator | Operator (AGENTS.md approval routes) | 230-SEC, 220-IAC |
| 5 | The public cut: visibility flip, announcement, enabling private vulnerability reporting | Operator | Operator's go in the launch checklist (0048 REQ-0048-17) | 240-EXT |
| 6 | Edit another lane's repo, service or live session | That lane | The owning lane; send it a request | 320-ORC |
| 7 | Any send to a person (Slack, Linear, email, mesh) from a subagent | Integrating lane | Operator | 240-EXT |

Never, with no override: force-push to `main`, an unsigned commit, a merge with red CI (430).

### Code owners

`.github/CODEOWNERS` maps a path to a review request. With approvals at 0 (F7) it gates nothing and a
lane never waits on it. `MAINTAINERS.md` names the humans and areas; the two files agree on handles.

### The `engineering/` tree

| Entry | Purpose | Retention |
|---|---|---|
| `engineering/plans` | One build plan per spec; owned by 0050 and its README | Stays; a landed plan stays as the record (plans README) |
| `engineering/suggestions.md` | Numbered suggestions with evidence | Stays until the launch checklist; deleted if still empty then (D11) |
| `engineering/roadmap-improvements.md` | Roadmap items, proposed until the owner accepts | Stays; each item moves to a spec or a ticket when accepted |
| `engineering/cluster-survey-atlas.md` | Where each signal lives on the estate | Christian to rule (D12) |
| `engineering/session-notes` | Intended for session notes; holds `.gitkeep` only | Christian to rule (D12) |

### Decisions

Classes: `ruled` (Christian decided it), `lane-decided` (the integrating lane decided it on 2026-10-11
and Christian may override), `proposed: Christian` (a decision only Christian can take).

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | The Linear ticket is the work item. The issue template is its shape, kept for people without Linear. A repo file never states a Linear field that this repo cannot read. | F3; the repo cannot see Linear. | lane-decided: integrating lane, 2026-10-11 |
| D2 | A PR cites `Part of CER-N` or `Part of spec NNNN`. `NNNN` names a file in `specs/`. A literal `CER-N` is not a citation. | 0053 D7 checks the form only; F2 shows the template ships a placeholder. | lane-decided: uas-engineer, 2026-10-11 |
| D3 | `work-check` reads the repo, not Linear and not a live PR. The PR body check stays in `text-check` (0053 D9). | One check per input; 0053 owns the body. | lane-decided: uas-engineer, 2026-10-11 |
| D4 | The allowed `task.toml` keys are the five of the Design section. `read_paths`, `write_paths`, `[providers]` and `[cicatrix]` fail `work-check`. | F5: blackwall drops unknown keys and ignores the path keys, so only this repo can catch a typo or a false sense of confinement. | lane-decided: uas-engineer, 2026-10-11 |
| D5 | `conventional_subject` is optional in the file and required for a task whose run opens a PR by hand. A wave step task takes its title from the executor (Notes, open). | `blackwall pr` refuses a missing subject (F5 source, `pr.rs` BadSubject); `wave-smoke-build` has none (F4). | lane-decided: uas-engineer, 2026-10-11 |
| D6 | Routes 1 and 2 stay class 200 with land-on-green as the standing green light. The lane reports each merge and does not ask first. | AGENTS.md rule 9; 0003 promotion ruling; estate land-on-green. | ruled: Christian, 2026-10-06 and 2026-10-08, cited by 0003 |
| D7 | Route 3 is gated. A lane never calls the protection API, not even for a check name it added. | AGENTS.md lists no such route; a setting change reaches every lane (0037 D6 exports and re-applies them). | proposed: Christian |
| D8 | Route 6: a lane edits only files in its brief. A change in another lane's repo is a request sent to that lane. | Estate land-on-green: "another lane's repo or service" is not moved without the operator. | lane-decided: integrating lane, 2026-10-11 |
| D9 | CODEOWNERS pattern lines name only people listed in `MAINTAINERS.md`. A co-approver goes in a comment, and into a pattern only after that person asks and picks the paths. | The estate attention rule: no agent spends a co-approver's attention by auto-request; the timing is Christian's. | lane-decided: integrating lane, 2026-10-11, from CODEOWNERS lines 1 to 7 |
| D10 | `require_code_owner_reviews` stays false while approvals stay 0 (0003). Turning it on is route 3. | A required code-owner review with one owner blocks every PR by that owner. | lane-decided: uas-engineer, 2026-10-11 |
| D11 | `suggestions.md` stays until the launch checklist (0048) is ticked. It is deleted then if it has no entry. | Empty since 25a4e81, no inbound link (F8). A file with no content costs a reader. | proposed: Christian |
| D12 | `cluster-survey-atlas.md` and `session-notes/` are removed, unless Christian keeps them. Default until ruled: both stay. | Both are stubs with no link or reader (F8); the repo runs no service (atlas, 2026-10-08). | proposed: Christian |

### Requirements (EARS)

- **[REQ-0057-01]** (Ubiquitous): The repository shall carry `.github/ISSUE_TEMPLATE/work-item.md` with the front matter keys `name` and `about`, and the eight H2 sections of F1 in that order.
- **[REQ-0057-02]** (Ubiquitous): The file `.github/ISSUE_TEMPLATE/config.yml` shall set `blank_issues_enabled: false`.
- **[REQ-0057-03]** (Event-driven): WHEN a PR opens, its body shall hold one line `Part of CER-N` with a numeric N, or one line `Part of spec NNNN` where `specs/NNNN-*.md` exists (D2).
- **[REQ-0057-04]** (Unwanted behavior): IF a PR body holds only the template placeholder `Part of CER-N`, THEN `text-check` shall report it as no citation.
- **[REQ-0057-05]** (Ubiquitous): The repository shall give every `tasks/<name>/` a `prompt.md`, and every `task.toml` key shall be one of D4's five (a `[vars]` table counts as one).
- **[REQ-0057-06]** (Unwanted behavior): IF a `task.toml` holds `read_paths`, `write_paths`, `[providers]`, `[cicatrix]` or an unknown key, THEN `work-check` shall exit non-zero and name the file and key.
- **[REQ-0057-07]** (State-driven): WHILE blackwall 1.2.0 ignores `read_paths` and `write_paths`, no spec or prompt shall rely on them to confine a run.
- **[REQ-0057-08]** (Optional feature): WHERE a `task.toml` sets `conventional_subject`, the subject shall match the 0051 type pattern and hold no ticket ID (0053 D7).
- **[REQ-0057-09]** (Ubiquitous): The CODEOWNERS file shall name only paths that exist in the tree and only owners that `MAINTAINERS.md` lists.
- **[REQ-0057-10]** (Ubiquitous): The `MAINTAINERS.md` file shall name only paths that exist, and each handle in its GitHub column shall own a CODEOWNERS pattern.
- **[REQ-0057-11]** (State-driven): WHILE required approvals are 0 and `require_code_owner_reviews` is false, no lane shall wait on a code owner before merging (D10).
- **[REQ-0057-12]** (Event-driven): WHEN a change needs route 3, 4, 5 or 7, the lane shall prepare it, list it under "Gated steps after merge" in the PR, and stop.
- **[REQ-0057-13]** (Event-driven): WHEN a lane merges under route 1 or 2, it shall use a merge commit pinned to the head SHA, after every required check reads SUCCESS on that head.
- **[REQ-0057-14]** (Ubiquitous): The `AGENTS.md` section "Approval routes" shall point to the route table of this spec and shall not contradict it.
- **[REQ-0057-15]** (Ubiquitous): The tree table above shall list every top-level entry of `engineering/`, with a purpose and a retention.
- **[REQ-0057-16]** (Unwanted behavior): IF a lane needs an edit in another lane's repo, service or live session, THEN it shall send that lane a request and shall not make the edit (D8).

## Plan

Order: P1 is the check. P2 and P3 are pointers and cleanup. This commit touches only the spec and the registry.

- [ ] P1, `xtask/src/work_check.rs`, the `work-check` arm in `main.rs`, `make work-check`, and a non-required CI job: REQ-01, 02, 05, 06, 09, 10, 15. Red first: Test rows 1, 3, 4, 5, 6, 7.
- [ ] P2, pointers: `AGENTS.md` approval routes link here (REQ-14), `CONTRIBUTING.md` links the work-item table, and the PR template `Part of` line gains "or `Part of spec NNNN`". `text-check` gets the numeric and spec-exists test (REQ-03, 04; Test row 2).
- [ ] P3, cleanup after Christian rules D11 and D12: delete or keep, in one `docs` PR.
- [ ] P4, read back route 3 once: `gh api repos/unsigned-gg/uas/branches/development/protection` prints approvals 0 and the required checks (F7). 0003's table still says `fork-gate` is not required (Notes, open).

## Test

The block runs today from the repo root on a scratch worktree and changes no tracked file. It is the
reference for P1. Each row prints `row N: ok`; a "plant" row passes only when the bad input is
rejected, so a check that accepts everything turns it red.

```bash
b=origin/development; s=$(mktemp -d); git worktree add -q --detach "$s" "$b"; trap 'git worktree remove --force "$s"' EXIT
cp specs/0057-work-items-and-approval-routes.md "$s/specs/"; cd "$s"; r=0
want() { [ "$2" = "$3" ] && echo "row $1: ok ($3)" || { echo "row $1: RED got $2 want $3"; r=1; }; }
tpl() { p=0; for h in Why Spec Scope 'Done when' 'Branch and PR' Evidence 'Gated steps after merge' Rollback; do n=$(grep -n "^## $h\$" "$1" | head -1 | cut -d: -f1); [ -n "$n" ] && [ "$n" -gt "$p" ] || return 1; p=$n; done; grep -q '^name:' "$1" && grep -q '^about:' "$1"; }
t=.github/ISSUE_TEMPLATE/work-item.md
want 1 "$(tpl $t && echo PASS || echo FAIL)" PASS
sed '/^## Done when/d' $t > bad.md; want 1p "$(tpl bad.md && echo PASS || echo FAIL)" FAIL
want 1c "$(grep -qx 'blank_issues_enabled: false' .github/ISSUE_TEMPLATE/config.yml && echo PASS || echo FAIL)" PASS
pl='^Part of (CER-[0-9]+|spec [0-9]{4})$'
cit() { l=$(grep -E "$pl" "$1" | head -1); [ -n "$l" ] || return 1; case "$l" in "Part of spec "*) ls specs/${l##* }-*.md >/dev/null 2>&1;; esac; }
printf 'Part of CER-9\n' > a; printf 'Part of spec 0057\n' > b; printf 'Part of spec 9999\n' > c; cp .github/pull_request_template.md d; printf 'nothing\n' > e
want 2 "$(cit a && cit b && echo PASS || echo FAIL)" PASS
want 2p "$(for f in c d e; do cit $f && echo ACCEPT || echo REJECT; done | tr '\n' ' ')" 'REJECT REJECT REJECT '
ok='^(provider|model|budget_tokens|conventional_subject|\[vars\]|ticket|#.*|)$'
tk() { for f in tasks/*/task.toml; do [ -f "$(dirname $f)/prompt.md" ] || return 1; done; ! cat "$@" | sed 's/ *=.*//;s/^ *//' | grep -vE "$ok" | grep -q .; }
want 3 "$(tk tasks/*/task.toml && echo PASS || echo FAIL)" PASS
cp tasks/harness-evidence/task.toml x.toml; echo 'read_paths = ["/x"]' >> x.toml;  want 4 "$(tk x.toml && echo PASS || echo FAIL)" FAIL
cp tasks/harness-evidence/task.toml y.toml; echo 'colour = 1' >> y.toml;           want 4b "$(tk y.toml && echo PASS || echo FAIL)" FAIL
co() { g=.github/CODEOWNERS; grep -vE '^(#|$)' $g | while read -r p o; do [ "$p" = '*' ] || [ -e "${p#/}" ] || { echo MISSING; return; }; for h in $o; do grep -q "\`${h#@}\`" MAINTAINERS.md || echo STRANGER; done; done; }
want 5 "$(co | sort -u | tr -d '\n')" ''
cp .github/CODEOWNERS C.bak; echo '/nope/ @todie' >> .github/CODEOWNERS;            want 6 "$(co | sort -u | tr -d '\n')" MISSING
cp C.bak .github/CODEOWNERS; echo '/docs/ @example-person' >> .github/CODEOWNERS;   want 6b "$(co | sort -u | tr -d '\n')" STRANGER
cp C.bak .github/CODEOWNERS
mp() { grep -o '`[a-z./_-]*/`' MAINTAINERS.md | tr -d '`' | while read -r p; do [ -e "$p" ] || echo MISSING; done; }
want 7 "$(mp | sort -u | tr -d '\n')" ''
echo '| Docs | X | `todie` |' >> MAINTAINERS.md; echo '| Area, `crates/gone/` | X | `todie` |' >> MAINTAINERS.md; want 7p "$(mp | sort -u | tr -d '\n')" MISSING
e() { for x in $(ls -A engineering); do grep -q "^| \`engineering/$x\`" specs/0057-work-items-and-approval-routes.md || echo "$x"; done | grep -v '^\.gitkeep$' | tr '\n' ' '; }
want 8 "$(e)" ''; touch engineering/stray.md; want 8p "$(e)" 'stray.md '
exit $r
```

Rows 1p, 2p, 4, 4b, 6, 6b, 7p and 8p are the negative controls. Row 8 reads this spec's own tree
table, so the table and the directory cannot drift. Row 2 uses only the form and the spec-exists
leg; the body check proper is `text-check` (0053). The tree table lists `engineering/plans`,
`engineering/session-notes` and the three files by their repo path.

| REQ | Row or check |
|---|---|
| 01, 02 | 1, 1p, 1c |
| 03, 04 | 2, 2p |
| 05, 06 | 3, 4, 4b |
| 09 | 5, 6, 6b |
| 10 | 7, 7p |
| 15 | 8, 8p |
| 07, 08, 11 to 14, 16 | manual, reviewer; REQ-11 and REQ-13 are read back with the P4 command |

## Notes

- Ruling that this spec is drafted: Christian, lane w25:p1, 2026-10-11 about 05:50Z, AskUserQuestion,
  label "Work items and approval routes" [reported: lane brief].
- Sibling specs: 0050 owns plans and D9 (the five checks that gate a wave PR); 0053 owns the
  `Part of` pattern and the body check; 0048 D12 and REQ-0048-14 own the existence of CODEOWNERS and
  MAINTAINERS; this spec owns their agreement. Spec 0045 D7 is the one place that names Linear.
- Caveat: F5 comes from a local blackwall checkout that is on no remote branch. The saga lane
  confirms the contract against the released 1.2.0 source before P1 lands.
- Open, for the saga lane: confirm F5 against the released 1.2.0 source (D4 rests on it).
- Open, for the integrating lane: spec 0003's table says `fork-gate` is not yet required, and the
  live protection requires it (F7); `required_signatures` is false although AGENTS.md rule 5 says
  never skip signing, so signing is checked by hooks and CI only (spec 0051 owns that).
- Open, for the integrating lane: how a wave step task gets its PR title when its `task.toml` has no
  `conventional_subject` (D5).
- Parked: a Linear-side check that a ticket has the eight sections. The repo cannot read Linear.
- Parked: a bug-report and a question issue template. `blank_issues_enabled: false` leaves the
  work item as the only form; the public cut (0048) decides whether outsiders need another.
