<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0057-work-items-and-approval-routes

Spec: `specs/0057-work-items-and-approval-routes.md` (status planned, 16 requirements, D1 to D12). Base: `origin/development` `b0958f9`, which carries the spec. Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **The spec's Test block is a regression reference, not a builder done-when [verified]:** I ran the fenced block on the base and rows 1, 1p, 1c, 2, 2p, 3, 4, 4b, 5, 6, 6b, 7, 7p, 7g, 8 and 8p all print `ok`. Every row is green today, so no row can show a package red. It also pins `b=origin/development`, so it never sees a builder's branch, and it re-implements the check in shell. The builders' done-when is the `xtask` test that mirrors each planted row, plus `cargo xtask work-check`, which exits 2 today (`unknown command`). The Test block stays as the cross-check: the closing package runs it once on `development`.
2. **Most of the spec's `text-check` item already exists [verified, `xtask/src/text_check.rs`]:** `is_part_of` (line 198) accepts `Part of CER-<digits>` and `Part of spec <4 digits>` as a whole line, and `body_warnings` warns when none is present. So REQ-0057-04 holds today and `body_with_template_placeholder_warns_once_about_the_part_of_line` (`xtask/tests/text_check.rs` line 370) pins it. The only missing leg is "the spec exists" (REQ-0057-03). It stays a warning: 0053 D9 says body checks never fail, and 0057 D3 leaves the body check in `text-check`.
3. **The PR template line must stay exactly `Part of CER-N` [verified]:** `xtask/tests/text_check.rs` lines 355 to 361 replace that literal string with `Part of CER-9` and expect zero warnings; line 370 expects one warning on the unedited template. Appending "or `Part of spec NNNN`" to the line makes `is_part_of` reject it and turns the first test red. Put the second form in the HTML comment above the line, and add no line there that `is_part_of` accepts (Test row 2p copies the template and demands REJECT).
4. **Test row 3 and D4 disagree [verified, read of the row]:** the row's regex lets `ticket` pass as a top-level key and rejects any `[vars]` key other than `ticket`. D4 counts the `[vars]` table as one key with free contents, and a top-level `ticket` is not a D4 key. `work-check` tracks the current table. The spec should say so (P1).
5. **`toml` is workspace-pinned but xtask does not use it [verified]:** `Cargo.toml` line 39 holds `toml = "1.1"`; the spec 0004 pin row says 1.1.8 and its "Used by" cell lists `uas-cli, uas-engine` only. `xtask/Cargo.toml` depends on `anyhow` and `sha2`; `text_check.rs` is std plus `git`. Plan: std-only line scanner in P8 (table-aware, a line that is not `key =` or `[table]` is a continuation of a multi-line value). Adding `toml` to xtask would need the 0004 cell edited and `pin-check` re-run [asserted, not run]. No pin is added by this plan.
6. **Where `work-check` runs is the spec's shape, not the brief's alternative [verified, spec Plan P1 and `ci.yml` lines 130 to 153]:** a separate, non-required CI job modelled on `text-check`, not a leg of `hygiene`. `hygiene` is a required check, so folding in would change what gates merges. The job reads only the repo, so it may run on push as well as on pull requests. Folding into `hygiene` stays an Open item for the integrator.
7. **Saga confirmation gates less than the spec implies [verified by reading D4 and the planned code]:** an allowlist of the five keys already rejects `read_paths` and `write_paths`. What F5 decides is whether blackwall 1.2.0 really drops them (the reason for rejecting) and so whether they stay off the list. P8 lands the allowlist with a named message for the two keys; P10 is the confirmation, and a different answer changes one constant and REQ-0057-06/07.
8. **Nothing in `engineering/` is lane-decided to edit [verified]:** `engineering/roadmap-improvements.md` line 3 already says every item is proposed until the owner accepts it, and `plans/` stays. The only retention edits are D11 and D12, which are the operator's (P11). REQ-0057-15 is closed by the check (P6), not by an edit.
9. **A D12 delete leaves a stale citation [verified]:** `engineering/plans/0048-licence-and-publication.md` line 101 cites `engineering/cluster-survey-atlas.md` in backticks. `spec-check` R12 checks links only, so it would not catch it. P11 edits that line in the same PR.
10. **Spec 0003's stale `fork-gate` row is owned elsewhere [verified]:** the branch `docs/fork-gate-required` (a separate worktree, unmerged) edits `specs/0003-branch-model-and-promotion.md` and the 0051, 0053 and 0050 text. This plan does not touch spec 0003. P9 reads protection back once; if the branch has not landed by then, the integrator reports the mismatch and does not edit 0003 here.
11. **Spec 0057 cites spec 0045 D7 but does not list 0045 in `depends_on` [verified, front matter lines 1 to 7]:** `spec-check` R03 requires each `depends_on` stem to exist, so adding `0045-security-disclosure-and-advisory-policy` is safe. 0045 D7 is the only other place the repo names Linear as tracker of record, and says nothing of this spec. P1 adds both pointers.
12. **CHANGELOG [verified, `AGENTS.md` rule 8 and spec 0053 D2]:** a builder writes none. The integrator adds one `docs(changelog)` commit per PR that carries a user-visible change. `CHANGELOG.md` line 10 already records the spec itself.
13. **Two repo facts the check legs rely on [verified]:** the Makefile `oss-check` bans operator paths and tailnet hostnames in tracked files, so fixtures in tests use `/nope/` style paths only. A plan section whose body holds the word BLOCKED is exempt from the shape warning, so a non-blocked package never uses that word.

## Plan items to packages

| Spec Plan item | Package |
|---|---|
| P1 `work_check.rs`, the `main.rs` arm, `make work-check`, the CI job: REQ-01, 02 | P2 (core, template and config legs, Makefile), P7 (CI job) |
| P1 REQ-05, 06 (task files) | P8 (allowlist, `prompt.md`, subject), P10 (F5 confirmation) |
| P1 REQ-09, 10 (CODEOWNERS and MAINTAINERS) | P4 |
| P1 REQ-15 (the `engineering/` tree) | P6 |
| P2 `AGENTS.md` approval routes (REQ-14), `CONTRIBUTING.md` work-item table, PR template line | P5 |
| P2 `text-check` numeric and spec-exists test (REQ-03, 04) | P3 (numeric half already exists, finding 2) |
| P3 cleanup after D11 and D12 | P11 |
| P4 read back route 3 once | P9 |
| Not in the spec's Plan: spec amendments from findings 4, 5, 11, and the 0045 D7 cross-reference | P1 |
| Status move | P12 |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0057-01, 02 | P2 (hold today; the check pins them) |
| REQ-0057-03 | P3 (the spec-exists leg), P5 (the template comment names both forms) |
| REQ-0057-04 | no change package: already true and pinned by an existing test (finding 2); P3 keeps it green |
| REQ-0057-05, 06 | P8; P10 confirms the premise (F5) |
| REQ-0057-07 | P10 (confirmation); `tasks/README.md` line 11 already says it |
| REQ-0057-08 | P8 (the type list and the ticket test) |
| REQ-0057-09, 10 | P4 |
| REQ-0057-11, 13 | P9 (read-back of protection and the merge method); the lane follows 13 by hand |
| REQ-0057-12, 16 | no package: manual rules, read by the reviewer |
| REQ-0057-14 | P5 |
| REQ-0057-15 | P6 |

11 of 16 requirements are closed by a change package (01 to 06, 08 to 10, 14, 15; 04 already holds). 07, 11 and 13 are closed by a read-back or a confirmation, and 12 and 16 are manual. Waiting on a ruling: the clean-up of D11 and D12 (P11), and the F5 confirmation (P10).

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| Wave 1 | P1, P2, P3 | P1: `specs/0057-...md`, `specs/0045-...md`. P2: `xtask/src/work_check.rs`, `xtask/src/work_check/template.rs`, `xtask/src/main.rs`, `Makefile`. P3: `xtask/src/text_check.rs`, `xtask/src/text_check_tests.rs`, `xtask/tests/text_check.rs` | Specs carry the amendments. `cargo xtask work-check` runs and passes with the template leg. A cited spec that does not exist warns |
| Wave 2 | P4, P5 | P4: `xtask/src/work_check.rs`, `xtask/src/work_check/owners.rs`. P5: `AGENTS.md`, `CONTRIBUTING.md`, `.github/pull_request_template.md` | CODEOWNERS and MAINTAINERS agree by check. Docs point at the spec and at `make work-check` |
| Wave 3 | P6, P7 | P6: `xtask/src/work_check.rs`, `xtask/src/work_check/tree.rs`. P7: `.github/workflows/ci.yml` | The `engineering/` tree is checked. The non-required `work-check` job exists |
| Wave 4 | P8, P9 | P8: `xtask/src/work_check.rs`, `xtask/src/work_check/tasks.rs`. P9: no file | Task files are checked. Protection is read back once |
| Closing | P10, P11, P12 | P10 and P11 wait on rulings; P12 edits spec 0057 and the registry after P11 | The status move; `complete` waits on P10 and P11 |

Order that keeps every check green on landing: each leg is written against the tree as it is today (the rows run `ok` on the base, finding 1), and the CI job (P7) lands after the template, owners and tree legs. Four packages edit `xtask/src/work_check.rs` (P2, P4, P6, P8), so they run in four different waves.

## Packages

### P1: amend specs 0057 and 0045 (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0057-work-items-and-approval-routes.md` (exists, 240 lines; stay under 300), `specs/0045-security-disclosure-and-advisory-policy.md` (exists, 122 lines).
- **Change:** (a) 0057 front matter: add `0045-security-disclosure-and-advisory-policy` to `depends_on`. (b) 0057 Plan P1 and P2: say the numeric leg of `text-check` already exists and the item is the spec-exists leg only; say the template's second form goes in the comment above the line and the line itself stays `Part of CER-N` (finding 3). (c) 0057 D4 or the Test notes: say a top-level `ticket` is not allowed and `[vars]` contents are free, so row 3 is looser than `work-check` (finding 4); add Test rows for REQ-08 (a planted subject with a ticket ID is rejected) and REQ-14 (`AGENTS.md` names this spec) or move them to the "manual" line with that reason. (d) 0057 Plan P1: name the CI job as the spec has it and record that folding into `hygiene` is the integrator's call (finding 6). (e) 0045 D7: add "The repo-side rule for work items is spec 0057 D1" to the Source cell or the Notes, one sentence.
- **Done-when:** `make spec-check JOBS=2 && test "$(grep -c '0045-security-disclosure' specs/0057-work-items-and-approval-routes.md)" -ge 1 && test "$(grep -c '0057' specs/0045-security-disclosure-and-advisory-policy.md)" -ge 1`. The two `grep` legs are asserted, not a Test row: the spec Test block has no row for a cross-reference.
- **Red-first:** run the two `grep` legs before the edit and watch the second print `0` and fail. After green, remove the `depends_on` entry in a scratch copy and watch the first `grep` leg fail; restore. Also run `make spec-check JOBS=2` with a wrong stem (`0045-nope`) and watch it report `depends_on` is not a spec; restore.

### P2: `work-check` core, the template and config legs (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/work_check.rs` (new), `xtask/src/work_check/template.rs` (new), `xtask/src/main.rs` (exists: one `mod`, one match arm, the usage line and the module doc list), `Makefile` (exists: add `work-check` to `.PHONY` at line 16 and a target after `text-check`). No `AGENTS.md` edit here (P5).
- **Change:** `run(root: &Path) -> ExitCode`, std only, the exit codes of `text_check.rs` (0 pass, 1 findings, 2 cannot run). Findings print as `work-check: FAIL <leg>: <path>: <detail>`; a pass prints `work-check: ok`. Each leg is a function `check(root: &Path, findings: &mut Vec<String>)` in its own module, so P4, P6 and P8 each add one `mod` and one call line. The template leg (REQ-0057-01, 02): `.github/ISSUE_TEMPLATE/work-item.md` has front matter lines `name:` and `about:`, and the eight lines `## Why`, `## Spec`, `## Scope`, `## Done when`, `## Branch and PR`, `## Evidence`, `## Gated steps after merge`, `## Rollback` in that order (first occurrence of each, line numbers strictly increasing); `.github/ISSUE_TEMPLATE/config.yml` holds the exact line `blank_issues_enabled: false`. A missing file is a finding that names the file, not a panic. `make work-check` runs `cargo xtask work-check`; the target takes no environment value.
- **Done-when:** `cargo nextest run -p xtask work_check -j 2 && cargo xtask work-check`. The unit tests build a temp tree and mirror Test rows 1, 1p and 1c: a good template passes; the template with `## Done when` removed fails; one with the sections swapped fails; `config.yml` with `blank_issues_enabled: true` fails; a missing `config.yml` fails.
- **Red-first:** before the change `cargo xtask work-check` exits 2 with `unknown command`. After green, make the template leg accept any file (return early) and watch the `1p` and `1c` tests go red; restore.

### P3: the spec-exists leg of the `Part of` line (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/text_check.rs` (exists), `xtask/src/text_check_tests.rs` (exists), `xtask/tests/text_check.rs` (exists).
- **Change:** add `part_of_spec(line: &str) -> Option<&str>` (the four digits of a `Part of spec NNNN` line, same whole-line rule as `is_part_of`), a `specs: Vec<String>` field on `Input` filled in `gather` from `git ls-tree --name-only <head> specs/` (head, not the working tree; names of the form `NNNN-*.md`), and a function `spec_citation_warning(body, specs)` that `evaluate` calls after `body_warnings`. A `Part of spec NNNN` line whose number has no `specs/NNNN-*.md` gives `PR body cites spec NNNN but specs/NNNN-*.md does not exist`. It is a warning and never fails (0053 D9). Do not change the `body_warnings(body)` signature: `text_check_body_warnings_are_advisory` calls it with one argument and expects 6 warnings. The integration fixture repo has no `specs/` directory; the new integration test adds `specs/0001-x.md` to its fixture first.
- **Done-when:** `cargo nextest run -p xtask -j 2`. It runs the unit tests, including the new `text_check_part_of_spec_must_exist`, and the integration tests that read the real template (`full_template_body_with_part_of_line_has_no_warning` and `body_with_template_placeholder_warns_once_about_the_part_of_line`).
- **Red-first:** write the two new tests first (spec 9999 warns; spec 0001 with the file present does not) and watch the first fail with no warning printed. After green, make `part_of_spec` return `None` always and watch the first go red; restore. Spec Test rows 2 and 2p cover the same cases in shell.

### P4: CODEOWNERS and MAINTAINERS parity (wave 2, needs P2)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/work_check.rs` (exists after P2: one `mod`, one call), `xtask/src/work_check/owners.rs` (new).
- **Change:** REQ-0057-09: for each non-blank, non-comment line of `.github/CODEOWNERS`, split pattern and owners; a pattern other than `*` must match a tracked path (`git ls-files -- <pattern without a leading slash>` prints at least one line); each owner `@h` must appear as `` `h` `` in `MAINTAINERS.md`. REQ-0057-10: every backticked token in `MAINTAINERS.md` that ends in `/` must exist on disk; the last backticked cell of each table row is a handle and some non-comment CODEOWNERS line must name `@handle`. A finding names the file and the token (`MISSING`, `STRANGER`, `GHOST` as the Test block's words).
- **Done-when:** `cargo nextest run -p xtask work_check -j 2 && cargo xtask work-check`. Unit tests mirror rows 5, 6, 6b, 7, 7p and 7g on a temp git repo: the real pair passes; `/nope/ @todie` appended fails MISSING; `/docs/ @example-person` appended fails STRANGER; a MAINTAINERS row with `crates/gone/` fails MISSING; a row with `ghost-user` fails GHOST.
- **Red-first:** run the new tests before the leg exists and watch them fail to compile or report no finding. After green, skip the `git ls-files` test and watch the `6` test go red; restore.

### P5: pointers in AGENTS.md, CONTRIBUTING.md and the PR template (wave 2, needs P2 and P3)
- **Persona:** `uas-engineer` (docs only).
- **Files:** `AGENTS.md` (exists, 85 lines), `CONTRIBUTING.md` (exists), `.github/pull_request_template.md` (exists).
- **Change:** `AGENTS.md`: (a) "Approval routes" (lines 71 to 77): keep the five bullets and add, in the spec's words, the rows for branch protection (route 3), the public cut (route 5), another lane's repo (route 6) and a send to a person (route 7), plus the sentence "a lane merges when every required check reads SUCCESS on the exact head, with a merge commit pinned to the head SHA"; end with "The route table is `specs/0057-work-items-and-approval-routes.md`." (b) rule 6: `Put Part of CER-N, or Part of spec NNNN when there is no ticket, in the PR body.` (c) Layout row for `xtask/`: add `work-check` to the command list. (d) Commands table: a `make work-check` row. `CONTRIBUTING.md` line 41: the same two forms, and a sentence that the issue template mirrors the Linear ticket (D1) with a pointer to the spec's work-item table. `.github/pull_request_template.md`: add one HTML comment above the last line that says to replace `CER-N` with the ticket number or to write the spec form; leave the last line exactly `Part of CER-N` and add no line starting `Part of ` (finding 3). The unrelated stale mention of `text-check` and `fetch-contracts` in the `xtask/` row is not part of this package.
- **Done-when:** `cargo nextest run -p xtask -j 2 && test "$(grep -c 'specs/0057-work-items-and-approval-routes.md' AGENTS.md)" -ge 1 && make work-check`. The `grep` leg is asserted (REQ-0057-14 has no Test row); the first leg proves the template edit left the existing body tests green.
- **Red-first:** the `grep` prints `0` before the edit and the leg fails. After green, change the template's last line to `Part of CER-N or spec NNNN` in a scratch copy and watch `full_template_body_with_part_of_line_has_no_warning` go red; restore.

### P6: the `engineering/` tree leg (wave 3, needs P4)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/work_check.rs` (exists: one `mod`, one call), `xtask/src/work_check/tree.rs` (new).
- **Change:** REQ-0057-15: read the table rows of `specs/0057-work-items-and-approval-routes.md` that start with `` | `engineering/ ``; every entry of `engineering/` other than `.gitkeep` must have a row whose first cell is `` `engineering/<entry>` ``. One-way, as Test row 8 is: a row without an entry is not a finding (a deleted entry leaves its row to the cleanup in P11). A missing spec file is a finding.
- **Done-when:** `cargo nextest run -p xtask work_check -j 2 && cargo xtask work-check`. Unit tests mirror rows 8 and 8p: the real tree passes; a stray `engineering/stray.md` fails and names `stray.md`.
- **Red-first:** run the stray-file test before the leg exists and watch it find nothing. After green, return early from the leg and watch the `8p` test go red; restore.

### P7: the non-required `work-check` CI job (wave 3, needs P2)
- **Persona:** `uas-builder`.
- **Files:** `.github/workflows/ci.yml` (exists).
- **Change:** a job `work-check` after `text-check`, same runner (`runs-on: {group: ent-default, labels: [ci-rust]}`), `timeout-minutes: 10`, the pinned `actions/checkout` SHA already used by `text-check`, one step `make work-check`. The `if:` is the fork guard of the `secrets` job (`github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository`), so it also runs on push. A comment above the job says it is informational until the operator adds it to the required list. No value from the PR reaches the shell. The header comment (lines 1 to 4) is unchanged: the required list does not change.
- **Done-when:** `actionlint -config-file .github/actionlint.yaml .github/workflows/ci.yml && test "$(grep -c '^  work-check:' .github/workflows/ci.yml)" = 1 && make hygiene JOBS=2`. After the PR is pushed, the integrator reads `gh pr checks <PR> --json name,state` and sees `work-check` SUCCESS on the head. The `grep` leg is asserted.
- **Red-first:** the `grep` prints `0` before the edit. After green, add a stray `uses:` with an unpinned ref in a scratch copy and watch `actionlint` or the pin rule fail; restore.

### P8: the task-file leg (wave 4, needs P6; waits on P10 only for the path-key message)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/work_check.rs` (exists: one `mod`, one call), `xtask/src/work_check/tasks.rs` (new).
- **Change:** REQ-0057-05, 06, 08. Every `tasks/<name>/` holds `prompt.md`. Every `tasks/*/task.toml` is scanned line by line, std only, tracking the current table: at top level the keys must be `provider`, `model`, `budget_tokens` or `conventional_subject`; the one allowed table is `[vars]`, whose keys are free; any other key or table (`read_paths`, `write_paths`, `[providers]`, `[cicatrix]`, `colour`) is a finding naming the file and the key. For `read_paths` and `write_paths` the message adds that blackwall 1.2.0 ignores them (spec F5). A line that is not `key =` or `[table]` is the continuation of a multi-line value. Where `conventional_subject` is set, the value must match the type alternation of `lefthook.yml` line 10 (`feat|fix|perf|refactor|docs|test|ci|build|infra|security|style|chore`, optional scope, optional `!`, then `: ` and text) and `crate::text_check::ticket_in_subject` must be false. Keep the type list in one constant, with a test that reads `lefthook.yml` and asserts the constant equals its alternation, so the two cannot drift (spec 0051 owns the regex).
- **Done-when:** `cargo nextest run -p xtask work_check -j 2 && cargo xtask work-check`. Unit tests mirror rows 3, 4 and 4b on temp trees: the three real task files pass; `harness-evidence` plus `read_paths = ["/x"]` fails naming the key; plus `colour = 1` fails; a top-level `ticket = "CER-9"` fails; a `tasks/x/` without `prompt.md` fails; a subject `fix(cli): CER-9 x` fails; a subject `oops` fails.
- **Red-first:** run the planted tests before the leg exists and watch them find nothing. After green, add `read_paths` to the allowlist in a scratch copy and watch the `4` test go red; restore.

### P9: read back protection and the merge method (wave 4, no builder)
- **Persona:** the integrating lane (a read-only `gh` call; no persona edits a file).
- **Files:** none (read-only).
- **Change:** run the spec's own read once, after P5 and P7 land, and put the output in the closing PR body under "Verified": approvals, code-owner review, the required contexts, and the two merge-method flags. If `fork-gate` is listed and spec 0003 still says it is not, report the mismatch to the operator and do not edit spec 0003 (finding 10).
- **Done-when:** `test "$(gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_pull_request_reviews.required_approving_review_count // 0')" = 0 && test "$(gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_pull_request_reviews.require_code_owner_reviews // false')" = false && test "$(gh api repos/unsigned-gg/uas --jq '[.allow_squash_merge,.allow_rebase_merge]|join(",")')" = false,false` exits 0. This is the read the spec's Test table points at for REQ-0057-11 and 13 (F7).
- **Red-first:** control: change the first expected value from `0` to `1` and watch the command exit non-zero; restore. A `gh` auth failure is an error, not a pass: confirm the same call returns a field you know (`.name`) first.

### P10: confirm F5 against the released blackwall 1.2.0 source (BLOCKED)
- **Persona:** the saga lane (not a uas builder).
- **Files:** none in this repo until the answer comes.
- **Blocked on:** the spec's Notes, "Open, for the saga lane: confirm F5 against the released 1.2.0 source (D4 rests on it)". The request is the integrator's send, not the planner's or a builder's. Unblocks: a reply that `read_paths` and `write_paths` are dropped as F5 says (then REQ-0057-07 is true and P8 stands) or a reply that blackwall applies them (then D4, REQ-0057-06 and 07 and the allowlist constant in P8 change, and a spec amendment goes first).
- **Done-when:** the reply cites the released source file and line. Without it the spec stays in progress.
- **Red-first:** not applicable; this is a fact, not a change.

### P11: ruling on D11 and D12, then the cleanup (BLOCKED)
- **Persona:** `uas-engineer`.
- **Files:** `engineering/suggestions.md`, `engineering/cluster-survey-atlas.md`, `engineering/session-notes/.gitkeep` (all exist), `specs/0057-work-items-and-approval-routes.md` (the tree table rows), `engineering/plans/0048-licence-and-publication.md` (line 101 cites the atlas).
- **Blocked on:** the operator's ruling on D11 (`suggestions.md`: deleted at the launch checklist if still empty) and D12 (`cluster-survey-atlas.md` and `session-notes/`: removed unless he keeps them). Both are `proposed: the operator` in the spec; the default until ruled is that all three stay, and no package deletes anything meanwhile. D11 also waits on the launch checklist of spec 0048 being ticked.
- **Change after the ruling:** delete what he rules out, remove the matching row from the spec's tree table in the same commit (the tree leg is one-way, so a stale row is harmless, but the table should stay true), and replace the atlas citation in plan 0048 with plain text. One `docs` PR (finding 9).
- **Done-when:** `make spec-check JOBS=2 && make work-check && test ! -e engineering/cluster-survey-atlas.md` for a D12 delete (adjust the last leg to the paths he ruled).
- **Red-first:** the `test ! -e` leg fails before the delete. After green, re-add `engineering/stray.md` and watch `make work-check` go red.

### P12: the status move (closing; the first half after wave 1, the second half last)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0057-work-items-and-approval-routes.md` (frontmatter `status`, the body status line), `specs/README.md` (the registry row, line 61).
- **Change:** per the plans README, the three places change in one commit. `planned` to `in-progress` with the first landed wave; `in-progress` to `complete` only when P1 to P9 have landed, P10 has an answer, P11 is ruled and done or recorded as kept, and the spec Test block exits 0 on `development`. While P10 or P11 is open the spec stays `in-progress`.
- **Done-when:** `make spec-check JOBS=2` exits 0 (R13 errors on a status mismatch between the frontmatter and the registry); for `complete`, also `bash <(awk '/^```bash/{c++; if(c==1){p=1;next}} /^```/{p=0} p' specs/0057-work-items-and-approval-routes.md)` run from a clean `development` checkout, which prints `ok` on every row and exits 0.
- **Red-first:** change the registry row to `complete` while the frontmatter says `in-progress` and watch `make spec-check JOBS=2` report a mismatch; restore.

## Open

- **Folding into `hygiene`:** the brief mentioned a shell leg in `hygiene`; the spec says a separate non-required job, and the plan follows the spec (finding 6). Making `work-check` required later is a branch-protection act (route 3, operator).
- **F5 stays reported:** it comes from a local blackwall checkout on no remote branch. P8 lands the allowlist before P10 answers; a different answer is one constant plus a spec amendment.
- **Row 8 is one-way:** a row naming a deleted entry passes. A reverse leg would catch it and is not in the spec.
- **Not run:** I ran the spec's Test block and read the code; I wrote no code, so no done-when above has run green. `actionlint`, `typos` and `cargo nextest` are installed on this host [verified, `command -v`].
- **Not planned:** a Linear-side check and a bug-report template are parked in the spec Notes.
